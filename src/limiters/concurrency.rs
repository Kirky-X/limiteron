//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 并发控制器模块
//!
//! 使用信号量实现并发控制。

use super::traits::Limiter;
use crate::error::FlowGuardError;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

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
    pub fn max_concurrent(mut self, max_concurrent: u64) -> Self {
        self.max_concurrent = Some(max_concurrent);
        self
    }

    /// 设置超时时间
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// 设置外部信号量（用于依赖注入）
    pub fn with_semaphore(mut self, semaphore: Arc<tokio::sync::Semaphore>) -> Self {
        self.semaphore = Some(semaphore);
        self
    }

    /// 构建 ConcurrencyLimiter 实例
    pub fn build(self) -> Result<ConcurrencyLimiter, FlowGuardError> {
        if let Some(semaphore) = self.semaphore {
            return Ok(ConcurrencyLimiter {
                semaphore,
                timeout: self.timeout,
                max_concurrent: 0,
            });
        }

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
    pub fn with_timeout(max_concurrent: u64, timeout: Duration) -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent as usize)),
            timeout: Some(timeout),
            max_concurrent,
        }
    }

    /// 创建 ConcurrencyLimiterBuilder 用于链式配置
    pub fn builder() -> ConcurrencyLimiterBuilder {
        ConcurrencyLimiterBuilder::new()
    }

    /// 使用依赖注入创建 ConcurrencyLimiter 实例
    pub fn with_dependencies(
        semaphore: Arc<tokio::sync::Semaphore>,
        timeout: Option<Duration>,
    ) -> Self {
        Self {
            semaphore,
            timeout,
            max_concurrent: 0,
        }
    }

    /// 获取最大并发数
    pub fn max_concurrent(&self) -> u64 {
        self.max_concurrent
    }

    /// 获取超时时间
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    /// 获取许可并执行操作
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
        let cost_u32 = cost as u32;
        if cost_u32 as u64 != cost {
            return Err(FlowGuardError::LimitError(
                "许可数量超出 u32 范围".to_string(),
            ));
        }

        match self.semaphore.try_acquire_many(cost_u32) {
            Ok(_permit) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrency_basic() {
        let limiter = ConcurrencyLimiter::new(10);

        let permit = limiter.acquire(1).await.unwrap();
        assert_eq!(limiter.available_permits(), 9);
        drop(permit);
        assert_eq!(limiter.available_permits(), 10);
    }

    #[tokio::test]
    async fn test_concurrency_try_acquire() {
        let limiter = ConcurrencyLimiter::new(2);

        // 保存 permit 以保持许可占用
        let permit1 = limiter.try_acquire(1);
        let permit2 = limiter.try_acquire(1);
        let permit3 = limiter.try_acquire(1);

        assert!(permit1.is_ok(), "第一次获取应该成功");
        assert!(permit2.is_ok(), "第二次获取应该成功");
        assert!(permit3.is_err(), "第三次获取应该失败，因为已达上限");

        // permit1 和 permit2 在此处 drop，释放许可
    }
}
