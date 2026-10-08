// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Centralized configuration constants for Limiteron.
//!
//! This module provides well-documented constants used throughout the library.
//! All magic numbers are defined here with their purpose and usage context.

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

// ============================================================================
// Rate Limiter Maximums
// ============================================================================

/// Maximum token bucket capacity (10M tokens).
///
/// Prevents excessive memory consumption (≈10MB per limiter instance at
/// 1 byte/token assumption); shared by config validation, the factory and
/// direct construction.
pub(crate) const MAX_TOKEN_BUCKET_CAPACITY: u64 = 10_000_000;

/// Maximum token refill rate per second (1M/s).
///
/// Prevents CPU over-consumption from high-frequency refill operations;
/// also reused as the leak-rate ceiling for leaky bucket limiters.
pub(crate) const MAX_TOKEN_BUCKET_REFILL_RATE: u64 = 1_000_000;

/// Maximum window request budget (10M).
///
/// Prevents oversized window bookkeeping; counter-based window limiters
/// (sharded sliding window / fixed window) scale O(1) with this value.
pub(crate) const MAX_WINDOW_REQUESTS: u64 = 10_000_000;

/// Maximum sliding window log entries (100K).
///
/// Each logged entry costs 16 bytes (`(u64, u64)`), so the ceiling bounds a
/// single instance at ≈1.6MB — six orders of magnitude tighter than the
/// counter-based window budget. Exact counting trades memory for precision;
/// larger budgets should use the sharded sliding window.
pub(crate) const MAX_SLIDING_LOG_REQUESTS: u64 = 100_000;

/// Maximum concurrency limit (100K).
///
/// Prevents oversized concurrency control structures.
pub(crate) const MAX_CONCURRENT_REQUESTS: u64 = 100_000;

// ============================================================================
// Circuit Breaker Constants
// ============================================================================

/// Default failure threshold for circuit breaker.
///
/// The circuit breaker transitions to open state after this many consecutive failures.
#[cfg(feature = "circuit-breaker")]
pub(crate) const DEFAULT_CIRCUIT_BREAKER_FAILURE_THRESHOLD: u64 = 5;

/// Default success threshold for circuit breaker half-open state.
///
/// The circuit breaker transitions to closed state after this many successes in half-open state.
#[cfg(feature = "circuit-breaker")]
pub(crate) const DEFAULT_CIRCUIT_BREAKER_SUCCESS_THRESHOLD: u64 = 3;

/// Default timeout duration for circuit breaker (30 seconds).
///
/// How long the circuit breaker remains open before attempting to half-open.
#[cfg(feature = "circuit-breaker")]
pub(crate) const DEFAULT_CIRCUIT_BREAKER_TIMEOUT_SECS: u64 = 30;

/// Maximum number of calls in half-open state.
///
/// Limits the number of trial requests when probing if the service has recovered.
#[cfg(feature = "circuit-breaker")]
pub(crate) const DEFAULT_CIRCUIT_BREAKER_HALF_OPEN_MAX_CALLS: u64 = 3;

/// Default slow call duration threshold for circuit breaker (500 milliseconds).
///
/// Calls exceeding this duration are considered slow calls.
#[cfg(feature = "circuit-breaker")]
pub(crate) const DEFAULT_CIRCUIT_BREAKER_SLOW_CALL_DURATION_MILLIS: u64 = 500;

/// Default slow call rate threshold for circuit breaker (50%).
///
/// The circuit breaker transitions to open state when the slow call rate exceeds this value.
#[cfg(feature = "circuit-breaker")]
pub(crate) const DEFAULT_CIRCUIT_BREAKER_SLOW_CALL_RATE_THRESHOLD: f64 = 0.5;

/// Default failure ratio threshold for manual circuit breaker (50%).
///
/// Ratio-triggered: the breaker opens when failures / total requests reaches this ratio.
#[cfg(feature = "circuit-breaker")]
pub(crate) const DEFAULT_MANUAL_CIRCUIT_BREAKER_FAILURE_THRESHOLD: f64 = 0.5;

/// Default minimum requests before evaluating the failure ratio (manual circuit breaker).
#[cfg(feature = "circuit-breaker")]
pub(crate) const DEFAULT_MANUAL_CIRCUIT_BREAKER_MIN_REQUESTS: u64 = 10;

/// Default cooldown for manual circuit breaker (30 seconds).
///
/// How long the breaker remains open before lazily transitioning to half-open.
#[cfg(feature = "circuit-breaker")]
pub(crate) const DEFAULT_MANUAL_CIRCUIT_BREAKER_HALF_OPEN_INTERVAL_SECS: u64 = 30;

/// Default consecutive successes required to close the manual circuit breaker from half-open.
#[cfg(feature = "circuit-breaker")]
pub(crate) const DEFAULT_MANUAL_CIRCUIT_BREAKER_HALF_OPEN_MAX_SUCCESSES: u64 = 3;

// ============================================================================
// Validation Constants
// ============================================================================

/// Maximum API key length (512 characters).
///
/// Standard length for API key validation.
#[cfg(feature = "validation")]
pub(crate) const MAX_API_KEY_LENGTH: usize = 512;

/// Maximum header value length (8192 characters).
///
/// Standard length for HTTP header validation.
#[cfg(feature = "validation")]
pub(crate) const MAX_HEADER_VALUE_LENGTH: usize = 8192;

/// Maximum path length (2048 characters).
///
/// Standard length for URL path validation.
#[cfg(feature = "validation")]
pub(crate) const MAX_PATH_LENGTH: usize = 2048;

/// Maximum ban reason length (500 characters).
///
/// Prevents overly long ban reasons that could cause display issues.
/// 不挂 validation 门：create_ban 的原因校验自包含、feature 关闭时仍执行
///（见 ban/types.rs create_ban 内注释），门控会在非 validation 组合下断编译。
pub(crate) const MAX_BAN_REASON_LENGTH: usize = 500;

/// Maximum user ID length (256 characters).
///
/// Standard length for user identifier validation.
#[cfg(feature = "validation")]
pub(crate) const MAX_USER_ID_LENGTH: usize = 256;

/// Maximum MAC address length (17 characters).
///
/// Standard MAC address format: XX:XX:XX:XX:XX:XX
#[cfg(feature = "validation")]
pub(crate) const MAX_MAC_ADDRESS_LENGTH: usize = 17;

/// Maximum IP address length (45 characters for IPv6 with port).
///
/// Covers both IPv4 and IPv6 address formats.
#[cfg(feature = "validation")]
pub(crate) const MAX_IP_ADDRESS_LENGTH: usize = 45;
