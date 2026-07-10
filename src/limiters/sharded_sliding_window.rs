//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 分片滑动窗口限流器模块
//!
//! 使用分片计数实现 O(1) 时间复杂度的限流检查。

use super::traits::{Limiter, validate_cost};
use crate::clock::{Clock, SystemClock};
use crate::error::FlowGuardError;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// 默认分片数量（每秒一个分片，支持60秒窗口）
const DEFAULT_SHARD_COUNT: usize = 60;

/// 分片滑动窗口限流器
///
/// 使用分片计数实现 O(1) 时间复杂度的限流检查。
/// 每个分片代表窗口内的一个时间片（如1秒），通过原子操作实现无锁并发。
///
/// # 设计原理
///
/// 传统滑动窗口需要存储所有请求的时间戳，时间复杂度为 O(n)。
/// 分片设计将时间窗口划分为固定数量的分片，每个分片记录该时间片内的请求数，
/// 从而将时间复杂度降低到 O(1)（分片数量固定）。
///
/// # 性能特点
///
/// - **时间复杂度**: O(SHARD_COUNT)，通常为 O(60) = O(1)
/// - **空间复杂度**: O(SHARD_COUNT)，固定内存占用
/// - **并发安全**: 完全无锁，使用原子操作
/// - **精度**: 分片粒度决定（默认1秒）
///
/// # 示例
///
/// ```rust
/// use limiteron::limiters::{ShardedSlidingWindowLimiter, Limiter};
/// use std::time::Duration;
///
/// #[tokio::main]
/// async fn main() {
///     // 创建窗口大小为 60 秒，最大请求数为 1000 的分片滑动窗口限流器
///     let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 1000);
///
///     // 尝试请求
///     let allowed = limiter.allow(1).await.unwrap();
///     assert!(allowed);
/// }
/// ```
pub struct ShardedSlidingWindowLimiter {
    /// 分片计数器数组
    ///
    /// 每个分片存储该时间片内的请求数。
    /// 使用 AtomicU64 实现无锁原子操作。
    shards: Box<[AtomicU64; DEFAULT_SHARD_COUNT]>,

    /// 分片时间戳数组
    ///
    /// 记录每个分片对应的时间片起始时间（秒级时间戳）。
    /// 用于判断分片是否过期需要重置。
    shard_timestamps: Box<[AtomicU64; DEFAULT_SHARD_COUNT]>,

    /// 窗口大小（秒）
    window_size_secs: u64,

    /// 每个分片代表的时间长度（秒）
    shard_duration_secs: u64,

    /// 最大请求数
    max_requests: u64,

    /// 最后清理时间（秒级时间戳）
    ///
    /// 用于定期触发分片清理，避免每次请求都清理。
    last_cleanup: AtomicU64,

    /// 时钟实例
    clock: Arc<dyn Clock>,
}

impl ShardedSlidingWindowLimiter {
    /// 创建新的分片滑动窗口限流器
    ///
    /// # 参数
    /// - `window_size`: 滑动窗口大小
    /// - `max_requests`: 窗口内最大请求数
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::limiters::ShardedSlidingWindowLimiter;
    /// use std::time::Duration;
    ///
    /// let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 1000);
    /// ```
    pub fn new(window_size: Duration, max_requests: u64) -> Self {
        Self::with_clock(window_size, max_requests, Arc::new(SystemClock))
    }

    /// 创建新的分片滑动窗口限流器,使用自定义时钟
    ///
    /// # 参数
    /// - `window_size`: 滑动窗口大小
    /// - `max_requests`: 窗口内最大请求数
    /// - `clock`: 时钟实现,用于时间注入(测试用)
    pub fn with_clock(window_size: Duration, max_requests: u64, clock: Arc<dyn Clock>) -> Self {
        let window_size_secs = window_size.as_secs().max(1);

        // 计算每个分片代表的时间长度
        let shard_duration_secs = (window_size_secs / DEFAULT_SHARD_COUNT as u64).max(1);

        // 获取当前时间戳（秒）
        let now_secs = clock.unix_timestamp();

        // 初始化分片计数器和时间戳
        let shards = Box::new([(); DEFAULT_SHARD_COUNT].map(|_| AtomicU64::new(0)));
        let shard_timestamps = Box::new([(); DEFAULT_SHARD_COUNT].map(|_| AtomicU64::new(0)));

        Self {
            shards,
            shard_timestamps,
            window_size_secs,
            shard_duration_secs,
            max_requests,
            last_cleanup: AtomicU64::new(now_secs),
            clock,
        }
    }

    /// 获取当前时间戳（秒）
    fn current_timestamp_secs(&self) -> u64 {
        self.clock.unix_timestamp()
    }

    /// 计算时间戳对应的分片索引
    #[inline]
    fn get_shard_index(&self, timestamp_secs: u64) -> usize {
        (timestamp_secs as usize) % DEFAULT_SHARD_COUNT
    }

    /// 获取当前分片索引并返回当前时间戳
    #[inline]
    fn get_current_shard(&self) -> (usize, u64) {
        let now_secs = self.current_timestamp_secs();
        let shard_index = self.get_shard_index(now_secs);
        (shard_index, now_secs)
    }

    /// 原子地更新当前分片的计数
    fn increment_shard(&self, shard_index: usize, now_secs: u64, cost: u64) -> u64 {
        let shard = &self.shards[shard_index];
        let timestamp = &self.shard_timestamps[shard_index];

        let expected_timestamp = now_secs / self.shard_duration_secs * self.shard_duration_secs;

        loop {
            let current_timestamp = timestamp.load(Ordering::Acquire);

            if current_timestamp == expected_timestamp {
                return shard.fetch_add(cost, Ordering::Release) + cost;
            }

            match timestamp.compare_exchange(
                current_timestamp,
                expected_timestamp,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    shard.store(cost, Ordering::Release);
                    return cost;
                }
                Err(_) => {
                    std::hint::spin_loop();
                }
            }
        }
    }

    /// 计算窗口内的总请求数
    fn calculate_window_count(&self, now_secs: u64) -> u64 {
        let window_start = now_secs.saturating_sub(self.window_size_secs);
        let mut total_count = 0u64;

        for i in 0..DEFAULT_SHARD_COUNT {
            let shard_timestamp = self.shard_timestamps[i].load(Ordering::Acquire);

            if shard_timestamp > window_start && shard_timestamp <= now_secs {
                total_count += self.shards[i].load(Ordering::Acquire);
            }
        }

        total_count
    }

    /// 清理过期的分片
    fn cleanup_expired_shards(&self, now_secs: u64) {
        let window_start = now_secs.saturating_sub(self.window_size_secs);

        for i in 0..DEFAULT_SHARD_COUNT {
            let shard_timestamp = self.shard_timestamps[i].load(Ordering::Acquire);

            if shard_timestamp <= window_start
                && shard_timestamp != 0
                && self.shard_timestamps[i]
                    .compare_exchange(shard_timestamp, 0, Ordering::Release, Ordering::Relaxed)
                    .is_ok()
            {
                self.shards[i].store(0, Ordering::Release);
            }
        }
    }

    /// 定期清理检查
    fn maybe_cleanup(&self, now_secs: u64) {
        let cleanup_interval = (self.shard_duration_secs / 10).max(1);
        let last = self.last_cleanup.load(Ordering::Acquire);

        if now_secs.saturating_sub(last) >= cleanup_interval
            && self
                .last_cleanup
                .compare_exchange(last, now_secs, Ordering::Release, Ordering::Relaxed)
                .is_ok()
        {
            self.cleanup_expired_shards(now_secs);
        }
    }

    /// 尝试消费指定数量的请求配额
    fn try_acquire(&self, cost: u64) -> bool {
        let (shard_index, now_secs) = self.get_current_shard();

        let current_count = self.calculate_window_count(now_secs);

        if current_count + cost > self.max_requests {
            return false;
        }

        self.increment_shard(shard_index, now_secs, cost);
        self.maybe_cleanup(now_secs);

        true
    }

    /// 获取当前窗口内的请求数（仅用于测试和监控）
    #[cfg(test)]
    pub fn get_window_count(&self) -> u64 {
        let now_secs = self.current_timestamp_secs();
        self.calculate_window_count(now_secs)
    }

    /// 获取指定分片的计数（仅用于测试）
    #[cfg(test)]
    pub fn get_shard_count(&self, index: usize) -> u64 {
        if index < DEFAULT_SHARD_COUNT {
            self.shards[index].load(Ordering::SeqCst)
        } else {
            0
        }
    }
}

#[async_trait]
impl Limiter for ShardedSlidingWindowLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError> {
        let cost = validate_cost(cost)?;
        Ok(self.try_acquire(cost))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;

    #[tokio::test]
    async fn test_sharded_basic() {
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 1000);

        for _ in 0..10 {
            assert!(limiter.allow(1).await.unwrap());
        }
        assert_eq!(limiter.get_window_count(), 10);
    }

    #[tokio::test]
    async fn test_sharded_exceed() {
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 5);

        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }

        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_sharded_with_mock_clock() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter = ShardedSlidingWindowLimiter::with_clock(Duration::from_secs(60), 5, clock);

        // 消费 5 个请求
        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }

        // 第 6 个应该失败
        assert!(!limiter.allow(1).await.unwrap());

        // 前进时间使窗口过期
        mock_clock.advance(Duration::from_secs(61));

        // 新的请求应该成功
        assert!(limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_sharded_zero_cost_rejected() {
        // validate_cost(0) 应返回错误
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 1000);
        let result = limiter.allow(0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sharded_cost_exceeds_max() {
        // cost > MAX_COST 应返回错误
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 1000);
        let result = limiter.allow(u64::MAX).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_sharded_get_shard_count_out_of_bounds() {
        // 越界索引应返回 0
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 1000);
        let count = limiter.get_shard_count(999);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_sharded_get_shard_count_valid_index() {
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 1000);
        // 有效索引应返回 0（初始状态）
        let count = limiter.get_shard_count(0);
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_sharded_get_window_count_after_requests() {
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 1000);
        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }
        let count = limiter.get_window_count();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_sharded_multiple_users() {
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 3);

        // user1 uses all 3 slots
        assert!(limiter.allow(1).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());

        // user2 - but this limiter is per-instance, not per-key.
        // It tracks total requests, not per-key.
    }

    #[tokio::test]
    async fn test_sharded_with_small_window() {
        // 使用非常小的窗口（1秒）测试 window_size_secs.max(1)
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(1), 10);
        assert!(limiter.allow(1).await.unwrap());
        assert_eq!(limiter.get_window_count(), 1);
    }

    #[tokio::test]
    async fn test_sharded_cost_greater_than_max() {
        // cost > max_requests 应被拒绝
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 5);
        // cost of 6 exceeds max of 5
        let result = limiter.allow(6).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}
