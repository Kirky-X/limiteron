#![allow(dead_code)]
#![allow(unused_imports)]

use ahash::AHashMap;
use limiteron::error::{ConsumeResult, StorageError};
use limiteron::limiters::{
    ConcurrencyLimiter, FixedWindowLimiter, Limiter, SlidingWindowLimiter, TokenBucketLimiter,
};
use limiteron::storage::{
    BanHistory, BanRecord, BanStorage, BanTarget, QuotaInfo, QuotaStorage, Storage,
};
use limiteron::FlowControlConfig as GovernorConfig;
use limiteron::Governor;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

// ==================== Mock Storage ====================

#[derive(Clone)]
pub struct MockQuotaStorage {
    quotas: Arc<RwLock<AHashMap<String, u64>>>,
}

#[derive(Clone)]
pub struct MockBanStorage {
    bans: Arc<RwLock<AHashMap<BanTarget, BanRecord>>>,
    history: Arc<RwLock<AHashMap<BanTarget, BanHistory>>>,
}

impl MockBanStorage {
    pub fn new() -> Self {
        Self {
            bans: Arc::new(RwLock::new(AHashMap::new())),
            history: Arc::new(RwLock::new(AHashMap::new())),
        }
    }

    pub fn clear(&self) {
        // Clear bans
    }
}

#[async_trait::async_trait]
impl BanStorage for MockBanStorage {
    async fn is_banned(
        &self,
        target: &BanTarget,
    ) -> Result<Option<BanRecord>, limiteron::error::StorageError> {
        let bans = self.bans.read().await;
        Ok(bans.get(target).cloned())
    }

    async fn save(&self, record: &BanRecord) -> Result<(), limiteron::error::StorageError> {
        let mut bans = self.bans.write().await;
        bans.insert(record.target.clone(), record.clone());

        let mut history = self.history.write().await;
        let hist = BanHistory {
            ban_times: record.ban_times,
            last_banned_at: record.banned_at,
        };
        history.insert(record.target.clone(), hist);
        Ok(())
    }

    async fn get_history(
        &self,
        target: &BanTarget,
    ) -> Result<Option<BanHistory>, limiteron::error::StorageError> {
        let history = self.history.read().await;
        Ok(history.get(target).cloned())
    }

    async fn increment_ban_times(
        &self,
        target: &BanTarget,
    ) -> Result<u64, limiteron::error::StorageError> {
        let mut bans = self.bans.write().await;
        if let Some(record) = bans.get_mut(target) {
            record.ban_times += 1;
            Ok(record.ban_times as u64)
        } else {
            Ok(1)
        }
    }

    async fn get_ban_times(
        &self,
        target: &BanTarget,
    ) -> Result<u64, limiteron::error::StorageError> {
        let bans = self.bans.read().await;
        if let Some(record) = bans.get(target) {
            Ok(record.ban_times as u64)
        } else {
            Ok(0)
        }
    }

    async fn remove_ban(&self, target: &BanTarget) -> Result<(), limiteron::error::StorageError> {
        let mut bans = self.bans.write().await;
        bans.remove(target);
        Ok(())
    }

    async fn cleanup_expired_bans(&self) -> Result<u64, limiteron::error::StorageError> {
        let mut bans = self.bans.write().await;
        let now = chrono::Utc::now();
        let mut count = 0;
        bans.retain(|_, record| {
            if record.expires_at <= now {
                count += 1;
                false
            } else {
                true
            }
        });
        Ok(count)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ==================== Test Helpers ====================

pub async fn create_governor() -> Arc<Governor> {
    let config = GovernorConfig::default();
    let storage = Arc::new(MockQuotaStorage::new());
    let ban_storage = Arc::new(MockBanStorage::new());

    Arc::new(
        Governor::new(
            config,
            storage.clone(), // MockQuotaStorage implements Storage now
            ban_storage,
            #[cfg(feature = "monitoring")]
            None,
            #[cfg(feature = "telemetry")]
            None,
        )
        .await
        .expect("Failed to create governor"),
    )
}

pub async fn wait_millis(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

pub async fn wait_secs(secs: u64) {
    tokio::time::sleep(Duration::from_secs(secs)).await;
}

pub fn create_test_request(user_id: &str, ip: &str) -> limiteron::matchers::RequestContext {
    let mut headers = AHashMap::new();
    headers.insert("user-id".to_string(), user_id.to_string());

    let mut ctx = limiteron::matchers::RequestContext::new();
    ctx.ip = Some(ip.to_string());
    ctx.method = "GET".to_string();
    ctx.path = "/test".to_string();
    ctx.headers = headers;
    ctx
}

impl MockQuotaStorage {
    pub fn new() -> Self {
        Self {
            quotas: Arc::new(RwLock::new(AHashMap::new())),
        }
    }

    pub fn clear(&self) {
        // Implementation for clearing storage
    }
}

#[async_trait::async_trait]
impl Storage for MockQuotaStorage {
    async fn get(&self, _key: &str) -> Result<Option<String>, limiteron::error::StorageError> {
        Ok(None)
    }

    async fn set(
        &self,
        _key: &str,
        _value: &str,
        _ttl: Option<u64>,
    ) -> Result<(), limiteron::error::StorageError> {
        Ok(())
    }

    async fn delete(&self, _key: &str) -> Result<(), limiteron::error::StorageError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl QuotaStorage for MockQuotaStorage {
    async fn get_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError> {
        let key = format!("{}:{}", user_id, resource);
        let quotas = self.quotas.read().await;
        if let Some(&used) = quotas.get(&key) {
            // Mock implementation: return dummy quota info since we don't store limits
            Ok(Some(QuotaInfo {
                consumed: used,
                limit: 0, // Unknown in this mock context
                window_start: chrono::Utc::now(),
                window_end: chrono::Utc::now(),
            }))
        } else {
            Ok(None)
        }
    }

    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
        limit: u64,
        _window: Duration,
    ) -> Result<ConsumeResult, StorageError> {
        let key = format!("{}:{}", user_id, resource);
        let mut quotas = self.quotas.write().await;
        let used = quotas.entry(key).or_insert(0);

        let allowed = *used + cost <= limit;
        if allowed {
            *used += cost;
        }

        Ok(ConsumeResult {
            allowed,
            remaining: limit.saturating_sub(*used),
            alert_triggered: false,
        })
    }

    async fn reset(
        &self,
        user_id: &str,
        resource: &str,
        _limit: u64,
        _window: Duration,
    ) -> Result<(), StorageError> {
        let key = format!("{}:{}", user_id, resource);
        let mut quotas = self.quotas.write().await;
        quotas.remove(&key);
        Ok(())
    }
}

// Removed duplicate definitions

pub fn assert_ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(val) => val,
        Err(e) => panic!("Expected Ok, got Err: {:?}", e),
    }
}

pub fn assert_err<T, E: std::fmt::Debug>(result: Result<T, E>) -> E {
    match result {
        Ok(_) => panic!("Expected Err, got Ok"),
        Err(e) => e,
    }
}

pub fn assert_true(value: bool, msg: &str) {
    assert!(value, "{}", msg);
}

pub fn assert_false(value: bool, msg: &str) {
    assert!(!value, "{}", msg);
}

use limiteron::L2Cache;

pub fn create_test_l2_cache() -> L2Cache {
    L2Cache::new(1000, Duration::from_secs(60))
}

pub fn create_token_bucket_limiter(capacity: u64, refill_rate: u64) -> TokenBucketLimiter {
    TokenBucketLimiter::new(capacity, refill_rate)
}

pub fn create_sliding_window_limiter(window: Duration, max_requests: u64) -> SlidingWindowLimiter {
    SlidingWindowLimiter::new(window, max_requests)
}

pub fn create_fixed_window_limiter(window: Duration, max_requests: u64) -> FixedWindowLimiter {
    FixedWindowLimiter::new(window, max_requests)
}

pub fn create_concurrency_limiter(max_concurrent: u64) -> ConcurrencyLimiter {
    ConcurrencyLimiter::new(max_concurrent)
}

pub fn assert_approx_eq(actual: u64, expected: u64, tolerance_percent: f64) {
    let diff = actual.abs_diff(expected);

    let tolerance = (expected as f64 * tolerance_percent / 100.0) as u64;
    assert!(
        diff <= tolerance,
        "Expected {} (approx), got {}",
        expected,
        actual
    );
}
