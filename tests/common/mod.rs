// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
// 公共助手被 5 个测试入口分别取用（common/e2e/integration/security/unified），
// dead_code 与 unused_imports 按 crate 判定：未被某入口用到的助手在该入口
// crate 内必然告警，属跨测试 crate 共享的已知误报，模块级放行。
#![allow(dead_code)]
#![allow(unused_imports)]

use limiteron::Governor;
use limiteron::config::{
    Action, ActionConfig, CacheBackend, FlowControlConfig as GovernorConfig, LimiterConfig,
    Matcher, MetricsBackend, Rule, StorageType,
};
use limiteron::{BanRecord, BanStorage, BanTarget, Storage};
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

use limiteron::oxcache::Cache;

pub async fn create_test_cache() -> Cache<String, String> {
    Cache::builder()
        .capacity(1000)
        .ttl(Duration::from_secs(60))
        .build()
        .await
        .unwrap()
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
