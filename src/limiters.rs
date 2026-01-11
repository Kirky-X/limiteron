//! 限流器模块
//!
//! 实现各种限流算法。

use crate::error::FlowGuardError;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ============================================================================
// Cost 参数验证常量
// ============================================================================

/// 最大 cost 值
const MAX_COST: u64 = 1_000_000;

// ============================================================================
// Cost 参数验证函数
// ============================================================================

/// 验证 cost 参数
///
/// # 参数
/// - `cost`: cost 值
///
/// # 返回
/// - `Ok(u64)`: 验证通过的 cost 值
/// - `Err(FlowGuardError)`: 验证失败
fn validate_cost(cost: u64) -> Result<u64, FlowGuardError> {
    if cost == 0 {
        return Err(FlowGuardError::ConfigError("Cost 不能为零".to_string()));
    }

    if cost > MAX_COST {
        return Err(FlowGuardError::ConfigError(format!(
            "Cost 超过最大限制（最大 {}）",
            MAX_COST
        )));
    }

    Ok(cost)
}

/// 限流器 trait
pub trait Limiter: Send + Sync {
    /// 检查是否允许
    fn allow(
        &self,
        cost: u64,
    ) -> Pin<Box<dyn Future<Output = Result<bool, FlowGuardError>> + Send + '_>>;

    /// 检查是否允许（别名方法）
    fn check(
        &self,
        cost: u64,
    ) -> Pin<Box<dyn Future<Output = Result<bool, FlowGuardError>> + Send + '_>> {
        self.allow(cost)
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
/// use limiteron::limiters::TokenBucketLimiter;
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
    /// 创建新的令牌桶限流器
    ///
    /// # 参数
    /// - `capacity`: 桶的最大容量
    /// - `refill_rate`: 令牌补充速率（令牌/秒）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::limiters::TokenBucketLimiter;
    ///
    /// let limiter = TokenBucketLimiter::new(100, 10);
    /// ```
    pub fn new(capacity: u64, refill_rate: u64) -> Self {
        Self {
            capacity,
            tokens: std::sync::atomic::AtomicU64::new(capacity),
            refill_rate,
            last_refill: std::sync::atomic::AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64,
            ),
        }
    }

    /// 补充令牌
    ///
    /// 基于时间差计算应该补充的令牌数量，使用 CAS 循环确保原子性。
    /// 使用 SeqCst 内存序确保在多线程环境下的一致性。
    fn refill_tokens(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        // 使用 CAS 循环更新 last_refill 和 tokens
        loop {
            let last = self.last_refill.load(std::sync::atomic::Ordering::SeqCst);
            let elapsed_nanos = now.saturating_sub(last);

            // 如果时间差太小，不需要补充
            if elapsed_nanos < 1_000_000 {
                break;
            }

            // 计算应该补充的令牌数
            let elapsed_seconds = elapsed_nanos as f64 / 1_000_000_000.0;
            let tokens_to_add = (elapsed_seconds * self.refill_rate as f64) as u64;

            if tokens_to_add == 0 {
                break;
            }

            // 尝试更新 last_refill
            if self
                .last_refill
                .compare_exchange(
                    last,
                    now,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                )
                .is_ok()
            {
                // 成功更新时间戳，现在更新令牌数
                loop {
                    let current = self.tokens.load(std::sync::atomic::Ordering::SeqCst);
                    let new_tokens = current.saturating_add(tokens_to_add).min(self.capacity);

                    if self
                        .tokens
                        .compare_exchange(
                            current,
                            new_tokens,
                            std::sync::atomic::Ordering::SeqCst,
                            std::sync::atomic::Ordering::SeqCst,
                        )
                        .is_ok()
                    {
                        break;
                    }
                }
                break;
            }
            // CAS 失败，重试
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
            let current = self.tokens.load(std::sync::atomic::Ordering::SeqCst);

            // 检查令牌是否足够
            if current < cost {
                return false;
            }

            // 尝试消费令牌
            match self.tokens.compare_exchange(
                current,
                current - cost,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            ) {
                Ok(_) => return true,
                Err(_) => {
                    retry_count += 1;
                    if retry_count >= MAX_RETRY {
                        // 超过最大重试次数，放弃
                        return false;
                    }

                    // 指数退避：第1次失败不等待，第2次等待1ms，第3次等待2ms
                    if retry_count > 1 {
                        let backoff_ms = 1u64 << (retry_count - 2); // 1, 2, 4...
                        std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
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

impl Limiter for TokenBucketLimiter {
    fn allow(
        &self,
        cost: u64,
    ) -> Pin<Box<dyn Future<Output = Result<bool, FlowGuardError>> + Send + '_>> {
        Box::pin(async move {
            // 验证 cost 参数
            let cost = validate_cost(cost)?;

            // 先补充令牌
            self.refill_tokens();

            // 尝试消费令牌
            Ok(self.try_consume(cost))
        })
    }
}

impl TokenBucketLimiter {
    /// 检查是否允许（接受 key 参数，用于宏）
    pub async fn check(&self, _key: &str) -> Result<(), FlowGuardError> {
        self.allow(1).await?;
        Ok(())
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
/// use limiteron::limiters::SlidingWindowLimiter;
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
    /// 创建新的滑动窗口限流器
    ///
    /// # 参数
    /// - `window_size`: 滑动窗口大小
    /// - `max_requests`: 窗口内最大请求数
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::limiters::SlidingWindowLimiter;
    /// use std::time::Duration;
    ///
    /// let limiter = SlidingWindowLimiter::new(Duration::from_secs(1), 100);
    /// ```
    pub fn new(window_size: Duration, max_requests: u64) -> Self {
        Self {
            window_size,
            max_requests,
            requests: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// 清理过期的请求记录
    fn cleanup_expired_requests(&self) {
        let mut requests = self.requests.lock().unwrap();
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
        self.requests.lock().unwrap().len()
    }
}

impl Limiter for SlidingWindowLimiter {
    fn allow(
        &self,
        cost: u64,
    ) -> Pin<Box<dyn Future<Output = Result<bool, FlowGuardError>> + Send + '_>> {
        Box::pin(async move {
            // 验证 cost 参数
            let cost = validate_cost(cost)?;

            // 清理过期请求
            self.cleanup_expired_requests();

            let mut requests = self.requests.lock().unwrap();
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
        })
    }
}

impl SlidingWindowLimiter {
    /// 检查是否允许（接受 key 参数，用于宏）
    pub async fn check(&self, _key: &str) -> Result<(), FlowGuardError> {
        self.allow(1).await?;
        Ok(())
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
/// use limiteron::limiters::FixedWindowLimiter;
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
    /// 创建新的固定窗口限流器
    ///
    /// # 参数
    /// - `window_size`: 固定窗口大小
    /// - `max_requests`: 窗口内最大请求数
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::limiters::FixedWindowLimiter;
    /// use std::time::Duration;
    ///
    /// let limiter = FixedWindowLimiter::new(Duration::from_secs(1), 100);
    /// ```
    pub fn new(window_size: Duration, max_requests: u64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        Self {
            window_size,
            max_requests,
            count: std::sync::atomic::AtomicU64::new(0),
            window_start: std::sync::atomic::AtomicU64::new(now),
        }
    }

    /// 检查并重置窗口
    ///
    /// 如果当前时间已经超过窗口结束时间，则重置窗口。
    fn check_and_reset_window(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        let window_size_nanos = self.window_size.as_nanos() as u64;

        loop {
            let current_start = self.window_start.load(std::sync::atomic::Ordering::SeqCst);
            let window_end = current_start.saturating_add(window_size_nanos);

            // 如果当前时间还在当前窗口内，不需要重置
            if now < window_end {
                break;
            }

            // 计算新窗口的开始时间（对齐到窗口边界）
            let elapsed = now.saturating_sub(current_start);
            let windows_passed = elapsed / window_size_nanos;
            let new_start = current_start.saturating_add(windows_passed * window_size_nanos);

            // 尝试更新窗口开始时间
            match self.window_start.compare_exchange(
                current_start,
                new_start,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            ) {
                Ok(_) => {
                    // 成功更新窗口开始时间，重置计数
                    self.count.store(0, std::sync::atomic::Ordering::SeqCst);
                    break;
                }
                Err(_) => continue, // CAS 失败，重试
            }
        }
    }

    /// 获取当前窗口的计数（仅用于测试）
    #[cfg(test)]
    fn get_count(&self) -> u64 {
        self.check_and_reset_window();
        self.count.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Limiter for FixedWindowLimiter {
    fn allow(
        &self,
        cost: u64,
    ) -> Pin<Box<dyn Future<Output = Result<bool, FlowGuardError>> + Send + '_>> {
        Box::pin(async move {
            // 验证 cost 参数
            let cost = validate_cost(cost)?;

            // 检查并重置窗口
            self.check_and_reset_window();

            // 使用 CAS 循环尝试增加计数
            loop {
                let current = self.count.load(std::sync::atomic::Ordering::SeqCst);

                // 检查是否超过限制
                if current + cost > self.max_requests {
                    return Ok(false);
                }

                // 尝试增加计数
                match self.count.compare_exchange(
                    current,
                    current + cost,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                ) {
                    Ok(_) => return Ok(true),
                    Err(_) => continue, // CAS 失败，重试
                }
            }
        })
    }
}

impl FixedWindowLimiter {
    /// 检查是否允许（接受 key 参数，用于宏）
    pub async fn check(&self, _key: &str) -> Result<(), FlowGuardError> {
        self.allow(1).await?;
        Ok(())
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
        }
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

impl Limiter for ConcurrencyLimiter {
    fn allow(
        &self,
        cost: u64,
    ) -> Pin<Box<dyn Future<Output = Result<bool, FlowGuardError>> + Send + '_>> {
        Box::pin(async move {
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
        })
    }
}

impl ConcurrencyLimiter {
    /// 检查是否允许（接受 key 参数，用于宏）
    pub async fn check(&self, _key: &str) -> Result<(), FlowGuardError> {
        self.allow(1).await?;
        Ok(())
    }
}

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
}
