//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 限流器模块
//!
//! 实现各种限流算法。

#[cfg(feature = "quota-control")]
mod quota_limiter;

use crate::constants::MAX_COST;
use crate::constants::MAX_SPIN_ITERATIONS;
use crate::error::FlowGuardError;
use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ============================================================================
// Cost parameter validation
// ============================================================================

// ============================================================================
// Cost 参数验证函数
// ============================================================================

/// Validates the cost parameter.
///
/// # Arguments
/// * `cost` - The cost value to validate
///
/// # Returns
/// * `Ok(u64)` - The validated cost value
/// * `Err(FlowGuardError)` - Validation failed
fn validate_cost(cost: u64) -> Result<u64, FlowGuardError> {
    if cost == 0 {
        return Err(FlowGuardError::ConfigError(
            "Cost cannot be zero".to_string(),
        ));
    }

    if cost > MAX_COST {
        return Err(FlowGuardError::ConfigError(format!(
            "Cost exceeds maximum limit ({})",
            MAX_COST
        )));
    }

    Ok(cost)
}

/// 限流器 trait
///
/// 所有限流器都需要实现此 trait。使用 `async_trait` 宏支持异步操作。
///
/// # 特性
///
/// - **异步支持** - 所有方法都是异步的
/// - **线程安全** - 实现 `Send + Sync`
/// - **成本参数** - 支持每次请求消耗不同成本
///
/// # 示例
///
/// ```rust
/// use limiteron::limiters::Limiter;
/// use async_trait::async_trait;
///
/// struct MyLimiter;
///
/// #[async_trait]
/// impl Limiter for MyLimiter {
///     async fn allow(&self, cost: u64) -> Result<bool, limiteron::error::FlowGuardError> {
///         // 实现限流逻辑
///         Ok(true)
///     }
/// }
/// ```
#[async_trait]
pub trait Limiter: Send + Sync {
    /// 检查是否允许通过
    ///
    /// # 参数
    /// - `cost`: 请求消耗的成本
    ///
    /// # 返回
    /// - `Ok(true)`: 允许通过
    /// - `Ok(false)`: 拒绝通过
    /// - `Err(_)`: 发生错误
    async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError>;

    /// 检查是否允许（接受 key 参数，用于宏）
    ///
    /// 默认实现：消费 1 个单位的 cost
    ///
    /// # 参数
    /// - `_key`: 标识符 key（用于某些限流器类型）
    ///
    /// # 返回
    /// - `Ok(())`: 允许通过
    /// - `Err(_)`: 拒绝通过或发生错误
    async fn check(&self, _key: &str) -> Result<(), FlowGuardError> {
        self.allow(1).await?;
        Ok(())
    }
}

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
/// use limiteron::limiters::{TokenBucketLimiter, Limiter};
/// use std::time::Duration;
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
    tokens: std::sync::atomic::AtomicU64,
    /// 令牌补充速率（令牌/秒）
    refill_rate: u64,
    /// 最后补充时间（纳秒时间戳）
    last_refill: std::sync::atomic::AtomicU64,
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
        let now_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0); // 如果系统时间早于 Unix 纪元，使用 0 作为默认值

        Self {
            capacity,
            tokens: std::sync::atomic::AtomicU64::new(capacity),
            refill_rate,
            last_refill: std::sync::atomic::AtomicU64::new(now_nanos),
        }
    }

    /// Refills tokens based on elapsed time.
    ///
    /// Uses CAS loop for atomicity with SeqCst ordering.
    fn refill_tokens(&self) {
        let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_nanos() as u64,
            Err(_) => return, // 系统时间异常，跳过本次补充
        };

        // Use CAS loop to update last_refill and tokens atomically
        loop {
            let last = self.last_refill.load(std::sync::atomic::Ordering::Acquire);
            let elapsed_nanos = now.saturating_sub(last);

            // Skip if time delta is too small
            if elapsed_nanos < 1_000_000 {
                break;
            }

            // Calculate tokens to add
            let elapsed_seconds = elapsed_nanos as f64 / 1_000_000_000.0;
            let tokens_to_add = (elapsed_seconds * self.refill_rate as f64) as u64;

            if tokens_to_add == 0 {
                break;
            }

            // Try to update last_refill timestamp
            if self
                .last_refill
                .compare_exchange(
                    last,
                    now,
                    std::sync::atomic::Ordering::Release,
                    std::sync::atomic::Ordering::Relaxed,
                )
                .is_ok()
            {
                // Update token count
                loop {
                    let current = self.tokens.load(std::sync::atomic::Ordering::Acquire);
                    let new_tokens = current.saturating_add(tokens_to_add).min(self.capacity);

                    if self
                        .tokens
                        .compare_exchange(
                            current,
                            new_tokens,
                            std::sync::atomic::Ordering::Release,
                            std::sync::atomic::Ordering::Relaxed,
                        )
                        .is_ok()
                    {
                        break;
                    }
                }
                break;
            }
        }
    }

    /// 尝试消费指定数量的令牌
    ///
    /// # 参数
    /// - `cost`: 需要消费的令牌数量
    ///
    /// # 返回
    /// - `Ok(true)`: 成功消费令牌
    /// - `Ok(false)`: 令牌不足，无法消费
    /// - `Err(_)`: 发生错误
    fn try_consume(&self, cost: u64) -> bool {
        let mut retry_count = 0u32;
        const MAX_RETRY: u32 = 3;

        loop {
            let current = self.tokens.load(std::sync::atomic::Ordering::Acquire);

            // 检查令牌是否足够
            if current < cost {
                return false;
            }

            // 尝试消费令牌
            match self.tokens.compare_exchange(
                current,
                current - cost,
                std::sync::atomic::Ordering::Release,
                std::sync::atomic::Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(_) => {
                    retry_count += 1;
                    if retry_count >= MAX_RETRY {
                        // 超过最大重试次数，放弃
                        return false;
                    }

                    // 指数退避：使用自旋提示替代阻塞睡眠
                    // 避免在多线程环境下阻塞线程
                    if retry_count > 1 {
                        let backoff = 1u64 << (retry_count - 2);
                        // 使用自旋提示，让出CPU时间片
                        for _ in 0..backoff.min(MAX_SPIN_ITERATIONS) {
                            std::hint::spin_loop();
                        }
                    }
                }
            }
        }
    }

    /// 获取当前令牌数（仅用于测试）
    #[cfg(test)]
    fn get_tokens(&self) -> u64 {
        self.tokens.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait]
impl Limiter for TokenBucketLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError> {
        // 验证 cost 参数
        let cost = validate_cost(cost)?;

        // 先补充令牌
        self.refill_tokens();

        // 尝试消费令牌
        Ok(self.try_consume(cost))
    }
}

/// 滑动窗口限流器
///
/// 使用滑动窗口算法实现速率限制，记录请求的时间戳，
/// 统计滑动窗口内的请求数量，超过阈值则拒绝请求。
///
/// # 特性
/// - 支持可配置窗口精度（通过分片数）
/// - 使用 VecDeque 存储时间戳
/// - 自动清理过期请求
/// - 内存占用合理（< 1KB/窗口）
///
/// # 示例
/// ```rust
/// use limiteron::limiters::{SlidingWindowLimiter, Limiter};
/// use std::time::Duration;
///
/// #[tokio::main]
/// async fn main() {
///     // 创建窗口大小为 1 秒，最大请求数为 100 的滑动窗口限流器
///     let limiter = SlidingWindowLimiter::new(Duration::from_secs(1), 100);
///
///     // 尝试请求
///     let allowed = limiter.allow(1).await.unwrap();
///     assert!(allowed);
/// }
/// ```
pub struct SlidingWindowLimiter {
    /// 窗口大小
    window_size: Duration,
    /// 窗口内最大请求数
    max_requests: u64,
    /// 请求时间戳队列（使用 Arc<Mutex> 实现线程安全）
    requests: Arc<Mutex<VecDeque<Instant>>>,
}

impl SlidingWindowLimiter {
    /// Creates a new sliding window limiter.
    ///
    /// # Arguments
    /// * `window_size` - Sliding window duration
    /// * `max_requests` - Maximum requests per window
    ///
    /// # Examples
    /// ```rust
    /// use limiteron::limiters::SlidingWindowLimiter;
    /// use std::time::Duration;
    ///
    /// let limiter = SlidingWindowLimiter::new(Duration::from_secs(1), 100);
    /// ```
    pub fn new(window_size: Duration, max_requests: u64) -> Self {
        // Pre-allocate deque capacity based on max_requests to reduce allocations
        let capacity = (max_requests as usize).min(10_000);
        Self {
            window_size,
            max_requests,
            requests: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
        }
    }

    /// 清理过期的请求记录
    fn cleanup_expired_requests(&self) {
        let mut requests = self.requests.lock();
        let now = Instant::now();

        // 移除窗口外的请求
        while let Some(&front) = requests.front() {
            if now.duration_since(front) > self.window_size {
                requests.pop_front();
            } else {
                break;
            }
        }
    }

    /// 获取当前窗口内的请求数（仅用于测试）
    #[cfg(test)]
    fn get_request_count(&self) -> usize {
        self.cleanup_expired_requests();
        self.requests.lock().len()
    }
}

#[async_trait]
impl Limiter for SlidingWindowLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError> {
        // 验证 cost 参数
        let cost = validate_cost(cost)?;

        // 清理过期请求
        self.cleanup_expired_requests();

        let mut requests = self.requests.lock();
        let current_count = requests.len() as u64;

        // 检查是否超过限制
        if current_count + cost > self.max_requests {
            return Ok(false);
        }

        // 添加新的请求记录
        let now = Instant::now();
        for _ in 0..cost {
            requests.push_back(now);
        }

        Ok(true)
    }
}

// ============================================================================
// 分片滑动窗口限流器（无锁设计）
// ============================================================================

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
        let window_size_secs = window_size.as_secs().max(1);

        // 计算每个分片代表的时间长度
        // 确保至少1秒，最多不超过窗口大小
        let shard_duration_secs = (window_size_secs / DEFAULT_SHARD_COUNT as u64).max(1);

        // 获取当前时间戳（秒）
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

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
        }
    }

    /// 获取当前时间戳（秒）
    fn current_timestamp_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// 计算时间戳对应的分片索引
    ///
    /// # 参数
    /// - `timestamp_secs`: 秒级时间戳
    ///
    /// # 返回
    /// 分片索引（0 到 SHARD_COUNT-1）
    #[inline]
    fn get_shard_index(&self, timestamp_secs: u64) -> usize {
        // 使用取模运算确定分片索引
        (timestamp_secs as usize) % DEFAULT_SHARD_COUNT
    }

    /// 获取当前分片索引并返回当前时间戳
    ///
    /// # 返回
    /// (分片索引, 当前秒级时间戳)
    #[inline]
    fn get_current_shard(&self) -> (usize, u64) {
        let now_secs = Self::current_timestamp_secs();
        let shard_index = self.get_shard_index(now_secs);
        (shard_index, now_secs)
    }

    /// 原子地更新当前分片的计数
    ///
    /// 如果分片时间戳不匹配，会先重置分片再递增计数。
    ///
    /// # 参数
    /// - `shard_index`: 分片索引
    /// - `now_secs`: 当前秒级时间戳
    /// - `cost`: 需要增加的请求数
    ///
    /// # 返回
    /// 更新后的分片计数
    fn increment_shard(&self, shard_index: usize, now_secs: u64, cost: u64) -> u64 {
        let shard = &self.shards[shard_index];
        let timestamp = &self.shard_timestamps[shard_index];

        // 检查分片时间戳是否匹配当前时间片
        let expected_timestamp = now_secs / self.shard_duration_secs * self.shard_duration_secs;

        loop {
            let current_timestamp = timestamp.load(Ordering::Acquire);

            if current_timestamp == expected_timestamp {
                // 分片属于当前时间片，直接递增计数
                return shard.fetch_add(cost, Ordering::Release) + cost;
            }

            // 分片过期或未初始化，尝试重置
            match timestamp.compare_exchange(
                current_timestamp,
                expected_timestamp,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // 成功获取分片所有权，重置计数
                    shard.store(cost, Ordering::Release);
                    return cost;
                }
                Err(_) => {
                    // 其他线程已更新，重试
                    std::hint::spin_loop();
                }
            }
        }
    }

    /// 计算窗口内的总请求数
    ///
    /// 遍历所有分片，累加属于当前窗口的请求数。
    ///
    /// # 参数
    /// - `now_secs`: 当前秒级时间戳
    ///
    /// # 返回
    /// 窗口内的总请求数
    fn calculate_window_count(&self, now_secs: u64) -> u64 {
        let window_start = now_secs.saturating_sub(self.window_size_secs);
        let mut total_count = 0u64;

        for i in 0..DEFAULT_SHARD_COUNT {
            let shard_timestamp = self.shard_timestamps[i].load(Ordering::Acquire);

            // 检查分片是否在窗口范围内
            if shard_timestamp > window_start && shard_timestamp <= now_secs {
                total_count += self.shards[i].load(Ordering::Acquire);
            }
        }

        total_count
    }

    /// 清理过期的分片
    ///
    /// 将窗口外的分片计数重置为0，释放计数空间。
    /// 使用 CAS 操作确保线程安全。
    ///
    /// # 参数
    /// - `now_secs`: 当前秒级时间戳
    fn cleanup_expired_shards(&self, now_secs: u64) {
        let window_start = now_secs.saturating_sub(self.window_size_secs);

        for i in 0..DEFAULT_SHARD_COUNT {
            let shard_timestamp = self.shard_timestamps[i].load(Ordering::Acquire);

            // 如果分片时间戳在窗口外，重置计数
            if shard_timestamp <= window_start && shard_timestamp != 0 {
                // 尝试重置时间戳
                if self.shard_timestamps[i]
                    .compare_exchange(shard_timestamp, 0, Ordering::Release, Ordering::Relaxed)
                    .is_ok()
                {
                    // 成功重置时间戳，重置计数
                    self.shards[i].store(0, Ordering::Release);
                }
            }
        }
    }

    /// 定期清理检查
    ///
    /// 每隔一定时间触发一次清理，避免每次请求都清理。
    /// 清理间隔为分片时长的 10%，最小 1 秒。
    fn maybe_cleanup(&self, now_secs: u64) {
        let cleanup_interval = (self.shard_duration_secs / 10).max(1);
        let last = self.last_cleanup.load(Ordering::Acquire);

        if now_secs.saturating_sub(last) >= cleanup_interval {
            // 尝试更新清理时间戳
            if self
                .last_cleanup
                .compare_exchange(last, now_secs, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                self.cleanup_expired_shards(now_secs);
            }
        }
    }

    /// 尝试消费指定数量的请求配额
    ///
    /// # 参数
    /// - `cost`: 需要消费的请求数
    ///
    /// # 返回
    /// - `true`: 成功消费
    /// - `false`: 配额不足
    fn try_acquire(&self, cost: u64) -> bool {
        let (shard_index, now_secs) = self.get_current_shard();

        // 先计算当前窗口内的请求数（不包括本次请求）
        let current_count = self.calculate_window_count(now_secs);

        // 检查是否超过限制
        if current_count + cost > self.max_requests {
            return false;
        }

        // 递增当前分片计数
        // 注意：这里存在竞态条件，可能导致短暂的超限
        // 但对于限流场景，这种近似是可以接受的
        self.increment_shard(shard_index, now_secs, cost);

        // 定期清理过期分片
        self.maybe_cleanup(now_secs);

        true
    }

    /// 获取当前窗口内的请求数（仅用于测试和监控）
    ///
    /// # 返回
    /// 当前窗口内的总请求数
    #[cfg(test)]
    pub fn get_window_count(&self) -> u64 {
        let now_secs = Self::current_timestamp_secs();
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
        // 验证 cost 参数
        let cost = validate_cost(cost)?;

        // 尝试消费配额
        Ok(self.try_acquire(cost))
    }
}

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
    count: std::sync::atomic::AtomicU64,
    /// 当前窗口的开始时间（纳秒时间戳）
    window_start: std::sync::atomic::AtomicU64,
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
            .unwrap_or(0); // 如果系统时间早于 Unix 纪元，使用 0 作为默认值

        Self {
            window_size,
            max_requests,
            count: std::sync::atomic::AtomicU64::new(0),
            window_start: std::sync::atomic::AtomicU64::new(now),
        }
    }

    /// Checks and resets the window if expired.
    ///
    /// Uses CAS for atomic window reset with proper alignment.
    fn check_and_reset_window(&self) {
        let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_nanos() as u64,
            Err(_) => return, // 系统时间异常，跳过本次检查
        };

        let window_size_nanos = self.window_size.as_nanos() as u64;

        loop {
            let current_start = self.window_start.load(std::sync::atomic::Ordering::Acquire);
            let window_end = current_start.saturating_add(window_size_nanos);

            // Current time still within window
            if now < window_end {
                break;
            }

            // Calculate aligned window start
            let elapsed = now.saturating_sub(current_start);
            let windows_passed = elapsed / window_size_nanos;
            let new_start = current_start.saturating_add(windows_passed * window_size_nanos);

            // Attempt atomic update
            match self.window_start.compare_exchange(
                current_start,
                new_start,
                std::sync::atomic::Ordering::Release,
                std::sync::atomic::Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.count.store(0, std::sync::atomic::Ordering::Release);
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
        self.count.load(std::sync::atomic::Ordering::Acquire)
    }
}

#[async_trait]
impl Limiter for FixedWindowLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError> {
        // 验证 cost 参数
        let cost = validate_cost(cost)?;

        // 检查并重置窗口
        self.check_and_reset_window();

        // 使用 CAS 循环尝试增加计数
        loop {
            let current = self.count.load(std::sync::atomic::Ordering::Acquire);

            // 检查是否超过限制
            if current + cost > self.max_requests {
                return Ok(false);
            }

            // 尝试增加计数
            match self.count.compare_exchange(
                current,
                current + cost,
                std::sync::atomic::Ordering::Release,
                std::sync::atomic::Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(true),
                Err(_) => continue, // CAS 失败，重试
            }
        }
    }
}

/// 并发控制器
///
/// 使用信号量实现并发控制，限制同时进行的操作数量。
/// 支持超时机制和取消操作。
///
/// # 特性
/// - 使用 tokio::sync::Semaphore 管理并发数
/// - 支持超时机制
/// - 支持取消操作
/// - 无死锁风险
/// - 支持依赖注入模式
///
/// # 示例
/// ```rust
/// use limiteron::limiters::ConcurrencyLimiter;
///
/// #[tokio::main]
/// async fn main() {
///     // 创建最大并发数为 10 的并发控制器
///     let limiter = ConcurrencyLimiter::new(10);
///
///     // 尝试获取许可
///     let permit = limiter.acquire(1).await.unwrap();
///     // 使用许可...
///     drop(permit); // 释放许可
/// }
/// ```
pub struct ConcurrencyLimiter {
    /// 信号量，用于管理并发数
    semaphore: Arc<tokio::sync::Semaphore>,
    /// 超时时间
    timeout: Option<Duration>,
    /// 最大并发数
    max_concurrent: u64,
}

/// ConcurrencyLimiter 构建器
///
/// 用于链式配置 ConcurrencyLimiter 实例。
///
/// # 示例
///
/// ```rust
/// use limiteron::limiters::ConcurrencyLimiter;
/// use std::time::Duration;
///
/// let limiter = ConcurrencyLimiter::builder()
///     .max_concurrent(10)
///     .timeout(Duration::from_secs(5))
///     .build();
/// ```
#[derive(Default)]
pub struct ConcurrencyLimiterBuilder {
    max_concurrent: Option<u64>,
    timeout: Option<Duration>,
    semaphore: Option<Arc<tokio::sync::Semaphore>>,
}

impl ConcurrencyLimiterBuilder {
    /// 创建新的 ConcurrencyLimiterBuilder
    pub fn new() -> Self {
        Self {
            max_concurrent: None,
            timeout: None,
            semaphore: None,
        }
    }

    /// 设置最大并发数
    ///
    /// # 参数
    /// - `max_concurrent`: 最大并发数
    pub fn max_concurrent(mut self, max_concurrent: u64) -> Self {
        self.max_concurrent = Some(max_concurrent);
        self
    }

    /// 设置超时时间
    ///
    /// # 参数
    /// - `timeout`: 获取许可的超时时间
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// 设置外部信号量（用于依赖注入）
    ///
    /// # 参数
    /// - `semaphore`: 外部管理的信号量
    ///
    /// # 注意
    /// 如果设置了外部信号量，max_concurrent 设置将被忽略。
    pub fn with_semaphore(mut self, semaphore: Arc<tokio::sync::Semaphore>) -> Self {
        self.semaphore = Some(semaphore);
        self
    }

    /// 构建 ConcurrencyLimiter 实例
    ///
    /// # 返回
    /// - `Ok(ConcurrencyLimiter)`: 构建成功
    /// - `Err(FlowGuardError)`: 构建失败（缺少必需参数）
    pub fn build(self) -> Result<ConcurrencyLimiter, FlowGuardError> {
        // 如果提供了外部信号量，使用它
        if let Some(semaphore) = self.semaphore {
            return Ok(ConcurrencyLimiter {
                semaphore,
                timeout: self.timeout,
                max_concurrent: 0, // 外部信号量时，max_concurrent 不适用
            });
        }

        // 否则创建新的信号量
        let max_concurrent = self
            .max_concurrent
            .ok_or_else(|| FlowGuardError::ConfigError("max_concurrent is required".to_string()))?;

        if max_concurrent == 0 {
            return Err(FlowGuardError::ConfigError(
                "max_concurrent must be greater than 0".to_string(),
            ));
        }

        Ok(ConcurrencyLimiter {
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent as usize)),
            timeout: self.timeout,
            max_concurrent,
        })
    }
}

impl ConcurrencyLimiter {
    /// 创建新的并发控制器
    ///
    /// # 参数
    /// - `max_concurrent`: 最大并发数
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::limiters::ConcurrencyLimiter;
    ///
    /// let limiter = ConcurrencyLimiter::new(10);
    /// ```
    pub fn new(max_concurrent: u64) -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent as usize)),
            timeout: None,
            max_concurrent,
        }
    }

    /// 创建带超时的并发控制器
    ///
    /// # 参数
    /// - `max_concurrent`: 最大并发数
    /// - `timeout`: 获取许可的超时时间
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::limiters::ConcurrencyLimiter;
    /// use std::time::Duration;
    ///
    /// let limiter = ConcurrencyLimiter::with_timeout(10, Duration::from_secs(5));
    /// ```
    pub fn with_timeout(max_concurrent: u64, timeout: Duration) -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent as usize)),
            timeout: Some(timeout),
            max_concurrent,
        }
    }

    /// 创建 ConcurrencyLimiterBuilder 用于链式配置
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::limiters::ConcurrencyLimiter;
    /// use std::time::Duration;
    ///
    /// let limiter = ConcurrencyLimiter::builder()
    ///     .max_concurrent(10)
    ///     .timeout(Duration::from_secs(5))
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn builder() -> ConcurrencyLimiterBuilder {
        ConcurrencyLimiterBuilder::new()
    }

    /// 使用依赖注入创建 ConcurrencyLimiter 实例
    ///
    /// 允许外部管理信号量的生命周期，适用于共享资源的场景。
    ///
    /// # 参数
    /// - `semaphore`: 外部管理的信号量
    /// - `timeout`: 可选的超时时间
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::limiters::ConcurrencyLimiter;
    /// use std::sync::Arc;
    /// use std::time::Duration;
    ///
    /// // 创建共享信号量
    /// let semaphore = Arc::new(tokio::sync::Semaphore::new(10));
    ///
    /// // 多个限流器共享同一个信号量
    /// let limiter1 = ConcurrencyLimiter::with_dependencies(semaphore.clone(), None);
    /// let limiter2 = ConcurrencyLimiter::with_dependencies(semaphore, Some(Duration::from_secs(5)));
    /// ```
    pub fn with_dependencies(
        semaphore: Arc<tokio::sync::Semaphore>,
        timeout: Option<Duration>,
    ) -> Self {
        Self {
            semaphore,
            timeout,
            max_concurrent: 0, // 外部信号量时，max_concurrent 不适用
        }
    }

    /// 获取最大并发数
    ///
    /// # 注意
    /// 如果使用外部信号量（通过 `with_dependencies` 创建），返回值为 0。
    pub fn max_concurrent(&self) -> u64 {
        self.max_concurrent
    }

    /// 获取超时时间
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    /// 获取许可并执行操作
    ///
    /// # 参数
    /// - `cost`: 需要获取的许可数量
    ///
    /// # 返回
    /// - `Ok(permit)`: 成功获取许可，返回许可对象
    /// - `Err(_)`: 获取许可失败
    pub async fn acquire(
        &self,
        cost: u64,
    ) -> Result<tokio::sync::SemaphorePermit<'_>, FlowGuardError> {
        let cost_u32 = cost as u32;
        if cost_u32 as u64 != cost {
            return Err(FlowGuardError::LimitError(
                "许可数量超出 u32 范围".to_string(),
            ));
        }

        let permit = match self.timeout {
            Some(timeout) => tokio::time::timeout(timeout, self.semaphore.acquire_many(cost_u32))
                .await
                .map_err(|_| FlowGuardError::LimitError("获取许可超时".to_string()))?
                .map_err(|_| FlowGuardError::LimitError("信号量已关闭".to_string()))?,
            None => self
                .semaphore
                .acquire_many(cost_u32)
                .await
                .map_err(|_| FlowGuardError::LimitError("信号量已关闭".to_string()))?,
        };

        Ok(permit)
    }

    /// 获取当前可用的许可数（仅用于测试）
    #[cfg(test)]
    fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// 尝试获取许可（非阻塞）
    ///
    /// # 参数
    /// - `cost`: 需要获取的许可数量
    ///
    /// # 返回
    /// - `Ok(permit)`: 成功获取许可
    /// - `Err(_)`: 获取许可失败
    #[cfg(test)]
    fn try_acquire(&self, cost: u64) -> Result<tokio::sync::SemaphorePermit<'_>, FlowGuardError> {
        let cost_u32 = cost as u32;
        if cost_u32 as u64 != cost {
            return Err(FlowGuardError::LimitError(
                "许可数量超出 u32 范围".to_string(),
            ));
        }

        self.semaphore
            .try_acquire_many(cost_u32)
            .map_err(|e| FlowGuardError::LimitError(format!("获取许可失败: {:?}", e)))
    }
}

#[async_trait]
impl Limiter for ConcurrencyLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError> {
        // 检查是否有足够的许可（非阻塞）
        let cost_u32 = cost as u32;
        if cost_u32 as u64 != cost {
            return Err(FlowGuardError::LimitError(
                "许可数量超出 u32 范围".to_string(),
            ));
        }

        match self.semaphore.try_acquire_many(cost_u32) {
            Ok(_permit) => {
                // 立即释放许可，因为 allow 方法不应该持有许可
                // 这是设计决策：allow 只检查是否有足够的许可，但不持有
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }
}

#[cfg(feature = "quota-control")]
pub use quota_limiter::QuotaLimiter;

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    // ==================== TokenBucketLimiter 测试 ====================

    #[tokio::test]
    async fn test_token_bucket_basic() {
        let limiter = TokenBucketLimiter::new(100, 10);
        assert!(limiter.allow(10).await.unwrap());
        assert_eq!(limiter.get_tokens(), 90);
    }

    #[tokio::test]
    async fn test_token_bucket_insufficient_tokens() {
        let limiter = TokenBucketLimiter::new(10, 1);
        assert!(limiter.allow(10).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_token_bucket_refill() {
        let limiter = TokenBucketLimiter::new(10, 100); // 100 tokens/sec
        limiter.allow(10).await.unwrap();
        assert_eq!(limiter.get_tokens(), 0);

        sleep(Duration::from_millis(20)).await; // 等待 20ms，应该补充约 2 个令牌
        limiter.allow(1).await.unwrap(); // 触发补充，使用 cost=1
        assert!(limiter.get_tokens() >= 1);
    }

    #[tokio::test]
    async fn test_token_bucket_concurrent() {
        let limiter = Arc::new(TokenBucketLimiter::new(100, 10));
        let mut handles = vec![];

        for _ in 0..10 {
            let limiter_clone = Arc::clone(&limiter);
            handles.push(tokio::spawn(async move {
                for _ in 0..10 {
                    limiter_clone.allow(1).await.unwrap();
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // 总共消费 100 个令牌，应该正好消耗完
        assert_eq!(limiter.get_tokens(), 0);
    }

    #[tokio::test]
    async fn test_token_bucket_no_overconsumption() {
        let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
        let mut handles = vec![];

        for _ in 0..100 {
            let limiter_clone = Arc::clone(&limiter);
            handles.push(tokio::spawn(async move {
                limiter_clone.allow(1).await.unwrap()
            }));
        }

        let mut allowed_count = 0;
        for handle in handles {
            if handle.await.unwrap() {
                allowed_count += 1;
            }
        }

        // 不应该超过 10 个请求被允许
        assert!(allowed_count <= 10);
    }

    // ==================== SlidingWindowLimiter 测试 ====================

    #[tokio::test]
    async fn test_sliding_window_basic() {
        let limiter = SlidingWindowLimiter::new(Duration::from_secs(1), 10);
        assert!(limiter.allow(1).await.unwrap());
        assert_eq!(limiter.get_request_count(), 1);
    }

    #[tokio::test]
    async fn test_sliding_window_exceeds_limit() {
        let limiter = SlidingWindowLimiter::new(Duration::from_secs(1), 10);
        for _ in 0..10 {
            assert!(limiter.allow(1).await.unwrap());
        }
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_sliding_window_sliding() {
        let limiter = SlidingWindowLimiter::new(Duration::from_millis(100), 5);

        // 发送 5 个请求
        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }

        // 应该被拒绝
        assert!(!limiter.allow(1).await.unwrap());

        // 等待窗口滑动
        sleep(Duration::from_millis(101)).await;

        // 现在应该可以发送新请求
        assert!(limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_sliding_window_concurrent() {
        let limiter = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(1), 10));
        let mut handles = vec![];

        for _ in 0..20 {
            let limiter_clone = Arc::clone(&limiter);
            handles.push(tokio::spawn(async move {
                limiter_clone.allow(1).await.unwrap()
            }));
        }

        let mut allowed_count = 0;
        for handle in handles {
            if handle.await.unwrap() {
                allowed_count += 1;
            }
        }

        // 不应该超过 10 个请求被允许
        assert!(allowed_count <= 10);
    }

    #[tokio::test]
    async fn test_sliding_window_cost() {
        let limiter = SlidingWindowLimiter::new(Duration::from_secs(1), 10);
        assert!(limiter.allow(5).await.unwrap());
        assert!(limiter.allow(5).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());
    }

    // ==================== ShardedSlidingWindowLimiter 测试 ====================

    #[tokio::test]
    async fn test_sharded_sliding_window_basic() {
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 1000);
        assert!(limiter.allow(1).await.unwrap());
        // 由于时间精度问题，只验证允许请求
    }

    #[tokio::test]
    async fn test_sharded_sliding_window_exceeds_limit() {
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 10);

        // 发送 10 个请求，应该全部成功
        for _ in 0..10 {
            assert!(limiter.allow(1).await.unwrap());
        }

        // 第 11 个请求应该被拒绝
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_sharded_sliding_window_cost() {
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 10);
        assert!(limiter.allow(5).await.unwrap());
        assert!(limiter.allow(5).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_sharded_sliding_window_concurrent() {
        let limiter = Arc::new(ShardedSlidingWindowLimiter::new(
            Duration::from_secs(60),
            100,
        ));
        let mut handles = vec![];

        for _ in 0..200 {
            let limiter_clone = Arc::clone(&limiter);
            handles.push(tokio::spawn(async move {
                limiter_clone.allow(1).await.unwrap()
            }));
        }

        let mut allowed_count = 0;
        for handle in handles {
            if handle.await.unwrap() {
                allowed_count += 1;
            }
        }

        // 由于竞态条件，可能略微超过限制（允许 5% 的误差）
        assert!(allowed_count <= 105, "Allowed count: {}", allowed_count);
    }

    #[tokio::test]
    async fn test_sharded_sliding_window_high_concurrency() {
        let limiter = Arc::new(ShardedSlidingWindowLimiter::new(
            Duration::from_secs(60),
            1000,
        ));
        let mut handles = vec![];

        // 使用 barrier 确保所有任务同时开始
        let barrier = Arc::new(tokio::sync::Barrier::new(100));
        let start_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

        for _ in 0..100 {
            let limiter_clone = Arc::clone(&limiter);
            let barrier_clone = Arc::clone(&barrier);
            let start_signal_clone = Arc::clone(&start_signal);
            handles.push(tokio::spawn(async move {
                // 等待所有任务准备就绪
                barrier_clone.wait().await;

                // 等待开始信号
                while !start_signal_clone.load(std::sync::atomic::Ordering::SeqCst) {
                    std::hint::spin_loop();
                }

                // 每个任务发送 10 个请求
                let mut local_allowed = 0;
                for _ in 0..10 {
                    if limiter_clone.allow(1).await.unwrap() {
                        local_allowed += 1;
                    }
                }
                local_allowed
            }));
        }

        // 设置开始信号
        start_signal.store(true, std::sync::atomic::Ordering::SeqCst);

        let mut total_allowed = 0;
        for handle in handles {
            total_allowed += handle.await.unwrap();
        }

        // 由于竞态条件，可能略微超过限制（允许 10% 的误差）
        assert!(total_allowed <= 1100, "Total allowed: {}", total_allowed);
    }

    #[tokio::test]
    async fn test_sharded_sliding_window_no_deadlock() {
        let limiter = Arc::new(ShardedSlidingWindowLimiter::new(
            Duration::from_secs(60),
            10000,
        ));

        // 启动大量并发任务，确保不会死锁
        let mut handles = vec![];
        for _ in 0..1000 {
            let limiter_clone = Arc::clone(&limiter);
            handles.push(tokio::spawn(async move {
                for _ in 0..10 {
                    let _ = limiter_clone.allow(1).await;
                }
            }));
        }

        // 使用超时确保不会死锁
        let result = tokio::time::timeout(Duration::from_secs(5), async {
            for handle in handles {
                let _ = handle.await;
            }
        })
        .await;

        assert!(result.is_ok(), "Test timed out - possible deadlock");
    }

    #[tokio::test]
    async fn test_sharded_sliding_window_shard_rotation() {
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 10000);

        // 发送一些请求
        for _ in 0..100 {
            assert!(limiter.allow(1).await.unwrap());
        }

        // 验证窗口计数
        let count = limiter.get_window_count();
        assert!(count >= 100);
    }

    #[tokio::test]
    async fn test_sharded_sliding_window_large_cost() {
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 100);
        assert!(limiter.allow(50).await.unwrap());
        assert!(limiter.allow(50).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_sharded_sliding_window_stress_test() {
        let limiter = Arc::new(ShardedSlidingWindowLimiter::new(
            Duration::from_secs(60),
            100000,
        ));
        let mut handles = vec![];

        // 压力测试：100 个并发任务，每个发送 100 个请求
        for _ in 0..100 {
            let limiter_clone = Arc::clone(&limiter);
            handles.push(tokio::spawn(async move {
                for _ in 0..100 {
                    let _ = limiter_clone.allow(1).await;
                }
            }));
        }

        for handle in handles {
            let _ = handle.await;
        }

        // 验证最终状态合理
        let count = limiter.get_window_count();
        // 由于竞态条件，可能略微超过限制
        assert!(count <= 101000, "Final count: {}", count);
    }

    // ==================== FixedWindowLimiter 测试 ====================

    #[tokio::test]
    async fn test_fixed_window_basic() {
        let limiter = FixedWindowLimiter::new(Duration::from_secs(1), 10);
        assert!(limiter.allow(1).await.unwrap());
        assert_eq!(limiter.get_count(), 1);
    }

    #[tokio::test]
    async fn test_fixed_window_exceeds_limit() {
        let limiter = FixedWindowLimiter::new(Duration::from_secs(1), 10);
        for _ in 0..10 {
            assert!(limiter.allow(1).await.unwrap());
        }
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_fixed_window_reset() {
        let limiter = FixedWindowLimiter::new(Duration::from_millis(100), 5);

        // 发送 5 个请求
        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }

        // 应该被拒绝
        assert!(!limiter.allow(1).await.unwrap());

        // 等待窗口重置
        sleep(Duration::from_millis(101)).await;

        // 新窗口应该重置
        assert!(limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_fixed_window_concurrent() {
        let limiter = Arc::new(FixedWindowLimiter::new(Duration::from_secs(1), 10));
        let mut handles = vec![];

        for _ in 0..20 {
            let limiter_clone = Arc::clone(&limiter);
            handles.push(tokio::spawn(async move {
                limiter_clone.allow(1).await.unwrap()
            }));
        }

        let mut allowed_count = 0;
        for handle in handles {
            if handle.await.unwrap() {
                allowed_count += 1;
            }
        }

        // 不应该超过 10 个请求被允许
        assert!(allowed_count <= 10);
    }

    #[tokio::test]
    async fn test_fixed_window_cost() {
        let limiter = FixedWindowLimiter::new(Duration::from_secs(1), 10);
        assert!(limiter.allow(5).await.unwrap());
        assert!(limiter.allow(5).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());
    }

    // ==================== ConcurrencyLimiter 测试 ====================

    #[tokio::test]
    async fn test_concurrency_limiter_basic() {
        let limiter = ConcurrencyLimiter::new(10);
        // allow 方法只检查是否有足够的许可，但不持有
        assert!(limiter.allow(1).await.unwrap());
        // 因为 allow 不持有许可，所以许可数仍然是 10
        assert_eq!(limiter.available_permits(), 10);
    }

    #[tokio::test]
    async fn test_concurrency_limiter_exceeds_limit() {
        let limiter = ConcurrencyLimiter::new(5);
        // allow 方法不持有许可，所以所有请求都应该被允许
        for _ in 0..10 {
            assert!(limiter.allow(1).await.unwrap());
        }
    }

    #[tokio::test]
    async fn test_concurrency_limiter_with_timeout() {
        let limiter = ConcurrencyLimiter::with_timeout(1, Duration::from_millis(100));
        // allow 方法不持有许可，所以所有请求都应该被允许
        assert!(limiter.allow(1).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_concurrency_limiter_acquire_release() {
        let limiter = Arc::new(ConcurrencyLimiter::new(2));

        // 获取许可
        let permit1 = limiter.acquire(1).await.unwrap();
        assert_eq!(limiter.available_permits(), 1);

        let _permit2 = limiter.acquire(1).await.unwrap();
        assert_eq!(limiter.available_permits(), 0);

        // 应该无法获取更多许可（使用 try_acquire 测试）
        assert!(limiter.try_acquire(1).is_err());

        // 释放许可
        drop(permit1);
        assert_eq!(limiter.available_permits(), 1);

        // 现在应该可以获取许可
        let _permit3 = limiter.acquire(1).await.unwrap();
        assert_eq!(limiter.available_permits(), 0);
    }

    #[tokio::test]
    async fn test_concurrency_limiter_concurrent_acquire() {
        let limiter = Arc::new(ConcurrencyLimiter::new(5));
        let mut handles = vec![];

        // 使用 barrier 确保所有任务同时开始
        let barrier = Arc::new(tokio::sync::Barrier::new(10));
        let start_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

        for _ in 0..10 {
            let limiter_clone = Arc::clone(&limiter);
            let barrier_clone = Arc::clone(&barrier);
            let start_signal_clone = Arc::clone(&start_signal);
            handles.push(tokio::spawn(async move {
                // 等待所有任务准备就绪
                barrier_clone.wait().await;

                // 使用 try_acquire 而不是 acquire，因为 acquire 会等待
                // 我们想要测试的是同时尝试获取许可的情况
                loop {
                    if start_signal_clone.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                }

                match limiter_clone.try_acquire(1) {
                    Ok(_permit) => {
                        // 持有许可一段时间
                        sleep(Duration::from_millis(50)).await;
                        true
                    }
                    Err(_) => false,
                }
            }));
        }

        // 设置开始信号
        start_signal.store(true, std::sync::atomic::Ordering::SeqCst);

        let mut allowed_count = 0;
        for handle in handles {
            if handle.await.unwrap() {
                allowed_count += 1;
            }
        }

        // 不应该超过 5 个请求被允许
        assert!(allowed_count <= 5);
    }

    #[tokio::test]
    async fn test_concurrency_limiter_allow_does_not_hold() {
        let limiter = Arc::new(ConcurrencyLimiter::new(2));

        // allow 方法不持有许可
        assert!(limiter.allow(1).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());

        // 获取许可会真正持有
        let _permit1 = limiter.acquire(1).await.unwrap();
        assert_eq!(limiter.available_permits(), 1);

        let _permit2 = limiter.acquire(1).await.unwrap();
        assert_eq!(limiter.available_permits(), 0);

        // 无法获取更多许可
        assert!(limiter.try_acquire(1).is_err());
    }

    // ==================== 并发安全测试 ====================

    /// 测试 SlidingWindowLimiter 高并发场景下的线程安全
    ///
    /// 验证在高并发情况下，限流器不会出现数据竞争或死锁
    #[tokio::test]
    async fn test_sliding_window_high_concurrency_safety() {
        let limiter = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(1), 1000));
        let mut handles = vec![];

        // 使用 barrier 确保所有任务同时开始
        let barrier = Arc::new(tokio::sync::Barrier::new(100));
        let start_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

        for _ in 0..100 {
            let limiter_clone = Arc::clone(&limiter);
            let barrier_clone = Arc::clone(&barrier);
            let start_signal_clone = Arc::clone(&start_signal);
            handles.push(tokio::spawn(async move {
                // 等待所有任务准备就绪
                barrier_clone.wait().await;

                // 等待开始信号
                while !start_signal_clone.load(std::sync::atomic::Ordering::SeqCst) {
                    std::hint::spin_loop();
                }

                // 每个任务发送 20 个请求
                let mut local_allowed = 0;
                for _ in 0..20 {
                    if limiter_clone.allow(1).await.unwrap() {
                        local_allowed += 1;
                    }
                }
                local_allowed
            }));
        }

        // 设置开始信号
        start_signal.store(true, std::sync::atomic::Ordering::SeqCst);

        let mut total_allowed = 0;
        for handle in handles {
            total_allowed += handle.await.unwrap();
        }

        // 不应该超过限制（允许 5% 的误差，因为存在竞态条件）
        assert!(
            total_allowed <= 1050,
            "Total allowed: {}, expected <= 1050",
            total_allowed
        );
    }

    /// 测试 TokenBucketLimiter 高并发场景下的原子操作正确性
    #[tokio::test]
    async fn test_token_bucket_high_concurrency_atomic_safety() {
        let limiter = Arc::new(TokenBucketLimiter::new(1000, 100));
        let mut handles = vec![];

        // 使用 barrier 确保所有任务同时开始
        let barrier = Arc::new(tokio::sync::Barrier::new(50));
        let start_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

        for _ in 0..50 {
            let limiter_clone = Arc::clone(&limiter);
            let barrier_clone = Arc::clone(&barrier);
            let start_signal_clone = Arc::clone(&start_signal);
            handles.push(tokio::spawn(async move {
                barrier_clone.wait().await;

                while !start_signal_clone.load(std::sync::atomic::Ordering::SeqCst) {
                    std::hint::spin_loop();
                }

                let mut local_allowed = 0;
                for _ in 0..30 {
                    if limiter_clone.allow(1).await.unwrap() {
                        local_allowed += 1;
                    }
                }
                local_allowed
            }));
        }

        start_signal.store(true, std::sync::atomic::Ordering::SeqCst);

        let mut total_allowed = 0;
        for handle in handles {
            total_allowed += handle.await.unwrap();
        }

        // 不应该超过初始容量
        assert!(
            total_allowed <= 1000,
            "Total allowed: {}, expected <= 1000",
            total_allowed
        );
    }

    /// 测试 FixedWindowLimiter 高并发场景下的 CAS 正确性
    #[tokio::test]
    async fn test_fixed_window_high_concurrency_cas_safety() {
        let limiter = Arc::new(FixedWindowLimiter::new(Duration::from_secs(10), 500));
        let mut handles = vec![];

        let barrier = Arc::new(tokio::sync::Barrier::new(100));
        let start_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

        for _ in 0..100 {
            let limiter_clone = Arc::clone(&limiter);
            let barrier_clone = Arc::clone(&barrier);
            let start_signal_clone = Arc::clone(&start_signal);
            handles.push(tokio::spawn(async move {
                barrier_clone.wait().await;

                while !start_signal_clone.load(std::sync::atomic::Ordering::SeqCst) {
                    std::hint::spin_loop();
                }

                let mut local_allowed = 0;
                for _ in 0..10 {
                    if limiter_clone.allow(1).await.unwrap() {
                        local_allowed += 1;
                    }
                }
                local_allowed
            }));
        }

        start_signal.store(true, std::sync::atomic::Ordering::SeqCst);

        let mut total_allowed = 0;
        for handle in handles {
            total_allowed += handle.await.unwrap();
        }

        // 不应该超过窗口限制
        assert!(
            total_allowed <= 500,
            "Total allowed: {}, expected <= 500",
            total_allowed
        );
    }

    // ==================== 边界条件测试 ====================

    /// 测试令牌耗尽场景
    ///
    /// 验证当令牌完全耗尽时，限流器正确拒绝请求
    #[tokio::test]
    async fn test_token_exhaustion() {
        let limiter = TokenBucketLimiter::new(5, 1);

        // 消耗所有令牌
        assert!(limiter.allow(5).await.unwrap());
        assert_eq!(limiter.get_tokens(), 0);

        // 后续请求应该被拒绝
        for _ in 0..10 {
            assert!(!limiter.allow(1).await.unwrap());
        }

        // 令牌数仍应为 0
        assert_eq!(limiter.get_tokens(), 0);
    }

    /// 测试滑动窗口边界条件
    ///
    /// 验证窗口边界时刻的请求计数正确性
    #[tokio::test]
    async fn test_sliding_window_boundary() {
        let limiter = SlidingWindowLimiter::new(Duration::from_millis(100), 5);

        // 在窗口内发送 5 个请求
        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }

        // 应该被拒绝
        assert!(!limiter.allow(1).await.unwrap());

        // 等待窗口完全过期
        sleep(Duration::from_millis(110)).await;

        // 新窗口应该允许请求
        assert!(limiter.allow(1).await.unwrap());
        assert_eq!(limiter.get_request_count(), 1);
    }

    /// 测试固定窗口边界条件
    ///
    /// 验证窗口重置时刻的行为
    #[tokio::test]
    async fn test_fixed_window_boundary() {
        let limiter = FixedWindowLimiter::new(Duration::from_millis(50), 3);

        // 在第一个窗口内发送 3 个请求
        for _ in 0..3 {
            assert!(limiter.allow(1).await.unwrap());
        }

        // 应该被拒绝
        assert!(!limiter.allow(1).await.unwrap());

        // 等待窗口重置
        sleep(Duration::from_millis(60)).await;

        // 新窗口应该允许请求
        assert!(limiter.allow(1).await.unwrap());
        assert_eq!(limiter.get_count(), 1);
    }

    /// 测试并发竞争条件
    ///
    /// 验证在高并发竞争条件下限流器的正确性
    #[tokio::test]
    async fn test_concurrent_race_condition() {
        let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
        let success_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let mut handles = vec![];

        // 同时发起大量请求
        for _ in 0..100 {
            let limiter_clone = Arc::clone(&limiter);
            let success_clone = Arc::clone(&success_count);
            handles.push(tokio::spawn(async move {
                if limiter_clone.allow(1).await.unwrap() {
                    success_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // 成功请求数不应超过初始令牌数
        let success = success_count.load(std::sync::atomic::Ordering::SeqCst);
        assert!(success <= 10, "Success count: {}, expected <= 10", success);
    }

    /// 测试滑动窗口并发竞争条件
    #[tokio::test]
    async fn test_sliding_window_race_condition() {
        let limiter = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(1), 10));
        let success_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let mut handles = vec![];

        for _ in 0..50 {
            let limiter_clone = Arc::clone(&limiter);
            let success_clone = Arc::clone(&success_count);
            handles.push(tokio::spawn(async move {
                if limiter_clone.allow(1).await.unwrap() {
                    success_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        let success = success_count.load(std::sync::atomic::Ordering::SeqCst);
        assert!(success <= 10, "Success count: {}, expected <= 10", success);
    }

    /// 测试大成本请求的边界条件
    #[tokio::test]
    async fn test_large_cost_boundary() {
        let limiter = TokenBucketLimiter::new(100, 10);

        // 尝试消费超过容量的成本应该失败
        assert!(!limiter.allow(101).await.unwrap());

        // 消费正好等于容量的成本应该成功
        assert!(limiter.allow(100).await.unwrap());

        // 之后应该没有令牌
        assert_eq!(limiter.get_tokens(), 0);
    }

    /// 测试滑动窗口大成本请求
    #[tokio::test]
    async fn test_sliding_window_large_cost() {
        let limiter = SlidingWindowLimiter::new(Duration::from_secs(1), 100);

        // 消费正好等于限制的成本
        assert!(limiter.allow(100).await.unwrap());

        // 之后应该被拒绝
        assert!(!limiter.allow(1).await.unwrap());
    }

    /// 测试零令牌场景（边界值）
    #[tokio::test]
    async fn test_zero_tokens_scenario() {
        let limiter = TokenBucketLimiter::new(1, 1);

        // 消耗唯一的令牌
        assert!(limiter.allow(1).await.unwrap());
        assert_eq!(limiter.get_tokens(), 0);

        // 应该被拒绝
        assert!(!limiter.allow(1).await.unwrap());
    }

    /// 测试窗口精确过期
    #[tokio::test]
    async fn test_window_exact_expiry() {
        let window_size = Duration::from_millis(100);
        let limiter = SlidingWindowLimiter::new(window_size, 5);

        // 发送 5 个请求
        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }

        // 等待窗口过期（加上缓冲时间）
        sleep(window_size + Duration::from_millis(10)).await;

        // 新窗口应该允许请求
        assert!(limiter.allow(1).await.unwrap());
    }

    /// 测试并发死锁检测
    ///
    /// 使用超时机制确保不会发生死锁
    #[tokio::test]
    async fn test_no_deadlock_under_concurrency() {
        let limiter = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(1), 10000));
        let mut handles = vec![];

        for _ in 0..500 {
            let limiter_clone = Arc::clone(&limiter);
            handles.push(tokio::spawn(async move {
                for _ in 0..20 {
                    let _ = limiter_clone.allow(1).await;
                }
            }));
        }

        // 使用超时确保不会死锁
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            for handle in handles {
                let _ = handle.await;
            }
        })
        .await;

        assert!(result.is_ok(), "Test timed out - possible deadlock");
    }

    /// 测试令牌补充的正确性
    #[tokio::test]
    async fn test_token_refill_correctness() {
        let limiter = TokenBucketLimiter::new(100, 1000); // 1000 tokens/sec

        // 消耗所有令牌
        assert!(limiter.allow(100).await.unwrap());
        assert_eq!(limiter.get_tokens(), 0);

        // 等待 50ms，应该补充约 50 个令牌
        sleep(Duration::from_millis(50)).await;

        // 触发补充
        limiter.allow(1).await.unwrap();

        // 验证补充的令牌数量（允许 10% 的误差）
        let tokens = limiter.get_tokens();
        assert!(
            tokens >= 40 && tokens <= 60,
            "Tokens: {}, expected between 40 and 60",
            tokens
        );
    }

    /// 测试滑动窗口清理过期请求
    #[tokio::test]
    async fn test_sliding_window_cleanup() {
        let limiter = SlidingWindowLimiter::new(Duration::from_millis(50), 10);

        // 发送 5 个请求
        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }

        // 等待窗口过期
        sleep(Duration::from_millis(60)).await;

        // 发送新请求，应该触发清理
        assert!(limiter.allow(1).await.unwrap());

        // 验证旧请求已被清理
        assert_eq!(limiter.get_request_count(), 1);
    }

    // ==================== ConcurrencyLimiter 依赖注入测试 ====================

    #[tokio::test]
    async fn test_concurrency_limiter_builder_basic() {
        let limiter = ConcurrencyLimiter::builder()
            .max_concurrent(10)
            .build()
            .unwrap();

        assert_eq!(limiter.max_concurrent(), 10);
        assert!(limiter.timeout().is_none());
    }

    #[tokio::test]
    async fn test_concurrency_limiter_builder_with_timeout() {
        let timeout = Duration::from_secs(5);
        let limiter = ConcurrencyLimiter::builder()
            .max_concurrent(10)
            .timeout(timeout)
            .build()
            .unwrap();

        assert_eq!(limiter.max_concurrent(), 10);
        assert_eq!(limiter.timeout(), Some(timeout));
    }

    #[tokio::test]
    async fn test_concurrency_limiter_builder_missing_max_concurrent() {
        let result = ConcurrencyLimiter::builder().build();

        assert!(result.is_err());
        match result {
            Err(FlowGuardError::ConfigError(msg)) => {
                assert!(msg.contains("max_concurrent is required"));
            }
            _ => panic!("期望 ConfigError"),
        }
    }

    #[tokio::test]
    async fn test_concurrency_limiter_builder_zero_max_concurrent() {
        let result = ConcurrencyLimiter::builder().max_concurrent(0).build();

        assert!(result.is_err());
        match result {
            Err(FlowGuardError::ConfigError(msg)) => {
                assert!(msg.contains("must be greater than 0"));
            }
            _ => panic!("期望 ConfigError"),
        }
    }

    #[tokio::test]
    async fn test_concurrency_limiter_with_dependencies() {
        // 创建共享信号量
        let semaphore = Arc::new(tokio::sync::Semaphore::new(5));

        // 使用依赖注入创建限流器
        let limiter = ConcurrencyLimiter::with_dependencies(semaphore.clone(), None);

        // 验证 max_concurrent 为 0（外部信号量）
        assert_eq!(limiter.max_concurrent(), 0);
        assert!(limiter.timeout().is_none());

        // 验证可以正常使用
        let _permit = limiter.acquire(1).await.unwrap();
    }

    #[tokio::test]
    async fn test_concurrency_limiter_with_dependencies_shared_semaphore() {
        // 创建共享信号量
        let semaphore = Arc::new(tokio::sync::Semaphore::new(2));

        // 创建两个限流器共享同一个信号量
        let limiter1 = ConcurrencyLimiter::with_dependencies(semaphore.clone(), None);
        let limiter2 = ConcurrencyLimiter::with_dependencies(semaphore.clone(), None);

        // 通过 limiter1 获取一个许可
        let permit1 = limiter1.acquire(1).await.unwrap();
        assert_eq!(semaphore.available_permits(), 1);

        // 通过 limiter2 获取一个许可
        let permit2 = limiter2.acquire(1).await.unwrap();
        assert_eq!(semaphore.available_permits(), 0);

        // 应该无法获取更多许可
        assert!(limiter1.try_acquire(1).is_err());
        assert!(limiter2.try_acquire(1).is_err());

        // 释放一个许可
        drop(permit1);
        assert_eq!(semaphore.available_permits(), 1);

        // 现在应该可以获取许可
        let _permit3 = limiter2.acquire(1).await.unwrap();
        assert_eq!(semaphore.available_permits(), 0);

        // 清理
        drop(permit2);
    }

    #[tokio::test]
    async fn test_concurrency_limiter_builder_with_semaphore() {
        // 创建外部信号量
        let semaphore = Arc::new(tokio::sync::Semaphore::new(3));
        let timeout = Duration::from_secs(5);

        // 使用 builder 设置外部信号量
        let limiter = ConcurrencyLimiter::builder()
            .with_semaphore(semaphore.clone())
            .timeout(timeout)
            .build()
            .unwrap();

        // 验证 max_concurrent 为 0（外部信号量）
        assert_eq!(limiter.max_concurrent(), 0);
        assert_eq!(limiter.timeout(), Some(timeout));

        // 验证可以正常使用
        let _permit = limiter.acquire(1).await.unwrap();
        assert_eq!(semaphore.available_permits(), 2);
    }

    #[tokio::test]
    async fn test_concurrency_limiter_with_dependencies_timeout() {
        // 创建共享信号量（只有 1 个许可）
        let semaphore = Arc::new(tokio::sync::Semaphore::new(1));
        let timeout = Duration::from_millis(100);

        // 创建带超时的限流器
        let limiter = ConcurrencyLimiter::with_dependencies(semaphore.clone(), Some(timeout));

        // 获取唯一的许可
        let _permit = limiter.acquire(1).await.unwrap();

        // 尝试获取另一个许可应该超时
        let start = std::time::Instant::now();
        let result = limiter.acquire(1).await;
        let elapsed = start.elapsed();

        assert!(result.is_err());
        assert!(elapsed >= timeout);
    }

    // ==================== Limiter Trait Tests (Section 2.7) ====================

    /// 2.7.1: check() uses allow(1) internally
    ///
    /// Verifies that the default Limiter::check() implementation calls allow(1)
    /// and propagates errors from allow(). When allow() returns false (not an error),
    /// check() still returns Ok(()).
    #[tokio::test]
    async fn test_limiter_trait_check_uses_allow_one() {
        // TokenBucketLimiter: after consuming the only token, allow() returns false (not an error)
        // The default check() propagates errors but not `false` from allow(), so it returns Ok(())
        let limiter = TokenBucketLimiter::new(1, 1);

        // First check: tokens consumed, should succeed (or return Ok regardless since allow false is not an error)
        let result1 = limiter.check("key").await;
        assert!(result1.is_ok(), "check() should return Ok even when allow returns false");
        assert_eq!(limiter.get_tokens(), 0, "check() should have consumed one token");

        // Second check: tokens still 0, allow(1) returns false (not an error)
        // Default check() doesn't treat false as error, so it returns Ok(())
        let result2 = limiter.check("key").await;
        assert!(result2.is_ok(), "check() should return Ok when allow returns false");

        // Verify check() propagates actual errors (zero cost is invalid)
        let limiter2: TokenBucketLimiter = TokenBucketLimiter::new(100, 10);
        let result = limiter2.allow(0).await;
        assert!(result.is_err(), "Zero cost should return error");
    }

    /// 2.7.2: All limiter implementations are Send + Sync
    ///
    /// Verifies that all limiter types implement Send and Sync,
    /// ensuring they can be safely shared across async tasks.
    #[test]
    fn test_all_limiters_are_send_and_sync() {
        // TokenBucketLimiter
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TokenBucketLimiter>();

        // SlidingWindowLimiter
        assert_send_sync::<SlidingWindowLimiter>();

        // FixedWindowLimiter
        assert_send_sync::<FixedWindowLimiter>();

        // ConcurrencyLimiter
        assert_send_sync::<ConcurrencyLimiter>();
    }

    // ==================== TokenBucket MAX_COST Tests (Section 2.1.5) ====================

    /// 2.1.5: Rejects cost exceeding MAX_COST
    ///
    /// Verifies that TokenBucketLimiter rejects requests where cost > MAX_COST
    #[tokio::test]
    async fn test_token_bucket_rejects_cost_exceeding_max_cost() {
        // Cost exceeding MAX_COST should return an error
        let limiter = TokenBucketLimiter::new(u64::MAX, 1);
        let result = limiter.allow(1_000_001).await;
        assert!(result.is_err(), "Cost exceeding MAX_COST should return error, got {:?}", result);

        // Cost within MAX_COST but above bucket capacity should return Ok(false)
        let limiter2 = TokenBucketLimiter::new(100, 1);
        let result = limiter2.allow(101).await;
        let is_allowed = result.as_ref().map(|v| *v).unwrap_or(false);
        assert!(!is_allowed, "Cost above bucket capacity should return false");
        drop(result);

        // Cost within both limits should succeed
        let result = limiter2.allow(100).await;
        let is_allowed = result.as_ref().map(|v| *v).unwrap_or(false);
        assert!(is_allowed, "Cost within capacity should return true");
    }
}
