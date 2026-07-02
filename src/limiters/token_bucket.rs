//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 令牌桶限流器模块
//!
//! 使用令牌桶算法实现速率限制。

use super::traits::{validate_cost, Limiter};
use crate::clock::{Clock, SystemClock};
use crate::error::FlowGuardError;
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// 令牌桶限流器
///
/// 使用令牌桶算法实现速率限制，令牌以恒定速率补充到桶中，
/// 请求到达时从桶中获取令牌，如果令牌不足则拒绝请求。
///
/// # 特性
/// - 使用 AtomicU64 实现令牌计数
/// - 使用 AtomicU64 实现最后补充时间
/// - 使用 CAS (Compare-And-Swap) 循环确保原子性
/// - 使用 SeqCst 内存序确保并发安全
///
/// # 示例
/// ```rust
/// use limiteron::limiters::TokenBucketLimiter;
/// use limiteron::limiters::Limiter;
///
/// #[tokio::main]
/// async fn main() {
///     // 创建容量为 100，补充速率为 10 令牌/秒的令牌桶
///     let limiter = TokenBucketLimiter::new(100, 10);
///
///     // 尝试消费 10 个令牌
///     let allowed = limiter.allow(10).await.unwrap();
///     assert!(allowed);
/// }
/// ```
pub struct TokenBucketLimiter {
    /// 桶的最大容量
    capacity: u64,
    /// 当前令牌数（使用原子操作）
    tokens: AtomicU64,
    /// 令牌补充速率（令牌/秒）
    refill_rate: u64,
    /// 最后补充时间（纳秒时间戳）
    last_refill: AtomicU64,
    /// 时钟实例
    clock: Arc<dyn Clock>,
}

impl TokenBucketLimiter {
    /// Creates a new token bucket limiter.
    ///
    /// # Arguments
    /// * `capacity` - Maximum tokens in the bucket
    /// * `refill_rate` - Tokens added per second
    ///
    /// # Examples
    /// ```rust
    /// use limiteron::limiters::TokenBucketLimiter;
    ///
    /// let limiter = TokenBucketLimiter::new(100, 10);
    /// ```
    pub fn new(capacity: u64, refill_rate: u64) -> Self {
        Self::with_clock(capacity, refill_rate, Arc::new(SystemClock))
    }

    /// Creates a new token bucket limiter with a custom clock.
    ///
    /// # Arguments
    /// * `capacity` - Maximum tokens in the bucket
    /// * `refill_rate` - Tokens added per second
    /// * `clock` - Clock implementation for time injection (useful for testing)
    pub fn with_clock(capacity: u64, refill_rate: u64, clock: Arc<dyn Clock>) -> Self {
        let now_nanos = clock.unix_timestamp_nanos();

        Self {
            capacity,
            tokens: AtomicU64::new(capacity),
            refill_rate,
            last_refill: AtomicU64::new(now_nanos),
            clock,
        }
    }

    /// 获取桶容量
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// 获取补充速率
    pub fn refill_rate(&self) -> u64 {
        self.refill_rate
    }

    /// 获取当前令牌数
    pub fn tokens(&self) -> u64 {
        self.tokens.load(Ordering::SeqCst)
    }

    /// 补充令牌
    fn refill_tokens(&self) {
        let now = self.clock.unix_timestamp_nanos();

        loop {
            let last = self.last_refill.load(Ordering::SeqCst);
            let elapsed_nanos = now.saturating_sub(last);

            // 至少经过1毫秒才补充
            if elapsed_nanos < 1_000_000 {
                break;
            }

            let elapsed_seconds = elapsed_nanos as f64 / 1_000_000_000.0;
            let tokens_to_add = (elapsed_seconds * self.refill_rate as f64) as u64;

            if tokens_to_add == 0 {
                break;
            }

            // 尝试更新最后补充时间
            if self
                .last_refill
                .compare_exchange(last, now, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                // 成功更新时间，补充令牌
                loop {
                    let current = self.tokens.load(Ordering::SeqCst);
                    let new_tokens = current.saturating_add(tokens_to_add).min(self.capacity);

                    if self
                        .tokens
                        .compare_exchange(current, new_tokens, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        break;
                    }
                }
                break;
            }
        }
    }
}

#[async_trait]
impl Limiter for TokenBucketLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError> {
        validate_cost(cost)?;

        // 先补充令牌
        self.refill_tokens();

        loop {
            let current = self.tokens.load(Ordering::SeqCst);

            // 检查令牌是否足够
            if current < cost {
                return Ok(false);
            }

            // 尝试消费令牌
            if self
                .tokens
                .compare_exchange(current, current - cost, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Ok(true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;
    use std::time::Duration;

    #[tokio::test]
    async fn test_token_bucket_basic() {
        let limiter = TokenBucketLimiter::new(100, 10);

        // 初始应该有 100 个令牌
        assert_eq!(limiter.tokens(), 100);

        // 消费 10 个令牌应该成功
        assert!(limiter.allow(10).await.unwrap());

        // 剩余 90 个令牌
        assert_eq!(limiter.tokens(), 90);
    }

    #[tokio::test]
    async fn test_token_bucket_exceed_capacity() {
        let limiter = TokenBucketLimiter::new(10, 10);

        // 消费 10 个令牌应该成功
        assert!(limiter.allow(10).await.unwrap());

        // 再消费 1 个应该失败
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_token_bucket_refill() {
        let limiter = TokenBucketLimiter::new(10, 1000); // 1000 tokens/sec

        // 消费所有令牌
        assert!(limiter.allow(10).await.unwrap());
        assert_eq!(limiter.tokens(), 0);

        // 等待一小段时间
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // 触发补充
        let _ = limiter.allow(1).await;

        // 应该有补充的令牌
        let tokens = limiter.tokens();
        assert!(tokens > 0, "Expected tokens > 0, got {}", tokens);
    }

    #[tokio::test]
    async fn test_token_bucket_zero_cost() {
        let limiter = TokenBucketLimiter::new(100, 10);
        let result = limiter.allow(0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_token_bucket_exceed_max_cost() {
        let limiter = TokenBucketLimiter::new(100, 10);
        let result = limiter.allow(1_000_001).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_token_bucket_with_mock_clock() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter = TokenBucketLimiter::with_clock(10, 100, clock);

        // 消费所有令牌
        assert!(limiter.allow(10).await.unwrap());
        assert_eq!(limiter.tokens(), 0);

        // 时间前进 1 秒
        mock_clock.advance(Duration::from_secs(1));

        // 触发补充,应该补充 100 个令牌(但受容量限制为 10)
        let _ = limiter.allow(1).await;
        let tokens = limiter.tokens();
        assert!(
            tokens > 0,
            "Expected tokens > 0 after time advance, got {}",
            tokens
        );
    }

    #[test]
    fn test_token_bucket_accessors() {
        let limiter = TokenBucketLimiter::new(500, 25);
        assert_eq!(limiter.capacity(), 500);
        assert_eq!(limiter.refill_rate(), 25);
        assert_eq!(limiter.tokens(), 500);
    }

    #[tokio::test]
    async fn test_token_bucket_check_default_impl() {
        use crate::limiters::Limiter;
        let limiter = TokenBucketLimiter::new(100, 10);
        // check() default impl calls allow(1)
        assert!(limiter.check("any_key").await.is_ok());
        assert_eq!(limiter.tokens(), 99);
    }

    #[tokio::test]
    async fn test_token_bucket_refill_capped_at_capacity() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter = TokenBucketLimiter::with_clock(10, 1000, clock);

        // 消费所有令牌
        assert!(limiter.allow(10).await.unwrap());
        assert_eq!(limiter.tokens(), 0);

        // 时间前进 100 秒，应该补充很多令牌，但受容量限制
        mock_clock.advance(Duration::from_secs(100));
        let _ = limiter.allow(1).await;

        // 容量限制为 10，所以最多 10 个令牌（减去刚才消费的 1）
        let tokens = limiter.tokens();
        assert!(
            tokens <= 10,
            "Expected tokens <= capacity (10), got {}",
            tokens
        );
    }
}
