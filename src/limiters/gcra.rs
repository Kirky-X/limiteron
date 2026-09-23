// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
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
use crate::error::LimiteronError;
use async_trait::async_trait;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

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
    /// 单调时间守卫：最近一次观测到的墙钟微秒值。
    ///
    /// 墙钟（SystemTime）可回拨——回拨后 now_us < EAT 会对所有请求持续拒绝
    /// 直到墙钟追回（拨到 epoch 之前时 unwrap_or(0) 甚至全量锁死）。
    /// 观测值取 `max(墙钟, last_now)`：回拨被冻结为"时间停滞"，既不锁死
    /// 也不产生虚假的突发放行，墙钟追回后自然恢复。
    last_now: AtomicU64,
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
        // interval=0 会使 EAT==TAT 恒成立、TAT 永不前进，限流完全失效。
        // 单点兜底钳制到最小 1µs（1e6 rps 封顶），公开构造器对 0 值宽容。
        let refill_interval_us = refill_interval_us.max(1);
        let now_us = Self::wall_us();

        Self {
            capacity,
            refill_interval_us,
            tat: RwLock::new(now_us),
            last_now: AtomicU64::new(now_us),
        }
    }

    /// Create a new GCRA limiter from rate specification
    ///
    /// # Arguments
    /// * `capacity` - Maximum burst size
    /// * `requests_per_second` - Sustained request rate
    ///
    /// rps=0 时退化为 1s 间隔；rps>1e6 时整除得 0，钳制到 1µs
    /// （按 1e6 rps 封顶），限流语义始终成立。
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
            .unwrap_or(1_000_000)
            .max(1);

        Self::new(capacity, refill_interval_us)
    }

    /// 读取墙钟微秒（可回拨、pre-epoch 时为 0 的原始值）
    fn wall_us() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0)
    }

    /// 单调化时间观测：`max(墙钟, 最近观测值)`。
    ///
    /// 墙钟回拨时返回冻结的 last_now（时间停滞语义，不锁死不假突放）；
    /// 墙钟前跳时推进并发布 last_now。拆出 wall 参数便于注入合成时间测试。
    fn monotonic_now(&self, wall: u64) -> u64 {
        let last = self.last_now.load(Ordering::Acquire);
        let now = wall.max(last);
        if now > last {
            let _ = self
                .last_now
                .compare_exchange(last, now, Ordering::Release, Ordering::Relaxed);
        }
        now
    }

    /// 当前单调观测时间（微秒）
    fn now_us(&self) -> u64 {
        self.monotonic_now(Self::wall_us())
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
        let now_us = self.now_us();
        let tat = *self.tat.read();

        if cost > self.capacity {
            return GcraCheckResult {
                allowed: false,
                remaining: 0,
                retry_after_us: 0,
            };
        }

        // Calculate Earliest Arrival Time (EAT)
        // 防御：超大 capacity × refill_interval_us 乘法防溢出（与 allow 一致，
        // debug panic / release 回绕会使只读决策给出错误的 allowed/retry_after）
        let eat = tat.saturating_sub(
            self.capacity
                .saturating_sub(1)
                .saturating_mul(self.refill_interval_us),
        );

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
        let now_us = self.now_us();
        let tat = *self.tat.read();

        // 防御：乘法与 allow/check 一致使用 saturating_mul，防止溢出失真
        let eat = tat.saturating_sub(
            self.capacity
                .saturating_sub(1)
                .saturating_mul(self.refill_interval_us),
        );

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
    /// * `Err(LimiteronError)` - Validation error
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError> {
        validate_cost(cost)?;

        if cost > self.capacity {
            return Ok(false);
        }

        Ok(self.allow_at(cost, self.now_us()))
    }
}

impl GcraLimiter {
    /// 核心判定：给定观测时间（微秒）的允许决策与 TAT 推进。
    ///
    /// now 由调用方注入（生产路径传 `self.now_us()`，测试传合成时间），
    /// 使回拨/突发语义可在无真实时间依赖下确定性验证。
    fn allow_at(&self, cost: u64, now_us: u64) -> bool {
        {
            let mut tat = self.tat.write();

            // Calculate Earliest Arrival Time (EAT)
            // 防御：公开构造器可能传入超大 capacity/cost，乘法用 saturating_mul 防溢出
            // （debug panic / release 回绕破坏限流不变量）。
            let eat = (*tat).saturating_sub(
                self.capacity
                    .saturating_sub(1)
                    .saturating_mul(self.refill_interval_us),
            );

            if now_us >= eat {
                // Request allowed, update TAT
                *tat = std::cmp::max(*tat, now_us)
                    .saturating_add(cost.saturating_mul(self.refill_interval_us));
                true
            } else {
                // Request denied
                false
            }
        }
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
    fn test_gcra_new_zero_interval_clamped() {
        // interval=0 会使 EAT==TAT 恒成立、TAT 永不前进（限流失效），必须钳制
        let limiter = GcraLimiter::new(10, 0);
        assert_eq!(limiter.refill_interval_us(), 1);
    }

    #[tokio::test]
    async fn test_gcra_with_rate_beyond_1m_rps_still_limits() {
        // rps > 1e6 时整除得 0：修复前 interval=0 → EAT==TAT 恒成立 → 全放行。
        // 钳制到 1µs 后按 1e6 rps 封顶，限流语义必须成立。
        let limiter = GcraLimiter::with_rate(100, 2_000_000);
        assert_eq!(limiter.refill_interval_us(), 1, "interval 应钳制到 1µs");

        // 持续请求：即便突发容量耗尽后 TAT 以 1µs/请求推进，真实时钟
        // 追不上 TAT 前进时也必须出现拒绝（除非单次调用开销 > 1µs，
        // 远超 RwLock CAS 路径的现实成本）
        let mut allowed = 0u64;
        for _ in 0..10_000 {
            if limiter.allow(1).await.unwrap() {
                allowed += 1;
            }
        }
        assert!(
            allowed < 10_000,
            "interval=0/1µs 场景下 10_000 次请求全部放行，限流失效"
        );
        assert!(
            allowed >= 100,
            "突发容量 {{100}} 应至少放行 100 次，实际 {allowed}"
        );
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

    #[test]
    fn test_gcra_monotonic_guard_freezes_on_rollback() {
        let limiter = GcraLimiter::new(10, 1_000_000);
        // 构造时 last_now = 真实墙钟，合成时间必须从基线之上推演
        let base = limiter.tat();

        // 首次观测建立基线
        assert_eq!(limiter.monotonic_now(base + 1_000_000), base + 1_000_000);
        // 墙钟回拨：冻结在最近观测值（时间停滞语义），不跟随回拨
        assert_eq!(limiter.monotonic_now(base + 500_000), base + 1_000_000);
        assert_eq!(
            limiter.monotonic_now(0),
            base + 1_000_000,
            "pre-epoch 墙钟不得锁死观测"
        );
        // 墙钟追回/前跳：正常推进并发布
        assert_eq!(limiter.monotonic_now(base + 1_500_000), base + 1_500_000);
    }

    #[tokio::test]
    async fn test_gcra_rollback_no_permanent_lockout() {
        // 修复前：墙钟回拨后 now_us < EAT 持续拒绝；pre-epoch 时 unwrap_or(0)
        // 使全部请求锁死直到墙钟追回。守卫下时间冻结为"停滞"，
        // 容量按冻结时间语义维持，恢复后可正常放行。
        let limiter = GcraLimiter::new(5, 1_000_000); // 1s interval
        let t0 = limiter.tat();

        // 合成时钟灌满 5 个突发容量
        for i in 0..5u64 {
            assert!(
                limiter.allow_at(1, t0 + i * 1_000),
                "第 {} 个突发请求应放行",
                i + 1
            );
        }
        assert!(!limiter.allow_at(1, t0 + 5_000), "突发耗尽后应拒绝");

        // "回拨"到 t0 之前（守卫下观测冻结）：容量为空的语义保持——拒绝但状态不破坏
        assert!(!limiter.allow_at(1, t0.saturating_sub(10_000_000)));

        // 墙钟恢复并前跳 5s：按 1s 间隔补满 5 个令牌，应放行
        assert!(limiter.allow_at(1, t0 + 5_000 + 5_000_000));
    }
}
