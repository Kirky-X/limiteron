//! 测试通用工具模块
//!
//! 提供测试中常用的工具函数和辅助结构。

use ahash::AHashMap;
use limiteron::{
    ban_manager::{BackoffConfig, BanManager, BanManagerConfig},
    config::{FlowControlConfig, LimiterConfig, Matcher as ConfigMatcher, Rule},
    error::{FlowGuardError, StorageError},
    governor::Governor,
    l2_cache::L2Cache,
    l3_cache::L3Cache,
    limiters::{ConcurrencyLimiter, FixedWindowLimiter, SlidingWindowLimiter, TokenBucketLimiter},
    quota_controller::{QuotaConfig, QuotaController, QuotaType},
    storage::{BanRecord, BanStorage, BanTarget, MemoryStorage, QuotaInfo, QuotaStorage},
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// 创建测试用的内存存储
pub fn create_memory_storage() -> Arc<MemoryStorage> {
    Arc::new(MemoryStorage::new())
}

/// 创建测试用的Governor
pub async fn create_test_governor() -> Result<Governor, FlowGuardError> {
    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: Default::default(),
        rules: vec![],
    };

    let storage = create_memory_storage();
    let ban_storage = create_memory_storage();

    Governor::new(config, storage, ban_storage, None, None).await
}

/// 创建简单的限流规则
pub fn create_simple_rate_limit_rule(id: &str, rate: u64, window: &str) -> Rule {
    Rule {
        id: id.to_string(),
        name: format!("{}-rule", id),
        priority: 100,
        matchers: vec![],
        limiters: vec![LimiterConfig::SlidingWindow {
            window_size: window.to_string(),
            max_requests: rate,
        }],
        action: Default::default(),
    }
}

/// 创建配额配置
pub fn create_quota_config(limit: u64, allow_overdraft: bool) -> QuotaConfig {
    QuotaConfig {
        quota_type: QuotaType::Count,
        limit,
        window_size: 3600,
        allow_overdraft,
        overdraft_limit_percent: if allow_overdraft { 20 } else { 0 },
        alert_config: Default::default(),
    }
}

/// 创建封禁管理器配置
pub fn create_ban_manager_config() -> BanManagerConfig {
    BanManagerConfig {
        backoff: BackoffConfig {
            first_duration: 5,   // 5秒（测试用）
            second_duration: 10, // 10秒
            third_duration: 20,  // 20秒
            fourth_duration: 40, // 40秒
            max_duration: 60,    // 60秒（测试用）
        },
        enable_auto_unban: true,
        auto_unban_interval: 10,
    }
}

/// 创建测试用的BanManager
pub async fn create_test_ban_manager() -> BanManager {
    let storage = create_memory_storage();
    BanManager::new(storage, Some(create_ban_manager_config()))
        .await
        .unwrap()
}

/// Mock存储实现 - 用于单元测试
pub struct MockQuotaStorage {
    quotas: Arc<tokio::sync::RwLock<std::collections::HashMap<String, QuotaInfo>>>,
}

impl MockQuotaStorage {
    pub fn new() -> Self {
        Self {
            quotas: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

    pub fn clear(&self) {
        self.quotas.blocking_write().clear();
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
        Ok(self.quotas.read().await.get(&key).cloned())
    }

    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
        limit: u64,
        window: std::time::Duration,
    ) -> Result<limiteron::error::ConsumeResult, StorageError> {
        let key = format!("{}:{}", user_id, resource);
        let mut quotas = self.quotas.write().await;

        let now = chrono::Utc::now();

        // 获取或创建配额信息
        let quota_info = quotas.entry(key.clone()).or_insert_with(|| QuotaInfo {
            consumed: 0,
            limit,
            window_start: now,
            window_end: now
                + chrono::Duration::from_std(window).unwrap_or(chrono::Duration::seconds(3600)),
        });

        // 检查窗口是否过期
        if now >= quota_info.window_end {
            quota_info.consumed = 0;
            quota_info.window_start = now;
            quota_info.window_end =
                now + chrono::Duration::from_std(window).unwrap_or(chrono::Duration::seconds(3600));
            quota_info.limit = limit; // 更新 limit
        }

        let total_limit = quota_info.limit;

        // 检查是否超过限制
        if quota_info.consumed + cost > total_limit {
            return Ok(limiteron::error::ConsumeResult {
                allowed: false,
                remaining: total_limit.saturating_sub(quota_info.consumed),
                alert_triggered: false,
            });
        }

        quota_info.consumed += cost;

        Ok(limiteron::error::ConsumeResult {
            allowed: true,
            remaining: total_limit.saturating_sub(quota_info.consumed),
            alert_triggered: false,
        })
    }

    async fn reset(
        &self,
        user_id: &str,
        resource: &str,
        limit: u64,
        window: std::time::Duration,
    ) -> Result<(), StorageError> {
        let key = format!("{}:{}", user_id, resource);
        let mut quotas = self.quotas.write().await;

        if let Some(quota_info) = quotas.get_mut(&key) {
            quota_info.consumed = 0;
            // 同时也更新配置
            quota_info.limit = limit;
            let now = chrono::Utc::now();
            quota_info.window_start = now;
            quota_info.window_end =
                now + chrono::Duration::from_std(window).unwrap_or(chrono::Duration::seconds(3600));
        }

        Ok(())
    }
}

/// Mock Ban存储实现
pub struct MockBanStorage {
    bans: Arc<tokio::sync::RwLock<std::collections::HashMap<String, BanRecord>>>,
}

impl MockBanStorage {
    pub fn new() -> Self {
        Self {
            bans: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

    pub fn clear(&self) {
        self.bans.blocking_write().clear();
    }

    fn get_key(target: &BanTarget) -> String {
        match target {
            BanTarget::Ip(ip) => format!("ip:{}", ip),
            BanTarget::UserId(user_id) => format!("user:{}", user_id),
            BanTarget::Mac(mac) => format!("mac:{}", mac),
        }
    }
}

#[async_trait::async_trait]
impl BanStorage for MockBanStorage {
    async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
        let key = Self::get_key(target);
        let ban = self.bans.read().await.get(&key).cloned();

        if let Some(ban) = ban {
            let now = chrono::Utc::now();
            if ban.expires_at > now {
                Ok(Some(ban))
            } else {
                // 过期了，删除记录
                self.bans.write().await.remove(&key);
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    async fn save(&self, ban: &BanRecord) -> Result<(), StorageError> {
        let key = Self::get_key(&ban.target);
        self.bans.write().await.insert(key, ban.clone());
        Ok(())
    }

    async fn get_history(
        &self,
        target: &BanTarget,
    ) -> Result<Option<limiteron::storage::BanHistory>, StorageError> {
        let key = Self::get_key(target);
        if let Some(ban) = self.bans.read().await.get(&key) {
            Ok(Some(limiteron::storage::BanHistory {
                ban_times: ban.ban_times,
                last_banned_at: ban.banned_at,
            }))
        } else {
            Ok(None)
        }
    }

    async fn increment_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let key = Self::get_key(target);
        let mut bans = self.bans.write().await;

        if let Some(ban) = bans.get_mut(&key) {
            ban.ban_times += 1;
            Ok(ban.ban_times as u64)
        } else {
            Ok(1)
        }
    }

    async fn get_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let key = Self::get_key(target);
        Ok(self
            .bans
            .read()
            .await
            .get(&key)
            .map(|b| b.ban_times as u64)
            .unwrap_or(0))
    }

    async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError> {
        let key = Self::get_key(target);
        self.bans.write().await.remove(&key);
        Ok(())
    }

    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        let now = chrono::Utc::now();
        let mut bans = self.bans.write().await;
        let mut count = 0;

        bans.retain(|_, ban| {
            if ban.expires_at < now {
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

/// 等待指定时间（简化测试代码）
pub async fn wait_millis(ms: u64) {
    sleep(Duration::from_millis(ms)).await;
}

/// 等待指定秒数（简化测试代码）
pub async fn wait_secs(secs: u64) {
    sleep(Duration::from_secs(secs)).await;
}

/// 创建测试用的请求上下文
pub fn create_test_request(user_id: &str, ip: &str) -> limiteron::matchers::RequestContext {
    limiteron::matchers::RequestContext {
        user_id: Some(user_id.to_string()),
        ip: Some(ip.to_string()),
        mac: None,
        device_id: None,
        api_key: None,
        headers: AHashMap::new(),
        path: "/test".to_string(),
        method: "GET".to_string(),
        client_ip: Some(ip.to_string()),
        query_params: AHashMap::new(),
    }
}

/// 断言结果为Ok
pub fn assert_ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(e) => panic!("Expected Ok, got Err: {:?}", e),
    }
}

/// 断言结果为Err
pub fn assert_err<T, E: std::fmt::Debug>(result: Result<T, E>) -> E {
    match result {
        Ok(_) => panic!("Expected Err, got Ok"),
        Err(e) => e,
    }
}

/// 断言布尔值为true
pub fn assert_true(value: bool, msg: &str) {
    assert!(value, "{}", msg);
}

/// 断言布尔值为false
pub fn assert_false(value: bool, msg: &str) {
    assert!(!value, "{}", msg);
}

/// 创建测试用的L2缓存
pub fn create_test_l2_cache() -> L2Cache {
    L2Cache::new(1000, Duration::from_secs(60))
}

/// 创建测试用的TokenBucket限流器
pub fn create_token_bucket_limiter(capacity: u64, refill_rate: u64) -> TokenBucketLimiter {
    TokenBucketLimiter::new(capacity, refill_rate)
}

/// 创建测试用的SlidingWindow限流器
pub fn create_sliding_window_limiter(window: Duration, max_requests: u64) -> SlidingWindowLimiter {
    SlidingWindowLimiter::new(window, max_requests)
}

/// 创建测试用的FixedWindow限流器
pub fn create_fixed_window_limiter(window: Duration, max_requests: u64) -> FixedWindowLimiter {
    FixedWindowLimiter::new(window, max_requests)
}

/// 创建测试用的Concurrency限流器
pub fn create_concurrency_limiter(max_concurrent: u64) -> ConcurrencyLimiter {
    ConcurrencyLimiter::new(max_concurrent)
}

/// 测试辅助函数：检查是否在允许的误差范围内
pub fn assert_approx_eq(actual: u64, expected: u64, tolerance_percent: f64) {
    let diff = if actual > expected {
        actual - expected
    } else {
        expected - actual
    };

    let tolerance = (expected as f64 * tolerance_percent / 100.0) as u64;
    assert!(
        diff <= tolerance,
        "Value {} is not within {}% of {} (tolerance: {})",
        actual,
        tolerance_percent,
        expected,
        tolerance
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_quota_storage() {
        let storage = MockQuotaStorage::new();

        // 消费配额
        let result = storage
            .consume("user1", "resource1", 100, 1000, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 900);

        // 再次消费
        let result = storage
            .consume("user1", "resource1", 500, 1000, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 400);

        // 超过限制
        let result = storage
            .consume("user1", "resource1", 500, 1000, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_mock_ban_storage() {
        let storage = MockBanStorage::new();

        // 添加封禁
        let ban = BanRecord {
            target: BanTarget::Ip("192.168.1.1".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "Test".to_string(),
        };

        storage.save(&ban).await.unwrap();

        // 查询封禁
        let result = storage
            .is_banned(&BanTarget::Ip("192.168.1.1".to_string()))
            .await
            .unwrap();
        assert!(result.is_some());

        // 查询历史
        let history = storage
            .get_history(&BanTarget::Ip("192.168.1.1".to_string()))
            .await
            .unwrap();
        assert!(history.is_some());
        assert_eq!(history.unwrap().ban_times, 1);

        // 等待过期
        sleep(Duration::from_secs(61)).await;

        // 再次查询（应该为None）
        let result = storage
            .is_banned(&BanTarget::Ip("192.168.1.1".to_string()))
            .await
            .unwrap();
        assert!(result.is_none());
    }
}
