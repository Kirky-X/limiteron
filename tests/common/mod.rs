#![allow(dead_code)]
#![allow(unused_imports)]

use ahash::AHashMap;
use limiteron::config::{
    Action, ActionConfig, CacheBackend, FlowControlConfig as GovernorConfig, LimiterConfig,
    Matcher, MetricsBackend, Rule, StorageType,
};
use limiteron::error::{ConsumeResult, StorageError};
use limiteron::limiters::{
    ConcurrencyLimiter, FixedWindowLimiter, Limiter, ShardedSlidingWindowLimiter,
    TokenBucketLimiter,
};
use limiteron::Governor;
use limiteron::{BanHistory, BanRecord, BanStorage, BanTarget, QuotaInfo, QuotaStorage, Storage};
use rand::Rng;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

// ==================== Mock Storage ====================

/// MockStorage 是 MockQuotaStorage 的别名，用于通用存储测试
pub type MockStorage = MockQuotaStorageInner;

/// 独立的 MockStorage 实现，用于通用存储测试
#[derive(Clone)]
pub struct MockQuotaStorageInner {
    data: Arc<RwLock<AHashMap<String, MockKvEntry>>>,
    error: Arc<RwLock<Option<StorageError>>>,
}

impl MockQuotaStorageInner {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(AHashMap::new())),
            error: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn inject_error(&self, error: StorageError) {
        let mut current = self.error.write().await;
        *current = Some(error);
    }

    pub async fn clear_error(&self) {
        let mut current = self.error.write().await;
        *current = None;
    }

    async fn check_error(&self) -> Result<(), StorageError> {
        let current = self.error.read().await;
        if let Some(ref err) = *current {
            return Err(err.clone());
        }
        Ok(())
    }
}

impl Default for MockQuotaStorageInner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Storage for MockQuotaStorageInner {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        self.check_error().await?;
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

    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError> {
        self.check_error().await?;
        let mut data = self.data.write().await;
        let expires_at = ttl.map(|t| chrono::Utc::now() + chrono::Duration::seconds(t as i64));
        data.insert(
            key.to_string(),
            MockKvEntry {
                value: value.to_string(),
                expires_at,
            },
        );
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.check_error().await?;
        let mut data = self.data.write().await;
        data.remove(key);
        Ok(())
    }
}

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
        #[allow(clippy::map_clone)]
        let mut records: Vec<_> = bans.values().map(|r| r.clone()).collect();

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
            storage: StorageType::Memory,
            cache: CacheBackend::Memory,
            metrics: MetricsBackend::Prometheus,
            trusted_proxies: Default::default(),
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
                on_exceed: Action::Reject,
                ban: None,
            },
        }],
    };
    let storage: Arc<dyn Storage> = Arc::new(MockQuotaStorage::new());
    let ban_storage: Arc<dyn BanStorage> = Arc::new(MockBanStorage::new());

    Arc::new(
        Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
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

pub fn create_ban_record(target: BanTarget, duration_secs: u64, reason: &str) -> BanRecord {
    let now = chrono::Utc::now();
    BanRecord {
        target,
        ban_times: 1,
        duration: Duration::from_secs(duration_secs),
        banned_at: now,
        expires_at: now + chrono::Duration::seconds(duration_secs as i64),
        is_manual: false,
        reason: reason.to_string(),
    }
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

pub fn create_sliding_window_limiter(
    window: Duration,
    max_requests: u64,
) -> ShardedSlidingWindowLimiter {
    ShardedSlidingWindowLimiter::new(window, max_requests)
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

// ==================== 增强的 Mock 行为配置 ====================

impl MockQuotaBehavior {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_fail_mode(mut self, fail: bool) -> Self {
        self.fail_mode = fail;
        self
    }

    pub fn with_force_over_limit(mut self, force: bool) -> Self {
        self.force_over_limit = force;
        self
    }

    pub fn with_force_expired(mut self, force: bool) -> Self {
        self.force_expired = force;
        self
    }

    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = Some(max);
        self
    }
}

impl MockBanBehavior {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_fail_mode(mut self, fail: bool) -> Self {
        self.fail_mode = fail;
        self
    }

    pub fn with_force_expired(mut self, force: bool) -> Self {
        self.force_expired = force;
        self
    }

    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = Some(max);
        self
    }
}

// ==================== RequestContext 构建器 ====================

pub struct RequestContextBuilder {
    ctx: limiteron::matchers::RequestContext,
}

impl RequestContextBuilder {
    pub fn new() -> Self {
        Self {
            ctx: limiteron::matchers::RequestContext::new(),
        }
    }

    pub fn user_id(mut self, user_id: &str) -> Self {
        self.ctx.user_id = Some(user_id.to_string());
        self
    }

    pub fn ip(mut self, ip: &str) -> Self {
        self.ctx.ip = Some(ip.to_string());
        self.ctx.client_ip = Some(ip.to_string());
        self
    }

    pub fn mac(mut self, mac: &str) -> Self {
        self.ctx.mac = Some(mac.to_string());
        self
    }

    pub fn device_id(mut self, device_id: &str) -> Self {
        self.ctx.device_id = Some(device_id.to_string());
        self
    }

    pub fn api_key(mut self, api_key: &str) -> Self {
        self.ctx.api_key = Some(api_key.to_string());
        self
    }

    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.ctx
            .headers
            .insert(key.to_lowercase(), value.to_string());
        self
    }

    pub fn path(mut self, path: &str) -> Self {
        self.ctx.path = path.to_string();
        self
    }

    pub fn method(mut self, method: &str) -> Self {
        self.ctx.method = method.to_string();
        self
    }

    pub fn query_param(mut self, key: &str, value: &str) -> Self {
        self.ctx
            .query_params
            .insert(key.to_string(), value.to_string());
        self
    }

    pub fn build(self) -> limiteron::matchers::RequestContext {
        self.ctx
    }
}

impl Default for RequestContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== 随机数据生成器 ====================

pub fn generate_user_id() -> String {
    format!("user_{}", generate_random_string(8))
}

pub fn generate_ip() -> String {
    format!(
        "{}.{}.{}.{}",
        rand::random::<u8>() % 254 + 1,
        rand::random::<u8>(),
        rand::random::<u8>(),
        rand::random::<u8>() % 254 + 1
    )
}

pub fn generate_mac() -> String {
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        rand::random::<u8>(),
        rand::random::<u8>(),
        rand::random::<u8>(),
        rand::random::<u8>(),
        rand::random::<u8>(),
        rand::random::<u8>()
    )
}

pub fn generate_api_key() -> String {
    format!("sk_{}", generate_random_string(32))
}

pub fn generate_device_id() -> String {
    format!("device_{}", generate_random_string(16))
}

pub fn generate_random_string(length: usize) -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    (0..length)
        .map(|_| {
            let idx = (rand::random::<u64>() % CHARSET.len() as u64) as usize;
            CHARSET[idx] as char
        })
        .collect()
}

// ==================== 专用断言宏 ====================

#[macro_export]
macro_rules! assert_allowed {
    ($result:expr) => {
        assert!(
            $result.allowed,
            "Expected request to be allowed, but was denied"
        );
    };
    ($result:expr, $msg:expr) => {
        assert!($result.allowed, "{}", $msg);
    };
}

#[macro_export]
macro_rules! assert_denied {
    ($result:expr) => {
        assert!(
            !$result.allowed,
            "Expected request to be denied, but was allowed"
        );
    };
    ($result:expr, $msg:expr) => {
        assert!(!$result.allowed, "{}", $msg);
    };
}

#[macro_export]
macro_rules! assert_remaining {
    ($result:expr, $expected:expr) => {
        assert_eq!(
            $result.remaining, $expected,
            "Expected remaining {}, got {}",
            $expected, $result.remaining
        );
    };
}

#[cfg(feature = "circuit-breaker")]
#[macro_export]
macro_rules! assert_circuit_closed {
    ($breaker:expr) => {
        assert!(
            $breaker.is_closed(),
            "Expected circuit breaker to be closed"
        );
    };
}

#[cfg(feature = "circuit-breaker")]
#[macro_export]
macro_rules! assert_circuit_open {
    ($breaker:expr) => {
        assert!($breaker.is_open(), "Expected circuit breaker to be open");
    };
}

#[cfg(feature = "circuit-breaker")]
#[macro_export]
macro_rules! assert_circuit_half_open {
    ($breaker:expr) => {
        assert!(
            $breaker.is_half_open(),
            "Expected circuit breaker to be half-open"
        );
    };
}

#[cfg(feature = "ban-manager")]
#[macro_export]
macro_rules! assert_banned {
    ($result:expr) => {
        assert!($result.is_some(), "Expected target to be banned");
    };
    ($result:expr, $msg:expr) => {
        assert!($result.is_some(), "{}", $msg);
    };
}

#[cfg(feature = "ban-manager")]
#[macro_export]
macro_rules! assert_not_banned {
    ($result:expr) => {
        assert!($result.is_none(), "Expected target to NOT be banned");
    };
    ($result:expr, $msg:expr) => {
        assert!($result.is_none(), "{}", $msg);
    };
}

#[macro_export]
macro_rules! assert_quota_usage {
    ($result:expr, $expected_percent:expr) => {
        let tolerance = 1.0;
        let diff = ($result.usage_percent - $expected_percent).abs();
        assert!(
            diff <= tolerance,
            "Expected usage percent {}%, got {}%",
            $expected_percent,
            $result.usage_percent
        );
    };
}

#[macro_export]
macro_rules! assert_alert_triggered {
    ($result:expr) => {
        assert!($result.alert_triggered, "Expected alert to be triggered");
    };
}

#[macro_export]
macro_rules! assert_no_alert {
    ($result:expr) => {
        assert!(
            !$result.alert_triggered,
            "Expected NO alert to be triggered"
        );
    };
}

// ==================== 配置生成器 ====================

pub fn create_basic_rule(rule_id: &str, capacity: u64, refill_rate: u64) -> Rule {
    Rule {
        id: rule_id.to_string(),
        name: format!("Test Rule {}", rule_id),
        priority: 100,
        matchers: vec![Matcher::User {
            user_ids: vec!["*".to_string()],
        }],
        limiters: vec![LimiterConfig::TokenBucket {
            capacity,
            refill_rate,
        }],
        action: ActionConfig {
            on_exceed: Action::Reject,
            ban: None,
        },
    }
}

pub fn create_sliding_window_rule(rule_id: &str, window_secs: u64, max_requests: u64) -> Rule {
    Rule {
        id: rule_id.to_string(),
        name: format!("Sliding Window Rule {}", rule_id),
        priority: 100,
        matchers: vec![Matcher::User {
            user_ids: vec!["*".to_string()],
        }],
        limiters: vec![LimiterConfig::SlidingWindow {
            window_size: format!("{}s", window_secs),
            max_requests,
        }],
        action: ActionConfig {
            on_exceed: Action::Reject,
            ban: None,
        },
    }
}

pub fn create_ip_rule(rule_id: &str, ips: &[&str], capacity: u64, refill_rate: u64) -> Rule {
    Rule {
        id: rule_id.to_string(),
        name: format!("IP Rule {}", rule_id),
        priority: 100,
        matchers: vec![Matcher::Ip {
            ip_ranges: ips.iter().map(|s| s.to_string()).collect(),
        }],
        limiters: vec![LimiterConfig::TokenBucket {
            capacity,
            refill_rate,
        }],
        action: ActionConfig {
            on_exceed: Action::Reject,
            ban: None,
        },
    }
}

// ==================== 封禁记录生成器 ====================
// 注意: create_ban_record 的未门控版本定义在文件上方（第 421 行），
// 此处仅保留 create_ip_ban_record / create_user_ban_record 的 ban-manager 门控版本。

#[cfg(feature = "ban-manager")]
pub fn create_ip_ban_record(ip: &str, duration_secs: u64) -> BanRecord {
    create_ban_record(BanTarget::Ip(ip.to_string()), duration_secs, "test ban")
}

#[cfg(feature = "ban-manager")]
pub fn create_user_ban_record(user_id: &str, duration_secs: u64) -> BanRecord {
    create_ban_record(
        BanTarget::UserId(user_id.to_string()),
        duration_secs,
        "test ban",
    )
}

// ==================== 测试夹具 ====================

pub struct TestFixture {
    pub user_ids: Vec<String>,
    pub ips: Vec<String>,
    pub macs: Vec<String>,
    pub api_keys: Vec<String>,
    pub device_ids: Vec<String>,
}

impl TestFixture {
    pub fn new() -> Self {
        Self {
            user_ids: Vec::new(),
            ips: Vec::new(),
            macs: Vec::new(),
            api_keys: Vec::new(),
            device_ids: Vec::new(),
        }
    }

    pub fn with_users(mut self, count: usize) -> Self {
        self.user_ids = (0..count).map(|_| generate_user_id()).collect();
        self
    }

    pub fn with_ips(mut self, count: usize) -> Self {
        self.ips = (0..count).map(|_| generate_ip()).collect();
        self
    }

    pub fn with_macs(mut self, count: usize) -> Self {
        self.macs = (0..count).map(|_| generate_mac()).collect();
        self
    }

    pub fn with_api_keys(mut self, count: usize) -> Self {
        self.api_keys = (0..count).map(|_| generate_api_key()).collect();
        self
    }

    pub fn with_device_ids(mut self, count: usize) -> Self {
        self.device_ids = (0..count).map(|_| generate_device_id()).collect();
        self
    }

    pub fn build(self) -> Self {
        self
    }
}

impl Default for TestFixture {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== Mock 存储单元测试 ====================

#[cfg(test)]
mod mock_storage_tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_quota_storage_basic_operations() {
        let storage = MockQuotaStorage::new();

        // 测试基本 KV 操作
        storage.set("key1", "value1", None).await.unwrap();
        let value = storage.get("key1").await.unwrap();
        assert_eq!(value, Some("value1".to_string()));

        storage.delete("key1").await.unwrap();
        let value = storage.get("key1").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_mock_quota_storage_with_ttl() {
        let storage = MockQuotaStorage::new();

        storage.set("key1", "value1", Some(1)).await.unwrap();
        let value = storage.get("key1").await.unwrap();
        assert_eq!(value, Some("value1".to_string()));

        tokio::time::sleep(Duration::from_millis(1100)).await;
        let value = storage.get("key1").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_mock_quota_storage_fail_mode() {
        let storage =
            MockQuotaStorage::with_behavior(MockQuotaBehavior::new().with_fail_mode(true));

        let result = storage.get("key1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_quota_storage_quota_operations() {
        let storage = MockQuotaStorage::new();

        // 测试配额消费
        let result = storage
            .consume("user1", "resource1", 10, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 90);

        // 测试配额获取
        let quota = storage.get_quota("user1", "resource1").await.unwrap();
        assert!(quota.is_some());
        assert_eq!(quota.unwrap().consumed, 10);

        // 测试配额重置
        storage
            .reset("user1", "resource1", 100, Duration::from_secs(60))
            .await
            .unwrap();
        let quota = storage.get_quota("user1", "resource1").await.unwrap();
        assert!(quota.is_none());
    }

    #[tokio::test]
    async fn test_mock_quota_storage_over_limit() {
        let storage = MockQuotaStorage::new();

        // 消费到限制
        let result = storage
            .consume("user1", "resource1", 100, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(result.allowed);

        // 超过限制
        let result = storage
            .consume("user1", "resource1", 1, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_mock_quota_storage_force_over_limit() {
        let storage =
            MockQuotaStorage::with_behavior(MockQuotaBehavior::new().with_force_over_limit(true));

        let result = storage
            .consume("user1", "resource1", 1, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!result.allowed);
        assert_eq!(result.remaining, 0);
    }

    #[tokio::test]
    async fn test_mock_quota_storage_force_expired() {
        let storage = MockQuotaStorage::new();

        // 先消费
        let result = storage
            .consume("user1", "resource1", 10, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(result.allowed);

        // 强制过期
        storage
            .set_behavior(MockQuotaBehavior::new().with_force_expired(true))
            .await;

        let quota = storage.get_quota("user1", "resource1").await.unwrap();
        assert!(quota.is_none());
    }

    #[tokio::test]
    async fn test_mock_quota_storage_max_entries() {
        let storage = MockQuotaStorage::with_behavior(MockQuotaBehavior::new().with_max_entries(2));

        storage.set("key1", "value1", None).await.unwrap();
        storage.set("key2", "value2", None).await.unwrap();

        let result = storage.set("key3", "value3", None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_ban_storage_basic_operations() {
        let storage = MockBanStorage::new();
        let target = BanTarget::Ip("192.168.1.1".to_string());

        // 测试未封禁
        let banned = storage.is_banned(&target).await.unwrap();
        assert!(banned.is_none());

        // 测试封禁
        let record = create_ban_record(target.clone(), 60, "test ban");
        storage.save(&record).await.unwrap();

        let banned = storage.is_banned(&target).await.unwrap();
        assert!(banned.is_some());

        // 测试解封
        storage.remove_ban(&target).await.unwrap();
        let banned = storage.is_banned(&target).await.unwrap();
        assert!(banned.is_none());
    }

    #[tokio::test]
    async fn test_mock_ban_storage_expiry() {
        let storage = MockBanStorage::new();
        let target = BanTarget::UserId("user1".to_string());

        // 创建即将过期的封禁
        let record = BanRecord {
            target: target.clone(),
            ban_times: 1,
            duration: Duration::from_millis(1000),
            banned_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::milliseconds(1000),
            is_manual: false,
            reason: "test".to_string(),
        };

        storage.save(&record).await.unwrap();

        let banned = storage.is_banned(&target).await.unwrap();
        assert!(banned.is_some());

        // 等待过期
        tokio::time::sleep(Duration::from_millis(1100)).await;

        let banned = storage.is_banned(&target).await.unwrap();
        assert!(banned.is_none());
    }

    #[tokio::test]
    async fn test_mock_ban_storage_fail_mode() {
        let storage = MockBanStorage::with_behavior(MockBanBehavior::new().with_fail_mode(true));

        let target = BanTarget::Ip("192.168.1.1".to_string());
        let result = storage.is_banned(&target).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_ban_storage_force_expired() {
        let storage = MockBanStorage::new();
        let target = BanTarget::Ip("192.168.1.1".to_string());

        // 创建封禁
        let record = create_ban_record(target.clone(), 60, "test ban");
        storage.save(&record).await.unwrap();

        // 强制过期
        storage
            .set_behavior(MockBanBehavior::new().with_force_expired(true))
            .await;

        let banned = storage.is_banned(&target).await.unwrap();
        assert!(banned.is_none());
    }

    #[tokio::test]
    async fn test_mock_ban_storage_history() {
        let storage = MockBanStorage::new();
        let target = BanTarget::UserId("user1".to_string());

        // 创建封禁
        let record = create_ban_record(target.clone(), 60, "test ban");
        storage.save(&record).await.unwrap();

        // 检查历史
        let history = storage.get_history(&target).await.unwrap();
        assert!(history.is_some());
        assert_eq!(history.unwrap().ban_times, 1);
    }

    #[tokio::test]
    async fn test_mock_ban_storage_increment_ban_times() {
        let storage = MockBanStorage::new();
        let target = BanTarget::UserId("user1".to_string());

        // 创建封禁
        let record = create_ban_record(target.clone(), 60, "test ban");
        storage.save(&record).await.unwrap();

        // 增加封禁次数
        let times = storage.increment_ban_times(&target).await.unwrap();
        assert_eq!(times, 2);

        let times = storage.get_ban_times(&target).await.unwrap();
        assert_eq!(times, 2);
    }

    #[tokio::test]
    async fn test_mock_ban_storage_cleanup_expired() {
        let storage = MockBanStorage::new();

        // 创建多个封禁
        let target1 = BanTarget::Ip("192.168.1.1".to_string());
        let target2 = BanTarget::Ip("192.168.1.2".to_string());

        // 即将过期的封禁
        let record1 = BanRecord {
            target: target1,
            ban_times: 1,
            duration: Duration::from_millis(50),
            banned_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::milliseconds(50),
            is_manual: false,
            reason: "test".to_string(),
        };

        // 长期封禁
        let record2 = create_ban_record(target2, 3600, "long ban");

        storage.save(&record1).await.unwrap();
        storage.save(&record2).await.unwrap();

        // 等待第一个过期
        tokio::time::sleep(Duration::from_millis(100)).await;

        let cleaned = storage.cleanup_expired_bans().await.unwrap();
        assert_eq!(cleaned, 1);
    }

    #[tokio::test]
    async fn test_mock_ban_storage_list_bans() {
        let storage = MockBanStorage::new();

        // 创建多个封禁
        for i in 0..5 {
            let target = BanTarget::Ip(format!("192.168.1.{}", i));
            let record = create_ban_record(target, 3600, "test ban");
            storage.save(&record).await.unwrap();
        }

        // 测试分页
        let bans = storage.list_bans(true, 0, 3).await.unwrap();
        assert_eq!(bans.len(), 3);

        let bans = storage.list_bans(true, 3, 3).await.unwrap();
        assert_eq!(bans.len(), 2);
    }

    #[tokio::test]
    async fn test_mock_ban_storage_max_entries() {
        let storage = MockBanStorage::with_behavior(MockBanBehavior::new().with_max_entries(2));

        let target1 = BanTarget::Ip("192.168.1.1".to_string());
        let target2 = BanTarget::Ip("192.168.1.2".to_string());
        let target3 = BanTarget::Ip("192.168.1.3".to_string());

        let record1 = create_ban_record(target1, 60, "test");
        let record2 = create_ban_record(target2, 60, "test");
        let record3 = create_ban_record(target3, 60, "test");

        storage.save(&record1).await.unwrap();
        storage.save(&record2).await.unwrap();

        let result = storage.save(&record3).await;
        assert!(result.is_err());
    }
}

/// Mock 存储基础设施测试
///
/// 这些测试验证 Mock 存储实现的正确性，确保测试基础设施可靠。
/// 已从 integration 目录移动到 common 目录，因为它们测试的是测试工具本身。
mod mock_tests;
