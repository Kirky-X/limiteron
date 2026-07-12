// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Fixed Window 属性测试
//!
//! 验证固定窗口算法的核心属性:
//! 1. 每个窗口内允许的请求数不超过limit
//! 2. 窗口重置后计数归零
//! 3. 跨窗口请求的正确性

use limiteron::MockClock;
use limiteron::limiters::{FixedWindowLimiter, Limiter};
use proptest::prelude::*;
use std::sync::Arc;
use std::time::Duration;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// 属性: 每个窗口内允许的请求数不超过max_requests
    #[test]
    fn test_fixed_window_respects_limit(
        max_requests in 1u64..500,
        window_seconds in 1u64..60,
        num_requests in 1usize..2000
    ) {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn limiteron::Clock> = mock_clock.clone();
        let limiter = FixedWindowLimiter::with_clock(
            Duration::from_secs(window_seconds),
            max_requests,
            clock,
        );

        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut allowed_count = 0u64;

        for _ in 0..num_requests {
            let allowed = rt.block_on(limiter.allow(1)).unwrap();
            if allowed {
                allowed_count += 1;
            }
        }

        // 属性: 单个窗口内允许的数量不超过max_requests
        prop_assert!(
            allowed_count <= max_requests,
            "Allowed {} requests in window but limit is {}",
            allowed_count,
            max_requests
        );
    }

    /// 属性: 窗口重置后,计数归零,新窗口可以重新允许请求
    #[test]
    fn test_fixed_window_resets_correctly(
        max_requests in 1u64..100,
        window_seconds in 1u64..30,
        num_windows in 1usize..5
    ) {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn limiteron::Clock> = mock_clock.clone();
        let limiter = FixedWindowLimiter::with_clock(
            Duration::from_secs(window_seconds),
            max_requests,
            clock,
        );

        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut total_allowed = 0u64;

        for _window in 0..num_windows {
            // 在当前窗口内尽可能多地请求
            let mut window_allowed = 0u64;
            for _ in 0..(max_requests + 10) {
                let allowed = rt.block_on(limiter.allow(1)).unwrap();
                if allowed {
                    window_allowed += 1;
                } else {
                    break;
                }
            }

            // 属性: 每个窗口允许的数量正好是max_requests
            prop_assert_eq!(
                window_allowed,
                max_requests,
                "Window allowed {} but expected {}",
                window_allowed,
                max_requests
            );

            total_allowed += window_allowed;

            // 前进到下一个窗口
            mock_clock.advance(Duration::from_secs(window_seconds + 1));
        }

        // 属性: 总允许数 = 窗口数 * 每个窗口的限制
        prop_assert_eq!(
            total_allowed,
            (num_windows as u64) * max_requests,
            "Total allowed {} but expected {} ({} windows * {} limit)",
            total_allowed,
            (num_windows as u64) * max_requests,
            num_windows,
            max_requests
        );
    }

    /// 属性: 窗口边界处请求处理正确
    #[test]
    fn test_fixed_window_boundary_behavior(
        max_requests in 1u64..50,
        window_seconds in 1u64..10
    ) {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn limiteron::Clock> = mock_clock.clone();
        let limiter = FixedWindowLimiter::with_clock(
            Duration::from_secs(window_seconds),
            max_requests,
            clock,
        );

        let rt = tokio::runtime::Runtime::new().unwrap();

        // 填充当前窗口
        for _ in 0..max_requests {
            let _ = rt.block_on(limiter.allow(1));
        }

        // 此时应该被限制
        prop_assert!(!rt.block_on(limiter.allow(1)).unwrap());

        // 前进刚好一个窗口（使用秒级，因为MockClock.advance使用as_secs）
        mock_clock.advance(Duration::from_secs(window_seconds + 1));

        // 新窗口应该允许请求
        prop_assert!(rt.block_on(limiter.allow(1)).unwrap());
    }

    /// 属性: 大cost请求正确处理
    #[test]
    fn test_fixed_window_large_cost_requests(
        max_requests in 10u64..100,
        window_seconds in 1u64..60,
        cost in 1u64..50
    ) {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn limiteron::Clock> = mock_clock.clone();
        let limiter = FixedWindowLimiter::with_clock(
            Duration::from_secs(window_seconds),
            max_requests,
            clock,
        );

        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut total_cost = 0u64;
        let num_requests = 100;

        for _ in 0..num_requests {
            let allowed = rt.block_on(limiter.allow(cost)).unwrap();
            if allowed {
                total_cost += cost;
            }
        }

        // 属性: 总cost不超过max_requests
        prop_assert!(
            total_cost <= max_requests,
            "Total cost {} exceeded max_requests {}",
            total_cost,
            max_requests
        );
    }

    /// 属性: 多个窗口后的累计计数正确
    #[test]
    fn test_fixed_window_cumulative_count_across_windows(
        max_requests in 1u64..20,
        window_seconds in 1u64..10
    ) {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn limiteron::Clock> = mock_clock.clone();
        let limiter = FixedWindowLimiter::with_clock(
            Duration::from_secs(window_seconds),
            max_requests,
            clock,
        );

        let rt = tokio::runtime::Runtime::new().unwrap();

        // 测试3个窗口
        for window in 0..3 {
            let mut count = 0u64;

            // 发送足够多的请求填满窗口
            for _ in 0..(max_requests * 2) {
                if rt.block_on(limiter.allow(1)).unwrap() {
                    count += 1;
                }
            }

            // 属性: 每个窗口计数一致
            prop_assert_eq!(
                count,
                max_requests,
                "Window {} had count {} but expected {}",
                window,
                count,
                max_requests
            );

            // 前进到下一窗口
            mock_clock.advance(Duration::from_secs(window_seconds + 1));
        }
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;

    #[tokio::test]
    async fn test_fixed_window_immediate_reset() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn limiteron::Clock> = mock_clock.clone();
        let limiter = FixedWindowLimiter::with_clock(Duration::from_secs(1), 5, clock);

        // 填满窗口
        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }

        // 下一个应该失败
        assert!(!limiter.allow(1).await.unwrap());

        // 前进时间
        mock_clock.advance(Duration::from_secs(2));

        // 新窗口应该成功
        assert!(limiter.allow(1).await.unwrap());
    }
}
