//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! Quota Limiter
//!
//! Implements a simple quota-based limiter that tracks usage per key
//! with configurable limits and time windows.

use crate::error::FlowGuardError;
#[cfg(feature = "quota-control")]
use crate::QuotaConfig;
use dashmap::DashMap;
use std::future::Future;
use std::pin::Pin;
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

impl QuotaLimiter {
    /// Creates a new QuotaLimiter with the given configuration.
    ///
    /// # Arguments
    /// * `config` - Quota configuration including limit, window size, etc.
    ///
    /// # Examples
    /// ```rust
    /// use limiteron::limiters::QuotaLimiter;
    /// use limiteron::QuotaConfig;
    /// use limiteron::QuotaType;
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
        Self {
            config,
            usage: Arc::new(DashMap::new()),
        }
    }

    /// Checks and consumes quota for the given key.
    ///
    /// # Arguments
    /// * `key` - The identifier key (user ID, API key, etc.)
    ///
    /// # Returns
    /// * `Ok(())` - Quota available, consumption successful
    /// * `Err(FlowGuardError)` - Quota exceeded or error
    async fn check_and_consume(&self, key: &str) -> Result<bool, FlowGuardError> {
        let now = Instant::now();
        let window_duration = Duration::from_secs(self.config.window_size);

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
            return Err(FlowGuardError::QuotaExceeded(format!(
                "Quota exceeded for key '{}': used {}/{}",
                key, record.usage, max_usage
            )));
        }

        record.usage += 1;
        Ok(true)
    }
}

impl crate::limiters::Limiter for QuotaLimiter {
    fn allow(
        &self,
        _cost: u64,
    ) -> Pin<Box<dyn Future<Output = Result<bool, FlowGuardError>> + Send + '_>> {
        Box::pin(async move {
            // For quota limiter, we need a key to track, but the allow() method doesn't provide one
            // This is a limitation - quota tracking requires a key
            // Return true to allow the request (quota enforcement happens via check() with key)
            Ok(true)
        })
    }

    fn check(
        &self,
        key: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), FlowGuardError>> + Send + '_>> {
        let key = key.to_string();
        Box::pin(async move {
            self.check_and_consume(&key).await?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limiters::Limiter;
    use crate::QuotaType;

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
        assert!(matches!(result, Err(FlowGuardError::QuotaExceeded(_))));
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
}
