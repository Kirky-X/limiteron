//! Lua脚本管理器
//!
//! 提供Redis Lua脚本的预加载、SHA缓存和版本管理功能。
//!
//! # 特性
//!
//! - **脚本预加载**: 避免重复传输脚本
//! - **SHA缓存**: 缓存脚本SHA避免重复计算
//! - **原子性操作**: 使用Lua脚本保证Redis操作的原子性
//! - **版本管理**: 支持脚本版本控制

use redis::{AsyncCommands, Script};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info, trace};

use crate::error::StorageError;

/// Lua脚本类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LuaScriptType {
    /// 滑动窗口限流
    SlidingWindow,
    /// 固定窗口限流
    FixedWindow,
    /// 配额扣减
    QuotaConsume,
    /// 配额重置
    QuotaReset,
    /// 令牌桶
    TokenBucket,
}

impl LuaScriptType {
    /// 获取脚本名称
    pub fn name(&self) -> &str {
        match self {
            LuaScriptType::SlidingWindow => "sliding_window",
            LuaScriptType::FixedWindow => "fixed_window",
            LuaScriptType::QuotaConsume => "quota_consume",
            LuaScriptType::QuotaReset => "quota_reset",
            LuaScriptType::TokenBucket => "token_bucket",
        }
    }

    /// 获取脚本版本
    pub fn version(&self) -> &str {
        match self {
            LuaScriptType::SlidingWindow => "1.0",
            LuaScriptType::FixedWindow => "1.0",
            LuaScriptType::QuotaConsume => "1.0",
            LuaScriptType::QuotaReset => "1.0",
            LuaScriptType::TokenBucket => "1.0",
        }
    }
}

/// 滑动窗口Lua脚本
///
/// 使用Redis Sorted Set实现滑动窗口算法
/// 参数: KEYS[1] - key, ARGV[1] - window_size (ms), ARGV[2] - max_requests, ARGV[3] - current_timestamp
/// 返回: (allowed: bool, current_count: int, reset_time: int)
pub const SLIDING_WINDOW_SCRIPT: &str = r#"
-- 获取参数
local key = KEYS[1]
local window_size = tonumber(ARGV[1])
local max_requests = tonumber(ARGV[2])
local current_timestamp = tonumber(ARGV[3])
local window_start = current_timestamp - window_size

-- 移除窗口外的元素
redis.call('ZREMRANGEBYSCORE', key, '-inf', window_start)

-- 获取当前窗口内的请求数
local current_count = redis.call('ZCARD', key)

-- 判断是否允许通过
local allowed = current_count < max_requests

-- 如果允许，添加当前请求
if allowed then
    redis.call('ZADD', key, current_timestamp, current_timestamp)
    -- 设置过期时间（窗口大小 + 1秒）
    redis.call('EXPIRE', key, math.ceil(window_size / 1000) + 1)
end

-- 计算重置时间（窗口开始时间 + 窗口大小）
local reset_time = window_start + window_size

-- 返回结果
return {allowed and 1 or 0, current_count, reset_time}
"#;

/// 固定窗口Lua脚本
///
/// 使用Redis String + TTL实现固定窗口算法
/// 参数: KEYS[1] - key, ARGV[1] - window_size (ms), ARGV[2] - max_requests, ARGV[3] - current_timestamp
/// 返回: (allowed: bool, current_count: int, reset_time: int)
pub const FIXED_WINDOW_SCRIPT: &str = r#"
-- 获取参数
local key = KEYS[1]
local window_size = tonumber(ARGV[1])
local max_requests = tonumber(ARGV[2])
local current_timestamp = tonumber(ARGV[3])

-- 计算当前窗口
local current_window = math.floor(current_timestamp / window_size) * window_size
local window_key = key .. ':' .. current_window

-- 获取当前计数
local current_count = tonumber(redis.call('GET', window_key)) or 0

-- 判断是否允许通过
local allowed = current_count < max_requests

-- 如果允许，增加计数
if allowed then
    redis.call('INCR', window_key)
    -- 设置过期时间（窗口大小 + 1秒）
    redis.call('EXPIRE', window_key, math.ceil(window_size / 1000) + 1)
end

-- 计算重置时间（下一个窗口开始时间）
local reset_time = current_window + window_size

-- 返回结果
return {allowed and 1 or 0, current_count, reset_time}
"#;

/// 配额扣减Lua脚本
///
/// 使用Redis Hash存储配额信息，支持透支
/// 参数: KEYS[1] - key, ARGV[1] - cost, ARGV[2] - limit, ARGV[3] - overdraft_limit, ARGV[4] - window_start, ARGV[5] - window_end, ARGV[6] - consumed_field, ARGV[7] - limit_field, ARGV[8] - window_start_field, ARGV[9] - window_end_field
/// 返回: (allowed: bool, remaining: int, consumed: int)
pub const QUOTA_CONSUME_SCRIPT: &str = r#"
-- 获取参数
local key = KEYS[1]
local cost = tonumber(ARGV[1])
local limit = tonumber(ARGV[2])
local overdraft_limit = tonumber(ARGV[3]) or 0
local window_start = tonumber(ARGV[4])
local window_end = tonumber(ARGV[5])
local consumed_field = ARGV[6]
local limit_field = ARGV[7]
local window_start_field = ARGV[8]
local window_end_field = ARGV[9]

-- 检查窗口是否过期
local stored_window_start = tonumber(redis.call('HGET', key, window_start_field))
if stored_window_start and stored_window_start ~= window_start then
    -- 窗口已过期，重置配额
    redis.call('HMSET', key, consumed_field, 0, window_start_field, window_start, window_end_field, window_end, limit_field, limit)
    redis.call('EXPIRE', key, math.ceil((window_end - window_start) / 1000) + 10)
elseif not stored_window_start then
    -- 首次消费，初始化配额信息
    redis.call('HMSET', key, consumed_field, 0, window_start_field, window_start, window_end_field, window_end, limit_field, limit)
    redis.call('EXPIRE', key, math.ceil((window_end - window_start) / 1000) + 10)
end

-- 获取当前已消费量
local consumed = tonumber(redis.call('HGET', key, consumed_field)) or 0

-- 计算剩余配额（包括透支）
local total_limit = limit + overdraft_limit
local remaining = total_limit - consumed

-- 判断是否允许消费
local allowed = remaining >= cost

-- 如果允许，扣减配额
if allowed then
    redis.call('HINCRBY', key, consumed_field, cost)
    consumed = consumed + cost
    remaining = total_limit - consumed
end

-- 返回结果
return {allowed and 1 or 0, remaining, consumed}
"#;

/// 配额重置Lua脚本
///
/// 重置配额计数
/// 参数: KEYS[1] - key, ARGV[1] - window_start, ARGV[2] - window_end, ARGV[3] - consumed_field, ARGV[4] - window_start_field, ARGV[5] - window_end_field
/// 返回: success (1) or fail (0)
pub const QUOTA_RESET_SCRIPT: &str = r#"
-- 获取参数
local key = KEYS[1]
local window_start = tonumber(ARGV[1])
local window_end = tonumber(ARGV[2])
local consumed_field = ARGV[3]
local window_start_field = ARGV[4]
local window_end_field = ARGV[5]

-- 重置配额
redis.call('HMSET', key, consumed_field, 0, window_start_field, window_start, window_end_field, window_end)
redis.call('EXPIRE', key, math.ceil((window_end - window_start) / 1000) + 10)

-- 返回成功
return 1
"#;

/// 令牌桶Lua脚本
///
/// 使用Redis Hash实现令牌桶算法
/// 参数: KEYS[1] - key, ARGV[1] - capacity, ARGV[2] - refill_rate (tokens/ms), ARGV[3] - current_timestamp, ARGV[4] - tokens_requested
/// 返回: (allowed: bool, tokens_remaining: int, refill_time: int)
pub const TOKEN_BUCKET_SCRIPT: &str = r#"
-- 获取参数
local key = KEYS[1]
local capacity = tonumber(ARGV[1])
local refill_rate = tonumber(ARGV[2])  -- tokens per millisecond
local current_timestamp = tonumber(ARGV[3])
local tokens_requested = tonumber(ARGV[4])

-- 获取令牌桶状态
local tokens = tonumber(redis.call('HGET', key, 'tokens')) or capacity
local last_refill = tonumber(redis.call('HGET', key, 'last_refill')) or current_timestamp

-- 计算需要补充的令牌数
local elapsed = current_timestamp - last_refill
if elapsed > 0 then
    local tokens_to_add = elapsed * refill_rate
    tokens = math.min(capacity, tokens + tokens_to_add)
end

-- 判断是否有足够的令牌
local allowed = tokens >= tokens_requested
local tokens_remaining = tokens

-- 如果允许，扣除令牌
if allowed then
    tokens = tokens - tokens_requested
    tokens_remaining = tokens
end

-- 更新令牌桶状态
redis.call('HMSET', key, 'tokens', tokens, 'last_refill', current_timestamp)
redis.call('EXPIRE', key, math.ceil(capacity / refill_rate / 1000) + 60)

-- 计算下次补充时间（补充1个令牌所需时间）
local refill_time = current_timestamp + math.ceil(1 / refill_rate)

-- 返回结果
return {allowed and 1 or 0, tokens_remaining, refill_time}
"#;

/// Lua脚本信息
#[derive(Debug, Clone)]
pub struct LuaScriptInfo {
    /// 脚本类型
    pub script_type: LuaScriptType,
    /// 脚本内容
    pub script: &'static str,
    /// SHA哈希（计算后填充）
    pub sha: Arc<parking_lot::Mutex<Option<String>>>,
}

impl LuaScriptInfo {
    /// 创建新的脚本信息
    pub fn new(script_type: LuaScriptType, script: &'static str) -> Self {
        Self {
            script_type,
            script,
            sha: Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    /// 获取脚本SHA，如果未计算则返回None
    pub fn get_sha(&self) -> Option<String> {
        self.sha.lock().clone()
    }

    /// 设置脚本SHA
    pub fn set_sha(&self, sha: String) {
        *self.sha.lock() = Some(sha);
    }
}

/// Lua脚本管理器
pub struct LuaScriptManager {
    /// 脚本映射
    scripts: HashMap<LuaScriptType, LuaScriptInfo>,
}

impl LuaScriptManager {
    /// 创建新的脚本管理器
    pub fn new() -> Self {
        let mut scripts = HashMap::new();

        // 注册所有脚本
        scripts.insert(
            LuaScriptType::SlidingWindow,
            LuaScriptInfo::new(LuaScriptType::SlidingWindow, SLIDING_WINDOW_SCRIPT),
        );
        scripts.insert(
            LuaScriptType::FixedWindow,
            LuaScriptInfo::new(LuaScriptType::FixedWindow, FIXED_WINDOW_SCRIPT),
        );
        scripts.insert(
            LuaScriptType::QuotaConsume,
            LuaScriptInfo::new(LuaScriptType::QuotaConsume, QUOTA_CONSUME_SCRIPT),
        );
        scripts.insert(
            LuaScriptType::QuotaReset,
            LuaScriptInfo::new(LuaScriptType::QuotaReset, QUOTA_RESET_SCRIPT),
        );
        scripts.insert(
            LuaScriptType::TokenBucket,
            LuaScriptInfo::new(LuaScriptType::TokenBucket, TOKEN_BUCKET_SCRIPT),
        );

        Self { scripts }
    }

    /// 获取脚本信息
    pub fn get_script(&self, script_type: LuaScriptType) -> Option<&LuaScriptInfo> {
        self.scripts.get(&script_type)
    }

    /// 获取所有脚本
    pub fn get_all_scripts(&self) -> Vec<&LuaScriptInfo> {
        self.scripts.values().collect()
    }

    /// 预加载所有脚本到Redis
    pub async fn preload_all_scripts<C>(&self, conn: &mut C) -> Result<(), StorageError>
    where
        C: AsyncCommands + redis::aio::ConnectionLike,
    {
        info!("开始预加载Lua脚本到Redis");

        for script_info in self.get_all_scripts() {
            self.preload_script(conn, script_info).await?;
        }

        info!("Lua脚本预加载完成");
        Ok(())
    }

    /// 预加载单个脚本
    pub async fn preload_script<C>(
        &self,
        conn: &mut C,
        script_info: &LuaScriptInfo,
    ) -> Result<(), StorageError>
    where
        C: AsyncCommands + redis::aio::ConnectionLike,
    {
        // 计算SHA
        let script = Script::new(script_info.script);
        let sha = script.get_hash().to_string();

        // 缓存SHA
        script_info.set_sha(sha.clone());

        // 执行SCRIPT LOAD预加载
        let _: String = redis::cmd("SCRIPT")
            .arg("LOAD")
            .arg(script_info.script)
            .query_async(conn)
            .await
            .map_err(|e| {
                error!("预加载脚本失败: {:?}, 错误: {}", script_info.script_type, e);
                StorageError::ConnectionError(format!("预加载脚本失败: {}", e))
            })?;

        debug!(
            "脚本预加载成功: {:?}, SHA: {}",
            script_info.script_type, sha
        );

        Ok(())
    }

    /// 执行脚本（使用SHA）
    pub async fn execute_script<C, T>(
        &self,
        conn: &mut C,
        script_type: LuaScriptType,
        keys: &[&str],
        args: &[&str],
    ) -> Result<T, StorageError>
    where
        C: AsyncCommands + redis::aio::ConnectionLike,
        T: redis::FromRedisValue,
    {
        let script_info = self
            .get_script(script_type)
            .ok_or_else(|| StorageError::QueryError(format!("未找到脚本: {:?}", script_type)))?;

        let sha = script_info
            .get_sha()
            .ok_or_else(|| StorageError::QueryError("脚本SHA未初始化".to_string()))?;

        trace!("执行脚本: {:?}, SHA: {}", script_type, sha);

        // 尝试使用SHA执行
        match redis::cmd("EVALSHA")
            .arg(&sha)
            .arg(keys.len())
            .arg(keys)
            .arg(args)
            .query_async::<_, T>(conn)
            .await
        {
            Ok(result) => Ok(result),
            Err(e) => {
                // 如果SHA不存在，重新加载脚本
                if e.to_string().contains("NOSCRIPT") {
                    debug!("脚本SHA不存在，重新加载: {:?}", script_type);
                    self.preload_script(conn, script_info).await?;

                    // 重试执行
                    redis::cmd("EVALSHA")
                        .arg(&sha)
                        .arg(keys.len())
                        .arg(keys)
                        .arg(args)
                        .query_async::<_, T>(conn)
                        .await
                        .map_err(|e| {
                            error!("脚本执行失败: {:?}, 错误: {}", script_type, e);
                            StorageError::QueryError(format!("脚本执行失败: {}", e))
                        })
                } else {
                    error!("脚本执行失败: {:?}, 错误: {}", script_type, e);
                    Err(StorageError::QueryError(format!("脚本执行失败: {}", e)))
                }
            }
        }
    }

    /// 执行脚本（直接使用脚本内容）
    pub async fn execute_script_direct<C, T>(
        &self,
        conn: &mut C,
        script_type: LuaScriptType,
        keys: &[&str],
        args: &[&str],
    ) -> Result<T, StorageError>
    where
        C: AsyncCommands + redis::aio::ConnectionLike,
        T: redis::FromRedisValue,
    {
        let script_info = self
            .get_script(script_type)
            .ok_or_else(|| StorageError::QueryError(format!("未找到脚本: {:?}", script_type)))?;

        trace!("直接执行脚本: {:?}", script_type);

        redis::cmd("EVAL")
            .arg(script_info.script)
            .arg(keys.len())
            .arg(keys)
            .arg(args)
            .query_async::<_, T>(conn)
            .await
            .map_err(|e| {
                error!("脚本执行失败: {:?}, 错误: {}", script_type, e);
                StorageError::QueryError(format!("脚本执行失败: {}", e))
            })
    }

    /// 刷新所有脚本的SHA缓存
    pub fn clear_sha_cache(&self) {
        for script_info in self.get_all_scripts() {
            *script_info.sha.lock() = None;
        }
        debug!("已清除所有脚本的SHA缓存");
    }
}

impl Default for LuaScriptManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lua_script_type_name() {
        assert_eq!(LuaScriptType::SlidingWindow.name(), "sliding_window");
        assert_eq!(LuaScriptType::FixedWindow.name(), "fixed_window");
        assert_eq!(LuaScriptType::QuotaConsume.name(), "quota_consume");
        assert_eq!(LuaScriptType::QuotaReset.name(), "quota_reset");
        assert_eq!(LuaScriptType::TokenBucket.name(), "token_bucket");
    }

    #[test]
    fn test_lua_script_type_version() {
        assert_eq!(LuaScriptType::SlidingWindow.version(), "1.0");
        assert_eq!(LuaScriptType::FixedWindow.version(), "1.0");
    }

    #[test]
    fn test_lua_script_manager_new() {
        let manager = LuaScriptManager::new();
        assert!(manager.get_script(LuaScriptType::SlidingWindow).is_some());
        assert!(manager.get_script(LuaScriptType::FixedWindow).is_some());
        assert!(manager.get_script(LuaScriptType::QuotaConsume).is_some());
        assert!(manager.get_script(LuaScriptType::QuotaReset).is_some());
        assert!(manager.get_script(LuaScriptType::TokenBucket).is_some());
    }

    #[test]
    fn test_lua_script_info() {
        let script_info = LuaScriptInfo::new(LuaScriptType::SlidingWindow, SLIDING_WINDOW_SCRIPT);
        assert_eq!(script_info.script_type, LuaScriptType::SlidingWindow);
        assert_eq!(script_info.script, SLIDING_WINDOW_SCRIPT);
        assert!(script_info.get_sha().is_none());

        script_info.set_sha("test_sha".to_string());
        assert_eq!(script_info.get_sha(), Some("test_sha".to_string()));
    }

    #[test]
    fn test_clear_sha_cache() {
        let manager = LuaScriptManager::new();

        // 设置一些SHA
        for script_info in manager.get_all_scripts() {
            script_info.set_sha("test_sha".to_string());
        }

        // 清除缓存
        manager.clear_sha_cache();

        // 验证已清除
        for script_info in manager.get_all_scripts() {
            assert!(script_info.get_sha().is_none());
        }
    }

    #[test]
    fn test_script_constants_validity() {
        // 验证脚本常量不为空
        assert!(!SLIDING_WINDOW_SCRIPT.is_empty());
        assert!(!FIXED_WINDOW_SCRIPT.is_empty());
        assert!(!QUOTA_CONSUME_SCRIPT.is_empty());
        assert!(!QUOTA_RESET_SCRIPT.is_empty());
        assert!(!TOKEN_BUCKET_SCRIPT.is_empty());

        // 验证脚本包含必要的Redis命令
        assert!(SLIDING_WINDOW_SCRIPT.contains("ZREMRANGEBYSCORE"));
        assert!(SLIDING_WINDOW_SCRIPT.contains("ZCARD"));
        assert!(SLIDING_WINDOW_SCRIPT.contains("ZADD"));

        assert!(FIXED_WINDOW_SCRIPT.contains("GET"));
        assert!(FIXED_WINDOW_SCRIPT.contains("INCR"));

        assert!(QUOTA_CONSUME_SCRIPT.contains("HGET"));
        assert!(QUOTA_CONSUME_SCRIPT.contains("HINCRBY"));
        assert!(QUOTA_CONSUME_SCRIPT.contains("HMSET"));

        assert!(TOKEN_BUCKET_SCRIPT.contains("HGET"));
        assert!(TOKEN_BUCKET_SCRIPT.contains("HMSET"));
    }
}
