// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 限流器 traits 模块
//!
//! 定义 Limiter trait 和通用验证函数。

use crate::constants::MAX_COST;
use crate::error::FlowGuardError;
use async_trait::async_trait;

/// Validates the cost parameter.
///
/// # Arguments
/// * `cost` - The cost value to validate
///
/// # Returns
/// * `Ok(u64)` - The validated cost value
/// * `Err(FlowGuardError)` - Validation failed
pub(crate) fn validate_cost(cost: u64) -> Result<u64, FlowGuardError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_cost_zero() {
        let result = validate_cost(0);
        assert!(result.is_err());
        match result {
            Err(FlowGuardError::ConfigError(msg)) => {
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
            Err(FlowGuardError::ConfigError(msg)) => {
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
            async fn allow(&self, _cost: u64) -> Result<bool, FlowGuardError> {
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
            async fn allow(&self, _cost: u64) -> Result<bool, FlowGuardError> {
                Err(FlowGuardError::LimitError("denied".to_string()))
            }
        }

        let limiter = ErrorLimiter;
        // check() propagates Err from allow()
        let result = limiter.check("any_key").await;
        assert!(result.is_err());
    }
}
