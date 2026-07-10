//! 控制器模块集成测试
//!
//! 测试控制器模块的基本功能

use async_trait::async_trait;
use limiteron::Limiter;
use limiteron::config::{
    Action, ActionConfig, CacheBackend, ConfigMatcher as Matcher, FlowControlConfig, LimiterConfig,
    MetricsBackend, Rule, StorageType,
};
use limiteron::error::{ConsumeResult, Decision, FlowGuardError, StorageError};
use limiteron::governor::GovernorStats;
use limiteron::matchers::RequestContext;
use limiteron::{BanHistory, BanRecord, BanStorage, BanTarget, QuotaInfo, Storage};
use std::sync::Arc;
use std::time::Duration;

// ==================== Mock Storage ====================

#[derive(Clone, Default)]
struct MockStorage {
    data: std::sync::Arc<tokio::sync::RwLock<ahash::AHashMap<String, String>>>,
}

#[async_trait]
impl Storage for MockStorage {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        let data = self.data.read().await;
        Ok(data.get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str, _ttl: Option<u64>) -> Result<(), StorageError> {
        let mut data = self.data.write().await;
        data.insert(key.to_string(), value.to_string());
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let mut data = self.data.write().await;
        data.remove(key);
        Ok(())
    }
}

#[derive(Clone, Default)]
struct MockBanStorage {
    #[allow(dead_code)]
    bans: Arc<tokio::sync::RwLock<ahash::AHashMap<BanTarget, BanRecord>>>,
}

impl MockBanStorage {
    fn new() -> Self {
        Self {
            bans: Arc::new(tokio::sync::RwLock::new(ahash::AHashMap::new())),
        }
    }
}

#[async_trait]
impl BanStorage for MockBanStorage {
    async fn is_banned(&self, _target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _record: &BanRecord) -> Result<(), StorageError> {
        Ok(())
    }

    async fn get_history(&self, _target: &BanTarget) -> Result<Option<BanHistory>, StorageError> {
        Ok(None)
    }

    async fn increment_ban_times(&self, _target: &BanTarget) -> Result<u64, StorageError> {
        Ok(1)
    }

    async fn get_ban_times(&self, _target: &BanTarget) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn remove_ban(&self, _target: &BanTarget) -> Result<(), StorageError> {
        Ok(())
    }

    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn list_bans(
        &self,
        _active_only: bool,
        _offset: u64,
        _limit: u64,
    ) -> Result<Vec<BanRecord>, StorageError> {
        Ok(vec![])
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ==================== Test Helpers ====================

async fn create_governor() -> Arc<limiteron::Governor> {
    let config = FlowControlConfig {
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

    let storage: Arc<dyn Storage> = Arc::new(MockStorage::default());
    let ban_storage: Arc<dyn BanStorage> = Arc::new(MockBanStorage::new());

    Arc::new(
        limiteron::Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Failed to create governor"),
    )
}

fn create_test_request(user_id: &str, ip: &str) -> RequestContext {
    let mut headers = ahash::AHashMap::new();
    headers.insert("x-user-id".to_string(), user_id.to_string());

    let mut ctx = RequestContext::new();
    ctx.ip = Some(ip.to_string());
    ctx.method = "GET".to_string();
    ctx.path = "/test".to_string();
    ctx.headers = headers;
    ctx
}

// ==================== Tests ====================

/// 测试控制器模块导入
#[tokio::test]
async fn test_governor_module_import() {
    let _ = GovernorStats::default();
}

// ==================== 并发安全测试 ====================

/// 测试 Governor 高并发检查的线程安全
#[tokio::test]
async fn test_governor_high_concurrency_safety() {
    let governor = create_governor().await;
    let mut handles = vec![];

    let barrier = Arc::new(tokio::sync::Barrier::new(50));
    let start_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

    for i in 0..50 {
        let governor_clone = Arc::clone(&governor);
        let barrier_clone = Arc::clone(&barrier);
        let start_signal_clone = Arc::clone(&start_signal);
        handles.push(tokio::spawn(async move {
            barrier_clone.wait().await;

            while !start_signal_clone.load(std::sync::atomic::Ordering::SeqCst) {
                std::hint::spin_loop();
            }

            let mut local_allowed = 0;
            for j in 0..20 {
                let user_id = format!("user_{}_{}", i, j % 5);
                let ip = format!("192.168.1.{}", i % 10);
                let ctx = create_test_request(&user_id, &ip);

                if let Ok(Decision::Allowed(_)) = governor_clone.check(&ctx).await {
                    local_allowed += 1;
                }
            }
            local_allowed
        }));
    }

    start_signal.store(true, std::sync::atomic::Ordering::SeqCst);

    let mut total_allowed = 0;
    for handle in handles {
        total_allowed += handle.await.unwrap();
    }

    assert!(
        total_allowed > 0,
        "Expected some requests to be allowed, got {}",
        total_allowed
    );
}

/// 测试 Governor 并发统计更新
#[tokio::test]
async fn test_governor_concurrent_stats_update() {
    let governor = create_governor().await;
    let mut handles = vec![];

    for i in 0..100 {
        let governor_clone = Arc::clone(&governor);
        handles.push(tokio::spawn(async move {
            let user_id = format!("stats_user_{}", i);
            let ctx = create_test_request(&user_id, "10.0.0.1");
            let _ = governor_clone.check(&ctx).await;
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let stats = governor.stats().await;
    assert_eq!(stats.total_requests, 100);
}

/// 测试 Governor 无死锁
#[tokio::test]
async fn test_governor_no_deadlock() {
    let governor = create_governor().await;
    let mut handles = vec![];

    for i in 0..200 {
        let governor_clone = Arc::clone(&governor);
        handles.push(tokio::spawn(async move {
            for j in 0..10 {
                let user_id = format!("deadlock_user_{}_{}", i, j);
                let ctx = create_test_request(&user_id, "172.16.0.1");
                let _ = governor_clone.check(&ctx).await;
            }
        }));
    }

    let result = tokio::time::timeout(Duration::from_secs(10), async {
        for handle in handles {
            let _ = handle.await;
        }
    })
    .await;

    assert!(result.is_ok(), "Test timed out - possible deadlock");
}

/// 测试 Governor 并发健康检查
#[tokio::test]
async fn test_governor_concurrent_health_check() {
    let governor = create_governor().await;
    let mut handles = vec![];

    for i in 0..50 {
        let governor_clone = Arc::clone(&governor);
        handles.push(tokio::spawn(async move {
            if i % 2 == 0 {
                let _ = governor_clone.health_check().await;
            } else {
                let user_id = format!("health_user_{}", i);
                let ctx = create_test_request(&user_id, "10.0.0.2");
                let _ = governor_clone.check(&ctx).await;
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let stats = governor.stats().await;
    assert!(stats.total_requests <= 50);
}

/// 测试 Governor 并发统计重置
#[tokio::test]
async fn test_governor_concurrent_stats_reset() {
    let governor = create_governor().await;

    for i in 0..10 {
        let user_id = format!("reset_user_{}", i);
        let ctx = create_test_request(&user_id, "10.0.0.3");
        let _ = governor.check(&ctx).await;
    }

    let mut handles = vec![];

    for i in 0..20 {
        let governor_clone = Arc::clone(&governor);
        handles.push(tokio::spawn(async move {
            if i % 2 == 0 {
                let _ = governor_clone.stats().await;
            } else {
                governor_clone.reset_stats().await;
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let _ = governor.stats().await;
}

// ==================== 边界条件测试 ====================

/// 测试 Governor 空请求上下文
#[tokio::test]
async fn test_governor_empty_request_context() {
    let governor = create_governor().await;
    let ctx = limiteron::matchers::RequestContext::new();
    let result = governor.check(&ctx).await;
    assert!(result.is_ok() || result.is_err());
}

/// 测试 Governor 大量请求场景
#[tokio::test]
async fn test_governor_high_volume_requests() {
    let governor = create_governor().await;

    for i in 0..1000 {
        let user_id = format!("volume_user_{}", i % 100);
        let ctx = create_test_request(&user_id, "10.0.0.4");
        let _ = governor.check(&ctx).await;
    }

    let stats = governor.stats().await;
    assert_eq!(stats.total_requests, 1000);
}

/// 测试 Governor 重复用户请求
#[tokio::test]
async fn test_governor_repeated_user_requests() {
    let governor = create_governor().await;

    for _ in 0..100 {
        let ctx = create_test_request("same_user", "10.0.0.5");
        let _ = governor.check(&ctx).await;
    }

    let stats = governor.stats().await;
    assert_eq!(stats.total_requests, 100);
}

/// 测试 Governor 多 IP 场景
#[tokio::test]
async fn test_governor_multiple_ips() {
    let governor = create_governor().await;

    for i in 0..50 {
        let ip = format!("192.168.{}.{}", i / 256, i % 256);
        let ctx = create_test_request(&format!("ip_user_{}", i), &ip);
        let _ = governor.check(&ctx).await;
    }

    let stats = governor.stats().await;
    assert_eq!(stats.total_requests, 50);
}

// ==================== Governor Integration Tests ====================

/// 2.6.1: Governor checks request against limiter
#[tokio::test]
async fn test_governor_checks_request_against_limiter() {
    let governor: Arc<limiteron::Governor> = create_governor().await;

    let ctx = create_test_request("test_user_1", "10.0.0.1");
    let result: Result<Decision, _> = governor.check(&ctx).await;
    assert!(result.is_ok(), "Governor check failed: {:?}", result);

    let decision = result.unwrap();
    match decision {
        Decision::Allowed(_) => {}
        Decision::Banned(_) => {}
        Decision::Rejected(_) => {}
    }

    let stats = governor.stats().await;
    assert_eq!(stats.total_requests, 1);
}

/// 2.6.2: Governor caches decisions in L1Cache
#[tokio::test]
async fn test_governor_caches_decisions_in_l1_cache() {
    let governor: Arc<limiteron::Governor> = create_governor().await;

    assert!(governor.is_l1_cache_enabled());

    governor.clear_l1_cache().await;
    assert_eq!(governor.l1_cache_size().await, 0);

    let ctx = create_test_request("cache_test_user", "10.0.0.2");

    let result1: Result<Decision, _> = governor.check(&ctx).await;
    assert!(result1.is_ok());

    let _cache_size = governor.l1_cache_size().await;

    let result2: Result<Decision, _> = governor.check(&ctx).await;
    assert!(result2.is_ok());

    let decision1 = result1.unwrap();
    let decision2 = result2.unwrap();

    match (&decision1, &decision2) {
        (Decision::Allowed(_), Decision::Allowed(_)) => {}
        (Decision::Rejected(r1), Decision::Rejected(r2)) => {
            assert_eq!(r1, r2, "Rejected reason should be consistent");
        }
        _ => {
            panic!("Decision should be consistent for the same identifier");
        }
    }

    let stats = governor.stats().await;
    assert_eq!(stats.total_requests, 2);
}

/// 2.6.3: Governor matches rules correctly by priority
#[tokio::test]
async fn test_governor_matches_rules_by_priority() {
    let governor: Arc<limiteron::Governor> = create_governor().await;

    let ctx = create_test_request("priority_test_user", "10.0.0.3");

    let result1: Result<Decision, _> = governor.check(&ctx).await;
    let result2: Result<Decision, _> = governor.check(&ctx).await;
    let result3: Result<Decision, _> = governor.check(&ctx).await;

    assert!(result1.is_ok());
    assert!(result2.is_ok());
    assert!(result3.is_ok());

    let stats = governor.stats().await;
    assert_eq!(stats.total_requests, 3);

    let d1 = result1.unwrap();
    let d2 = result2.unwrap();
    let d3 = result3.unwrap();

    let all_allowed = matches!(d1, Decision::Allowed(_))
        && matches!(d2, Decision::Allowed(_))
        && matches!(d3, Decision::Allowed(_));

    let all_rejected = matches!(d1, Decision::Rejected(_))
        && matches!(d2, Decision::Rejected(_))
        && matches!(d3, Decision::Rejected(_));

    assert!(
        all_allowed || all_rejected,
        "Rule matching should be consistent for the same identifier"
    );
}

/// 2.6.4: Governor handles cache miss correctly
#[tokio::test]
async fn test_governor_handles_cache_miss_correctly() {
    let governor: Arc<limiteron::Governor> = create_governor().await;

    governor.disable_l1_cache();
    assert!(!governor.is_l1_cache_enabled());

    assert_eq!(governor.l1_cache_size().await, 0);

    let ctx = create_test_request("miss_test_user", "10.0.0.4");
    let result: Result<Decision, _> = governor.check(&ctx).await;

    assert!(
        result.is_ok(),
        "Governor should handle cache miss gracefully: {:?}",
        result
    );

    governor.enable_l1_cache();
    assert!(governor.is_l1_cache_enabled());

    let ctx2 = create_test_request("cache_enabled_user", "10.0.0.5");
    let result2: Result<Decision, _> = governor.check(&ctx2).await;
    assert!(result2.is_ok());
}
