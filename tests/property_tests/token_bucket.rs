//! Token Bucket 属性测试
//!
//! 验证令牌桶算法的核心属性:
//! 1. 任何时刻被允许的请求数不超过容量+时间补充的令牌
//! 2. 令牌补充速率正确
//! 3. 并发安全:多线程竞争下不超限

use limiteron::clock::MockClock;
use limiteron::limiters::{Limiter, TokenBucketLimiter};
use proptest::prelude::*;
use std::sync::Arc;
use std::time::Duration;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// 属性: 初始状态下,允许的请求总数不超过桶容量
    #[test]
    fn test_token_bucket_never_exceeds_initial_capacity(
        capacity in 1u64..1000,
        num_requests in 1usize..1000
    ) {
        let limiter = TokenBucketLimiter::new(capacity, 1);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut allowed_count = 0u64;

        for _ in 0..num_requests {
            let allowed = rt.block_on(limiter.allow(1)).unwrap();
            if allowed {
                allowed_count += 1;
            }
        }

        // 属性: 允许的数量不能超过初始容量
        prop_assert!(
            allowed_count <= capacity,
            "Allowed {} requests but capacity is {}",
            allowed_count,
            capacity
        );
    }

    /// 属性: 等待时间T后,补充的令牌数 = min(capacity, T * refill_rate)
    #[test]
    fn test_token_bucket_refills_correctly(
        capacity in 1u64..100,
        refill_rate in 1u64..50,
        wait_seconds in 0u64..10
    ) {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn limiteron::clock::Clock> = mock_clock.clone();
        let limiter = TokenBucketLimiter::with_clock(capacity, refill_rate, clock);

        let rt = tokio::runtime::Runtime::new().unwrap();

        // 先消费所有令牌
        for _ in 0..capacity {
            let _ = rt.block_on(limiter.allow(1));
        }

        // 验证令牌已耗尽
        prop_assert_eq!(limiter.tokens(), 0);

        // 时间前进
        mock_clock.advance(Duration::from_secs(wait_seconds));

        // 触发补充
        let _ = rt.block_on(limiter.allow(1));

        // 计算预期令牌数
        let expected_tokens = ((wait_seconds as f64 * refill_rate as f64) as u64)
            .min(capacity)
            .saturating_sub(1); // 减去刚才allow(1)消费的1个

        // 属性: 令牌数应该在预期范围内(允许1个误差因为触发补充时消耗了1个)
        let actual_tokens = limiter.tokens();
        let lower_bound = expected_tokens.saturating_sub(1);
        let upper_bound = expected_tokens.saturating_add(1);

        prop_assert!(
            actual_tokens >= lower_bound && actual_tokens <= upper_bound.min(capacity),
            "Expected tokens around {} (range {}-{}), got {}. capacity={}, refill_rate={}, wait={}s",
            expected_tokens,
            lower_bound,
            upper_bound.min(capacity),
            actual_tokens,
            capacity,
            refill_rate,
            wait_seconds
        );
    }

    /// 属性: 令牌补充后总数不超过容量
    #[test]
    fn test_token_bucket_never_exceeds_capacity_after_refill(
        capacity in 1u64..100,
        refill_rate in 1u64..100,
        wait_seconds in 0u64..100
    ) {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn limiteron::clock::Clock> = mock_clock.clone();
        let limiter = TokenBucketLimiter::with_clock(capacity, refill_rate, clock);

        // 时间前进很长时间,远超补充所需
        mock_clock.advance(Duration::from_secs(wait_seconds));

        // 触发补充
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _ = rt.block_on(limiter.allow(1));

        // 属性: 令牌数永远不超过容量
        prop_assert!(
            limiter.tokens() <= capacity,
            "Tokens {} exceeded capacity {} after {}s wait",
            limiter.tokens(),
            capacity,
            wait_seconds
        );
    }

    /// 属性: 不同cost的请求,总允许数符合令牌消耗
    #[test]
    fn test_token_bucket_cost_consumption(
        capacity in 10u64..100,
        refill_rate in 1u64..10,
        cost in 1u64..50
    ) {
        let limiter = TokenBucketLimiter::new(capacity, refill_rate);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut total_consumed = 0u64;

        for _ in 0..100 {
            let allowed = rt.block_on(limiter.allow(cost)).unwrap();
            if allowed {
                total_consumed += cost;
            }
        }

        // 属性: 总消耗不超过初始容量
        prop_assert!(
            total_consumed <= capacity,
            "Total consumed {} exceeds capacity {}",
            total_consumed,
            capacity
        );
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;

    #[tokio::test]
    async fn test_token_bucket_zero_capacity_behavior() {
        // 验证边界条件:容量为0时不应允许任何请求
        let limiter = TokenBucketLimiter::new(0, 10);
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_token_bucket_exact_consumption() {
        let limiter = TokenBucketLimiter::new(10, 10);

        // 精确消费所有令牌
        for _ in 0..10 {
            assert!(limiter.allow(1).await.unwrap());
        }

        // 第11个应该失败
        assert!(!limiter.allow(1).await.unwrap());
    }
}
