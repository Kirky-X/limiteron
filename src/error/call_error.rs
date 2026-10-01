// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 熔断器包装调用的结果错误类型
//!
//! async（[`crate::circuit::CircuitBreaker`]）与 sync
//! （[`crate::sync::SyncCircuitBreaker`]）两版熔断器共用的显式拒绝语义：
//! 熔断拒绝不是注入的错误值，而是 [`CircuitCallError::Open`] 变体；
//! 调用自身的失败经 [`CircuitCallError::Inner`] 透传原始错误。

use std::fmt;

/// 熔断器包装的调用结果。
///
/// - [`CircuitCallError::Open`]：熔断打开（冷却中或半开探针配额已满），
///   闭包未执行即被拒绝（快速失败）；
/// - [`CircuitCallError::Inner`]：调用被放行但自身失败，透传原始错误。
///
/// # 示例
///
/// ```rust
/// use limiteron::error::CircuitCallError;
///
/// // 熔断打开：闭包未执行
/// let rejected: CircuitCallError<String> = CircuitCallError::Open;
/// assert!(rejected.is_open());
/// assert_eq!(rejected.into_inner(), None);
///
/// // 调用被放行但自身失败：透传原始错误
/// let failed: CircuitCallError<String> = CircuitCallError::Inner("boom".into());
/// assert_eq!(failed.into_inner().as_deref(), Some("boom"));
/// ```
#[derive(Debug, PartialEq, Eq)]
pub enum CircuitCallError<E> {
    /// 熔断打开：闭包未执行。
    Open,
    /// 调用被放行但失败：透传原始错误。
    Inner(E),
}

impl<E> CircuitCallError<E> {
    /// 熔断是否处于打开拒绝态（`true` 表示闭包未执行）。
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open)
    }

    /// 取透传的原始错误（Open 态返回 `None`）。
    pub fn into_inner(self) -> Option<E> {
        match self {
            Self::Open => None,
            Self::Inner(e) => Some(e),
        }
    }
}

impl<E: fmt::Display> fmt::Display for CircuitCallError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => write!(f, "circuit breaker is open"),
            Self::Inner(e) => write!(f, "{e}"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for CircuitCallError<E> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_call_error_helpers() {
        assert!(CircuitCallError::<&str>::Open.is_open());
        assert_eq!(CircuitCallError::Inner("x").into_inner(), Some("x"));
        assert!(CircuitCallError::<&str>::Open.into_inner().is_none());
        assert_eq!(
            CircuitCallError::<&str>::Open.to_string(),
            "circuit breaker is open"
        );
        assert_eq!(CircuitCallError::Inner("boom").to_string(), "boom");
    }
}
