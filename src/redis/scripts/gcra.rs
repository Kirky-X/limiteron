//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! GCRA (Generic Cell Rate Algorithm) Lua script execution.
//!
//! This module provides GCRA algorithm implementation using Redis Lua scripts
//! for distributed rate limiting with consistent behavior across instances.

use super::constants::GCRA_SCRIPT;
use crate::error::StorageError;

/// GCRA script execution result
#[derive(Debug, Clone)]
pub struct GcraResult {
    /// Whether the request is allowed
    pub allowed: bool,
    /// Remaining capacity
    pub remaining: i64,
    /// Milliseconds to wait before retry (0 if allowed)
    pub retry_after_ms: i64,
}

/// Execute GCRA rate limiting script
///
/// # Arguments
/// * `conn` - Redis connection (must support EVAL)
/// * `key` - Rate limit key
/// * `capacity` - Maximum burst capacity
/// * `refill_interval_ms` - Milliseconds between each token refill
/// * `cost` - Cost of this request
/// * `now_ms` - Current timestamp in milliseconds
///
/// # Returns
/// * `Ok(GcraResult)` - Rate limiting result
/// * `Err(StorageError)` - Execution error
///
/// # Example
///
/// ```rust,ignore
/// use limiteron::redis::scripts::gcra::execute_gcra;
///
/// let result = execute_gcra(
///     &redis_conn,
///     "ratelimit:user:123",
///     100,    // capacity
///     1000,   // 1 token per second
///     1,      // cost
///     now_ms,
/// ).await?;
/// ```
pub async fn execute_gcra(
    conn: &mut redis::aio::Connection,
    key: &str,
    capacity: u64,
    refill_interval_ms: u64,
    cost: u64,
    now_ms: u64,
) -> Result<GcraResult, StorageError> {
    let result: Vec<i64> = redis::cmd("EVAL")
        .arg(GCRA_SCRIPT)
        .arg(1) // number of keys
        .arg(key)
        .arg(capacity)
        .arg(refill_interval_ms)
        .arg(cost)
        .arg(now_ms)
        .query_async(conn)
        .await
        .map_err(|e| StorageError::QueryError(format!("GCRA script execution failed: {}", e)))?;

    if result.len() != 3 {
        return Err(StorageError::QueryError(
            "GCRA script returned unexpected number of values".to_string(),
        ));
    }

    Ok(GcraResult {
        allowed: result[0] == 1,
        remaining: result[1],
        retry_after_ms: result[2],
    })
}

/// Execute GCRA script using EVALSHA for better performance
///
/// This function uses the pre-loaded script SHA to avoid sending
/// the full script text on each invocation.
///
/// # Arguments
/// * `conn` - Redis connection
/// * `sha` - Pre-loaded script SHA
/// * `key` - Rate limit key
/// * `capacity` - Maximum burst capacity
/// * `refill_interval_ms` - Milliseconds between each token refill
/// * `cost` - Cost of this request
/// * `now_ms` - Current timestamp in milliseconds
///
/// # Returns
/// * `Ok(GcraResult)` - Rate limiting result
/// * `Err(StorageError)` - Execution error
pub async fn execute_gcra_with_sha(
    conn: &mut redis::aio::Connection,
    sha: &str,
    key: &str,
    capacity: u64,
    refill_interval_ms: u64,
    cost: u64,
    now_ms: u64,
) -> Result<GcraResult, StorageError> {
    let result: Vec<i64> = redis::cmd("EVALSHA")
        .arg(sha)
        .arg(1) // number of keys
        .arg(key)
        .arg(capacity)
        .arg(refill_interval_ms)
        .arg(cost)
        .arg(now_ms)
        .query_async(conn)
        .await
        .map_err(|e| StorageError::QueryError(format!("GCRA EVALSHA execution failed: {}", e)))?;

    if result.len() != 3 {
        return Err(StorageError::QueryError(
            "GCRA script returned unexpected number of values".to_string(),
        ));
    }

    Ok(GcraResult {
        allowed: result[0] == 1,
        remaining: result[1],
        retry_after_ms: result[2],
    })
}

/// Load GCRA script into Redis and return its SHA
///
/// This should be called once during initialization to cache the script.
///
/// # Arguments
/// * `conn` - Redis connection
///
/// # Returns
/// * `Ok(String)` - Script SHA
/// * `Err(StorageError)` - Load error
pub async fn load_gcra_script(conn: &mut redis::aio::Connection) -> Result<String, StorageError> {
    let sha: String = redis::cmd("SCRIPT")
        .arg("LOAD")
        .arg(GCRA_SCRIPT)
        .query_async(conn)
        .await
        .map_err(|e| StorageError::QueryError(format!("GCRA script load failed: {}", e)))?;

    Ok(sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gcra_result_debug() {
        let result = GcraResult {
            allowed: true,
            remaining: 50,
            retry_after_ms: 0,
        };
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("allowed"));
        assert!(debug_str.contains("remaining"));
        assert!(debug_str.contains("retry_after_ms"));
    }

    #[test]
    fn test_gcra_result_clone() {
        let result = GcraResult {
            allowed: false,
            remaining: 0,
            retry_after_ms: 1000,
        };
        let cloned = result.clone();
        assert_eq!(result.allowed, cloned.allowed);
        assert_eq!(result.remaining, cloned.remaining);
        assert_eq!(result.retry_after_ms, cloned.retry_after_ms);
    }
}
