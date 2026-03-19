#![allow(dead_code)]
#![allow(unused_imports)]

use ahash::AHashMap;
use limiteron::error::{ConsumeResult, StorageError};
use limiteron::limiters::{
    ConcurrencyLimiter, FixedWindowLimiter, Limiter, SlidingWindowLimiter, TokenBucketLimiter,
};
use limiteron::storage_trait::{
    BanHistory, BanRecord, BanStorage, BanTarget, QuotaInfo, QuotaStorage, Storage,
};
use limiteron::config::{ActionConfig, FlowControlConfig as GovernorConfig, LimiterConfig, Matcher, Rule};
use limiteron::Governor;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

// ==================== Mock Storage ====================

#[derive(Clone, Default)]
pub struct MockQuotaBehavior {
    fail_mode: bool,
    force_over_limit: bool,
    force_expired: bool,
    max_entries: Option<usize>,
}

#[derive(Clone, Default)]
pub struct MockBanBehavior {
    fail_mode: bool,
    force_expired: bool,
    max_entries: Option<usize>,
}

#[derive(Clone)]
struct MockKvEntry {
    value: String,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone)]
struct MockQuotaEntry {
    consumed: u64,
    limit: u64,
    window_start: chrono::DateTime<chrono::Utc>,
    window_end: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone)]
pub struct MockQuotaStorage {
    data: Arc<RwLock<AHashMap<String, MockKvEntry>>>,
    quotas: Arc<RwLock<AHashMap<String, MockQuotaEntry>>>,
    behavior: Arc<RwLock<MockQuotaBehavior>>,
}

#[derive(Clone)]
pub struct MockBanStorage {
    bans: Arc<RwLock<AHashMap<BanTarget, BanRecord>>>,
    history: Arc<RwLock<AHashMap<BanTarget, BanHistory>>>,
    behavior: Arc<RwLock<MockBanBehavior>>,
}

impl MockBanStorage {
    pub fn new() -> Self {
        Self::with_behavior(MockBanBehavior::default())
    }

    pub fn with_behavior(behavior: MockBanBehavior) -> Self {
        Self {
            bans: Arc::new(RwLock::new(AHashMap::new())),
            history: Arc::new(RwLock::new(AHashMap::new())),
            behavior: Arc::new(RwLock::new(behavior)),
        }
    }

    pub async fn set_behavior(&self, behavior: MockBanBehavior) {
        let mut current = self.behavior.write().await;
        *current = behavior;
    }

    pub async fn clear(&self) {
        let mut bans = self.bans.write().await;
        let mut history = self.history.write().await;
        bans.clear();
        history.clear();
    }

    async fn should_fail(&self) -> bool {
        self.behavior.read().await.fail_mode
    }

    async fn is_force_expired(&self) -> bool {
        self.behavior.read().await.force_expired
    }

    async fn can_insert(&self, current_len: usize) -> Result<(), StorageError> {
        let behavior = self.behavior.read().await;
        if let Some(max_entries) = behavior.max_entries {
            if current_len >= max_entries {
                return Err(StorageError::QueryError("超过最大封禁条目限制".to_string()));
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl BanStorage for MockBanStorage {
    async fn is_banned(
        &self,
        target: &BanTarget,
    ) -> Result<Option<BanRecord>, limiteron::error::StorageError> {
        if self.should_fail().await {
            return Err(StorageError::QueryError(
                "MockBanStorage is_banned失败".to_string(),
            ));
        }

        if self.is_force_expired().await {
            return Ok(None);
        }

        let mut bans = self.bans.write().await;
        let now = chrono::Utc::now();
        if let Some(record) = bans.get(target) {
            if record.expires_at > now {
                return Ok(Some(record.clone()));
            }
        }
        bans.remove(target);
        Ok(None)
    }

    async fn save(&self, record: &BanRecord) -> Result<(), limiteron::error::StorageError> {
        if self.should_fail().await {
            return Err(StorageError::QueryError(
                "MockBanStorage save失败".to_string(),
            ));
        }

        let mut bans = self.bans.write().await;
        self.can_insert(bans.len()).await?;
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
        if self.should_fail().await {
            return Err(StorageError::QueryError(
                "MockBanStorage get_history失败".to_string(),
            ));
        }

        let history = self.history.read().await;
        Ok(history.get(target).cloned())
    }

    async fn increment_ban_times(
        &self,
        target: &BanTarget,
    ) -> Result<u64, limiteron::error::StorageError> {
        if self.should_fail().await {
            return Err(StorageError::QueryError(
                "MockBanStorage increment_ban_times失败".to_string(),
            ));
        }

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
        if self.should_fail().await {
            return Err(StorageError::QueryError(
                "MockBanStorage get_ban_times失败".to_string(),
            ));
        }

        let bans = self.bans.read().await;
        if let Some(record) = bans.get(target) {
            Ok(record.ban_times as u64)
        } else {
            Ok(0)
        }
    }

    async fn remove_ban(&self, target: &BanTarget) -> Result<(), limiteron::error::StorageError> {
        if self.should_fail().await {
            return Err(StorageError::QueryError(
                "MockBanStorage remove_ban失败".to_string(),
            ));
        }

        let mut bans = self.bans.write().await;
        bans.remove(target);
        Ok(())
    }

    async fn cleanup_expired_bans(&self) -> Result<u64, limiteron::error::StorageError> {
        if self.should_fail().await {
            return Err(StorageError::QueryError(
                "MockBanStorage cleanup_expired_bans失败".to_string(),
            ));
        }

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

    async fn list_bans(
        &self,
        active_only: bool,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BanRecord>, StorageError> {
        if self.should_fail().await {
            return Err(StorageError::QueryError(
                "MockBanStorage list_bans失败".to_string(),
            ));
        }

        let bans = self.bans.read().await;
        let now = chrono::Utc::now();
        let mut records: Vec<_> = bans.values().cloned().collect();

        if active_only {
            records.retain(|r| r.expires_at > now);
        }

        let total = records.len() as u64;
        let start = offset as usize;
        let end = (offset.saturating_add(limit)) as usize;

        if start >= total as usize {
            return Ok(vec![]);
        }

        Ok(records.into_iter().skip(start).take(end - start).collect())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ==================== Test Helpers ====================

pub async fn create_governor() -> Arc<Governor> {
    let config = GovernorConfig {
        version: "1.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
        },
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 1000,
                refill_rate: 100,
            }],
            action: ActionConfig {
                on_exceed: "reject".to_string(),
                ban: None,
            },
        }],
    };
    let storage: Arc<dyn Storage> = Arc::new(MockQuotaStorage::new());
    let ban_storage: Arc<dyn BanStorage> = Arc::new(MockBanStorage::new());

    Arc::new(
        Governor::new(
            config,
            storage,
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
    headers.insert("x-user-id".to_string(), user_id.to_string());

    let mut ctx = limiteron::matchers::RequestContext::new();
    ctx.ip = Some(ip.to_string());
    ctx.method = "GET".to_string();
    ctx.path = "/test".to_string();
    ctx.headers = headers;
    ctx
}

impl MockQuotaStorage {
    pub fn new() -> Self {
        Self::with_behavior(MockQuotaBehavior::default())
    }

    pub fn with_behavior(behavior: MockQuotaBehavior) -> Self {
        Self {
            data: Arc::new(RwLock::new(AHashMap::new())),
            quotas: Arc::new(RwLock::new(AHashMap::new())),
            behavior: Arc::new(RwLock::new(behavior)),
        }
    }

    pub async fn set_behavior(&self, behavior: MockQuotaBehavior) {
        let mut current = self.behavior.write().await;
        *current = behavior;
    }

    pub async fn clear(&self) {
        let mut data = self.data.write().await;
        let mut quotas = self.quotas.write().await;
        data.clear();
        quotas.clear();
    }

    async fn should_fail(&self) -> bool {
        self.behavior.read().await.fail_mode
    }

    async fn should_force_over_limit(&self) -> bool {
        self.behavior.read().await.force_over_limit
    }

    async fn should_force_expired(&self) -> bool {
        self.behavior.read().await.force_expired
    }

    async fn can_insert(&self, current_len: usize) -> Result<(), StorageError> {
        let behavior = self.behavior.read().await;
        if let Some(max_entries) = behavior.max_entries {
            if current_len >= max_entries {
                return Err(StorageError::QueryError("超过最大配额条目限制".to_string()));
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Storage for MockQuotaStorage {
    async fn get(&self, key: &str) -> Result<Option<String>, limiteron::error::StorageError> {
        if self.should_fail().await {
            return Err(StorageError::QueryError(
                "MockQuotaStorage get失败".to_string(),
            ));
        }

        let mut data = self.data.write().await;
        if let Some(entry) = data.get(key) {
            if let Some(expires_at) = entry.expires_at {
                if expires_at <= chrono::Utc::now() {
                    data.remove(key);
                    return Ok(None);
                }
            }
            return Ok(Some(entry.value.clone()));
        }
        Ok(None)
    }

    async fn set(
        &self,
        key: &str,
        value: &str,
        ttl: Option<u64>,
    ) -> Result<(), limiteron::error::StorageError> {
        if self.should_fail().await {
            return Err(StorageError::QueryError(
                "MockQuotaStorage set失败".to_string(),
            ));
        }

        let mut data = self.data.write().await;
        self.can_insert(data.len()).await?;
        let expires_at = ttl.map(|ttl| chrono::Utc::now() + chrono::Duration::seconds(ttl as i64));
        data.insert(
            key.to_string(),
            MockKvEntry {
                value: value.to_string(),
                expires_at,
            },
        );
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), limiteron::error::StorageError> {
        if self.should_fail().await {
            return Err(StorageError::QueryError(
                "MockQuotaStorage delete失败".to_string(),
            ));
        }

        let mut data = self.data.write().await;
        data.remove(key);
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
        if self.should_fail().await {
            return Err(StorageError::QueryError(
                "MockQuotaStorage get_quota失败".to_string(),
            ));
        }

        let key = format!("{}:{}", user_id, resource);
        let mut quotas = self.quotas.write().await;
        let now = chrono::Utc::now();
        if self.should_force_expired().await {
            quotas.remove(&key);
            return Ok(None);
        }

        if let Some(entry) = quotas.get(&key) {
            if entry.window_end <= now {
                quotas.remove(&key);
                return Ok(None);
            }

            return Ok(Some(QuotaInfo {
                consumed: entry.consumed,
                limit: entry.limit,
                window_start: entry.window_start,
                window_end: entry.window_end,
            }));
        }

        Ok(None)
    }

    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
        limit: u64,
        window: Duration,
    ) -> Result<ConsumeResult, StorageError> {
        if self.should_fail().await {
            return Err(StorageError::QueryError(
                "MockQuotaStorage consume失败".to_string(),
            ));
        }

        if self.should_force_over_limit().await {
            return Ok(ConsumeResult {
                allowed: false,
                remaining: 0,
                alert_triggered: true,
                usage_percent: 100.0,
            });
        }

        let key = format!("{}:{}", user_id, resource);
        let mut quotas = self.quotas.write().await;
        self.can_insert(quotas.len()).await?;
        let now = chrono::Utc::now();
        let window_end =
            now + chrono::Duration::from_std(window).unwrap_or_else(|_| chrono::Duration::hours(1));

        let entry = quotas.entry(key).or_insert(MockQuotaEntry {
            consumed: 0,
            limit,
            window_start: now,
            window_end,
        });

        if entry.window_end <= now || self.should_force_expired().await {
            entry.consumed = 0;
            entry.limit = limit;
            entry.window_start = now;
            entry.window_end = window_end;
        }

        let new_consumed = entry.consumed + cost;
        let allowed = new_consumed <= limit;

        // 计算使用率百分比
        let usage_percent = if limit > 0 {
            (new_consumed as f64 / limit as f64) * 100.0
        } else {
            0.0
        };

        // 判断是否触发告警（默认使用 80% 阈值）
        let alert_threshold = 80.0;
        let alert_triggered = usage_percent >= alert_threshold;

        if allowed {
            entry.consumed = new_consumed;
        }

        Ok(ConsumeResult {
            allowed,
            remaining: limit.saturating_sub(entry.consumed),
            alert_triggered,
            usage_percent,
        })
    }

    async fn reset(
        &self,
        user_id: &str,
        resource: &str,
        _limit: u64,
        _window: Duration,
    ) -> Result<(), StorageError> {
        if self.should_fail().await {
            return Err(StorageError::QueryError(
                "MockQuotaStorage reset失败".to_string(),
            ));
        }

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

use oxcache::Cache;

pub async fn create_test_cache() -> Cache<String, String> {
    Cache::builder()
        .capacity(1000)
        .ttl(Duration::from_secs(60))
        .build()
        .await
        .unwrap()
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
