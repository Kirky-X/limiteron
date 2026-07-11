// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Centralized configuration constants for Limiteron.
//!
//! This module provides well-documented constants used throughout the library.
//! All magic numbers are defined here with their purpose and usage context.

#![allow(dead_code)]

/// Maximum cost value for rate limiting operations.
///
/// This limit prevents excessive resource consumption from a single request.
/// Must be greater than 0 and less than or equal to 1,000,000.
///
/// # Usage
///
/// Used in [`validate_cost()`] to ensure cost parameters are within acceptable bounds.
///
/// [`validate_cost()`]: crate::limiters::validate_cost
pub(crate) const MAX_COST: u64 = 1_000_000;

/// Minimum valid cost value for rate limiting operations.
///
/// Cost values must be positive to prevent no-op operations.
pub(crate) const MIN_COST: u64 = 1;

// ============================================================================
// Cache Constants
// ============================================================================

/// Default capacity for L1 cache when not specified.
///
/// This value provides reasonable out-of-box performance for most applications.
/// Represents 10,000 cache entries.
pub(crate) const DEFAULT_L1_CACHE_CAPACITY: usize = 10_000;

/// Default TTL for L1 cache entries (5 minutes).
///
/// After this duration, cache entries are considered stale and may be evicted.
pub(crate) const DEFAULT_L1_CACHE_TTL_SECS: u64 = 300;

/// Default cleanup interval for L1 cache (1 minute).
///
/// How often the cache performs expiration checks and cleanup.
pub(crate) const DEFAULT_L1_CACHE_CLEANUP_INTERVAL_SECS: u64 = 60;

/// Default LRU eviction threshold (90%).
///
/// When cache capacity utilization exceeds this percentage,
/// cache will start evicting least recently used entries.
pub(crate) const DEFAULT_L1_CACHE_LRU_THRESHOLD: f64 = 0.9;

/// Default capacity for L2 cache when not specified.
///
/// This value provides reasonable out-of-box performance for most applications.
/// Represents 10,000 cache entries.
pub(crate) const DEFAULT_L2_CACHE_CAPACITY: usize = 10_000;

/// Default TTL for L2 cache entries (5 minutes).
///
/// After this duration, cache entries are considered stale and may be evicted.
pub(crate) const DEFAULT_L2_CACHE_TTL_SECS: u64 = 300;

/// Default cleanup interval for L2 cache (1 minute).
///
/// How often to cache performs expiration checks and cleanup.
pub(crate) const DEFAULT_L2_CACHE_CLEANUP_INTERVAL_SECS: u64 = 60;

/// Default LRU eviction threshold (90%).
///
/// When cache capacity utilization exceeds this percentage,
/// cache will start evicting least recently used entries.
pub(crate) const DEFAULT_L2_CACHE_LRU_THRESHOLD: f64 = 0.9;

// ============================================================================
// Circuit Breaker Constants
// ============================================================================
// Circuit Breaker Constants
// ============================================================================

/// Default failure threshold for circuit breaker.
///
/// The circuit breaker transitions to open state after this many consecutive failures.
pub(crate) const DEFAULT_CIRCUIT_BREAKER_FAILURE_THRESHOLD: u64 = 5;

/// Default success threshold for circuit breaker half-open state.
///
/// The circuit breaker transitions to closed state after this many successes in half-open state.
pub(crate) const DEFAULT_CIRCUIT_BREAKER_SUCCESS_THRESHOLD: u64 = 3;

/// Default timeout duration for circuit breaker (30 seconds).
///
/// How long the circuit breaker remains open before attempting to half-open.
pub(crate) const DEFAULT_CIRCUIT_BREAKER_TIMEOUT_SECS: u64 = 30;

/// Maximum number of calls in half-open state.
///
/// Limits the number of trial requests when probing if the service has recovered.
pub(crate) const DEFAULT_CIRCUIT_BREAKER_HALF_OPEN_MAX_CALLS: u64 = 3;

/// Default slow call duration threshold for circuit breaker (500 milliseconds).
///
/// Calls exceeding this duration are considered slow calls.
pub(crate) const DEFAULT_CIRCUIT_BREAKER_SLOW_CALL_DURATION_MILLIS: u64 = 500;

/// Default slow call rate threshold for circuit breaker (50%).
///
/// The circuit breaker transitions to open state when the slow call rate exceeds this value.
pub(crate) const DEFAULT_CIRCUIT_BREAKER_SLOW_CALL_RATE_THRESHOLD: f64 = 0.5;

// ============================================================================
// Ban Manager Constants
// ============================================================================

/// Default ban duration for first offense (1 minute).
///
/// Initial ban duration when a user violates rules for the first time.
pub(crate) const DEFAULT_FIRST_OFFENSE_DURATION_SECS: u64 = 60;

/// Default ban duration for second offense (5 minutes).
///
/// Second offense uses exponential backoff to increase penalty.
pub(crate) const DEFAULT_SECOND_OFFENSE_DURATION_SECS: u64 = 300;

/// Default ban duration for third offense (30 minutes).
///
/// Third offense significantly increases the ban duration.
pub(crate) const DEFAULT_THIRD_OFFENSE_DURATION_SECS: u64 = 1800;

/// Default ban duration for fourth offense (2 hours).
///
/// Fourth offense and beyond use this duration.
pub(crate) const DEFAULT_FOURTH_OFFENSE_DURATION_SECS: u64 = 7200;

/// Maximum ban duration (24 hours).
///
/// Caps all ban durations at 24 hours to prevent excessive blocking.
pub(crate) const DEFAULT_MAX_BAN_DURATION_SECS: u64 = 86400;

/// Auto-unban check interval (1 minute).
///
/// How often the ban manager checks for expired bans to release.
pub(crate) const DEFAULT_AUTO_UNBAN_CHECK_INTERVAL_SECS: u64 = 60;

/// Default pagination limit for ban queries.
///
/// Standard page size when listing ban records.
pub(crate) const DEFAULT_BAN_PAGINATION_LIMIT: u32 = 100;

/// Maximum pagination limit for ban queries.
///
/// Prevents excessive memory usage when querying large ban lists.
pub(crate) const MAX_BAN_PAGINATION_LIMIT: u32 = 1000;

/// Maximum ban reason length (500 characters).
///
/// Prevents overly long ban reasons that could cause display issues.
pub(crate) const MAX_BAN_REASON_LENGTH: usize = 500;

/// Maximum user ID length (256 characters).
///
/// Standard length for user identifier validation.
pub(crate) const MAX_USER_ID_LENGTH: usize = 256;

/// Maximum MAC address length (17 characters).
///
/// Standard MAC address format: XX:XX:XX:XX:XX:XX
pub(crate) const MAX_MAC_ADDRESS_LENGTH: usize = 17;

/// Maximum IP address length (45 characters for IPv6 with port).
///
/// Covers both IPv4 and IPv6 address formats.
pub(crate) const MAX_IP_ADDRESS_LENGTH: usize = 45;

// ============================================================================
// Rate Limiter Constants
// ============================================================================

/// Default token bucket capacity (100 tokens).
///
/// Standard out-of-box capacity for token bucket limiters.
pub(crate) const DEFAULT_TOKEN_BUCKET_CAPACITY: u64 = 100;

/// Default token refill rate (10 tokens/second).
///
/// Standard refill rate for token bucket limiters.
pub(crate) const DEFAULT_TOKEN_BUCKET_REFILL_RATE: u64 = 10;

/// Default sliding window size (60 seconds).
///
/// Standard window duration for sliding window limiters.
pub(crate) const DEFAULT_SLIDING_WINDOW_SIZE_SECS: u64 = 60;

/// Default maximum requests per window (1000 requests).
///
/// Standard request limit for sliding window limiters.
pub(crate) const DEFAULT_SLIDING_WINDOW_MAX_REQUESTS: u64 = 1000;

/// Default fixed window size (60 seconds).
///
/// Standard window duration for fixed window limiters.
pub(crate) const DEFAULT_FIXED_WINDOW_SIZE_SECS: u64 = 60;

/// Default maximum requests per fixed window (1000 requests).
///
/// Standard request limit for fixed window limiters.
pub(crate) const DEFAULT_FIXED_WINDOW_MAX_REQUESTS: u64 = 1000;

/// Default concurrency limit (50 concurrent operations).
///
/// Standard concurrency limit for concurrent access control.
pub(crate) const DEFAULT_CONCURRENCY_LIMIT: u64 = 50;

// ============================================================================
// Retry and Backoff Constants
// ============================================================================

/// Maximum retry attempts for transient failures.
///
/// Default number of retry attempts before giving up.
pub(crate) const DEFAULT_MAX_RETRIES: u32 = 3;

/// Initial delay for exponential backoff (10 milliseconds).
///
/// Starting delay before the first retry.
pub(crate) const DEFAULT_INITIAL_BACKOFF_MS: u64 = 10;

/// Maximum backoff delay (30 seconds).
///
/// Caps the exponential backoff to prevent excessive delays.
pub(crate) const DEFAULT_MAX_BACKOFF_MS: u64 = 30000;

/// Maximum spin loop iterations for exponential backoff.
///
/// Used in retry logic to limit spin loop iterations.
pub(crate) const MAX_SPIN_ITERATIONS: u64 = 1000;

// ============================================================================
// Validation Constants
// ============================================================================

/// Maximum API key length (512 characters).
///
/// Standard length for API key validation.
pub(crate) const MAX_API_KEY_LENGTH: usize = 512;

/// Maximum header value length (8192 characters).
///
/// Standard length for HTTP header validation.
pub(crate) const MAX_HEADER_VALUE_LENGTH: usize = 8192;

/// Maximum path length (2048 characters).
///
/// Standard length for URL path validation.
pub(crate) const MAX_PATH_LENGTH: usize = 2048;

// ============================================================================
// Time Conversion Constants
// ============================================================================

/// Seconds per minute.
pub(crate) const SECONDS_PER_MINUTE: u64 = 60;

/// Seconds per hour.
pub(crate) const SECONDS_PER_HOUR: u64 = 3600;

/// Seconds per day.
pub(crate) const SECONDS_PER_DAY: u64 = 86400;

/// Milliseconds per second.
pub(crate) const MS_PER_SECOND: u64 = 1000;

/// Nanoseconds per millisecond.
pub(crate) const NS_PER_MS: u64 = 1_000_000;

/// Nanoseconds per second.
pub(crate) const NS_PER_SECOND: u64 = 1_000_000_000;

// ============================================================================
// Configuration Valid Values
// ============================================================================

/// Valid storage backend types (DBNexus-based)
pub const VALID_STORAGE_TYPES: &[&str] = &["postgresql", "memory"];

/// Valid cache backend types
pub const VALID_CACHE_TYPES: &[&str] = &["memory", "redis"];

/// Valid metrics backend types
pub const VALID_METRICS_TYPES: &[&str] = &["prometheus", "opentelemetry"];
