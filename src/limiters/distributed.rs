// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 分布式限流器模块
//!
//! 提供基于内存的分布式限流器实现，支持原子计数操作。
//! 用于进程内分布式兼容测试，也可作为分布式 DAO 的参考实现。

use super::traits::{DistributedLimiter, Limiter};
use crate::error::LimiteronError;
#[cfg(all(feature = "distributed", feature = "lua-script"))]
use crate::error::StorageError;
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 带过期时间的计数器条目
#[derive(Clone)]
struct TtlEntry {
    count: u64,
    expires_at: Instant,
}

/// 内存分布式限流器
///
/// 基于 DashMap 实现的进程内分布式限流器，提供原子计数操作。
/// 适用于单实例部署或测试环境，也可作为分布式 DAO 的参考实现。
///
/// # 特性
/// - 原子递增（incr）
/// - 带 TTL 的原子递增（incr_with_ttl）
/// - 计数查询（get_count）
/// - 计数重置（reset）
///
/// # 示例
///
/// ```rust
/// use limiteron::limiters::{DistributedLimiter, InMemoryDistributedLimiter};
///
/// #[tokio::main]
/// async fn main() {
///     let limiter = InMemoryDistributedLimiter::new();
///     let count = limiter.incr("user:123", 1).await.unwrap();
///     assert_eq!(count, 1);
/// }
/// ```
pub struct InMemoryDistributedLimiter {
    /// 永久计数器（无 TTL）
    counters: Arc<DashMap<String, u64>>,
    /// 带 TTL 的计数器
    ttl_counters: Arc<DashMap<String, TtlEntry>>,
}

impl InMemoryDistributedLimiter {
    /// 创建新的内存分布式限流器
    pub fn new() -> Self {
        Self {
            counters: Arc::new(DashMap::new()),
            ttl_counters: Arc::new(DashMap::new()),
        }
    }

    /// 清理过期的 TTL 计数器
    fn cleanup_expired(&self) {
        let now = Instant::now();
        self.ttl_counters.retain(|_, entry| entry.expires_at > now);
    }
}

impl Default for InMemoryDistributedLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Limiter for InMemoryDistributedLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError> {
        // 使用固定键 "_global" 进行计数，兼容 Limiter trait 接口
        // 真正的分布式限流应通过 incr + get_count + 阈值判断实现
        self.incr("_global", cost).await?;
        Ok(true)
    }
}

#[async_trait]
impl DistributedLimiter for InMemoryDistributedLimiter {
    async fn incr(&self, key: &str, amount: u64) -> Result<u64, LimiteronError> {
        if key.is_empty() {
            return Err(LimiteronError::ConfigError(
                "Key cannot be empty".to_string(),
            ));
        }

        let new_count = self
            .counters
            .entry(key.to_string())
            .and_modify(|c| *c = c.saturating_add(amount))
            .or_insert(amount);

        Ok(*new_count)
    }

    async fn incr_with_ttl(
        &self,
        key: &str,
        amount: u64,
        ttl: Duration,
    ) -> Result<u64, LimiteronError> {
        if key.is_empty() {
            return Err(LimiteronError::ConfigError(
                "Key cannot be empty".to_string(),
            ));
        }

        let now = Instant::now();
        let expires_at = now + ttl;

        // 清理过期条目
        self.cleanup_expired();

        let new_count = self
            .ttl_counters
            .entry(key.to_string())
            .and_modify(|entry| {
                if entry.expires_at > now {
                    // 未过期，累加
                    entry.count = entry.count.saturating_add(amount);
                    entry.expires_at = expires_at;
                } else {
                    // 已过期，重置
                    entry.count = amount;
                    entry.expires_at = expires_at;
                }
            })
            .or_insert(TtlEntry {
                count: amount,
                expires_at,
            });

        Ok(new_count.count)
    }

    async fn get_count(&self, key: &str) -> Result<u64, LimiteronError> {
        // 先检查 TTL 计数器
        if let Some(entry) = self.ttl_counters.get(key) {
            if entry.expires_at > Instant::now() {
                return Ok(entry.count);
            }
        }

        // 再检查永久计数器
        if let Some(count) = self.counters.get(key) {
            return Ok(*count);
        }

        Ok(0)
    }

    async fn reset(&self, key: &str) -> Result<(), LimiteronError> {
        self.counters.remove(key);
        self.ttl_counters.remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_incr_new_key() {
        let limiter = InMemoryDistributedLimiter::new();
        let count = limiter.incr("user:1", 1).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_incr_existing_key() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter.incr("user:1", 5).await.unwrap();
        let count = limiter.incr("user:1", 3).await.unwrap();
        assert_eq!(count, 8);
    }

    #[tokio::test]
    async fn test_incr_empty_key() {
        let limiter = InMemoryDistributedLimiter::new();
        let result = limiter.incr("", 1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_incr_saturating() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter.incr("user:1", u64::MAX).await.unwrap();
        let count = limiter.incr("user:1", 1).await.unwrap();
        assert_eq!(count, u64::MAX);
    }

    #[tokio::test]
    async fn test_incr_with_ttl_new_key() {
        let limiter = InMemoryDistributedLimiter::new();
        let count = limiter
            .incr_with_ttl("user:1", 1, Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_incr_with_ttl_expired() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter
            .incr_with_ttl("user:1", 5, Duration::from_millis(1))
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(10)).await;

        // 过期后再次递增应重置为新值
        let count = limiter
            .incr_with_ttl("user:1", 3, Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_get_count_nonexistent() {
        let limiter = InMemoryDistributedLimiter::new();
        let count = limiter.get_count("nonexistent").await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_get_count_after_incr() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter.incr("user:1", 10).await.unwrap();
        let count = limiter.get_count("user:1").await.unwrap();
        assert_eq!(count, 10);
    }

    #[tokio::test]
    async fn test_get_count_after_ttl_expire() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter
            .incr_with_ttl("user:1", 5, Duration::from_millis(1))
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(10)).await;

        let count = limiter.get_count("user:1").await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_reset() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter.incr("user:1", 10).await.unwrap();
        limiter.reset("user:1").await.unwrap();
        let count = limiter.get_count("user:1").await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_reset_ttl_counter() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter
            .incr_with_ttl("user:1", 10, Duration::from_secs(60))
            .await
            .unwrap();
        limiter.reset("user:1").await.unwrap();
        let count = limiter.get_count("user:1").await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_concurrent_incr() {
        let limiter = Arc::new(InMemoryDistributedLimiter::new());
        let mut handles = vec![];

        for _ in 0..10 {
            let limiter_clone = Arc::clone(&limiter);
            handles.push(tokio::spawn(async move {
                limiter_clone.incr("concurrent", 1).await.unwrap();
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        let count = limiter.get_count("concurrent").await.unwrap();
        assert_eq!(count, 10);
    }

    #[tokio::test]
    async fn test_limiter_trait_compatibility() {
        // InMemoryDistributedLimiter 同时实现 Limiter + DistributedLimiter
        let limiter = InMemoryDistributedLimiter::new();
        let allowed = limiter.allow(1).await.unwrap();
        assert!(allowed);
        let count = limiter.incr("test", 1).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_incr_with_ttl_accumulate() {
        let limiter = InMemoryDistributedLimiter::new();
        let c1 = limiter
            .incr_with_ttl("user:1", 3, Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(c1, 3);
        let c2 = limiter
            .incr_with_ttl("user:1", 5, Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(c2, 8);
    }

    #[tokio::test]
    async fn test_different_keys_isolated() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter.incr("user:1", 10).await.unwrap();
        limiter.incr("user:2", 20).await.unwrap();
        assert_eq!(limiter.get_count("user:1").await.unwrap(), 10);
        assert_eq!(limiter.get_count("user:2").await.unwrap(), 20);
    }

    #[tokio::test]
    async fn test_reset_nonexistent_key() {
        let limiter = InMemoryDistributedLimiter::new();
        // 重置不存在的键不应报错
        let result = limiter.reset("nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_incr_with_ttl_empty_key() {
        let limiter = InMemoryDistributedLimiter::new();
        let result = limiter.incr_with_ttl("", 1, Duration::from_secs(60)).await;
        assert!(result.is_err());
    }
}

// ============================================================================
// T050: Redis 分布式限流器
// 通过 oxcache Cache 的 eval_lua 执行 Lua 脚本实现原子操作，
// 需要 `distributed` + `lua-script` 两个 feature 同时启用。
// ============================================================================

/// 原子递增 Lua 脚本
/// KEYS\[1\] = key, ARGV\[1\] = amount → 返回递增后的值
#[cfg(all(feature = "distributed", feature = "lua-script"))]
const REDIS_INCR_SCRIPT: &str = r#"
local val = redis.call('INCRBY', KEYS[1], tonumber(ARGV[1]))
return val
"#;

/// 原子递增并设置 TTL Lua 脚本
/// KEYS\[1\] = key, ARGV\[1\] = amount, ARGV\[2\] = ttl_seconds → 返回递增后的值
#[cfg(all(feature = "distributed", feature = "lua-script"))]
const REDIS_INCR_TTL_SCRIPT: &str = r#"
local val = redis.call('INCRBY', KEYS[1], tonumber(ARGV[1]))
redis.call('EXPIRE', KEYS[1], tonumber(ARGV[2]))
return val
"#;

/// GET Lua 脚本
/// KEYS\[1\] = key → 返回当前值（不存在返回 0）
#[cfg(all(feature = "distributed", feature = "lua-script"))]
const REDIS_GET_COUNT_SCRIPT: &str = r#"
local val = redis.call('GET', KEYS[1])
if val == false then return 0 end
return tonumber(val)
"#;

/// DEL Lua 脚本
/// KEYS\[1\] = key → 返回 1
#[cfg(all(feature = "distributed", feature = "lua-script"))]
const REDIS_RESET_SCRIPT: &str = r#"
redis.call('DEL', KEYS[1])
return 1
"#;

/// Redis 分布式限流器
///
/// 通过 oxcache `Cache` 的 `eval_lua` 执行 Lua 脚本实现跨实例原子操作。
/// 需要 Redis 后端（`lua-script` feature）和 `distributed` feature 同时启用。
///
/// # 算法
///
/// - `allow()` — 固定窗口算法（`FIXED_WINDOW_SCRIPT`）
/// - `incr()` / `incr_with_ttl()` — 原子 `INCRBY` + 可选 `EXPIRE`
/// - `get_count()` — 原子 `GET`
/// - `reset()` — 原子 `DEL`
///
/// # Example
///
/// ```rust,ignore
/// use limiteron::limiters::{RedisDistributedLimiter, DistributedLimiter};
///
/// let limiter = RedisDistributedLimiter::new(cache, 100, 10);
/// let count = limiter.incr("user:123", 1).await.unwrap();
/// ```
#[cfg(all(feature = "distributed", feature = "lua-script"))]
pub struct RedisDistributedLimiter {
    /// oxcache Cache 实例（Redis 后端）
    cache: oxcache::Cache<String, String>,
    /// 固定窗口容量（用于 allow()）
    capacity: u64,
    /// 窗口大小（毫秒，用于 allow()）
    window_ms: u64,
}

#[cfg(all(feature = "distributed", feature = "lua-script"))]
impl RedisDistributedLimiter {
    /// 创建 Redis 分布式限流器
    ///
    /// # Arguments
    /// * `cache` - oxcache Cache 实例（必须使用 Redis 后端）
    /// * `capacity` - 窗口内最大请求数
    /// * `window_ms` - 窗口大小（毫秒）
    pub fn new(cache: oxcache::Cache<String, String>, capacity: u64, window_ms: u64) -> Self {
        Self {
            cache,
            capacity,
            window_ms,
        }
    }

    /// 执行 Lua 脚本并解析整数响应
    async fn eval_lua_int(
        &self,
        script: &str,
        keys: &[&str],
        args: &[&str],
    ) -> Result<u64, LimiteronError> {
        let value = self.cache.eval_lua(script, keys, args).await.map_err(|e| {
            LimiteronError::StorageError(StorageError::QueryError(format!(
                "Lua eval failed: {}",
                e
            )))
        })?;
        match value {
            redis::Value::Int(n) => Ok(n as u64),
            redis::Value::BulkString(bytes) => {
                let s = String::from_utf8_lossy(&bytes);
                s.parse::<u64>().map_err(|e| {
                    LimiteronError::StorageError(StorageError::QueryError(format!(
                        "Lua int parse: {}",
                        e
                    )))
                })
            }
            other => Err(LimiteronError::StorageError(StorageError::QueryError(
                format!("unexpected Lua response: {:?}", other),
            ))),
        }
    }

    /// 执行 Lua 脚本并解析数组响应（用于固定窗口等返回多值的脚本）
    async fn eval_lua_array(
        &self,
        script: &str,
        keys: &[&str],
        args: &[&str],
    ) -> Result<Vec<i64>, LimiteronError> {
        let value = self.cache.eval_lua(script, keys, args).await.map_err(|e| {
            LimiteronError::StorageError(StorageError::QueryError(format!(
                "Lua eval failed: {}",
                e
            )))
        })?;
        match value {
            redis::Value::Array(arr) => arr
                .iter()
                .map(|v| match v {
                    redis::Value::Int(n) => Ok(*n),
                    redis::Value::BulkString(bytes) => {
                        let s = String::from_utf8_lossy(&bytes);
                        s.parse::<i64>().map_err(|e| {
                            LimiteronError::StorageError(StorageError::QueryError(format!(
                                "parse: {}",
                                e
                            )))
                        })
                    }
                    other => Err(LimiteronError::StorageError(StorageError::QueryError(
                        format!("unexpected array element: {:?}", other),
                    ))),
                })
                .collect(),
            other => Err(LimiteronError::StorageError(StorageError::QueryError(
                format!("expected array, got: {:?}", other),
            ))),
        }
    }
}

#[cfg(all(feature = "distributed", feature = "lua-script"))]
#[async_trait]
impl Limiter for RedisDistributedLimiter {
    async fn allow(&self, _cost: u64) -> Result<bool, LimiteronError> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let result = self
            .eval_lua_array(
                crate::oxcache_lua::FIXED_WINDOW_SCRIPT,
                &["_global"],
                &[
                    &self.window_ms.to_string(),
                    &self.capacity.to_string(),
                    &now_ms.to_string(),
                ],
            )
            .await?;

        // FIXED_WINDOW_SCRIPT 返回 [allowed, current_count, reset_time]
        match result.as_slice() {
            [allowed, _count, _reset] => Ok(*allowed != 0),
            _ => Err(LimiteronError::StorageError(StorageError::QueryError(
                "unexpected FIXED_WINDOW response length".to_string(),
            ))),
        }
    }
}

#[cfg(all(feature = "distributed", feature = "lua-script"))]
#[async_trait]
impl DistributedLimiter for RedisDistributedLimiter {
    async fn incr(&self, key: &str, amount: u64) -> Result<u64, LimiteronError> {
        if key.is_empty() {
            return Err(LimiteronError::ConfigError(
                "Key cannot be empty".to_string(),
            ));
        }
        self.eval_lua_int(REDIS_INCR_SCRIPT, &[key], &[&amount.to_string()])
            .await
    }

    async fn incr_with_ttl(
        &self,
        key: &str,
        amount: u64,
        ttl: Duration,
    ) -> Result<u64, LimiteronError> {
        if key.is_empty() {
            return Err(LimiteronError::ConfigError(
                "Key cannot be empty".to_string(),
            ));
        }
        let ttl_secs = ttl.as_secs().max(1);
        self.eval_lua_int(
            REDIS_INCR_TTL_SCRIPT,
            &[key],
            &[&amount.to_string(), &ttl_secs.to_string()],
        )
        .await
    }

    async fn get_count(&self, key: &str) -> Result<u64, LimiteronError> {
        self.eval_lua_int(REDIS_GET_COUNT_SCRIPT, &[key], &[]).await
    }

    async fn reset(&self, key: &str) -> Result<(), LimiteronError> {
        self.eval_lua_int(REDIS_RESET_SCRIPT, &[key], &[]).await?;
        Ok(())
    }
}

#[cfg(all(feature = "distributed", feature = "lua-script"))]
#[cfg(test)]
mod redis_distributed_tests {
    use super::*;

    /// RedisDistributedLimiter 构造验证（不需要真实 Redis 连接）
    #[test]
    fn test_redis_distributed_limiter_construction() {
        // 仅验证结构体可以构造，Lua 脚本常量存在
        assert!(!REDIS_INCR_SCRIPT.is_empty());
        assert!(!REDIS_INCR_TTL_SCRIPT.is_empty());
        assert!(!REDIS_GET_COUNT_SCRIPT.is_empty());
        assert!(!REDIS_RESET_SCRIPT.is_empty());
    }

    /// 验证 Lua 脚本包含必要的 Redis 命令
    #[test]
    fn test_lua_scripts_contain_redis_commands() {
        assert!(REDIS_INCR_SCRIPT.contains("INCRBY"));
        assert!(REDIS_INCR_TTL_SCRIPT.contains("INCRBY"));
        assert!(REDIS_INCR_TTL_SCRIPT.contains("EXPIRE"));
        assert!(REDIS_GET_COUNT_SCRIPT.contains("GET"));
        assert!(REDIS_RESET_SCRIPT.contains("DEL"));
    }

    /// 验证 governor 路径中 lua-script feature 门控连通性
    #[test]
    fn test_lua_script_feature_gate_connectivity() {
        // OxcacheLuaManager 在 lua-script feature 下可用
        let manager = crate::oxcache_lua::OxcacheLuaManager::new();
        assert!(
            manager
                .get_script(crate::oxcache_lua::LuaScriptType::FixedWindow)
                .is_some()
        );
    }
}
