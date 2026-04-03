//! 并发属性测试
//!
//! 验证在并发竞争条件下,限流器的核心安全属性:
//! 1. N个并发请求中,被允许的数量不超过配置的上限
//! 2. 多线程竞争下不出现数据竞争
//! 3. 各种限流器在并发下的行为一致性

use limiteron::clock::MockClock;
use limiteron::limiters::{FixedWindowLimiter, Limiter, TokenBucketLimiter};
use proptest::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Barrier;

/// 并发测试辅助函数: 同时启动N个任务,统计允许的数量
async fn concurrent_allow_test(limiter: Arc<dyn Limiter>, num_tasks: usize) -> u64 {
    let barrier = Arc::new(Barrier::new(num_tasks));
    let allowed_count = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::with_capacity(num_tasks);

    for _ in 0..num_tasks {
        let limiter_clone = limiter.clone();
        let barrier_clone = barrier.clone();
        let count_clone = allowed_count.clone();

        handles.push(tokio::spawn(async move {
            // 所有任务等待同一 barrier 确保真正并发
            barrier_clone.wait().await;

            if limiter_clone.allow(1).await.unwrap() {
                count_clone.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    // 等待所有任务完成
    for handle in handles {
        handle.await.unwrap();
    }

    allowed_count.load(Ordering::SeqCst)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// 属性: TokenBucket 并发请求,允许数不超过容量
    #[test]
    fn test_token_bucket_concurrent_never_exceeds_limit(
        capacity in 1u64..100,
        num_concurrent in 10usize..200
    ) {
        let limiter = Arc::new(TokenBucketLimiter::new(capacity, 1));

        let rt = tokio::runtime::Runtime::new().unwrap();
        let allowed = rt.block_on(concurrent_allow_test(limiter, num_concurrent));

        // 核心属性: 并发下允许数不超过容量
        prop_assert!(
            allowed <= capacity,
            "Concurrent: allowed {} exceeds capacity {} with {} tasks",
            allowed,
            capacity,
            num_concurrent
        );
    }

    /// 属性: FixedWindow 并发请求,允许数不超过窗口限制
    #[test]
    fn test_fixed_window_concurrent_never_exceeds_limit(
        max_requests in 1u64..100,
        num_concurrent in 10usize..200
    ) {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn limiteron::clock::Clock> = mock_clock.clone();
        let limiter = Arc::new(FixedWindowLimiter::with_clock(
            Duration::from_secs(60),
            max_requests,
            clock,
        ));

        let rt = tokio::runtime::Runtime::new().unwrap();
        let allowed = rt.block_on(concurrent_allow_test(limiter, num_concurrent));

        // 核心属性: 并发下允许数不超过窗口限制
        prop_assert!(
            allowed <= max_requests,
            "Concurrent: allowed {} exceeds max_requests {} with {} tasks",
            allowed,
            max_requests,
            num_concurrent
        );
    }

    /// 属性: 高并发下TokenBucket的令牌消耗精确性
    #[test]
    fn test_token_bucket_concurrent_exact_consumption(
        capacity in 10u64..50,
        num_concurrent in 50usize..150
    ) {
        let limiter = Arc::new(TokenBucketLimiter::new(capacity, 0)); // refill_rate=0,不补充

        let rt = tokio::runtime::Runtime::new().unwrap();
        let allowed = rt.block_on(concurrent_allow_test(limiter, num_concurrent));

        // 属性: 允许数精确等于容量(当并发数>=容量时)
        if num_concurrent as u64 >= capacity {
            prop_assert_eq!(
                allowed,
                capacity,
                "Expected exactly {} allowed but got {} with {} concurrent tasks",
                capacity,
                allowed,
                num_concurrent
            );
        } else {
            // 并发数小于容量时,应该全部允许
            prop_assert_eq!(
                allowed,
                num_concurrent as u64,
                "Expected {} allowed but got {}",
                num_concurrent,
                allowed
            );
        }
    }

    /// 属性: 不同 refill_rate 下的并发行为
    #[test]
    fn test_token_bucket_concurrent_with_refill(
        capacity in 10u64..50,
        refill_rate in 1u64..10,
        num_concurrent in 20usize..100
    ) {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn limiteron::clock::Clock> = mock_clock.clone();
        let limiter = Arc::new(TokenBucketLimiter::with_clock(
            capacity,
            refill_rate,
            clock,
        ));

        let rt = tokio::runtime::Runtime::new().unwrap();

        // 第一轮并发
        let allowed_first = rt.block_on(concurrent_allow_test(limiter.clone(), num_concurrent));

        // 时间前进,补充令牌
        mock_clock.advance(Duration::from_secs(1));

        // 第二轮并发
        let allowed_second = rt.block_on(concurrent_allow_test(limiter, num_concurrent));

        // 属性: 第一轮不超过容量
        prop_assert!(
            allowed_first <= capacity,
            "First round: {} exceeded capacity {}",
            allowed_first,
            capacity
        );

        // 属性: 第二轮有补充后,允许数与补充相关
        let expected_refill = (refill_rate as usize).min(capacity as usize);
        prop_assert!(
            allowed_second <= (expected_refill as u64).min(capacity),
            "Second round: {} exceeded expected refill {}",
            allowed_second,
            expected_refill
        );
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_token_bucket_stress() {
        let limiter = Arc::new(TokenBucketLimiter::new(100, 10));
        let num_tasks = 500;

        let allowed = concurrent_allow_test(limiter, num_tasks).await;

        // 应该正好允许100个(初始容量)
        assert_eq!(allowed, 100, "Expected 100 allowed, got {}", allowed);
    }

    #[tokio::test]
    async fn test_concurrent_fixed_window_stress() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn limiteron::clock::Clock> = mock_clock.clone();
        let limiter = Arc::new(FixedWindowLimiter::with_clock(
            Duration::from_secs(60),
            50,
            clock,
        ));
        let num_tasks = 300;

        let allowed = concurrent_allow_test(limiter, num_tasks).await;

        // 应该正好允许50个
        assert_eq!(allowed, 50, "Expected 50 allowed, got {}", allowed);
    }

    #[tokio::test]
    async fn test_concurrent_multiple_waves() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn limiteron::clock::Clock> = mock_clock.clone();
        let limiter = Arc::new(TokenBucketLimiter::with_clock(10, 10, clock));

        // 第一波: 消耗所有令牌
        let allowed1 = concurrent_allow_test(limiter.clone(), 50).await;
        assert_eq!(allowed1, 10);

        // 前进1秒,补充10个令牌
        mock_clock.advance(Duration::from_secs(1));

        // 第二波: 应该又能允许10个
        let allowed2 = concurrent_allow_test(limiter, 50).await;
        assert_eq!(allowed2, 10);
    }
}
