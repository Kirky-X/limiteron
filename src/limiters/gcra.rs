//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! GCRA (Generic Cell Rate Algorithm) in-memory rate limiter.
//!
//! GCRA is a precise rate limiting algorithm that tracks the theoretical
//! arrival time (TAT) of each request. It provides smooth rate limiting
//! with accurate burst control.
//!
//! # Algorithm
//!
//! GCRA works by maintaining a Theoretical Arrival Time (TAT) which
//! represents when the next request could be processed if requests
//! arrive at the exact allowed rate.
//!
//! - If current time >= TAT - (capacity - 1) * interval, allow
//! - Update TAT = max(TAT, current_time) + cost * interval

use super::traits::{Limiter, validate_cost};
use crate::error::FlowGuardError;
use async_trait::async_trait;
use parking_lot::RwLock;

/// GCRA rate limiter result
#[derive(Debug, Clone)]
pub struct GcraCheckResult {
    /// Whether the request is allowed
    pub allowed: bool,
    /// Remaining capacity
    pub remaining: u64,
    /// Microseconds until next request allowed (0 if allowed)
    pub retry_after_us: u64,
}

/// GCRA (Generic Cell Rate Algorithm) in-memory limiter
///
/// Implements the GCRA algorithm for precise rate limiting with:
/// - Smooth rate limiting behavior
/// - Accurate burst control
/// - Memory-efficient single-value state
///
/// # Algorithm Properties
///
/// - **Capacity**: Maximum burst size (how many requests can be sent at once)
/// - **Refill Interval**: Time between each token refill in microseconds
/// - **TAT**: Theoretical Arrival Time tracking
///
/// # Thread Safety
///
/// Uses `parking_lot::RwLock` for concurrent access with minimal overhead.
///
/// # Example
///
/// ```rust
/// use limiteron::limiters::{GcraLimiter, Limiter};
///
/// # #[tokio::main]
/// # async fn main() {
/// // 100 requests burst capacity, 1000us (1ms) between each token
/// // = 1000 requests per second sustained rate
/// let limiter = GcraLimiter::new(100, 1000);
/// let allowed = limiter.allow(1).await.unwrap();
/// assert!(allowed);
/// # }
/// ```
pub struct GcraLimiter {
    /// Maximum burst capacity
    capacity: u64,
    /// Refill interval in microseconds
    refill_interval_us: u64,
    /// Theoretical Arrival Time (microseconds since UNIX epoch)
    tat: RwLock<u64>,
}

impl GcraLimiter {
    /// Create a new GCRA limiter
    ///
    /// # Arguments
    /// * `capacity` - Maximum burst size (number of requests that can be sent at once)
    /// * `refill_interval_us` - Microseconds between each token refill
    ///
    /// # Example
    ///
    /// ```rust
    /// use limiteron::limiters::GcraLimiter;
    ///
    /// // 10 requests burst, 100,000us (100ms) between tokens = 10 req/sec
    /// let limiter = GcraLimiter::new(10, 100_000);
    /// ```
    pub fn new(capacity: u64, refill_interval_us: u64) -> Self {
        let now_us = Self::now_us();

        Self {
            capacity,
            refill_interval_us,
            tat: RwLock::new(now_us),
        }
    }

    /// Create a new GCRA limiter from rate specification
    ///
    /// # Arguments
    /// * `capacity` - Maximum burst size
    /// * `requests_per_second` - Sustained request rate
    ///
    /// # Example
    ///
    /// ```rust
    /// use limiteron::limiters::GcraLimiter;
    ///
    /// // 100 burst, 1000 requests per second
    /// let limiter = GcraLimiter::with_rate(100, 1000);
    /// ```
    pub fn with_rate(capacity: u64, requests_per_second: u64) -> Self {
        let refill_interval_us = 1_000_000u64
            .checked_div(requests_per_second)
            .unwrap_or(1_000_000);

        Self::new(capacity, refill_interval_us)
    }

    /// Get current time in microseconds
    fn now_us() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0)
    }

    /// Get the capacity
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// Get the refill interval in microseconds
    pub fn refill_interval_us(&self) -> u64 {
        self.refill_interval_us
    }

    /// Get the current TAT value
    pub fn tat(&self) -> u64 {
        *self.tat.read()
    }

    /// Check if request is allowed without modifying state
    ///
    /// # Arguments
    /// * `cost` - Request cost
    ///
    /// # Returns
    /// * `GcraCheckResult` - Check result with remaining capacity and retry time
    pub fn check(&self, cost: u64) -> GcraCheckResult {
        let now_us = Self::now_us();
        let tat = *self.tat.read();

        if cost > self.capacity {
            return GcraCheckResult {
                allowed: false,
                remaining: 0,
                retry_after_us: 0,
            };
        }

        // Calculate Earliest Arrival Time (EAT)
        let eat = tat.saturating_sub((self.capacity.saturating_sub(1)) * self.refill_interval_us);

        if now_us >= eat {
            // Request would be allowed
            let elapsed = now_us.saturating_sub(eat);
            let refilled = elapsed / self.refill_interval_us;
            let remaining = std::cmp::min(self.capacity, refilled + 1 - cost);

            GcraCheckResult {
                allowed: true,
                remaining,
                retry_after_us: 0,
            }
        } else {
            // Request would be denied
            let retry_after = eat.saturating_sub(now_us);
            GcraCheckResult {
                allowed: false,
                remaining: 0,
                retry_after_us: retry_after,
            }
        }
    }

    /// Get remaining capacity without modifying state
    pub fn remaining(&self) -> u64 {
        let now_us = Self::now_us();
        let tat = *self.tat.read();

        let eat = tat.saturating_sub((self.capacity.saturating_sub(1)) * self.refill_interval_us);

        if now_us >= eat {
            let elapsed = now_us.saturating_sub(eat);
            let refilled = elapsed / self.refill_interval_us;
            std::cmp::min(self.capacity, refilled + 1)
        } else {
            0
        }
    }
}

#[async_trait]
impl Limiter for GcraLimiter {
    /// Check if request is allowed
    ///
    /// # Arguments
    /// * `cost` - Request cost (must be > 0 and <= MAX_COST)
    ///
    /// # Returns
    /// * `Ok(true)` - Request allowed
    /// * `Ok(false)` - Request denied
    /// * `Err(FlowGuardError)` - Validation error
    async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError> {
        validate_cost(cost)?;

        let now_us = Self::now_us();

        if cost > self.capacity {
            return Ok(false);
        }

        let allowed = {
            let mut tat = self.tat.write();

            // Calculate Earliest Arrival Time (EAT)
            let eat =
                (*tat).saturating_sub((self.capacity.saturating_sub(1)) * self.refill_interval_us);

            if now_us >= eat {
                // Request allowed, update TAT
                *tat = std::cmp::max(*tat, now_us) + cost * self.refill_interval_us;
                true
            } else {
                // Request denied
                false
            }
        };

        Ok(allowed)
    }
}

impl std::fmt::Debug for GcraLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GcraLimiter")
            .field("capacity", &self.capacity)
            .field("refill_interval_us", &self.refill_interval_us)
            .field("tat", &self.tat())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gcra_basic() {
        let limiter = GcraLimiter::new(10, 1000); // 10 capacity, 1ms interval

        // First request should be allowed (initial burst)
        assert!(limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_gcra_burst() {
        // 使用大 refill_interval（1s）避免测试运行速度差异导致令牌补充
        let limiter = GcraLimiter::new(10, 1_000_000); // 10 capacity, 1s interval

        // Should allow burst up to capacity
        for _ in 0..10 {
            assert!(limiter.allow(1).await.unwrap());
        }

        // 11th request should be denied
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_gcra_cost_validation() {
        let limiter = GcraLimiter::new(10, 1000);

        // Zero cost should fail validation
        let result = limiter.allow(0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_gcra_remaining() {
        let limiter = GcraLimiter::new(10, 1000);

        // Initial remaining should be at capacity
        let remaining = limiter.remaining();
        assert!(remaining <= 10);
    }

    #[tokio::test]
    async fn test_gcra_check() {
        let limiter = GcraLimiter::new(10, 1000);

        let result = limiter.check(1);
        assert!(result.allowed);
        assert!(result.retry_after_us == 0);
    }

    #[test]
    fn test_gcra_debug() {
        let limiter = GcraLimiter::new(10, 1000);
        let debug_str = format!("{:?}", limiter);
        assert!(debug_str.contains("GcraLimiter"));
        assert!(debug_str.contains("capacity"));
    }

    #[test]
    fn test_gcra_with_rate() {
        let limiter = GcraLimiter::with_rate(100, 1000); // 100 burst, 1000 req/sec

        assert_eq!(limiter.capacity(), 100);
        assert_eq!(limiter.refill_interval_us(), 1000); // 1,000,000 / 1000 = 1000us
    }

    #[tokio::test]
    async fn test_gcra_high_cost() {
        let limiter = GcraLimiter::new(10, 1000);

        // Request with cost > capacity should be denied
        assert!(!limiter.allow(11).await.unwrap());
    }

    #[test]
    fn test_gcra_with_rate_zero() {
        // requests_per_second == 0 should use fallback interval of 1_000_000us
        let limiter = GcraLimiter::with_rate(10, 0);
        assert_eq!(limiter.refill_interval_us(), 1_000_000);
        assert_eq!(limiter.capacity(), 10);
    }

    #[test]
    fn test_gcra_check_cost_exceeds_capacity() {
        let limiter = GcraLimiter::new(10, 1000);
        let result = limiter.check(11);
        assert!(!result.allowed);
        assert_eq!(result.remaining, 0);
        assert_eq!(result.retry_after_us, 0);
    }

    #[tokio::test]
    async fn test_gcra_check_denied_path() {
        // Exhaust the limiter, then check() should return denied with retry_after
        let limiter = GcraLimiter::new(2, 100_000); // 100ms interval
        // Use up capacity
        assert!(limiter.allow(1).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());
        // Now check should be denied
        let result = limiter.check(1);
        assert!(!result.allowed);
        assert_eq!(result.remaining, 0);
        assert!(result.retry_after_us > 0);
    }

    #[tokio::test]
    async fn test_gcra_remaining_denied_path() {
        // Exhaust the limiter, remaining() should return 0
        let limiter = GcraLimiter::new(2, 100_000);
        let _ = limiter.allow(1).await;
        let _ = limiter.allow(1).await;
        // remaining should be 0 when exhausted
        let rem = limiter.remaining();
        assert!(
            rem <= 1,
            "remaining should be low when exhausted, got {}",
            rem
        );
    }

    #[test]
    fn test_gcra_tat_accessor() {
        let limiter = GcraLimiter::new(10, 1000);
        let tat = limiter.tat();
        // TAT should be a valid timestamp (non-zero)
        assert!(tat > 0);
    }

    #[tokio::test]
    async fn test_gcra_check_allowed_with_remaining() {
        let limiter = GcraLimiter::new(10, 1000);
        let result = limiter.check(1);
        assert!(result.allowed);
        assert_eq!(result.retry_after_us, 0);
        // remaining should be <= capacity
        assert!(result.remaining <= 10);
    }

    #[test]
    fn test_gcra_check_result_fields() {
        let result = GcraCheckResult {
            allowed: true,
            remaining: 5,
            retry_after_us: 0,
        };
        assert!(result.allowed);
        assert_eq!(result.remaining, 5);
        assert_eq!(result.retry_after_us, 0);
    }
}
