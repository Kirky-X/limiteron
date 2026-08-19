// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Quota Limiter
//!
//! Implements a simple quota-based limiter that tracks usage per key
//! with configurable limits and time windows.

use crate::error::LimiteronError;
#[cfg(feature = "quota-control")]
use crate::quota::QuotaConfig;
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Quota usage record for a single key
#[derive(Debug, Clone)]
struct QuotaRecord {
    /// Current usage count
    usage: u64,
    /// Window start time
    window_start: Instant,
}

/// QuotaLimiter - A simple quota-based rate limiter
///
/// Tracks usage per identifier key within a time window.
/// When a key exceeds its quota limit, requests are rejected.
pub struct QuotaLimiter {
    /// Quota configuration
    config: QuotaConfig,
    /// Per-key usage tracking (key -> usage, window_start)
    usage: Arc<DashMap<String, QuotaRecord>>,
}

/// 链式/无 key 场景（`Limiter::allow` 不提供 key）使用的匿名配额桶键。
/// 避免与真实用户键冲突的低概率前缀。
const ANONYMOUS_QUOTA_KEY: &str = "__limiteron_anonymous_quota__";

/// 每 key 用量记录的跟踪上限（diting MED-001：高基数 key 内存约束）
const QUOTA_MAX_TRACKED_KEYS: usize = 10_000;

impl QuotaLimiter {
    /// Creates a new QuotaLimiter with the given configuration.
    ///
    /// # Arguments
    /// * `config` - Quota configuration including limit, window size, etc.
    ///
    /// # Panic
    ///
    /// `config.window_size == 0` 时 panic（audit-L-003）。
    /// `window_size = 0` 会导致 `Duration::from_secs(0)` 窗口立即过期，
    /// 每次请求都重置 usage，配额限制形同虚设——这是配置 bug，应在开发阶段发现。
    /// 用 `assert!` 而非 `Result` 以保持 API 兼容性（Rule 12：失败必须显性化）。
    ///
    /// # Examples
    /// ```rust
    /// use limiteron::limiters::QuotaLimiter;
    /// use limiteron::quota::QuotaConfig;
    /// use limiteron::quota::QuotaType;
    ///
    /// let config = QuotaConfig {
    ///     quota_type: QuotaType::Count,
    ///     limit: 1000,
    ///     window_size: 3600,
    ///     allow_overdraft: false,
    ///     overdraft_limit_percent: 20,
    ///     alert_config: Default::default(),
    /// };
    /// let limiter = QuotaLimiter::new(config);
    /// ```
    pub fn new(config: QuotaConfig) -> Self {
        assert!(
            config.window_size > 0,
            "QuotaConfig.window_size must be greater than 0 (audit-L-003); \
             window_size=0 would cause immediate window expiry, making quota useless"
        );
        Self {
            config,
            usage: Arc::new(DashMap::new()),
        }
    }

    /// 获取配额上限
    ///
    /// # 注意
    ///
    /// 仅供 `LimiterManager` 参数一致性校验使用（audit-M-002），不应在业务代码中调用。
    /// 业务代码应通过 `Limiter::check` 接口与 limiter 交互，而非直接读取配置。
    pub fn max(&self) -> u64 {
        self.config.limit
    }

    /// 获取配额周期
    ///
    /// # 注意
    ///
    /// 仅供 `LimiterManager` 参数一致性校验使用（audit-M-002），不应在业务代码中调用。
    /// 业务代码应通过 `Limiter::check` 接口与 limiter 交互，而非直接读取配置。
    pub fn period(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.config.window_size)
    }

    /// Checks and consumes quota for the given key.
    ///
    /// # Arguments
    /// * `key` - The identifier key (user ID, API key, etc.)
    ///
    /// # Returns
    /// * `Ok(())` - Quota available, consumption successful
    /// * `Err(LimiteronError)` - Quota exceeded or error
    async fn check_and_consume(&self, key: &str) -> Result<bool, LimiteronError> {
        let now = Instant::now();
        let window_duration = Duration::from_secs(self.config.window_size);

        // diting MED-001：防攻击者可控的高基数 key 无限增长（OOM DoS）——
        // 超过跟踪上限时清理已过期窗口的记录，把内存约束在 ~上限 + 单窗口新增以内。
        if self.usage.len() > QUOTA_MAX_TRACKED_KEYS {
            self.usage
                .retain(|_, rec| now.duration_since(rec.window_start) < window_duration);
        }

        let mut record = self
            .usage
            .entry(key.to_string())
            .or_insert_with(|| QuotaRecord {
                usage: 0,
                window_start: now,
            });

        // Check if window has expired
        if now.duration_since(record.window_start) >= window_duration {
            // Reset for new window
            record.usage = 0;
            record.window_start = now;
        }

        // Check if quota allows overdraft
        let max_usage = if self.config.allow_overdraft {
            let overdraft_limit =
                self.config.limit * self.config.overdraft_limit_percent as u64 / 100;
            self.config.limit + overdraft_limit
        } else {
            self.config.limit
        };

        if record.usage >= max_usage {
            return Err(LimiteronError::QuotaExceeded(format!(
                "Quota exceeded for key '{}': used {}/{}",
                key, record.usage, max_usage
            )));
        }

        record.usage += 1;
        Ok(true)
    }
}

#[async_trait]
impl crate::limiters::Limiter for QuotaLimiter {
    async fn allow(&self, _cost: u64) -> Result<bool, LimiteronError> {
        // 链式/无 key 场景下无法按用户键跟踪：对内部匿名桶消耗配额，
        // 使配额规则经决策链挂载时真实生效（diting MED-004/005 修复）。
        // 超出限制映射为 Ok(false)（拒绝语义），而非错误语义。
        match self.check_and_consume(ANONYMOUS_QUOTA_KEY).await {
            Ok(ok) => Ok(ok),
            Err(LimiteronError::QuotaExceeded(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn check(&self, key: &str) -> Result<(), LimiteronError> {
        self.check_and_consume(key).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limiters::Limiter;
    use crate::quota::QuotaType;

    fn create_test_config() -> QuotaConfig {
        QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 10,
            window_size: 60,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_quota_limiter_allows_within_limit() {
        let config = create_test_config();
        let limiter = QuotaLimiter::new(config);

        // Should allow 10 requests
        for i in 0..10 {
            let result = limiter.check("user1").await;
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }
    }

    #[tokio::test]
    async fn test_quota_limiter_rejects_over_limit() {
        let config = create_test_config();
        let limiter = QuotaLimiter::new(config);

        // Use up the quota
        for _ in 0..10 {
            let _ = limiter.check("user1").await;
        }

        // Next request should be rejected
        let result = limiter.check("user1").await;
        assert!(result.is_err());
        assert!(matches!(result, Err(LimiteronError::QuotaExceeded(_))));
    }

    #[tokio::test]
    async fn test_quota_limiter_independent_keys() {
        let config = create_test_config();
        let limiter = QuotaLimiter::new(config);

        // user1 uses 10 requests
        for _ in 0..10 {
            let _ = limiter.check("user1").await;
        }

        // user2 should still be able to make requests
        let result = limiter.check("user2").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_quota_limiter_with_overdraft() {
        let mut config = create_test_config();
        config.allow_overdraft = true;
        config.overdraft_limit_percent = 20; // 20% overdraft

        let limiter = QuotaLimiter::new(config);

        // Should allow 10 + 2 = 12 requests (10 limit + 20% overdraft)
        for i in 0..12 {
            let result = limiter.check("user1").await;
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }

        // Next request should be rejected
        let result = limiter.check("user1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_quota_limiter_allow_method() {
        // allow() 现在对匿名桶消耗配额（diting MED-005 修复）：
        // 到上限后返回 Ok(false)（拒绝语义），使链式挂载真实生效。
        let config = create_test_config(); // limit = 10
        let limiter = QuotaLimiter::new(config);

        // 前 10 次允许
        for _ in 0..10 {
            let result = limiter.allow(1).await;
            assert!(result.is_ok());
            assert!(result.unwrap());
        }

        // 第 11 次拒绝
        let result = limiter.allow(1).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_quota_limiter_allow_with_zero_cost() {
        // cost 参数在配额语义下不影响消耗：与正常调用一致受匿名桶限制
        let config = create_test_config(); // limit = 10
        let limiter = QuotaLimiter::new(config);

        for _ in 0..10 {
            let result = limiter.allow(0).await;
            assert!(result.unwrap());
        }
        let result = limiter.allow(0).await;
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_quota_limiter_window_expiry_reset() {
        // 使用 1 秒窗口，等待过期后验证重置
        let mut config = create_test_config();
        config.window_size = 1; // 1 秒窗口
        config.limit = 3;

        let limiter = QuotaLimiter::new(config);

        // 用完配额
        for _ in 0..3 {
            assert!(limiter.check("user1").await.is_ok());
        }
        // 此时应被拒绝
        assert!(limiter.check("user1").await.is_err());

        // 等待窗口过期
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // 窗口重置后应再次允许
        let result = limiter.check("user1").await;
        assert!(
            result.is_ok(),
            "Request after window reset should be allowed"
        );
    }

    #[tokio::test]
    async fn test_quota_limiter_overdraft_boundary() {
        // 测试透支边界：limit + overdraft_limit 恰好用完
        let mut config = create_test_config();
        config.limit = 10;
        config.allow_overdraft = true;
        config.overdraft_limit_percent = 50; // 50% overdraft = 5 extra

        let limiter = QuotaLimiter::new(config);

        // Should allow 10 + 5 = 15 requests
        for i in 0..15 {
            let result = limiter.check("user1").await;
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }

        // 16th should be rejected
        let result = limiter.check("user1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_quota_limiter_check_propagates_error() {
        // 当 check_and_consume 返回 Err 时，check 应传播错误
        let config = create_test_config();
        let limiter = QuotaLimiter::new(config);

        // 用完配额
        for _ in 0..10 {
            let _ = limiter.check("user1").await;
        }

        // 下一个 check 应返回 QuotaExceeded 错误
        let result = limiter.check("user1").await;
        assert!(result.is_err());
        assert!(matches!(result, Err(LimiteronError::QuotaExceeded(_))));
    }

    // ========================================================================
    // audit-macro-followup 修复20 (L-003): window_size=0 panic 测试
    // ========================================================================

    #[test]
    #[should_panic(expected = "QuotaConfig.window_size must be greater than 0")]
    fn test_quota_limiter_window_size_zero_panics() {
        // audit-L-003: window_size=0 会导致窗口立即过期，配额限制失效
        // 应在 new() 阶段 panic 而非静默接受错误配置（Rule 12）
        let mut config = create_test_config();
        config.window_size = 0;
        let _ = QuotaLimiter::new(config);
    }

    #[tokio::test]
    async fn test_quota_limiter_window_size_one_works() {
        // 边界：window_size=1（最小合法值）应正常工作
        let mut config = create_test_config();
        config.window_size = 1;
        config.limit = 3;
        let limiter = QuotaLimiter::new(config);

        // window_size=1s，前 3 个请求应成功
        for _ in 0..3 {
            assert!(limiter.check("boundary_user").await.is_ok());
        }
        // 第 4 个应失败
        assert!(limiter.check("boundary_user").await.is_err());
    }
}
