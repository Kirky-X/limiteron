//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 全局限流器管理器
//!
//! 为 `flow_control` 宏提供全局共享的 limiter 实例。

use super::{ConcurrencyLimiter, FixedWindowLimiter, TokenBucketLimiter};
use ahash::AHashMap as HashMap;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

/// 全局限流器管理器
pub(crate) struct LimiterManager {
    rate_limiters: Mutex<HashMap<String, Arc<TokenBucketLimiter>>>,
    quota_limiters: Mutex<HashMap<String, Arc<FixedWindowLimiter>>>,
    concurrency_limiters: Mutex<HashMap<String, Arc<ConcurrencyLimiter>>>,
}

impl LimiterManager {
    /// 创建新的限流器管理器
    pub fn new() -> Self {
        Self {
            rate_limiters: Mutex::new(HashMap::new()),
            quota_limiters: Mutex::new(HashMap::new()),
            concurrency_limiters: Mutex::new(HashMap::new()),
        }
    }

    /// 获取或创建速率限制器
    pub fn get_rate_limiter(
        &self,
        key: &str,
        capacity: u64,
        refill_rate: u64,
    ) -> Arc<TokenBucketLimiter> {
        let mut limiters = self.rate_limiters.lock();
        if let Some(limiter) = limiters.get(key) {
            return limiter.clone();
        }
        let limiter = Arc::new(TokenBucketLimiter::new(capacity, refill_rate));
        limiters.insert(key.to_string(), limiter.clone());
        limiter
    }

    /// 获取或创建配额限制器
    pub fn get_quota_limiter(
        &self,
        key: &str,
        duration: Duration,
        max_requests: u64,
    ) -> Arc<FixedWindowLimiter> {
        let mut limiters = self.quota_limiters.lock();
        if let Some(limiter) = limiters.get(key) {
            return limiter.clone();
        }
        let limiter = Arc::new(FixedWindowLimiter::new(duration, max_requests));
        limiters.insert(key.to_string(), limiter.clone());
        limiter
    }

    /// 获取或创建并发限制器
    pub fn get_concurrency_limiter(
        &self,
        key: &str,
        max_concurrent: u64,
    ) -> Arc<ConcurrencyLimiter> {
        let mut limiters = self.concurrency_limiters.lock();
        if let Some(limiter) = limiters.get(key) {
            return limiter.clone();
        }
        // 使用带超时的并发限制器，超时时间 50ms
        let limiter = Arc::new(ConcurrencyLimiter::with_timeout(
            max_concurrent,
            Duration::from_millis(50),
        ));
        limiters.insert(key.to_string(), limiter.clone());
        limiter
    }

    /// 清除所有限流器
    pub fn clear(&self) {
        self.rate_limiters.lock().clear();
        self.quota_limiters.lock().clear();
        self.concurrency_limiters.lock().clear();
    }
}

impl Default for LimiterManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_rate_limiter_same_key_returns_same_instance() {
        let manager = LimiterManager::new();
        let first = manager.get_rate_limiter("user:1", 10, 1);
        let second = manager.get_rate_limiter("user:1", 20, 2);
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn test_get_rate_limiter_different_keys_returns_different_instances() {
        let manager = LimiterManager::new();
        let first = manager.get_rate_limiter("user:1", 10, 1);
        let second = manager.get_rate_limiter("user:2", 10, 1);
        assert!(!Arc::ptr_eq(&first, &second));
    }
}

lazy_static::lazy_static! {
    /// 全局限流器管理器实例
    pub(crate) static ref GLOBAL_LIMITER_MANAGER: LimiterManager = LimiterManager::new();
}
