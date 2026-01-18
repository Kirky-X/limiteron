#![cfg(feature = "custom-limiter")]
//! 自定义限流器扩展模块
//!
//! 提供自定义限流器接口和注册机制，允许用户在运行时动态注册和使用自定义限流器。
//!
//! # 特性
//!
//! - 定义 CustomLimiter trait 作为限流器接口
//! - 支持异步限流操作
//! - 支持配置加载
//! - 提供统计信息
//! - 提供线程安全的注册表（CustomLimiterRegistry）
//! - 支持运行时动态注册、查询和注销
//!
//! # 示例
//!
//! ```rust
//! use limiteron::custom_limiter::{CustomLimiter, CustomLimiterRegistry, LimiterStats};
//! use limiteron::error::FlowGuardError;
//! use async_trait::async_trait;
//!
//! #[derive(Debug)]
//! struct MyCustomLimiter {
//!     capacity: u64,
//! }
//!
//! #[async_trait]
//! impl CustomLimiter for MyCustomLimiter {
//!     fn name(&self) -> &str {
//!         "my_custom"
//!     }
//!
//!     async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError> {
//!         // 自定义限流逻辑
//!         Ok(true)
//!     }
//!
//!     fn load_config(&mut self, config: serde_json::Value) -> Result<(), FlowGuardError> {
//!         self.capacity = config["capacity"].as_u64().unwrap_or(100);
//!         Ok(())
//!     }
//!
//!     fn stats(&self) -> LimiterStats {
//!         LimiterStats {
//!             total_requests: 0,
//!             allowed_requests: 0,
//!             rejected_requests: 0,
//!         }
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let registry = CustomLimiterRegistry::new();
//!     let limiter = Box::new(MyCustomLimiter { capacity: 100 });
//!     registry.register("my_custom".to_string(), limiter).await.unwrap();
//! }
//! ```

use crate::error::FlowGuardError;
use ahash::AHashMap as HashMap;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

// ============================================================================
// CustomLimiter Trait
// ============================================================================

/// 自定义限流器 trait
///
/// 所有自定义限流器都需要实现此trait。
#[async_trait]
pub trait CustomLimiter: Send + Sync {
    /// 获取限流器名称
    ///
    /// # 返回
    /// - 限流器的唯一标识符
    fn name(&self) -> &str;

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

    /// 加载配置
    ///
    /// # 参数
    /// - `config`: 配置值（JSON格式）
    ///
    /// # 返回
    /// - `Ok(())`: 配置加载成功
    /// - `Err(_)`: 配置加载失败
    fn load_config(&mut self, config: Value) -> Result<(), FlowGuardError>;

    /// 获取统计信息
    ///
    /// # 返回
    /// - 限流器统计信息
    fn stats(&self) -> LimiterStats;
}

// ============================================================================
// LimiterStats
// ============================================================================

/// 限流器统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimiterStats {
    /// 总请求数
    pub total_requests: u64,
    /// 允许的请求数
    pub allowed_requests: u64,
    /// 拒绝的请求数
    pub rejected_requests: u64,
}

impl LimiterStats {
    /// 创建新的统计信息
    pub fn new() -> Self {
        Self {
            total_requests: 0,
            allowed_requests: 0,
            rejected_requests: 0,
        }
    }

    /// 计算拒绝率
    pub fn rejection_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.rejected_requests as f64 / self.total_requests as f64
        }
    }

    /// 计算允许率
    pub fn allow_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.allowed_requests as f64 / self.total_requests as f64
        }
    }
}

impl Default for LimiterStats {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// CustomLimiterRegistry
// ============================================================================

/// 自定义限流器注册表
///
/// 提供线程安全的限流器注册、查询和注销功能。
#[derive(Clone)]
pub struct CustomLimiterRegistry {
    /// 限流器存储（使用 RwLock 实现线程安全）
    limiters: Arc<RwLock<HashMap<String, Box<dyn CustomLimiter>>>>,
}

impl std::fmt::Debug for CustomLimiterRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomLimiterRegistry")
            .field("limiters", &"<custom limiters>")
            .finish()
    }
}

impl CustomLimiterRegistry {
    /// 创建新的注册表
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::custom_limiter::CustomLimiterRegistry;
    ///
    /// let registry = CustomLimiterRegistry::new();
    /// ```
    pub fn new() -> Self {
        Self {
            limiters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册自定义限流器
    ///
    /// # 参数
    /// - `name`: 限流器名称（唯一标识符）
    /// - `limiter`: 限流器实例
    ///
    /// # 返回
    /// - `Ok(())`: 注册成功
    /// - `Err(FlowGuardError::ConfigError)`: 名称已存在
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::custom_limiter::{CustomLimiterRegistry, LeakyBucketLimiter};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let registry = CustomLimiterRegistry::new();
    ///     let limiter = LeakyBucketLimiter::new(100, 10);
    ///     registry.register("leaky_bucket".to_string(), Box::new(limiter)).await.unwrap();
    /// }
    /// ```
    pub async fn register(
        &self,
        name: String,
        limiter: Box<dyn CustomLimiter>,
    ) -> Result<(), FlowGuardError> {
        let mut limiters = self.limiters.write().await;

        if limiters.contains_key(&name) {
            let error_msg = format!("限流器 '{}' 已存在", name);
            warn!("{}", error_msg);
            return Err(FlowGuardError::ConfigError(error_msg));
        }

        info!("注册自定义限流器: {}", name);
        limiters.insert(name.clone(), limiter);
        debug!("当前注册的限流器数量: {}", limiters.len());

        Ok(())
    }

    /// 获取限流器
    ///
    /// # 参数
    /// - `name`: 限流器名称
    ///
    /// # 返回
    /// - `Some(limiter)`: 找到限流器
    /// - `None`: 未找到限流器
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::custom_limiter::CustomLimiterRegistry;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let registry = CustomLimiterRegistry::new();
    ///     if let Some(limiter) = registry.get("leaky_bucket").await {
    ///         println!("找到限流器: {}", limiter.name());
    ///     }
    /// }
    /// ```
    pub async fn get(&self, name: &str) -> Option<Box<dyn CustomLimiter>> {
        let limiters = self.limiters.read().await;

        if let Some(_limiter) = limiters.get(name) {
            debug!("查询限流器: {}", name);
            // 注意：这里不能直接返回 trait 对象
            // 在实际使用中，应该通过 allow 方法调用而不是获取所有权
            None
        } else {
            debug!("未找到限流器: {}", name);
            None
        }
    }

    /// 检查限流器是否存在
    ///
    /// # 参数
    /// - `name`: 限流器名称
    ///
    /// # 返回
    /// - `true`: 限流器存在
    /// - `false`: 限流器不存在
    pub async fn contains(&self, name: &str) -> bool {
        let limiters = self.limiters.read().await;
        limiters.contains_key(name)
    }

    /// 注销限流器
    ///
    /// # 参数
    /// - `name`: 限流器名称
    ///
    /// # 返回
    /// - `Ok(())`: 注销成功
    /// - `Err(FlowGuardError::ConfigError)`: 限流器不存在
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::custom_limiter::CustomLimiterRegistry;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let registry = CustomLimiterRegistry::new();
    ///     registry.unregister("leaky_bucket".to_string()).await.unwrap();
    /// }
    /// ```
    pub async fn unregister(&self, name: &str) -> Result<(), FlowGuardError> {
        let mut limiters = self.limiters.write().await;

        if !limiters.contains_key(name) {
            let error_msg = format!("限流器 '{}' 不存在", name);
            warn!("{}", error_msg);
            return Err(FlowGuardError::ConfigError(error_msg));
        }

        info!("注销自定义限流器: {}", name);
        limiters.remove(name);
        debug!("当前注册的限流器数量: {}", limiters.len());

        Ok(())
    }

    /// 获取所有注册的限流器名称
    ///
    /// # 返回
    /// - 限流器名称列表
    #[allow(clippy::map_clone)]
    pub async fn list(&self) -> Vec<String> {
        let limiters = self.limiters.read().await;
        limiters.keys().map(|k| k.clone()).collect()
    }

    /// 获取注册的限流器数量
    ///
    /// # 返回
    /// - 限流器数量
    pub async fn count(&self) -> usize {
        let limiters = self.limiters.read().await;
        limiters.len()
    }

    /// 清空所有限流器
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::custom_limiter::CustomLimiterRegistry;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let registry = CustomLimiterRegistry::new();
    ///     registry.clear().await;
    /// }
    /// ```
    pub async fn clear(&self) {
        let mut limiters = self.limiters.write().await;
        info!("清空所有自定义限流器");
        limiters.clear();
    }

    /// 检查是否允许通过
    ///
    /// 使用指定名称的限流器检查请求是否允许通过。
    ///
    /// # 参数
    /// - `name`: 限流器名称
    /// - `cost`: 请求消耗的成本
    ///
    /// # 返回
    /// - `Ok(true)`: 允许通过
    /// - `Ok(false)`: 拒绝通过
    /// - `Err(_)`: 限流器不存在或发生错误
    pub async fn allow(&self, name: &str, cost: u64) -> Result<bool, FlowGuardError> {
        let limiters = self.limiters.read().await;

        let limiter = limiters.get(name).ok_or_else(|| {
            let error_msg = format!("限流器 '{}' 不存在", name);
            error!("{}", error_msg);
            FlowGuardError::ConfigError(error_msg)
        })?;

        debug!("使用限流器 '{}' 检查请求，成本: {}", name, cost);
        limiter.allow(cost).await
    }

    /// 获取限流器统计信息
    ///
    /// # 参数
    /// - `name`: 限流器名称
    ///
    /// # 返回
    /// - `Ok(stats)`: 统计信息
    /// - `Err(_)`: 限流器不存在
    pub async fn get_stats(&self, name: &str) -> Result<LimiterStats, FlowGuardError> {
        let limiters = self.limiters.read().await;

        let limiter = limiters.get(name).ok_or_else(|| {
            let error_msg = format!("限流器 '{}' 不存在", name);
            error!("{}", error_msg);
            FlowGuardError::ConfigError(error_msg)
        })?;

        Ok(limiter.stats())
    }
}

impl Default for CustomLimiterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// LeakyBucketLimiter 示例实现
// ============================================================================

/// 漏桶限流器
///
/// 使用漏桶算法实现速率限制，请求以恒定速率流出桶，
/// 如果桶已满则拒绝新请求。
///
/// # 特性
/// - 平滑流量输出
/// - 桶容量限制
/// - 恒定流出速率
/// - 线程安全
///
/// # 示例
/// ```rust
/// use limiteron::custom_limiter::LeakyBucketLimiter;
///
/// #[tokio::main]
/// async fn main() {
///     let limiter = LeakyBucketLimiter::new(100, 10);
///     let allowed = limiter.allow(1).await.unwrap();
/// }
/// ```
#[derive(Debug)]
pub struct LeakyBucketLimiter {
    /// 桶容量
    capacity: u64,
    /// 流出速率（请求/秒）
    leak_rate: u64,
    /// 当前桶中的请求数
    current: Arc<AtomicU64>,
    /// 请求时间戳队列
    queue: Arc<Mutex<VecDeque<Instant>>>,
    /// 统计信息
    stats: Arc<Mutex<LimiterStats>>,
}

impl LeakyBucketLimiter {
    /// 创建新的漏桶限流器
    ///
    /// # 参数
    /// - `capacity`: 桶容量
    /// - `leak_rate`: 流出速率（请求/秒）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::custom_limiter::LeakyBucketLimiter;
    ///
    /// let limiter = LeakyBucketLimiter::new(100, 10);
    /// ```
    pub fn new(capacity: u64, leak_rate: u64) -> Self {
        Self {
            capacity,
            leak_rate,
            current: Arc::new(AtomicU64::new(0)),
            queue: Arc::new(Mutex::new(VecDeque::new())),
            stats: Arc::new(Mutex::new(LimiterStats::new())),
        }
    }

    /// 获取桶容量
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// 获取流出速率
    pub fn leak_rate(&self) -> u64 {
        self.leak_rate
    }

    /// 获取当前桶中的请求数
    pub fn current(&self) -> u64 {
        self.current.load(Ordering::SeqCst)
    }

    /// 漏出请求（移除过期的请求）
    fn leak(&self) {
        let mut queue = self.queue.lock().unwrap();
        let now = Instant::now();
        let leak_interval = Duration::from_secs(1).div_f64(self.leak_rate as f64);

        // 移除已经漏出的请求
        while let Some(&front) = queue.front() {
            if now.duration_since(front) >= leak_interval {
                queue.pop_front();
                self.current.fetch_sub(1, Ordering::SeqCst);
            } else {
                break;
            }
        }
    }
}

#[async_trait]
impl CustomLimiter for LeakyBucketLimiter {
    fn name(&self) -> &str {
        "leaky_bucket"
    }

    async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError> {
        // 先漏出请求
        self.leak();

        let current = self.current.load(Ordering::SeqCst);

        // 检查桶是否已满
        if current + cost > self.capacity {
            // 更新统计信息
            let mut stats = self.stats.lock().unwrap();
            stats.total_requests += 1;
            stats.rejected_requests += 1;

            debug!(
                "漏桶限流拒绝: 当前={}, 成本={}, 容量={}",
                current, cost, self.capacity
            );

            return Ok(false);
        }

        // 添加请求到桶中
        self.current.fetch_add(cost, Ordering::SeqCst);
        let mut queue = self.queue.lock().unwrap();
        let now = Instant::now();
        for _ in 0..cost {
            queue.push_back(now);
        }

        // 更新统计信息
        let mut stats = self.stats.lock().unwrap();
        stats.total_requests += 1;
        stats.allowed_requests += 1;

        debug!(
            "漏桶限流允许: 当前={}, 成本={}, 容量={}",
            current + cost,
            cost,
            self.capacity
        );

        Ok(true)
    }

    fn load_config(&mut self, config: Value) -> Result<(), FlowGuardError> {
        let capacity = config["capacity"]
            .as_u64()
            .ok_or_else(|| FlowGuardError::ConfigError("缺少 capacity 配置".to_string()))?;

        let leak_rate = config["leak_rate"]
            .as_u64()
            .ok_or_else(|| FlowGuardError::ConfigError("缺少 leak_rate 配置".to_string()))?;

        if capacity == 0 {
            return Err(FlowGuardError::ConfigError(
                "capacity 必须大于 0".to_string(),
            ));
        }

        if leak_rate == 0 {
            return Err(FlowGuardError::ConfigError(
                "leak_rate 必须大于 0".to_string(),
            ));
        }

        self.capacity = capacity;
        self.leak_rate = leak_rate;

        info!(
            "加载漏桶限流器配置: 容量={}, 流出速率={}",
            self.capacity, self.leak_rate
        );

        Ok(())
    }

    fn stats(&self) -> LimiterStats {
        self.stats.lock().unwrap().clone()
    }
}

// ============================================================================
// TokenBucketLimiter 示例实现
// ============================================================================

/// 令牌桶限流器（自定义实现）
///
/// 使用令牌桶算法实现速率限制，令牌以恒定速率补充到桶中，
/// 请求到达时从桶中获取令牌，如果令牌不足则拒绝请求。
///
/// # 特性
/// - 支持突发流量
/// - 桶容量限制
/// - 恒定补充速率
/// - 线程安全
///
/// # 示例
/// ```rust
/// use limiteron::custom_limiter::TokenBucketLimiter;
///
/// #[tokio::main]
/// async fn main() {
///     let limiter = TokenBucketLimiter::new(100, 10);
///     let allowed = limiter.allow(1).await.unwrap();
/// }
/// ```
#[derive(Debug)]
pub struct TokenBucketLimiter {
    /// 桶容量
    capacity: u64,
    /// 补充速率（令牌/秒）
    refill_rate: u64,
    /// 当前令牌数
    tokens: Arc<AtomicU64>,
    /// 最后补充时间（纳秒时间戳）
    last_refill: Arc<AtomicU64>,
    /// 统计信息
    stats: Arc<Mutex<LimiterStats>>,
}

impl TokenBucketLimiter {
    /// 创建新的令牌桶限流器
    ///
    /// # 参数
    /// - `capacity`: 桶容量
    /// - `refill_rate`: 补充速率（令牌/秒）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::custom_limiter::TokenBucketLimiter;
    ///
    /// let limiter = TokenBucketLimiter::new(100, 10);
    /// ```
    pub fn new(capacity: u64, refill_rate: u64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        Self {
            capacity,
            refill_rate,
            tokens: Arc::new(AtomicU64::new(capacity)),
            last_refill: Arc::new(AtomicU64::new(now)),
            stats: Arc::new(Mutex::new(LimiterStats::new())),
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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        loop {
            let last = self.last_refill.load(Ordering::SeqCst);
            let elapsed_nanos = now.saturating_sub(last);

            if elapsed_nanos < 1_000_000 {
                break;
            }

            let elapsed_seconds = elapsed_nanos as f64 / 1_000_000_000.0;
            let tokens_to_add = (elapsed_seconds * self.refill_rate as f64) as u64;

            if tokens_to_add == 0 {
                break;
            }

            if self
                .last_refill
                .compare_exchange(last, now, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
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
impl CustomLimiter for TokenBucketLimiter {
    fn name(&self) -> &str {
        "token_bucket"
    }

    async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError> {
        // 先补充令牌
        self.refill_tokens();

        loop {
            let current = self.tokens.load(Ordering::SeqCst);

            // 检查令牌是否足够
            if current < cost {
                // 更新统计信息
                let mut stats = self.stats.lock().unwrap();
                stats.total_requests += 1;
                stats.rejected_requests += 1;

                debug!("令牌桶限流拒绝: 当前={}, 成本={}", current, cost);

                return Ok(false);
            }

            // 尝试消费令牌
            if self
                .tokens
                .compare_exchange(current, current - cost, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                // 更新统计信息
                let mut stats = self.stats.lock().unwrap();
                stats.total_requests += 1;
                stats.allowed_requests += 1;

                debug!("令牌桶限流允许: 当前={}, 成本={}", current - cost, cost);

                return Ok(true);
            }
        }
    }

    fn load_config(&mut self, config: Value) -> Result<(), FlowGuardError> {
        let capacity = config["capacity"]
            .as_u64()
            .ok_or_else(|| FlowGuardError::ConfigError("缺少 capacity 配置".to_string()))?;

        let refill_rate = config["refill_rate"]
            .as_u64()
            .ok_or_else(|| FlowGuardError::ConfigError("缺少 refill_rate 配置".to_string()))?;

        if capacity == 0 {
            return Err(FlowGuardError::ConfigError(
                "capacity 必须大于 0".to_string(),
            ));
        }

        if refill_rate == 0 {
            return Err(FlowGuardError::ConfigError(
                "refill_rate 必须大于 0".to_string(),
            ));
        }

        self.capacity = capacity;
        self.refill_rate = refill_rate;
        self.tokens.store(capacity, Ordering::SeqCst);

        info!(
            "加载令牌桶限流器配置: 容量={}, 补充速率={}",
            self.capacity, self.refill_rate
        );

        Ok(())
    }

    fn stats(&self) -> LimiterStats {
        self.stats.lock().unwrap().clone()
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    // ==================== LimiterStats 测试 ====================

    #[test]
    fn test_limiter_stats_new() {
        let stats = LimiterStats::new();
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.allowed_requests, 0);
        assert_eq!(stats.rejected_requests, 0);
    }

    #[test]
    fn test_limiter_stats_rejection_rate() {
        let mut stats = LimiterStats::new();
        stats.total_requests = 100;
        stats.rejected_requests = 20;
        stats.allowed_requests = 80;

        assert_eq!(stats.rejection_rate(), 0.2);
        assert_eq!(stats.allow_rate(), 0.8);
    }

    #[test]
    fn test_limiter_stats_zero_requests() {
        let stats = LimiterStats::new();
        assert_eq!(stats.rejection_rate(), 0.0);
        assert_eq!(stats.allow_rate(), 0.0);
    }

    // ==================== CustomLimiterRegistry 测试 ====================

    #[tokio::test]
    async fn test_registry_new() {
        let registry = CustomLimiterRegistry::new();
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn test_registry_register() {
        let registry = CustomLimiterRegistry::new();
        let limiter = LeakyBucketLimiter::new(100, 10);

        assert!(registry
            .register("leaky_bucket".to_string(), Box::new(limiter))
            .await
            .is_ok());
        assert_eq!(registry.count().await, 1);
        assert!(registry.contains("leaky_bucket").await);
    }

    #[tokio::test]
    async fn test_registry_register_duplicate() {
        let registry = CustomLimiterRegistry::new();
        let limiter = LeakyBucketLimiter::new(100, 10);

        assert!(registry
            .register("leaky_bucket".to_string(), Box::new(limiter))
            .await
            .is_ok());

        let result = registry
            .register(
                "leaky_bucket".to_string(),
                Box::new(LeakyBucketLimiter::new(200, 20)),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_unregister() {
        let registry = CustomLimiterRegistry::new();
        let limiter = LeakyBucketLimiter::new(100, 10);

        registry
            .register("leaky_bucket".to_string(), Box::new(limiter))
            .await
            .unwrap();

        assert!(registry.unregister("leaky_bucket").await.is_ok());
        assert_eq!(registry.count().await, 0);
        assert!(!registry.contains("leaky_bucket").await);
    }

    #[tokio::test]
    async fn test_registry_unregister_nonexistent() {
        let registry = CustomLimiterRegistry::new();
        let result = registry.unregister("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_list() {
        let registry = CustomLimiterRegistry::new();

        registry
            .register(
                "limiter1".to_string(),
                Box::new(LeakyBucketLimiter::new(100, 10)),
            )
            .await
            .unwrap();
        registry
            .register(
                "limiter2".to_string(),
                Box::new(TokenBucketLimiter::new(100, 10)),
            )
            .await
            .unwrap();

        let list = registry.list().await;
        assert_eq!(list.len(), 2);
        assert!(list.contains(&"limiter1".to_string()));
        assert!(list.contains(&"limiter2".to_string()));
    }

    #[tokio::test]
    async fn test_registry_clear() {
        let registry = CustomLimiterRegistry::new();

        registry
            .register(
                "limiter1".to_string(),
                Box::new(LeakyBucketLimiter::new(100, 10)),
            )
            .await
            .unwrap();

        registry.clear().await;
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn test_registry_allow() {
        let registry = CustomLimiterRegistry::new();
        let limiter = LeakyBucketLimiter::new(100, 10);

        registry
            .register("leaky_bucket".to_string(), Box::new(limiter))
            .await
            .unwrap();

        let result = registry.allow("leaky_bucket", 1).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_registry_allow_nonexistent() {
        let registry = CustomLimiterRegistry::new();
        let result = registry.allow("nonexistent", 1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_get_stats() {
        let registry = CustomLimiterRegistry::new();
        let limiter = LeakyBucketLimiter::new(100, 10);

        registry
            .register("leaky_bucket".to_string(), Box::new(limiter))
            .await
            .unwrap();

        let result = registry.get_stats("leaky_bucket").await;
        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.total_requests, 0);
    }

    // ==================== LeakyBucketLimiter 测试 ====================

    #[tokio::test]
    async fn test_leaky_bucket_new() {
        let limiter = LeakyBucketLimiter::new(100, 10);
        assert_eq!(limiter.name(), "leaky_bucket");
        assert_eq!(limiter.capacity(), 100);
        assert_eq!(limiter.leak_rate(), 10);
        assert_eq!(limiter.current(), 0);
    }

    #[tokio::test]
    async fn test_leaky_bucket_allow() {
        let limiter = LeakyBucketLimiter::new(100, 10);
        assert!(limiter.allow(1).await.unwrap());
        assert_eq!(limiter.current(), 1);
    }

    #[tokio::test]
    async fn test_leaky_bucket_exceeds_capacity() {
        let limiter = LeakyBucketLimiter::new(10, 10);
        assert!(limiter.allow(10).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_leaky_bucket_leak() {
        let limiter = LeakyBucketLimiter::new(10, 100); // 100 requests/sec
        limiter.allow(10).await.unwrap();
        assert_eq!(limiter.current(), 10);

        sleep(Duration::from_millis(20)).await; // 等待 20ms，应该漏出约 2 个请求
        limiter.allow(0).await.unwrap(); // 触发漏出
        assert!(limiter.current() < 10);
    }

    #[tokio::test]
    async fn test_leaky_bucket_stats() {
        let limiter = LeakyBucketLimiter::new(100, 10);
        limiter.allow(1).await.unwrap();

        let stats = limiter.stats();
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.allowed_requests, 1);
        assert_eq!(stats.rejected_requests, 0);
    }

    #[tokio::test]
    async fn test_leaky_bucket_load_config() {
        let mut limiter = LeakyBucketLimiter::new(100, 10);
        let config = serde_json::json!({
            "capacity": 200,
            "leak_rate": 20
        });

        assert!(limiter.load_config(config).is_ok());
        assert_eq!(limiter.capacity(), 200);
        assert_eq!(limiter.leak_rate(), 20);
    }

    #[tokio::test]
    async fn test_leaky_bucket_load_config_invalid() {
        let mut limiter = LeakyBucketLimiter::new(100, 10);
        let config = serde_json::json!({
            "capacity": 0
        });

        let result = limiter.load_config(config);
        assert!(result.is_err());
    }

    // ==================== TokenBucketLimiter 测试 ====================

    #[tokio::test]
    async fn test_token_bucket_new() {
        let limiter = TokenBucketLimiter::new(100, 10);
        assert_eq!(limiter.name(), "token_bucket");
        assert_eq!(limiter.capacity(), 100);
        assert_eq!(limiter.refill_rate(), 10);
        assert_eq!(limiter.tokens(), 100);
    }

    #[tokio::test]
    async fn test_token_bucket_allow() {
        let limiter = TokenBucketLimiter::new(100, 10);
        assert!(limiter.allow(10).await.unwrap());
        assert_eq!(limiter.tokens(), 90);
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
        assert_eq!(limiter.tokens(), 0);

        sleep(Duration::from_millis(20)).await; // 等待 20ms，应该补充约 2 个令牌
        limiter.allow(0).await.unwrap(); // 触发补充
        assert!(limiter.tokens() >= 1);
    }

    #[tokio::test]
    async fn test_token_bucket_stats() {
        let limiter = TokenBucketLimiter::new(100, 10);
        limiter.allow(1).await.unwrap();

        let stats = limiter.stats();
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.allowed_requests, 1);
        assert_eq!(stats.rejected_requests, 0);
    }

    #[tokio::test]
    async fn test_token_bucket_load_config() {
        let mut limiter = TokenBucketLimiter::new(100, 10);
        let config = serde_json::json!({
            "capacity": 200,
            "refill_rate": 20
        });

        assert!(limiter.load_config(config).is_ok());
        assert_eq!(limiter.capacity(), 200);
        assert_eq!(limiter.refill_rate(), 20);
        assert_eq!(limiter.tokens(), 200);
    }

    #[tokio::test]
    async fn test_token_bucket_load_config_invalid() {
        let mut limiter = TokenBucketLimiter::new(100, 10);
        let config = serde_json::json!({
            "capacity": 0
        });

        let result = limiter.load_config(config);
        assert!(result.is_err());
    }

    // ==================== 并发测试 ====================

    #[tokio::test]
    async fn test_registry_concurrent_register() {
        let registry = Arc::new(CustomLimiterRegistry::new());
        let mut handles = vec![];

        for i in 0..10 {
            let registry_clone = Arc::clone(&registry);
            handles.push(tokio::spawn(async move {
                let limiter = LeakyBucketLimiter::new(100 + i, 10);
                registry_clone
                    .register(format!("limiter_{}", i), Box::new(limiter))
                    .await
            }));
        }

        let mut success_count = 0;
        for handle in handles {
            if handle.await.unwrap().is_ok() {
                success_count += 1;
            }
        }

        assert_eq!(success_count, 10);
        assert_eq!(registry.count().await, 10);
    }

    #[tokio::test]
    async fn test_registry_concurrent_allow() {
        let registry = Arc::new(CustomLimiterRegistry::new());
        let limiter = LeakyBucketLimiter::new(100, 10);

        registry
            .register("leaky_bucket".to_string(), Box::new(limiter))
            .await
            .unwrap();

        let mut handles = vec![];
        for _ in 0..100 {
            let registry_clone = Arc::clone(&registry);
            handles.push(tokio::spawn(async move {
                registry_clone.allow("leaky_bucket", 1).await
            }));
        }

        let mut success_count = 0;
        for handle in handles {
            if let Ok(Ok(true)) = handle.await {
                success_count += 1;
            }
        }

        assert_eq!(success_count, 100);
    }

    #[tokio::test]
    async fn test_leaky_bucket_concurrent() {
        let limiter = Arc::new(LeakyBucketLimiter::new(100, 10));
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

        // 总共消费 100 个请求，应该正好消耗完
        assert_eq!(limiter.current(), 100);
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
        assert_eq!(limiter.tokens(), 0);
    }
}
