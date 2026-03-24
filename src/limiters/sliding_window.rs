//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 滑动窗口限流器模块
//!
//! 使用滑动窗口算法实现速率限制。

use super::traits::{validate_cost, Limiter};
use crate::error::FlowGuardError;
use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 滑动窗口限流器
///
/// 使用滑动窗口算法实现速率限制，将时间窗口划分为多个小段，
/// 记录每个小段的请求时间，统计窗口内的请求数。
///
/// **注意**: 此实现使用 O(n) 复杂度，高并发场景下性能较差。
/// 建议使用 `ShardedSlidingWindowLimiter` 替代。
///
/// # 特性
/// - 使用 VecDeque 存储请求时间戳
/// - O(n) 时间复杂度
/// - 线程安全
#[deprecated(
    since = "0.1.1",
    note = "使用 `ShardedSlidingWindowLimiter` 替代。此实现使用 O(n) 复杂度，高并发场景下性能较差。"
)]
pub struct SlidingWindowLimiter {
    /// 窗口大小
    window_size: Duration,
    /// 最大请求数
    max_requests: u64,
    /// 请求时间戳队列
    requests: Arc<Mutex<VecDeque<Instant>>>,
}

impl SlidingWindowLimiter {
    /// Creates a new sliding window limiter.
    ///
    /// # Arguments
    /// * `window_size` - Time window duration
    /// * `max_requests` - Maximum requests allowed in the window
    ///
    /// # Examples
    /// ```rust,ignore
    /// use limiteron::limiters::SlidingWindowLimiter;
    /// use std::time::Duration;
    ///
    /// let limiter = SlidingWindowLimiter::new(Duration::from_secs(60), 100);
    /// ```
    #[deprecated(
        since = "0.1.1",
        note = "使用 `ShardedSlidingWindowLimiter` 替代。"
    )]
    pub fn new(window_size: Duration, max_requests: u64) -> Self {
        Self {
            window_size,
            max_requests,
            requests: Arc::new(Mutex::new(VecDeque::with_capacity(
                max_requests as usize + 10,
            ))),
        }
    }

    /// 获取窗口大小
    #[deprecated(since = "0.1.1", note = "使用 `ShardedSlidingWindowLimiter` 替代。")]
    pub fn window_size(&self) -> Duration {
        self.window_size
    }

    /// 获取最大请求数
    #[deprecated(since = "0.1.1", note = "使用 `ShardedSlidingWindowLimiter` 替代。")]
    pub fn max_requests(&self) -> u64 {
        self.max_requests
    }
}

#[async_trait]
impl Limiter for SlidingWindowLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError> {
        validate_cost(cost)?;

        let now = Instant::now();
        let mut requests = self.requests.lock();

        // 移除过期的请求记录
        let cutoff = now - self.window_size;
        while let Some(&front) = requests.front() {
            if front <= cutoff {
                requests.pop_front();
            } else {
                break;
            }
        }

        // 检查是否超过限制
        let current_count = requests.len() as u64;
        if current_count + cost > self.max_requests {
            return Ok(false);
        }

        // 添加新的请求记录
        for _ in 0..cost {
            requests.push_back(now);
        }

        Ok(true)
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sliding_window_basic() {
        let limiter = SlidingWindowLimiter::new(Duration::from_secs(60), 10);

        // 连续请求应该成功
        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }
    }

    #[tokio::test]
    async fn test_sliding_window_exceed() {
        let limiter = SlidingWindowLimiter::new(Duration::from_secs(60), 3);

        // 前3个请求成功
        assert!(limiter.allow(1).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());

        // 第4个请求失败
        assert!(!limiter.allow(1).await.unwrap());
    }
}
