// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Lua script integration using oxcache's lua-script feature.
//!
//! This module provides Lua script execution through oxcache, which includes
//! comprehensive security validation, SHA caching, and connection pooling.

use crate::error::StorageError;
use ahash::AHashMap as HashMap;

/// Lua script type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LuaScriptType {
    /// Sliding window rate limiting
    SlidingWindow,
    /// Fixed window rate limiting
    FixedWindow,
    /// Quota consumption
    QuotaConsume,
    /// Quota reset
    QuotaReset,
    /// Token bucket algorithm
    TokenBucket,
    /// Sliding window log (cost-weighted exact sliding window)
    SlidingWindowLog,
    /// Token bucket with burst allowance (debt-based overdraft)
    BurstTokenBucket,
}

impl LuaScriptType {
    /// Get script name
    pub fn name(&self) -> &str {
        match self {
            LuaScriptType::SlidingWindow => "sliding_window",
            LuaScriptType::FixedWindow => "fixed_window",
            LuaScriptType::QuotaConsume => "quota_consume",
            LuaScriptType::QuotaReset => "quota_reset",
            LuaScriptType::TokenBucket => "token_bucket",
            LuaScriptType::SlidingWindowLog => "sliding_window_log",
            LuaScriptType::BurstTokenBucket => "burst_token_bucket",
        }
    }

    /// Get script version
    pub fn version(&self) -> &str {
        match self {
            LuaScriptType::SlidingWindow => "1.0",
            LuaScriptType::FixedWindow => "1.0",
            LuaScriptType::QuotaConsume => "1.0",
            LuaScriptType::QuotaReset => "1.0",
            LuaScriptType::TokenBucket => "1.0",
            LuaScriptType::SlidingWindowLog => "1.0",
            LuaScriptType::BurstTokenBucket => "1.0",
        }
    }
}

/// Sliding window Lua script
///
/// Uses Redis Sorted Set for sliding window algorithm
/// Parameters: KEYS\[1\] - key, KEYS\[2\] - member 序号计数器（同毫秒去重）
/// ARGV\[1\] - window_size (ms), ARGV\[2\] - max_requests
/// （历史教训：曾用客户端传入时间戳——多实例时钟漂移会放大窗口；
/// member 曾直接用时间戳——同毫秒请求互相覆盖导致 ZCARD 少计、限流被绕过）
/// Returns: (allowed: bool, current_count: int, reset_time: int)
pub const SLIDING_WINDOW_SCRIPT: &str = r#"
-- get parameters
local key = KEYS[1]
local seq_key = KEYS[2]
local window_size = tonumber(ARGV[1])
local max_requests = tonumber(ARGV[2])

-- 时钟源：Redis 单点时间（TIME），多实例口径一致
local t = redis.call('TIME')
local current_timestamp = tonumber(t[1]) * 1000 + math.floor(tonumber(t[2]) / 1000)
local window_start = current_timestamp - window_size

-- remove elements outside the window
redis.call('ZREMRANGEBYSCORE', key, '-inf', window_start)

-- get request count within the current window
local current_count = redis.call('ZCARD', key)

-- decide whether to allow
local allowed = current_count < max_requests

-- if allowed, add the current request
if allowed then
    -- member 唯一：时间戳 + 单调序号（同毫秒请求不再互相覆盖）
    local seq = redis.call('INCR', seq_key)
    redis.call('EXPIRE', seq_key, math.ceil(window_size / 1000) + 1)
    redis.call('ZADD', key, current_timestamp, current_timestamp .. ':' .. seq)
    -- set expiry (window size + 1 second)
    redis.call('EXPIRE', key, math.ceil(window_size / 1000) + 1)
end

-- compute reset time（最早成员 + 窗口长度；空集回退当前时间）
local reset_time = current_timestamp
local oldest = redis.call('ZRANGE', key, 0, 0)
if #oldest > 0 then
    local oldest_score = redis.call('ZSCORE', key, oldest[1])
    reset_time = tonumber(oldest_score) + window_size
end

-- return result
return {allowed and 1 or 0, current_count, reset_time}
"#;

/// Fixed window Lua script
///
/// Uses Redis String + TTL for fixed window algorithm
/// Parameters: KEYS\[1\] - key, KEYS\[2\] - window_key（调用方按
/// `floor(now/window)*window` 派生并显式声明）
/// ARGV\[1\] - window_size (ms), ARGV\[2\] - max_requests
/// （历史教训：曾用客户端时间戳在脚本内派生未声明的 window_key——
/// EVAL key 声明契约破坏，Redis Cluster 下路由错误分片）
/// Returns: (allowed: bool, current_count: int, reset_time: int)
pub const FIXED_WINDOW_SCRIPT: &str = r#"
-- get parameters
local key = KEYS[1]
local window_key = KEYS[2]
local window_size = tonumber(ARGV[1])
local max_requests = tonumber(ARGV[2])

-- 时钟源：Redis 单点时间（TIME），多实例口径一致
local t = redis.call('TIME')
local current_timestamp = tonumber(t[1]) * 1000 + math.floor(tonumber(t[2]) / 1000)

-- get the current count
local current_count = tonumber(redis.call('GET', window_key)) or 0

-- decide whether to allow
local allowed = current_count < max_requests

-- if allowed, increment the count
if allowed then
    redis.call('INCR', window_key)
    -- set expiry (window size + 1 second)
    redis.call('EXPIRE', window_key, math.ceil(window_size / 1000) + 1)
end

-- compute reset time (start of the next window)
local reset_time = current_window + window_size

-- return result
return {allowed and 1 or 0, current_count, reset_time}
"#;

/// Quota consumption Lua script
///
/// Uses Redis Hash for quota storage with overdraft support
/// Parameters: KEYS\[1\] - key, ARGV\[1\] - cost, ARGV\[2\] - limit, ARGV\[3\] - overdraft_limit, ARGV\[4\] - window_start, ARGV\[5\] - window_end, ARGV\[6\] - consumed_field, ARGV\[7\] - limit_field, ARGV\[8\] - window_start_field, ARGV\[9\] - window_end_field
/// Returns: (allowed: bool, remaining: int, consumed: int)
pub const QUOTA_CONSUME_SCRIPT: &str = r#"
-- get parameters
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

-- check whether the window has expired
local stored_window_start = tonumber(redis.call('HGET', key, window_start_field))
if stored_window_start and stored_window_start ~= window_start then
    -- window expired, reset quota
    redis.call('HMSET', key, consumed_field, 0, window_start_field, window_start, window_end_field, window_end, limit_field, limit)
    redis.call('EXPIRE', key, math.ceil((window_end - window_start) / 1000) + 10)
elseif not stored_window_start then
    -- first consumption, initialize quota info
    redis.call('HMSET', key, consumed_field, 0, window_start_field, window_start, window_end_field, window_end, limit_field, limit)
    redis.call('EXPIRE', key, math.ceil((window_end - window_start) / 1000) + 10)
else
    -- window not expired, update limit info (keep metadata consistent)
    redis.call('HSET', key, limit_field, limit)
end

-- get the current consumed amount
local consumed = tonumber(redis.call('HGET', key, consumed_field)) or 0

-- compute remaining quota (including overdraft)
local total_limit = limit + overdraft_limit
local remaining = total_limit - consumed

-- decide whether consumption is allowed
local allowed = remaining >= cost

-- if allowed, deduct the quota
if allowed then
    redis.call('HINCRBY', key, consumed_field, cost)
    consumed = consumed + cost
    remaining = total_limit - consumed
end

-- return result
return {allowed and 1 or 0, remaining, consumed}
"#;

/// Quota reset Lua script
///
/// Resets quota counter
/// Parameters: KEYS\[1\] - key, ARGV\[1\] - window_start, ARGV\[2\] - window_end, ARGV\[3\] - consumed_field, ARGV\[4\] - window_start_field, ARGV\[5\] - window_end_field
/// Returns: success (1) or fail (0)
pub const QUOTA_RESET_SCRIPT: &str = r#"
-- get parameters
local key = KEYS[1]
local window_start = tonumber(ARGV[1])
local window_end = tonumber(ARGV[2])
local consumed_field = ARGV[3]
local window_start_field = ARGV[4]
local window_end_field = ARGV[5]

-- reset quota
redis.call('HMSET', key, consumed_field, 0, window_start_field, window_start, window_end_field, window_end)
redis.call('EXPIRE', key, math.ceil((window_end - window_start) / 1000) + 10)

-- return success
return 1
"#;

/// Token bucket Lua script
///
/// Uses Redis Hash for token bucket algorithm
/// Parameters: KEYS\[1\] - key, ARGV\[1\] - capacity, ARGV\[2\] - refill_rate (tokens/ms), ARGV\[3\] - current_timestamp, ARGV\[4\] - tokens_requested
/// Returns: (allowed: bool, tokens_remaining: int, refill_time: int)
pub const TOKEN_BUCKET_SCRIPT: &str = r#"
-- get parameters
local key = KEYS[1]
local capacity = tonumber(ARGV[1])
local refill_rate = tonumber(ARGV[2])  -- tokens per millisecond
local tokens_requested = tonumber(ARGV[3])

-- 除零守卫：refill_rate=0 时 capacity/refill_rate 与 1/refill_rate 产生 inf
--（EXPIRE 收到 inf 报错、脚本失败），钳最小正值
local safe_rate = math.max(refill_rate, 1e-9)

-- 时钟源：Redis 单点时间（TIME），多实例口径一致
local t = redis.call('TIME')
local current_timestamp = tonumber(t[1]) * 1000 + math.floor(tonumber(t[2]) / 1000)

-- get token bucket state
local tokens = tonumber(redis.call('HGET', key, 'tokens')) or capacity
local last_refill = tonumber(redis.call('HGET', key, 'last_refill')) or current_timestamp

-- compute the number of tokens to refill
-- last_refill 仅在真实推进时更新：时钟回拨（elapsed <= 0）保留原基点，
-- 回正后一次性补足，避免 (回拨点, 回正点) 区间补充被二次发放
local elapsed = current_timestamp - last_refill
if elapsed > 0 then
    local tokens_to_add = elapsed * refill_rate
    tokens = math.min(capacity, tokens + tokens_to_add)
    last_refill = current_timestamp
end

-- decide whether there are enough tokens
local allowed = tokens >= tokens_requested
local tokens_remaining = tokens

-- if allowed, deduct the tokens
if allowed then
    tokens = tokens - tokens_requested
    tokens_remaining = tokens
end

-- update token bucket state
redis.call('HMSET', key, 'tokens', tokens, 'last_refill', last_refill)
redis.call('EXPIRE', key, math.ceil(capacity / safe_rate / 1000) + 60)

-- compute next refill time (time to refill 1 token)
local refill_time = current_timestamp + math.ceil(1 / safe_rate)

-- return result
return {allowed and 1 or 0, tokens_remaining, refill_time}
"#;

/// Sliding window log Lua script (cost-weighted exact sliding window)
///
/// 每请求成本可变的精确滑动窗口（对应进程内 `SlidingWindowLogLimiter`
/// 语义）：条目成本编码进 member，窗口内总量由伴随账本增量维护；与
/// `SLIDING_WINDOW_SCRIPT`（每请求计 1）的区别是支持 `cost > 1` 的
/// 差异化请求。
///
/// # 复杂度
///
/// 每请求 O(新增过期数)，摊还 O(1)——窗口总量由 `KEYS[3]` 账本增量
/// 维护，逐出时仅对将过期前缀求和（ZRANGEBYSCORE 取前缀 + HINCRBY
/// 负调整），不做全窗扫描。
///
/// # 约束
///
/// - `max_requests` 上限 100_000（与进程内 `MAX_SLIDING_LOG_REQUESTS`
///   同源），`cost` 不得超过 `max_requests`；超限返回错误哨兵
/// - `KEYS[3]` 账本必须与 `KEYS[1]` zset 一一对应（同一脚本的 KEYS
///   声明保证），跨脚本混用账本会破坏总量一致性
/// - 语义验证依赖 Redis 环境的 e2e/testcontainers，本仓内测试为结构
///   锚点（沙箱无 Redis，见工作区 AGENTS.md 环境限制）
/// - 当前为脚本库资产：crate 内暂无调用方，下游可经 `OxcacheLuaManager`
///   配合 `execute_lua_script`/`execute_cached_script` 自行接线；接线时
///   Rust 侧应对 `max_requests` 重复进程内上限校验（纵深防御）。
///
/// Parameters: KEYS\[1\] - zset key, KEYS\[2\] - member 序号计数器,
/// KEYS\[3\] - 窗口总量账本（hash）
/// ARGV\[1\] - window_size (ms), ARGV\[2\] - max_requests（窗口内总量上限）, ARGV\[3\] - cost
/// Returns: (allowed: bool, window_used: int, reset_time: int)
pub const SLIDING_WINDOW_LOG_SCRIPT: &str = r#"
-- get parameters
local key = KEYS[1]
local seq_key = KEYS[2]
local total_key = KEYS[3]
local window_size = tonumber(ARGV[1])
local max_requests = tonumber(ARGV[2])
local cost = tonumber(ARGV[3])

-- 脚本侧防御性守卫（与 BURST_TOKEN_BUCKET_SCRIPT 的全参数显性守卫对齐）
if window_size == nil or window_size <= 0 then
  return redis.error_reply('sliding window log: window_size must be positive')
end
if max_requests == nil or max_requests > 100000 then
  return redis.error_reply('sliding window log: max_requests missing or exceeds ceiling 100000')
end
if cost == nil or cost <= 0 or cost > max_requests then
  return redis.error_reply('sliding window log: cost must be positive and <= max_requests')
end
-- cost 须为正整数：非整数会在 HINCRBY 处报 Redis 内部错误而非本脚本的
-- 领域错误信息，入口统一校验给出可读语义
if math.floor(cost) ~= cost then
  return redis.error_reply('sliding window log: cost must be a positive integer')
end

-- 时钟源：Redis 单点时间（TIME），多实例口径一致
local t = redis.call('TIME')
local current_timestamp = tonumber(t[1]) * 1000 + math.floor(tonumber(t[2]) / 1000)
local window_start = current_timestamp - window_size

-- 取出将逐出前缀并求和（仅扫新增过期部分，摊还 O(1)；成本编码在
-- member 第三段 "ts:seq:cost"，编码损坏显性失败而非静默兜底）
local expiring = redis.call('ZRANGEBYSCORE', key, '-inf', window_start)
local expired_cost = 0
for _, member in ipairs(expiring) do
    local cost_part = string.match(member, '[^:]+$')
    local c = tonumber(cost_part)
    if c == nil then
        return redis.error_reply('sliding window log: corrupted member encoding: ' .. member)
    end
    expired_cost = expired_cost + c
end
if #expiring > 0 then
    redis.call('ZREMRANGEBYSCORE', key, '-inf', window_start)
    redis.call('HINCRBY', total_key, 'total', -expired_cost)
end

-- 窗口总量从账本读取（O(1)）。账本与 zset 须同刻同值维护，漂移双向
-- 自愈：账本缺失且 zset 非空（外部删除/驱逐）属状态损坏，显性失败；
-- 两侧任一方先过期/清空（EXPIRE 采样竞态）则空窗即重置归零
local window_used = tonumber(redis.call('HGET', total_key, 'total'))
if window_used == nil then
    if redis.call('ZCARD', key) > 0 then
        return redis.error_reply('sliding window log: total ledger missing while zset alive')
    end
    redis.call('HSET', total_key, 'total', 0)
    window_used = 0
elseif window_used ~= 0 and redis.call('ZCARD', key) == 0 then
    -- 反向漂移对称自愈：zset 被先过期/清空而账本残留非零（幽灵成本，
    -- 判定持续偏拒绝）——空窗即重置归零
    redis.call('HSET', total_key, 'total', 0)
    window_used = 0
elseif window_used ~= 0 and redis.call('ZCARD', key) == 0 then
    -- 反向漂移对称自愈：zset 被先过期/清空而账本残留非零（幽灵成本，
    -- 判定持续偏拒绝）——空窗即重置归零
    redis.call('HSET', total_key, 'total', 0)
    window_used = 0
end

-- decide whether to allow（请求成本整体入窗判定）
local allowed = window_used + cost <= max_requests

-- if allowed, add the current request（member 唯一：ts + 单调序号 + cost）
if allowed then
    local seq = redis.call('INCR', seq_key)
    redis.call('EXPIRE', seq_key, math.ceil(window_size / 1000) + 1)
    redis.call('ZADD', key, current_timestamp, current_timestamp .. ':' .. seq .. ':' .. cost)
    redis.call('HINCRBY', total_key, 'total', cost)
    redis.call('EXPIRE', key, math.ceil(window_size / 1000) + 1)
    redis.call('EXPIRE', total_key, math.ceil(window_size / 1000) + 1)
    window_used = window_used + cost
end

-- compute reset time（最早成员 + 窗口长度；空集回退当前时间）
local reset_time = current_timestamp
local oldest = redis.call('ZRANGE', key, 0, 0)
if #oldest > 0 then
    local oldest_score = redis.call('ZSCORE', key, oldest[1])
    reset_time = tonumber(oldest_score) + window_size
end

-- return result
return {allowed and 1 or 0, window_used, reset_time}
"#;
/// Token bucket with burst allowance Lua script (debt-based overdraft)
///
/// 透支（burst）型令牌桶：余额不足时允许借债放行（透支上限
/// `burst_allowance`，脚本侧约束 `0 <= burst_allowance <= capacity`），
/// 债务记为负令牌，未来补充先偿还债务——与
/// `TOKEN_BUCKET_SCRIPT`（严格 `tokens >= requested` 才放行）的区别是
/// 允许短时突发超过存量，适合容忍瞬时超速但限制透支总量的场景。
/// TTL 按 `(capacity + burst_allowance) / refill_rate` 推算并钳制 24h
/// 上限（rate 极小时推算值可达天文量级）；`last_refill` 仅在真实推进时
/// 更新（时钟回拨期保留原基点，防补充二次发放）。语义验证依赖 Redis
/// 环境 e2e，本仓内测试为结构锚点。
/// Parameters: KEYS\[1\] - key
/// ARGV\[1\] - capacity, ARGV\[2\] - refill_rate (tokens/ms), ARGV\[3\] - tokens_requested, ARGV\[4\] - burst_allowance
/// Returns: (allowed: bool, balance: int, debt: int)
pub const BURST_TOKEN_BUCKET_SCRIPT: &str = r#"
-- get parameters
local key = KEYS[1]
local capacity = tonumber(ARGV[1])
local refill_rate = tonumber(ARGV[2])  -- tokens per millisecond
local tokens_requested = tonumber(ARGV[3])
local burst_allowance = tonumber(ARGV[4])

-- 脚本侧防御性校验：透支额度不得超过容量（防透支面失控）、参数缺失显性失败
if capacity == nil or refill_rate == nil or tokens_requested == nil or burst_allowance == nil then
  return redis.error_reply('burst token bucket: missing parameters')
end
if burst_allowance < 0 or burst_allowance > capacity then
  return redis.error_reply('burst token bucket: burst_allowance must be in [0, capacity]')
end

-- 除零守卫：refill_rate=0 时 capacity/refill_rate 与 1/refill_rate 产生 inf
--（EXPIRE 收到 inf 报错、脚本失败），钳最小正值
local safe_rate = math.max(refill_rate, 1e-9)

-- 时钟源：Redis 单点时间（TIME），多实例口径一致
local t = redis.call('TIME')
local current_timestamp = tonumber(t[1]) * 1000 + math.floor(tonumber(t[2]) / 1000)

-- get bucket state（余额可为负：负值即未偿还的债务）
local balance = tonumber(redis.call('HGET', key, 'balance'))
local last_refill = tonumber(redis.call('HGET', key, 'last_refill'))
if balance == nil then
    balance = capacity
    last_refill = current_timestamp
end
if last_refill == nil then
    last_refill = current_timestamp
end

-- refill：补充先偿还债务（余额向 capacity 方向恢复，负余额自然先归零）。
-- last_refill 仅在真实推进时更新：时钟回拨（elapsed <= 0）保留原基点，
-- 回正后一次性补足，避免 (回拨点, 回正点) 区间补充被二次发放
local elapsed = current_timestamp - last_refill
if elapsed > 0 then
    balance = math.min(capacity, balance + elapsed * refill_rate)
    last_refill = current_timestamp
end

-- 判定：余额足够直接放行；不足时在透支额度内借债放行
local allowed = false
if balance >= tokens_requested then
    allowed = true
    balance = balance - tokens_requested
else
    local shortfall = tokens_requested - balance
    if shortfall <= burst_allowance then
        allowed = true
        balance = balance - tokens_requested
    end
end

-- update bucket state（TTL 钳制 24h 上限：rate 极小时推算值可达天文量级，
-- 防 key 在共享 Redis 上长期驻留）
redis.call('HMSET', key, 'balance', balance, 'last_refill', last_refill)
local ttl = math.min(math.ceil((capacity + burst_allowance) / safe_rate / 1000) + 60, 86400)
redis.call('EXPIRE', key, ttl)

-- debt：未偿还债务（余额为负时取绝对值，非负为 0）
local debt = 0
if balance < 0 then
    debt = -balance
end

-- return result
return {allowed and 1 or 0, balance, debt}
"#;

/// Lua script information
#[derive(Debug, Clone)]
pub struct LuaScriptInfo {
    /// Script type
    pub script_type: LuaScriptType,
    /// Script content
    pub script: &'static str,
}

impl LuaScriptInfo {
    /// Create new script info
    pub fn new(script_type: LuaScriptType, script: &'static str) -> Self {
        Self {
            script_type,
            script,
        }
    }
}

/// Lua script manager using oxcache for execution
#[derive(Clone)]
pub struct OxcacheLuaManager {
    /// Script mapping
    scripts: HashMap<LuaScriptType, LuaScriptInfo>,
}

impl OxcacheLuaManager {
    /// Create new script manager
    pub fn new() -> Self {
        let mut scripts = HashMap::new();

        // Register all scripts
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
        scripts.insert(
            LuaScriptType::SlidingWindowLog,
            LuaScriptInfo::new(LuaScriptType::SlidingWindowLog, SLIDING_WINDOW_LOG_SCRIPT),
        );
        scripts.insert(
            LuaScriptType::BurstTokenBucket,
            LuaScriptInfo::new(LuaScriptType::BurstTokenBucket, BURST_TOKEN_BUCKET_SCRIPT),
        );

        Self { scripts }
    }

    /// Get script info
    pub fn get_script(&self, script_type: LuaScriptType) -> Option<&LuaScriptInfo> {
        self.scripts.get(&script_type)
    }

    /// Get all scripts
    pub fn get_all_scripts(&self) -> Vec<&LuaScriptInfo> {
        self.scripts.values().collect()
    }

    /// Get script content by type
    pub fn get_script_content(&self, script_type: LuaScriptType) -> Option<&'static str> {
        self.scripts.get(&script_type).map(|info| info.script)
    }
}

impl Default for OxcacheLuaManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert oxcache result to limiteron StorageError
fn convert_lua_error(e: impl std::fmt::Display) -> StorageError {
    StorageError::QueryError(format!("Lua script execution failed: {}", e))
}

/// Execute Lua script using oxcache Cache
///
/// This function provides a compatibility layer that allows the existing
/// Lua script constants to be executed through oxcache's eval_lua API.
///
/// # Arguments
///
/// * `cache` - oxcache Cache instance (must be Redis-backed)
/// * `script` - Lua script content
/// * `keys` - Redis keys for the script
/// * `args` - Arguments for the script
///
/// # Returns
///
/// Result containing the script execution result as a string
#[cfg(feature = "lua-script")]
pub async fn execute_lua_script(
    cache: &oxcache::Cache<String, String>,
    script: &str,
    keys: &[&str],
    args: &[&str],
) -> Result<String, StorageError> {
    cache
        .eval_lua(script, keys, args)
        .await
        .map_err(convert_lua_error)
        .map(|v| format!("{:?}", v))
}

/// Load script and get SHA using oxcache Cache
///
/// # Arguments
///
/// * `cache` - oxcache Cache instance (must be Redis-backed)
/// * `script` - Lua script content
///
/// # Returns
///
/// Result containing the SHA hash of the script
#[cfg(feature = "lua-script")]
pub async fn load_script(
    cache: &oxcache::Cache<String, String>,
    script: &str,
) -> Result<String, StorageError> {
    cache.script_load(script).await.map_err(convert_lua_error)
}

/// Execute cached script using SHA via oxcache Cache
///
/// # Arguments
///
/// * `cache` - oxcache Cache instance (must be Redis-backed)
/// * `sha` - SHA hash of the pre-loaded script
/// * `keys` - Redis keys for the script
/// * `args` - Arguments for the script
///
/// # Returns
///
/// Result containing the script execution result as a string
#[cfg(feature = "lua-script")]
pub async fn execute_cached_script(
    cache: &oxcache::Cache<String, String>,
    sha: &str,
    keys: &[&str],
    args: &[&str],
) -> Result<String, StorageError> {
    cache
        .eval_sha(sha, keys, args)
        .await
        .map_err(convert_lua_error)
        .map(|v| format!("{:?}", v))
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
        assert_eq!(LuaScriptType::SlidingWindowLog.name(), "sliding_window_log");
        assert_eq!(LuaScriptType::BurstTokenBucket.name(), "burst_token_bucket");
    }

    #[test]
    fn test_lua_script_type_version() {
        assert_eq!(LuaScriptType::SlidingWindow.version(), "1.0");
        assert_eq!(LuaScriptType::FixedWindow.version(), "1.0");
    }

    #[test]
    fn test_oxcache_lua_manager_new() {
        let manager = OxcacheLuaManager::new();
        assert!(manager.get_script(LuaScriptType::SlidingWindow).is_some());
        assert!(manager.get_script(LuaScriptType::FixedWindow).is_some());
        assert!(manager.get_script(LuaScriptType::QuotaConsume).is_some());
        assert!(manager.get_script(LuaScriptType::QuotaReset).is_some());
        assert!(manager.get_script(LuaScriptType::TokenBucket).is_some());
        assert!(
            manager
                .get_script(LuaScriptType::SlidingWindowLog)
                .is_some()
        );
        assert!(
            manager
                .get_script(LuaScriptType::BurstTokenBucket)
                .is_some()
        );
    }

    #[test]
    fn test_lua_script_info() {
        let script_info = LuaScriptInfo::new(LuaScriptType::SlidingWindow, SLIDING_WINDOW_SCRIPT);
        assert_eq!(script_info.script_type, LuaScriptType::SlidingWindow);
        assert_eq!(script_info.script, SLIDING_WINDOW_SCRIPT);
    }

    #[test]
    fn test_get_script_content() {
        let manager = OxcacheLuaManager::new();
        assert!(
            manager
                .get_script_content(LuaScriptType::SlidingWindow)
                .is_some()
        );
        assert!(
            manager
                .get_script_content(LuaScriptType::FixedWindow)
                .is_some()
        );
        assert!(
            manager
                .get_script_content(LuaScriptType::TokenBucket)
                .is_some()
        );
    }

    #[test]
    #[allow(clippy::const_is_empty)]
    fn test_script_constants_validity() {
        // Validate script constants are not empty
        assert!(!SLIDING_WINDOW_SCRIPT.is_empty());
        assert!(!FIXED_WINDOW_SCRIPT.is_empty());
        assert!(!QUOTA_CONSUME_SCRIPT.is_empty());
        assert!(!QUOTA_RESET_SCRIPT.is_empty());
        assert!(!TOKEN_BUCKET_SCRIPT.is_empty());
        assert!(!SLIDING_WINDOW_LOG_SCRIPT.is_empty());
        assert!(!BURST_TOKEN_BUCKET_SCRIPT.is_empty());

        // Validate scripts contain necessary Redis commands
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

        assert!(SLIDING_WINDOW_LOG_SCRIPT.contains("ZREMRANGEBYSCORE"));
        assert!(SLIDING_WINDOW_LOG_SCRIPT.contains("ZADD"));
        assert!(SLIDING_WINDOW_LOG_SCRIPT.contains("INCR"));

        assert!(BURST_TOKEN_BUCKET_SCRIPT.contains("HGET"));
        assert!(BURST_TOKEN_BUCKET_SCRIPT.contains("HMSET"));
    }

    #[test]
    fn test_convert_lua_error_message_format() {
        let err = convert_lua_error("connection refused");
        match err {
            StorageError::QueryError(msg) => {
                assert!(msg.contains("Lua script execution failed"));
                assert!(msg.contains("connection refused"));
            }
            other => panic!("expected QueryError, got {:?}", other),
        }
    }

    #[test]
    fn test_convert_lua_error_empty_message() {
        let err = convert_lua_error("");
        match err {
            StorageError::QueryError(msg) => {
                assert!(msg.contains("Lua script execution failed"));
            }
            other => panic!("expected QueryError, got {:?}", other),
        }
    }

    #[test]
    fn test_lua_script_type_version_all_variants() {
        assert_eq!(LuaScriptType::SlidingWindow.version(), "1.0");
        assert_eq!(LuaScriptType::FixedWindow.version(), "1.0");
        assert_eq!(LuaScriptType::QuotaConsume.version(), "1.0");
        assert_eq!(LuaScriptType::QuotaReset.version(), "1.0");
        assert_eq!(LuaScriptType::TokenBucket.version(), "1.0");
        assert_eq!(LuaScriptType::SlidingWindowLog.version(), "1.0");
        assert_eq!(LuaScriptType::BurstTokenBucket.version(), "1.0");
    }

    #[test]
    fn test_oxcache_lua_manager_default() {
        let manager = OxcacheLuaManager::default();
        assert_eq!(manager.get_all_scripts().len(), 7);
    }

    #[test]
    fn test_oxcache_lua_manager_get_all_scripts() {
        let manager = OxcacheLuaManager::new();
        let scripts = manager.get_all_scripts();
        assert_eq!(scripts.len(), 7);
        // 验证所有脚本类型都存在
        let script_types: Vec<LuaScriptType> = scripts.iter().map(|s| s.script_type).collect();
        assert!(script_types.contains(&LuaScriptType::SlidingWindow));
        assert!(script_types.contains(&LuaScriptType::FixedWindow));
        assert!(script_types.contains(&LuaScriptType::QuotaConsume));
        assert!(script_types.contains(&LuaScriptType::QuotaReset));
        assert!(script_types.contains(&LuaScriptType::TokenBucket));
        assert!(script_types.contains(&LuaScriptType::SlidingWindowLog));
        assert!(script_types.contains(&LuaScriptType::BurstTokenBucket));
    }

    // ==================== 新脚本语义结构断言（mock 路径：不依赖真 Redis） ====================

    #[test]
    fn test_sliding_window_log_script_semantics() {
        // 增量记账语义（摊还 O(1)）：伴随账本 KEYS[3] 维护窗口总量，
        // 仅对将逐出前缀求和（ZRANGEBYSCORE 取前缀 + HINCRBY 负调整），
        // 不做全窗扫描（对比：进程内同语义实现的单调确认游标）
        assert!(SLIDING_WINDOW_LOG_SCRIPT.contains("ZRANGEBYSCORE"));
        assert!(SLIDING_WINDOW_LOG_SCRIPT.contains("HINCRBY', total_key, 'total', -expired_cost"));
        assert!(SLIDING_WINDOW_LOG_SCRIPT.contains("HINCRBY', total_key, 'total', cost"));
        assert!(SLIDING_WINDOW_LOG_SCRIPT.contains("HGET', total_key, 'total'"));
        // cost 加权语义：member 编码 "ts:seq:cost"，编码损坏显性失败
        assert!(SLIDING_WINDOW_LOG_SCRIPT.contains(" .. ':' .. seq .. ':' .. cost"));
        assert!(SLIDING_WINDOW_LOG_SCRIPT.contains("corrupted member encoding"));
        // 脚本侧上限与进程内 MAX_SLIDING_LOG_REQUESTS 同源
        assert!(SLIDING_WINDOW_LOG_SCRIPT.contains("exceeds ceiling 100000"));
        // 全参数显性守卫（window_size / cost 整数性）与透支桶对齐
        assert!(SLIDING_WINDOW_LOG_SCRIPT.contains("window_size must be positive"));
        assert!(SLIDING_WINDOW_LOG_SCRIPT.contains("cost must be a positive integer"));
        // 状态损坏显性化：zset 非空而账本缺失（外部删除/驱逐）显性失败，
        // 空窗自愈归零（覆盖两 key EXPIRE 先后到期竞态漂移）
        assert!(SLIDING_WINDOW_LOG_SCRIPT.contains("total ledger missing while zset alive"));
        assert!(SLIDING_WINDOW_LOG_SCRIPT.contains("HSET', total_key, 'total', 0"));
        // 反向漂移（zset 先到期账本残留非零）对称自愈：空窗即归零
        assert!(
            SLIDING_WINDOW_LOG_SCRIPT
                .contains("window_used ~= 0 and redis.call('ZCARD', key) == 0")
        );
        // 时钟源对齐（Redis TIME）
        assert!(SLIDING_WINDOW_LOG_SCRIPT.contains("redis.call('TIME')"));
    }

    #[test]
    fn test_burst_token_bucket_script_semantics() {
        // 透支语义：shortfall ≤ burst_allowance 借债放行，债务 = max(0, -balance)
        assert!(BURST_TOKEN_BUCKET_SCRIPT.contains("shortfall <= burst_allowance"));
        assert!(BURST_TOKEN_BUCKET_SCRIPT.contains("debt = -balance"));
        // 脚本侧防御：透支额度 ∈ [0, capacity]，参数缺失显性失败
        assert!(BURST_TOKEN_BUCKET_SCRIPT.contains("burst_allowance must be in [0, capacity]"));
        assert!(BURST_TOKEN_BUCKET_SCRIPT.contains("missing parameters"));
        // TTL 钳制 24h（rate 极小时推算值可达天文量级，防 key 长期驻留）
        assert!(BURST_TOKEN_BUCKET_SCRIPT.contains("86400"));
        // 回拨修复：last_refill 仅在真实推进时更新（防补充二次发放）
        assert!(BURST_TOKEN_BUCKET_SCRIPT.contains("last_refill = current_timestamp"));
        assert!(BURST_TOKEN_BUCKET_SCRIPT.contains("elapsed > 0"));
        // 除零守卫与既有令牌桶对齐
        assert!(BURST_TOKEN_BUCKET_SCRIPT.contains("safe_rate"));
    }

    #[test]
    fn test_token_bucket_script_refill_baseline_consistency() {
        // 透支/普通两令牌桶的回拨基点语义一致：last_refill 仅在 elapsed > 0
        // 时推进（两脚本同 commit 修复，避免语义分叉）
        let guard = "last_refill = current_timestamp";
        assert!(TOKEN_BUCKET_SCRIPT.contains(guard));
        assert!(BURST_TOKEN_BUCKET_SCRIPT.contains(guard));
        assert!(TOKEN_BUCKET_SCRIPT.contains("'last_refill', last_refill"));
        assert!(BURST_TOKEN_BUCKET_SCRIPT.contains("'last_refill', last_refill"));
    }
}
