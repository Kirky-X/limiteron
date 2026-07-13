// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 限流器 traits 模块
//!
//! 定义 Limiter trait 和通用验证函数。

use crate::constants::MAX_COST;
use crate::error::LimiteronError;
use async_trait::async_trait;
#[cfg(feature = "distributed")]
use std::time::Duration;

/// Validates the cost parameter.
///
/// # Arguments
/// * `cost` - The cost value to validate
///
/// # Returns
/// * `Ok(u64)` - The validated cost value
/// * `Err(LimiteronError)` - Validation failed
pub(crate) fn validate_cost(cost: u64) -> Result<u64, LimiteronError> {
    if cost == 0 {
        return Err(LimiteronError::ConfigError(
            "Cost cannot be zero".to_string(),
        ));
    }

    if cost > MAX_COST {
        return Err(LimiteronError::ConfigError(format!(
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
///     async fn allow(&self, cost: u64) -> Result<bool, limiteron::error::LimiteronError> {
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
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError>;

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
    async fn check(&self, _key: &str) -> Result<(), LimiteronError> {
        self.allow(1).await?;
        Ok(())
    }
}

/// 分布式限流器 trait
///
/// 扩展 [`Limiter`] trait，提供原子计数操作，支持分布式 DAO（如 BulwarkDao）。
/// 进程内限流器只需实现 `Limiter` trait；分布式限流器需实现 `DistributedLimiter`。
///
/// # 特性
///
/// - **原子计数** - `incr`/`incr_with_ttl` 方法支持原子递增
/// - **TTL 支持** - `incr_with_ttl` 方法支持带过期时间的递增（滑动窗口）
/// - **状态查询** - `get_count` 方法获取当前计数
/// - **状态重置** - `reset` 方法重置计数器
///
/// # 示例
///
/// ```rust
/// use limiteron::limiters::{DistributedLimiter, InMemoryDistributedLimiter};
///
/// #[tokio::main]
/// async fn main() {
///     let limiter = InMemoryDistributedLimiter::new();
///     let count = limiter.incr("user:123", 1).await.unwrap();
///     assert_eq!(count, 1);
/// }
/// ```
#[cfg(feature = "distributed")]
#[async_trait]
pub trait DistributedLimiter: Limiter {
    /// 原子递增计数器，返回递增后的值
    ///
    /// # 参数
    /// - `key`: 计数器键
    /// - `amount`: 递增量
    ///
    /// # 返回
    /// - `Ok(u64)`: 递增后的值
    /// - `Err(_)`: 发生错误
    async fn incr(&self, key: &str, amount: u64) -> Result<u64, LimiteronError>;

    /// 原子递增并设置 TTL（用于滑动窗口）
    ///
    /// # 参数
    /// - `key`: 计数器键
    /// - `amount`: 递增量
    /// - `ttl`: 过期时间
    ///
    /// # 返回
    /// - `Ok(u64)`: 递增后的值
    /// - `Err(_)`: 发生错误
    async fn incr_with_ttl(
        &self,
        key: &str,
        amount: u64,
        ttl: Duration,
    ) -> Result<u64, LimiteronError>;

    /// 获取当前计数
    ///
    /// # 参数
    /// - `key`: 计数器键
    ///
    /// # 返回
    /// - `Ok(u64)`: 当前计数值（不存在则为 0）
    /// - `Err(_)`: 发生错误
    async fn get_count(&self, key: &str) -> Result<u64, LimiteronError>;

    /// 重置计数器
    ///
    /// # 参数
    /// - `key`: 计数器键
    ///
    /// # 返回
    /// - `Ok(())`: 重置成功
    /// - `Err(_)`: 发生错误
    async fn reset(&self, key: &str) -> Result<(), LimiteronError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_cost_zero() {
        let result = validate_cost(0);
        assert!(result.is_err());
        match result {
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(msg.contains("Cost cannot be zero"))
            }
            _ => panic!("expected ConfigError for zero cost"),
        }
    }

    #[test]
    fn test_validate_cost_exceeds_max() {
        let result = validate_cost(crate::constants::MAX_COST + 1);
        assert!(result.is_err());
        match result {
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(msg.contains("Cost exceeds maximum limit"))
            }
            _ => panic!("expected ConfigError for exceeding max cost"),
        }
    }

    #[test]
    fn test_validate_cost_valid() {
        assert_eq!(validate_cost(1).unwrap(), 1);
        assert_eq!(
            validate_cost(crate::constants::MAX_COST).unwrap(),
            crate::constants::MAX_COST
        );
    }

    #[tokio::test]
    async fn test_limiter_check_default_impl() {
        struct AllowAllLimiter;
        #[async_trait]
        impl Limiter for AllowAllLimiter {
            async fn allow(&self, _cost: u64) -> Result<bool, LimiteronError> {
                Ok(true)
            }
        }

        let limiter = AllowAllLimiter;
        // check() default impl calls allow(1) and returns Ok(())
        assert!(limiter.check("any_key").await.is_ok());
    }

    #[tokio::test]
    async fn test_limiter_check_default_impl_propagates_error() {
        struct ErrorLimiter;
        #[async_trait]
        impl Limiter for ErrorLimiter {
            async fn allow(&self, _cost: u64) -> Result<bool, LimiteronError> {
                Err(LimiteronError::LimitError("denied".to_string()))
            }
        }

        let limiter = ErrorLimiter;
        // check() propagates Err from allow()
        let result = limiter.check("any_key").await;
        assert!(result.is_err());
    }
}
