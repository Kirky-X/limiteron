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

    /// 非消费预检（T603）
    ///
    /// 查询「当前状态下消费 `cost` 是否可行」以及标准限流头数据
    /// （limit/remaining/reset），**绝不修改限流器状态**。
    /// 调用方以 `snapshot.remaining >= cost` 判断可行性。
    ///
    /// 默认实现返回 `Err`（限流器未支持预检），保持对所有既有实现者的
    /// 源兼容；建议各限流器基于自身原子量覆盖实现。
    async fn peek(&self, cost: u64) -> Result<RateLimitSnapshot, LimiteronError> {
        let _ = cost;
        Err(LimiteronError::Other(
            "peek is not supported by this limiter".to_string(),
        ))
    }

    /// 查询剩余额度（T603，非消费）
    ///
    /// 返回标准限流头数据；默认实现返回 `Err`（未支持）。
    async fn remaining(&self) -> Result<RateLimitSnapshot, LimiteronError> {
        Err(LimiteronError::Other(
            "remaining is not supported by this limiter".to_string(),
        ))
    }

    /// 检查是否允许（接受 key 参数，用于宏）
    ///
    /// 默认实现：消费 1 个单位的 cost
    ///
    /// # 参数
    /// - `_key`: 标识符 key（用于某些限流器类型）
    ///
    /// # 返回
    /// - `Ok(())`: 允许通过
    /// - `Err(LimiteronError::LimitError)`: 被限流拒绝
    /// - `Err(_)`: 发生错误
    ///
    /// 注意：`allow` 返回 `Ok(false)`（拒绝）必须映射为 `Err`，
    /// 不能静默吞掉——否则限流器形同虚设。
    async fn check(&self, _key: &str) -> Result<(), LimiteronError> {
        if self.allow(1).await? {
            Ok(())
        } else {
            Err(LimiteronError::LimitError(
                "rate limit exceeded".to_string(),
            ))
        }
    }
}

/// 标准限流头数据（T603）
///
/// 对应 IETF draft-ietf-httpapi-ratelimit-headers 的三个标准头：
/// `RateLimit-Limit` / `RateLimit-Remaining` / `RateLimit-Reset`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RateLimitSnapshot {
    /// 窗口/桶容量上限（RateLimit-Limit）
    pub limit: u64,
    /// 当前剩余额度（RateLimit-Remaining）
    pub remaining: u64,
    /// 距额度重置的秒数（RateLimit-Reset；0 表示已可用或即时恢复）
    pub reset_secs: u64,
}

impl RateLimitSnapshot {
    /// 以给定 cost 计算可行性（非消费判定）
    pub fn allows(&self, cost: u64) -> bool {
        self.remaining >= cost
    }

    /// 渲染为标准限流头键值对
    ///
    /// 返回 `(header_name, header_value)` 三元组列表，供 HTTP 中间件
    /// （如 tower `RateLimitLayer`）直接写入响应。
    pub fn headers(&self) -> [(&'static str, String); 3] {
        [
            ("RateLimit-Limit", self.limit.to_string()),
            ("RateLimit-Remaining", self.remaining.to_string()),
            ("RateLimit-Reset", self.reset_secs.to_string()),
        ]
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

    #[tokio::test]
    async fn test_limiter_check_default_impl_rejects_on_allow_false() {
        // 修复回归测试：allow 返回 Ok(false)（拒绝）时，check 必须返回
        // Err(LimitError)，不得静默吞掉拒绝语义（旧实现返回 Ok(())）
        struct DenyAllLimiter;
        #[async_trait]
        impl Limiter for DenyAllLimiter {
            async fn allow(&self, _cost: u64) -> Result<bool, LimiteronError> {
                Ok(false)
            }
        }

        let limiter = DenyAllLimiter;
        let result = limiter.check("any_key").await;
        match result {
            Err(LimiteronError::LimitError(msg)) => {
                assert!(msg.contains("rate limit exceeded"))
            }
            other => panic!(
                "expected Err(LimitError) for rejection, got {:?}",
                other.is_ok()
            ),
        }
    }

    // ========================================================================
    // T603：peek/remaining 默认实现与 RateLimitSnapshot
    // ========================================================================

    #[tokio::test]
    async fn test_t603_peek_default_impl_returns_unsupported() {
        struct BareLimiter;
        #[async_trait]
        impl Limiter for BareLimiter {
            async fn allow(&self, _cost: u64) -> Result<bool, LimiteronError> {
                Ok(true)
            }
        }

        let limiter = BareLimiter;
        assert!(
            limiter.peek(1).await.is_err(),
            "默认 peek 应返回 Err（未支持）"
        );
        assert!(
            limiter.remaining().await.is_err(),
            "默认 remaining 应返回 Err（未支持）"
        );
    }

    #[test]
    fn test_t603_rate_limit_snapshot_headers_and_allows() {
        let snapshot = RateLimitSnapshot {
            limit: 100,
            remaining: 42,
            reset_secs: 7,
        };
        assert!(snapshot.allows(42), "remaining == cost 应判定可行");
        assert!(snapshot.allows(1));
        assert!(!snapshot.allows(43), "remaining < cost 应判定不可行");

        let headers = snapshot.headers();
        assert_eq!(headers[0], ("RateLimit-Limit", "100".to_string()));
        assert_eq!(headers[1], ("RateLimit-Remaining", "42".to_string()));
        assert_eq!(headers[2], ("RateLimit-Reset", "7".to_string()));
    }

    #[tokio::test]
    async fn test_t603_peek_is_zero_side_effect() {
        use crate::limiters::TokenBucketLimiter;

        let limiter = TokenBucketLimiter::new(100, 10);
        let before = limiter.peek(10).await.unwrap();
        assert_eq!(before.limit, 100);
        assert_eq!(before.remaining, 100, "满桶 peek 剩余应为容量");

        // peek 之后 allow 的结果与 peek 判定一致（peek 零副作用）
        assert!(before.allows(10));
        assert!(limiter.allow(10).await.unwrap());

        let after = limiter.peek(10).await.unwrap();
        assert_eq!(after.remaining, 90, "peek 不得重复扣减，allow 恰好扣减一次");
        assert_eq!(after.limit, 100);
        assert!(after.reset_secs <= 1, "缺口 10/速率 10 → 重置 ≤1s");
    }

    #[tokio::test]
    async fn test_t603_remaining_reflects_state() {
        use crate::limiters::{FixedWindowLimiter, TokenBucketLimiter};
        use std::time::Duration;

        // 令牌桶
        let tb = TokenBucketLimiter::new(50, 5);
        Limiter::allow(&tb, 20).await.unwrap();
        let snap = tb.remaining().await.unwrap();
        assert_eq!(snap.remaining, 30);
        assert_eq!(snap.limit, 50);

        // 固定窗口
        let fw = FixedWindowLimiter::new(Duration::from_secs(60), 10);
        Limiter::allow(&fw, 4).await.unwrap();
        let snap = fw.remaining().await.unwrap();
        assert_eq!(snap.remaining, 6, "固定窗口 remaining = max - count");
        assert_eq!(snap.limit, 10);
        assert!(
            snap.reset_secs <= 60 && snap.reset_secs > 0,
            "重置时间应在窗口内"
        );
    }

    #[tokio::test]
    async fn test_t603_peek_zero_cost_is_valid() {
        use crate::limiters::TokenBucketLimiter;

        let limiter = TokenBucketLimiter::new(100, 10);
        // cost=0 与 allow(0) 语义对齐：配置校验错误
        assert!(limiter.peek(0).await.is_err());
    }
}
