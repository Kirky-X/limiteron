//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 固定窗口限流器模块
//!
//! 使用固定窗口算法实现速率限制。

use super::traits::{validate_cost, Limiter};
use crate::error::FlowGuardError;
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// 固定窗口限流器
///
/// 使用固定窗口算法实现速率限制，将时间划分为固定长度的窗口，
/// 每个窗口独立计数，窗口到期自动重置。
///
/// # 特性
/// - 使用 AtomicU64 记录计数
/// - 使用 AtomicU64 记录窗口开始时间
/// - 窗口到期精确重置
/// - 并发安全
///
/// # 示例
/// ```rust
/// use limiteron::limiters::{FixedWindowLimiter, Limiter};
/// use std::time::Duration;
///
/// #[tokio::main]
/// async fn main() {
///     // 创建窗口大小为 1 秒，最大请求数为 100 的固定窗口限流器
///     let limiter = FixedWindowLimiter::new(Duration::from_secs(1), 100);
///
///     // 尝试请求
///     let allowed = limiter.allow(1).await.unwrap();
///     assert!(allowed);
/// }
/// ```
pub struct FixedWindowLimiter {
    /// 窗口大小
    window_size: Duration,
    /// 窗口内最大请求数
    max_requests: u64,
    /// 当前窗口的计数
    count: AtomicU64,
    /// 当前窗口的开始时间（纳秒时间戳）
    window_start: AtomicU64,
}

impl FixedWindowLimiter {
    /// Creates a new fixed window limiter.
    ///
    /// # Arguments
    /// * `window_size` - Fixed window duration
    /// * `max_requests` - Maximum requests per window
    ///
    /// # Examples
    /// ```rust
    /// use limiteron::limiters::FixedWindowLimiter;
    /// use std::time::Duration;
    ///
    /// let limiter = FixedWindowLimiter::new(Duration::from_secs(1), 100);
    /// ```
    pub fn new(window_size: Duration, max_requests: u64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        Self {
            window_size,
            max_requests,
            count: AtomicU64::new(0),
            window_start: AtomicU64::new(now),
        }
    }

    /// Checks and resets the window if expired.
    fn check_and_reset_window(&self) {
        let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_nanos() as u64,
            Err(_) => return,
        };

        let window_size_nanos = self.window_size.as_nanos() as u64;

        loop {
            let current_start = self.window_start.load(Ordering::Acquire);
            let window_end = current_start.saturating_add(window_size_nanos);

            if now < window_end {
                break;
            }

            let elapsed = now.saturating_sub(current_start);
            let windows_passed = elapsed / window_size_nanos;
            let new_start = current_start.saturating_add(windows_passed * window_size_nanos);

            match self.window_start.compare_exchange(
                current_start,
                new_start,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.count.store(0, Ordering::Release);
                    break;
                }
                Err(_) => continue,
            }
        }
    }

    /// 获取当前窗口的计数（仅用于测试）
    #[cfg(test)]
    fn get_count(&self) -> u64 {
        self.check_and_reset_window();
        self.count.load(Ordering::Acquire)
    }
}

#[async_trait]
impl Limiter for FixedWindowLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError> {
        let cost = validate_cost(cost)?;
        self.check_and_reset_window();

        loop {
            let current = self.count.load(Ordering::Acquire);

            if current + cost > self.max_requests {
                return Ok(false);
            }

            match self.count.compare_exchange(
                current,
                current + cost,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(true),
                Err(_) => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fixed_window_basic() {
        let limiter = FixedWindowLimiter::new(Duration::from_secs(60), 10);

        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }
        assert_eq!(limiter.get_count(), 5);
    }

    #[tokio::test]
    async fn test_fixed_window_exceed() {
        let limiter = FixedWindowLimiter::new(Duration::from_secs(60), 3);

        assert!(limiter.allow(1).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());
    }
}
