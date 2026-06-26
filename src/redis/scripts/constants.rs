//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! Lua script constants for Redis-based rate limiting.
//!
//! This module contains the Lua script constants used for distributed
//! rate limiting algorithms, migrated from oxcache_lua.rs.

/// GCRA (Generic Cell Rate Algorithm) Lua script
///
/// Uses Redis Hash for storing TAT (Theoretical Arrival Time) and remaining tokens.
/// Parameters: KEYS[1] - key, ARGV[1] - capacity, ARGV[2] - refill_interval (ms),
///             ARGV[3] - cost, ARGV[4] - current_timestamp (ms)
///
/// Returns: (allowed: 1/0, remaining: int, retry_after_ms: int)
pub const GCRA_SCRIPT: &str = r#"
-- 获取参数
local key = KEYS[1]
local capacity = tonumber(ARGV[1])
local refill_interval = tonumber(ARGV[2])  -- ms between each token refill
local cost = tonumber(ARGV[3])
local now = tonumber(ARGV[4])  -- current timestamp in ms

-- 获取当前状态
local tat = tonumber(redis.call('HGET', key, 'tat')) or now
local remaining = tonumber(redis.call('HGET', key, 'remaining')) or capacity

-- 计算最早可处理时间 (EAT)
local eat = tat - (capacity - 1) * refill_interval

-- 检查是否可以处理
if now >= eat then
    -- 可以处理，更新 TAT
    local new_tat = math.max(tat, now) + cost * refill_interval

    -- 计算剩余令牌
    local elapsed = now - eat
    local refilled = math.floor(elapsed / refill_interval)
    local new_remaining = math.min(capacity, refilled + 1 - cost)

    -- 更新状态
    redis.call('HMSET', key, 'tat', new_tat, 'remaining', new_remaining)
    redis.call('EXPIRE', key, math.ceil(capacity * refill_interval / 1000) + 60)

    -- 返回允许
    return {1, math.max(0, new_remaining), 0}
else
    -- 不可处理，计算需要等待的时间
    local retry_after = eat - now

    -- 获取当前剩余令牌（不更新）
    local current_remaining = math.max(0, remaining)

    return {0, current_remaining, math.ceil(retry_after)}
end
"#;

/// Sliding window Lua script (migrated from oxcache_lua.rs)
///
/// Uses Redis Sorted Set for sliding window algorithm
/// Parameters: KEYS[1] - key, ARGV[1] - window_size (ms), ARGV[2] - max_requests,
///             ARGV[3] - current_timestamp
/// Returns: (allowed: bool, current_count: int, reset_time: int)
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

/// Fixed window Lua script (migrated from oxcache_lua.rs)
///
/// Uses Redis String + TTL for fixed window algorithm
/// Parameters: KEYS[1] - key, ARGV[1] - window_size (ms), ARGV[2] - max_requests,
///             ARGV[3] - current_timestamp
/// Returns: (allowed: bool, current_count: int, reset_time: int)
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

/// Token bucket Lua script (migrated from oxcache_lua.rs)
///
/// Uses Redis Hash for token bucket algorithm
/// Parameters: KEYS[1] - key, ARGV[1] - capacity, ARGV[2] - refill_rate (tokens/ms),
///             ARGV[3] - current_timestamp, ARGV[4] - tokens_requested
/// Returns: (allowed: bool, tokens_remaining: int, refill_time: int)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gcra_script_not_empty() {
        assert!(!GCRA_SCRIPT.is_empty());
        assert!(GCRA_SCRIPT.contains("HGET"));
        assert!(GCRA_SCRIPT.contains("HMSET"));
        assert!(GCRA_SCRIPT.contains("EXPIRE"));
    }

    #[test]
    fn test_sliding_window_script_not_empty() {
        assert!(!SLIDING_WINDOW_SCRIPT.is_empty());
        assert!(SLIDING_WINDOW_SCRIPT.contains("ZREMRANGEBYSCORE"));
        assert!(SLIDING_WINDOW_SCRIPT.contains("ZCARD"));
        assert!(SLIDING_WINDOW_SCRIPT.contains("ZADD"));
    }

    #[test]
    fn test_fixed_window_script_not_empty() {
        assert!(!FIXED_WINDOW_SCRIPT.is_empty());
        assert!(FIXED_WINDOW_SCRIPT.contains("GET"));
        assert!(FIXED_WINDOW_SCRIPT.contains("INCR"));
    }

    #[test]
    fn test_token_bucket_script_not_empty() {
        assert!(!TOKEN_BUCKET_SCRIPT.is_empty());
        assert!(TOKEN_BUCKET_SCRIPT.contains("HGET"));
        assert!(TOKEN_BUCKET_SCRIPT.contains("HMSET"));
    }
}
