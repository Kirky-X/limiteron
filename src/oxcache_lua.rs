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
        }
    }
}

/// Sliding window Lua script
///
/// Uses Redis Sorted Set for sliding window algorithm
/// Parameters: KEYS\[1\] - key, ARGV\[1\] - window_size (ms), ARGV\[2\] - max_requests, ARGV\[3\] - current_timestamp
/// Returns: (allowed: bool, current_count: int, reset_time: int)
pub const SLIDING_WINDOW_SCRIPT: &str = r#"
-- get parameters
local key = KEYS[1]
local window_size = tonumber(ARGV[1])
local max_requests = tonumber(ARGV[2])
local current_timestamp = tonumber(ARGV[3])
local window_start = current_timestamp - window_size

-- remove elements outside the window
redis.call('ZREMRANGEBYSCORE', key, '-inf', window_start)

-- get request count within the current window
local current_count = redis.call('ZCARD', key)

-- decide whether to allow
local allowed = current_count < max_requests

-- if allowed, add the current request
if allowed then
    redis.call('ZADD', key, current_timestamp, current_timestamp)
    -- set expiry (window size + 1 second)
    redis.call('EXPIRE', key, math.ceil(window_size / 1000) + 1)
end

-- compute reset time (window start + window size)
local reset_time = window_start + window_size

-- return result
return {allowed and 1 or 0, current_count, reset_time}
"#;

/// Fixed window Lua script
///
/// Uses Redis String + TTL for fixed window algorithm
/// Parameters: KEYS\[1\] - key, ARGV\[1\] - window_size (ms), ARGV\[2\] - max_requests, ARGV\[3\] - current_timestamp
/// Returns: (allowed: bool, current_count: int, reset_time: int)
pub const FIXED_WINDOW_SCRIPT: &str = r#"
-- get parameters
local key = KEYS[1]
local window_size = tonumber(ARGV[1])
local max_requests = tonumber(ARGV[2])
local current_timestamp = tonumber(ARGV[3])

-- compute the current window
local current_window = math.floor(current_timestamp / window_size) * window_size
local window_key = key .. ':' .. current_window

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
local current_timestamp = tonumber(ARGV[3])
local tokens_requested = tonumber(ARGV[4])

-- get token bucket state
local tokens = tonumber(redis.call('HGET', key, 'tokens')) or capacity
local last_refill = tonumber(redis.call('HGET', key, 'last_refill')) or current_timestamp

-- compute the number of tokens to refill
local elapsed = current_timestamp - last_refill
if elapsed > 0 then
    local tokens_to_add = elapsed * refill_rate
    tokens = math.min(capacity, tokens + tokens_to_add)
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
redis.call('HMSET', key, 'tokens', tokens, 'last_refill', current_timestamp)
redis.call('EXPIRE', key, math.ceil(capacity / refill_rate / 1000) + 60)

-- compute next refill time (time to refill 1 token)
local refill_time = current_timestamp + math.ceil(1 / refill_rate)

-- return result
return {allowed and 1 or 0, tokens_remaining, refill_time}
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
    }

    #[test]
    fn test_oxcache_lua_manager_default() {
        let manager = OxcacheLuaManager::default();
        assert_eq!(manager.get_all_scripts().len(), 5);
    }

    #[test]
    fn test_oxcache_lua_manager_get_all_scripts() {
        let manager = OxcacheLuaManager::new();
        let scripts = manager.get_all_scripts();
        assert_eq!(scripts.len(), 5);
        // 验证所有脚本类型都存在
        let script_types: Vec<LuaScriptType> = scripts.iter().map(|s| s.script_type).collect();
        assert!(script_types.contains(&LuaScriptType::SlidingWindow));
        assert!(script_types.contains(&LuaScriptType::FixedWindow));
        assert!(script_types.contains(&LuaScriptType::QuotaConsume));
        assert!(script_types.contains(&LuaScriptType::QuotaReset));
        assert!(script_types.contains(&LuaScriptType::TokenBucket));
    }
}
