// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Sliding Window 属性测试
//!
//! 验证滑动窗口算法的核心属性:
//! 1. 滑动窗口内请求数不超过限制
//! 2. 过期请求正确移除
//! 3. 窗口滑动连续性

#![allow(deprecated)]

use limiteron::limiters::{Limiter, sliding_window::SlidingWindowLimiter};
use proptest::prelude::*;
use std::time::Duration;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// 属性: 滑动窗口内允许的请求数不超过max_requests
    #[test]
    fn test_sliding_window_respects_limit(
        max_requests in 1u64..200,
        window_seconds in 1u64..60,
        num_requests in 1usize..500
    ) {
        let limiter = SlidingWindowLimiter::new(
            Duration::from_secs(window_seconds),
            max_requests,
        );

        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut allowed_count = 0u64;

        for _ in 0..num_requests {
            let allowed = rt.block_on(limiter.allow(1)).unwrap();
            if allowed {
                allowed_count += 1;
            }
        }

        // 属性: 窗口内允许的数量不超过max_requests
        prop_assert!(
            allowed_count <= max_requests,
            "Sliding window allowed {} but limit is {}",
            allowed_count,
            max_requests
        );
    }

    /// 属性: 不同cost的请求,总消耗不超过限制
    #[test]
    fn test_sliding_window_cost_tracking(
        max_requests in 10u64..100,
        window_seconds in 1u64..30,
        cost in 1u64..20
    ) {
        let limiter = SlidingWindowLimiter::new(
            Duration::from_secs(window_seconds),
            max_requests,
        );

        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut total_cost = 0u64;

        for _ in 0..200 {
            let allowed = rt.block_on(limiter.allow(cost)).unwrap();
            if allowed {
                total_cost += cost;
            }
        }

        // 属性: 总消耗不超过限制
        prop_assert!(
            total_cost <= max_requests,
            "Total cost {} exceeded max_requests {}",
            total_cost,
            max_requests
        );
    }

    /// 属性: 窗口大小影响请求通过率
    #[test]
    fn test_sliding_window_size_impact(
        max_requests in 10u64..50,
        short_window in 1u64..10,
        long_window in 10u64..100
    ) {
        // 确保long_window > short_window
        let short_window = short_window.min(long_window.saturating_sub(1));
        let long_window = long_window.max(short_window + 1);

        let short_limiter = SlidingWindowLimiter::new(
            Duration::from_secs(short_window),
            max_requests,
        );
        let long_limiter = SlidingWindowLimiter::new(
            Duration::from_secs(long_window),
            max_requests,
        );

        let rt = tokio::runtime::Runtime::new().unwrap();

        // 在相同时间内发送相同数量的请求
        let num_requests = (max_requests * 2) as usize;
        let mut short_allowed = 0u64;
        let mut long_allowed = 0u64;

        for _ in 0..num_requests {
            if rt.block_on(short_limiter.allow(1)).unwrap() {
                short_allowed += 1;
            }
            if rt.block_on(long_limiter.allow(1)).unwrap() {
                long_allowed += 1;
            }
        }

        // 属性: 两者都不超过限制
        prop_assert!(short_allowed <= max_requests);
        prop_assert!(long_allowed <= max_requests);
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;

    #[tokio::test]
    async fn test_sliding_window_basic_allow() {
        let limiter = SlidingWindowLimiter::new(Duration::from_secs(60), 10);

        // 前10个请求应该都成功
        for _ in 0..10 {
            assert!(limiter.allow(1).await.unwrap());
        }

        // 第11个应该失败
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_sliding_window_expiration() {
        let limiter = SlidingWindowLimiter::new(Duration::from_millis(100), 5);

        // 填满窗口
        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }

        // 此时应该被限制
        assert!(!limiter.allow(1).await.unwrap());

        // 等待窗口过期
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 过期后应该允许新请求
        assert!(limiter.allow(1).await.unwrap());
    }
}
