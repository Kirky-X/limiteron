// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
#![allow(dead_code)]
#![allow(unused_imports)]

use ahash::AHashMap;
use limiteron::Governor;
use limiteron::config::{
    Action, ActionConfig, CacheBackend, FlowControlConfig as GovernorConfig, LimiterConfig,
    Matcher, MetricsBackend, Rule, StorageType,
};
use limiteron::error::{ConsumeResult, StorageError};
use limiteron::limiters::{
    ConcurrencyLimiter, FixedWindowLimiter, Limiter, ShardedSlidingWindowLimiter,
    TokenBucketLimiter,
};
use limiteron::tokio::sync::RwLock;
use limiteron::{BanHistory, BanRecord, BanStorage, BanTarget, QuotaInfo, QuotaStorage, Storage};
use rand::Rng;
use std::sync::Arc;
use std::time::Duration;

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
    let storage: Arc<dyn Storage> = Arc::new(limiteron::storage::MemoryStorage::new());
    let ban_storage: Arc<dyn BanStorage> = Arc::new(limiteron::storage::MemoryBanStorage::new());

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

use limiteron::oxcache::Cache;

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

