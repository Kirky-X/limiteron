//! 存储抽象层
//!
//! 定义存储接口和基本实现。

use crate::error::{ConsumeResult, StorageError};
use async_trait::async_trait;

/// 存储接口
#[async_trait]
pub trait Storage: Send + Sync {
    /// 获取值
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError>;

    /// 设置值
    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError>;

    /// 删除值
    async fn delete(&self, key: &str) -> Result<(), StorageError>;
}

/// 配额存储接口
#[async_trait]
pub trait QuotaStorage: Send + Sync {
    /// 获取配额信息
    async fn get_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError>;

    /// 消费配额
    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
        limit: u64,
        window: std::time::Duration,
    ) -> Result<ConsumeResult, StorageError>;

    /// 重置配额
    async fn reset(
        &self,
        user_id: &str,
        resource: &str,
        limit: u64,
        window: std::time::Duration,
    ) -> Result<(), StorageError>;
}

/// 封禁存储接口
#[async_trait]
pub trait BanStorage: Send + Sync {
    /// 检查是否被封禁
    async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError>;

    /// 获取封禁记录（别名）
    async fn get_ban(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
        self.is_banned(target).await
    }

    /// 保存封禁记录（别名）
    async fn add_ban(&self, record: &BanRecord) -> Result<(), StorageError> {
        self.save(record).await
    }

    /// 保存封禁记录
    async fn save(&self, record: &BanRecord) -> Result<(), StorageError>;

    /// 获取封禁历史
    async fn get_history(&self, target: &BanTarget) -> Result<Option<BanHistory>, StorageError>;

    /// 增加封禁次数
    async fn increment_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError>;

    /// 获取封禁次数
    async fn get_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError>;

    /// 移除封禁记录
    async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError>;

    /// 清理过期封禁
    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError>;

    /// 获取Any引用（用于类型转换）
    fn as_any(&self) -> &dyn std::any::Any;
}

/// 配额信息
#[derive(Debug, Clone)]
pub struct QuotaInfo {
    pub consumed: u64,
    pub limit: u64,
    pub window_start: chrono::DateTime<chrono::Utc>,
    pub window_end: chrono::DateTime<chrono::Utc>,
}

/// 封禁目标
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum BanTarget {
    Ip(String),
    UserId(String),
    Mac(String),
}

/// 封禁范围
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BanScope {
    Ip,
    UserId,
    Mac,
}

/// 封禁记录
#[derive(Debug, Clone)]
pub struct BanRecord {
    pub target: BanTarget,
    pub ban_times: u32,
    pub duration: std::time::Duration,
    pub banned_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub is_manual: bool,
    pub reason: String,
}

/// 封禁历史
#[derive(Debug, Clone)]
pub struct BanHistory {
    pub ban_times: u32,
    pub last_banned_at: chrono::DateTime<chrono::Utc>,
}

/// 封禁配置
#[derive(Debug, Clone)]
pub struct BanConfig {
    pub initial_duration: std::time::Duration,
    pub backoff_multiplier: f64,
    pub max_duration: std::time::Duration,
}

/// 内存存储实现
pub struct MemoryStorage {
    data: dashmap::DashMap<String, (String, Option<u64>)>,
    quota_data: dashmap::DashMap<String, QuotaEntry>,
    bans: dashmap::DashMap<BanTarget, BanRecord>,
    history: dashmap::DashMap<BanTarget, BanHistory>,
}

/// 配额条目（包含配额信息和TTL）
#[derive(Debug, Clone)]
struct QuotaEntry {
    /// 配额信息
    info: QuotaInfo,
    /// TTL（过期时间戳，毫秒）
    _ttl: Option<u64>,
}

impl Clone for MemoryStorage {
    fn clone(&self) -> Self {
        Self {
            data: dashmap::DashMap::new(),
            quota_data: dashmap::DashMap::new(),
            bans: dashmap::DashMap::new(),
            history: dashmap::DashMap::new(),
        }
    }
}

impl MemoryStorage {
    /// 创建新的内存存储
    pub fn new() -> Self {
        Self {
            data: dashmap::DashMap::new(),
            quota_data: dashmap::DashMap::new(),
            bans: dashmap::DashMap::new(),
            history: dashmap::DashMap::new(),
        }
    }
}

#[async_trait]
impl BanStorage for MemoryStorage {
    async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
        let now = chrono::Utc::now();
        if let Some(record) = self.bans.get(target) {
            // 检查是否过期
            if record.expires_at > now {
                Ok(Some(record.clone()))
            } else {
                // 过期了，删除记录
                self.bans.remove(target);
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    async fn get_history(&self, target: &BanTarget) -> Result<Option<BanHistory>, StorageError> {
        Ok(self.history.get(target).map(|h| h.clone()))
    }

    async fn increment_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let new_times = if let Some(record) = self.bans.get(target) {
            record.ban_times + 1
        } else {
            1
        };

        // 如果存在记录，更新它
        if let Some(mut record) = self.bans.get_mut(target) {
            record.ban_times = new_times;
        } else {
            // 如果不存在，创建一个新的封禁记录
            let new_record = BanRecord {
                target: target.clone(),
                ban_times: new_times as u32,
                duration: std::time::Duration::from_secs(0),
                banned_at: chrono::Utc::now(),
                expires_at: chrono::Utc::now(),
                is_manual: false,
                reason: "Incremented ban times".to_string(),
            };
            self.bans.insert(target.clone(), new_record);
        }

        Ok(new_times as u64)
    }

    async fn get_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        if let Some(record) = self.bans.get(target) {
            Ok(record.ban_times as u64)
        } else {
            Ok(0)
        }
    }

    async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError> {
        self.bans.remove(target);
        Ok(())
    }

    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        let now = chrono::Utc::now();
        let mut count = 0;
        self.bans.retain(|_, record| {
            if record.expires_at <= now {
                count += 1;
                false
            } else {
                true
            }
        });
        Ok(count)
    }

    async fn save(&self, record: &BanRecord) -> Result<(), StorageError> {
        self.bans.insert(record.target.clone(), record.clone());

        // 更新历史
        let history = BanHistory {
            ban_times: record.ban_times,
            last_banned_at: record.banned_at,
        };
        self.history.insert(record.target.clone(), history);

        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        Ok(self.data.get(key).map(|entry| entry.0.clone()))
    }

    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError> {
        self.data.insert(key.to_string(), (value.to_string(), ttl));
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.data.remove(key);
        Ok(())
    }
}

#[async_trait]
impl QuotaStorage for MemoryStorage {
    async fn get_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError> {
        let key = format!("quota:{}:{}", user_id, resource);
        if let Some(entry) = self.quota_data.get(&key) {
            return Ok(Some(entry.info.clone()));
        }
        Ok(None)
    }

    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
        limit: u64,
        window: std::time::Duration,
    ) -> Result<ConsumeResult, StorageError> {
        let key = format!("quota:{}:{}", user_id, resource);
        let now = chrono::Utc::now();

        // 使用 DashMap 的 entry API 进行原子操作 (虽然 DashMap 本身不是事务性的，但在锁期间是安全的)
        // 注意：DashMap 的 entry 锁住的是单个 key
        let mut entry = self.quota_data.entry(key.clone()).or_insert_with(|| {
            let window_end =
                now + chrono::Duration::from_std(window).unwrap_or(chrono::Duration::hours(24));
            QuotaEntry {
                info: QuotaInfo {
                    consumed: 0,
                    limit,
                    window_start: now,
                    window_end,
                },
                _ttl: None,
            }
        });

        // 检查窗口是否过期
        if now >= entry.info.window_end {
            entry.info.consumed = 0;
            entry.info.window_start = now;
            entry.info.window_end =
                now + chrono::Duration::from_std(window).unwrap_or(chrono::Duration::hours(24));
            entry.info.limit = limit; // 更新 limit
        }

        // 计算剩余配额
        let current_consumed = entry.info.consumed;
        let new_consumed = current_consumed + cost;
        let allowed = new_consumed <= limit;

        // 如果允许，扣减配额
        if allowed {
            entry.info.consumed = new_consumed;
        }

        Ok(ConsumeResult {
            allowed,
            remaining: limit.saturating_sub(entry.info.consumed),
            alert_triggered: entry.info.consumed > limit, // 简单告警逻辑，实际上可能需要更复杂的判断
        })
    }

    async fn reset(
        &self,
        user_id: &str,
        resource: &str,
        limit: u64,
        window: std::time::Duration,
    ) -> Result<(), StorageError> {
        let key = format!("quota:{}:{}", user_id, resource);
        let now = chrono::Utc::now();
        let window_end =
            now + chrono::Duration::from_std(window).unwrap_or(chrono::Duration::hours(24));

        self.quota_data.insert(
            key,
            QuotaEntry {
                info: QuotaInfo {
                    consumed: 0,
                    limit,
                    window_start: now,
                    window_end,
                },
                _ttl: None,
            },
        );

        Ok(())
    }
}

/// Mock配额存储
pub struct MockQuotaStorage;

#[async_trait]
impl QuotaStorage for MockQuotaStorage {
    async fn get_quota(
        &self,
        _user_id: &str,
        _resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError> {
        Ok(None)
    }

    async fn consume(
        &self,
        _user_id: &str,
        _resource: &str,
        _cost: u64,
        _limit: u64,
        _window: std::time::Duration,
    ) -> Result<ConsumeResult, StorageError> {
        Ok(ConsumeResult {
            allowed: true,
            remaining: 1000,
            alert_triggered: false,
        })
    }

    async fn reset(
        &self,
        _user_id: &str,
        _resource: &str,
        _limit: u64,
        _window: std::time::Duration,
    ) -> Result<(), StorageError> {
        Ok(())
    }
}

/// Mock封禁存储
pub struct MockBanStorage;

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

    /// 增加封禁次数
    async fn increment_ban_times(&self, _target: &BanTarget) -> Result<u64, StorageError> {
        Ok(0)
    }

    /// 获取封禁次数
    async fn get_ban_times(&self, _target: &BanTarget) -> Result<u64, StorageError> {
        Ok(0)
    }

    /// 移除封禁记录
    async fn remove_ban(&self, _target: &BanTarget) -> Result<(), StorageError> {
        Ok(())
    }

    /// 清理过期封禁
    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        Ok(0)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::Hash;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_memory_storage_set_get() {
        let storage = MemoryStorage::new();
        storage.set("key1", "value1", None).await.unwrap();
        let value = storage.get("key1").await.unwrap();
        assert_eq!(value, Some("value1".to_string()));
    }

    #[tokio::test]
    async fn test_memory_storage_delete() {
        let storage = MemoryStorage::new();
        storage.set("key1", "value1", None).await.unwrap();
        storage.delete("key1").await.unwrap();
        let value = storage.get("key1").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_memory_storage_get_not_found() {
        let storage = MemoryStorage::new();
        let value = storage.get("nonexistent").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_mock_quota_storage() {
        let storage = MockQuotaStorage;
        let result = storage
            .consume(
                "user1",
                "resource1",
                10,
                1000,
                std::time::Duration::from_secs(60),
            )
            .await
            .unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 1000);
        assert!(!result.alert_triggered);
    }

    #[tokio::test]
    async fn test_mock_quota_storage_get_quota() {
        let storage = MockQuotaStorage;
        let quota = storage.get_quota("user1", "resource1").await.unwrap();
        assert!(quota.is_none());
    }

    #[tokio::test]
    async fn test_mock_quota_storage_reset() {
        let storage = MockQuotaStorage;
        storage
            .reset(
                "user1",
                "resource1",
                1000,
                std::time::Duration::from_secs(3600),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_mock_ban_storage() {
        let storage = MockBanStorage;
        let target = BanTarget::UserId("user1".to_string());
        let is_banned = storage.is_banned(&target).await.unwrap();
        assert!(is_banned.is_none());
    }

    #[tokio::test]
    async fn test_mock_ban_storage_save() {
        let storage = MockBanStorage;
        let record = BanRecord {
            target: BanTarget::UserId("user1".to_string()),
            ban_times: 1,
            duration: std::time::Duration::from_secs(300),
            banned_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(300),
            is_manual: false,
            reason: "test".to_string(),
        };
        storage.save(&record).await.unwrap();
    }

    #[tokio::test]
    async fn test_mock_ban_storage_get_history() {
        let storage = MockBanStorage;
        let target = BanTarget::UserId("user1".to_string());
        let history = storage.get_history(&target).await.unwrap();
        assert!(history.is_none());
    }

    #[test]
    fn test_ban_target_equality() {
        let target1 = BanTarget::UserId("user1".to_string());
        let target2 = BanTarget::UserId("user1".to_string());
        assert_eq!(target1, target2);
    }

    #[test]
    fn test_ban_target_hash() {
        let target1 = BanTarget::UserId("user1".to_string());
        let target2 = BanTarget::UserId("user1".to_string());
        use ahash::AHasher;
        use std::hash::Hasher;
        let mut hasher1 = AHasher::default();
        let mut hasher2 = AHasher::default();
        target1.hash(&mut hasher1);
        target2.hash(&mut hasher2);
        assert_eq!(hasher1.finish(), hasher2.finish());
    }

    // ==================== MemoryStorage 完整功能测试 ====================

    /// 测试 MemoryStorage 的 Send + Sync 属性
    #[test]
    fn test_memory_storage_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MemoryStorage>();
    }

    /// 测试 MemoryStorage 实现 QuotaStorage
    #[tokio::test]
    async fn test_memory_storage_quota_consume() {
        let storage = MemoryStorage::new();
        let result = storage
            .consume(
                "user1",
                "api_calls",
                10,
                100,
                std::time::Duration::from_secs(60),
            )
            .await
            .unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 90);
    }

    /// 测试 MemoryStorage 配额耗尽
    #[tokio::test]
    async fn test_memory_storage_quota_exhausted() {
        let storage = MemoryStorage::new();
        // 消费 100 次，每次 1，limit 是 100
        for _ in 0..100 {
            let result = storage
                .consume(
                    "user1",
                    "api_calls",
                    1,
                    100,
                    std::time::Duration::from_secs(60),
                )
                .await
                .unwrap();
            assert!(result.allowed);
        }
        // 第 101 次应该被拒绝
        let result = storage
            .consume(
                "user1",
                "api_calls",
                1,
                100,
                std::time::Duration::from_secs(60),
            )
            .await
            .unwrap();
        assert!(!result.allowed);
        assert_eq!(result.remaining, 0);
    }

    /// 测试 MemoryStorage 获取配额
    #[tokio::test]
    async fn test_memory_storage_get_quota() {
        let storage = MemoryStorage::new();
        // 先消费一些配额
        storage
            .consume(
                "user1",
                "api_calls",
                25,
                100,
                std::time::Duration::from_secs(60),
            )
            .await
            .unwrap();

        let quota = storage.get_quota("user1", "api_calls").await.unwrap();
        assert!(quota.is_some());
        let q = quota.unwrap();
        assert_eq!(q.consumed, 25);
        assert_eq!(q.limit, 100);
    }

    /// 测试 MemoryStorage 重置配额
    #[tokio::test]
    async fn test_memory_storage_reset_quota() {
        let storage = MemoryStorage::new();
        // 先消费一些配额
        storage
            .consume(
                "user1",
                "api_calls",
                50,
                100,
                std::time::Duration::from_secs(60),
            )
            .await
            .unwrap();

        // 重置配额
        storage
            .reset(
                "user1",
                "api_calls",
                200,
                std::time::Duration::from_secs(120),
            )
            .await
            .unwrap();

        // 验证配额已重置
        let quota = storage.get_quota("user1", "api_calls").await.unwrap();
        assert!(quota.is_some());
        assert_eq!(quota.unwrap().consumed, 0);
    }

    /// 测试 MemoryStorage 实现 BanStorage - 添加封禁
    #[tokio::test]
    async fn test_memory_storage_add_ban() {
        let storage = MemoryStorage::new();
        let target = BanTarget::Ip("192.168.1.100".to_string());
        let record = BanRecord {
            target: target.clone(),
            ban_times: 1,
            duration: std::time::Duration::from_secs(300),
            banned_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(300),
            is_manual: false,
            reason: "Rate limit exceeded".to_string(),
        };

        storage.add_ban(&record).await.unwrap();

        // 验证封禁已添加
        let is_banned = storage.is_banned(&target).await.unwrap();
        assert!(is_banned.is_some());
        assert_eq!(is_banned.unwrap().ban_times, 1);
    }

    /// 测试 MemoryStorage 封禁检查
    #[tokio::test]
    async fn test_memory_storage_is_banned() {
        let storage = MemoryStorage::new();
        let target = BanTarget::UserId("banned_user".to_string());

        // 未封禁时应该返回 None
        let is_banned = storage.is_banned(&target).await.unwrap();
        assert!(is_banned.is_none());

        // 添加封禁
        let record = BanRecord {
            target: target.clone(),
            ban_times: 1,
            duration: std::time::Duration::from_secs(300),
            banned_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(300),
            is_manual: false,
            reason: "Test ban".to_string(),
        };
        storage.add_ban(&record).await.unwrap();

        // 验证封禁存在
        let is_banned = storage.is_banned(&target).await.unwrap();
        assert!(is_banned.is_some());
    }

    /// 测试 MemoryStorage 移除封禁
    #[tokio::test]
    async fn test_memory_storage_remove_ban() {
        let storage = MemoryStorage::new();
        let target = BanTarget::Ip("192.168.1.200".to_string());

        // 添加封禁
        let record = BanRecord {
            target: target.clone(),
            ban_times: 1,
            duration: std::time::Duration::from_secs(300),
            banned_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(300),
            is_manual: false,
            reason: "Test ban".to_string(),
        };
        storage.add_ban(&record).await.unwrap();

        // 移除封禁
        storage.remove_ban(&target).await.unwrap();

        // 验证封禁已移除
        let is_banned = storage.is_banned(&target).await.unwrap();
        assert!(is_banned.is_none());
    }

    /// 测试 MemoryStorage 封禁历史
    #[tokio::test]
    async fn test_memory_storage_ban_history() {
        let storage = MemoryStorage::new();
        let target = BanTarget::UserId("history_user".to_string());

        // 添加封禁
        let record = BanRecord {
            target: target.clone(),
            ban_times: 3,
            duration: std::time::Duration::from_secs(300),
            banned_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(300),
            is_manual: false,
            reason: "Multiple violations".to_string(),
        };
        storage.add_ban(&record).await.unwrap();

        // 验证历史记录
        let history = storage.get_history(&target).await.unwrap();
        assert!(history.is_some());
        assert_eq!(history.unwrap().ban_times, 3);
    }

    /// 测试 MemoryStorage 增加封禁次数
    #[tokio::test]
    async fn test_memory_storage_increment_ban_times() {
        let storage = MemoryStorage::new();
        let target = BanTarget::Ip("192.168.1.300".to_string());

        // 初始封禁次数
        let times = storage.get_ban_times(&target).await.unwrap();
        assert_eq!(times, 0);

        // 增加封禁次数
        let new_times = storage.increment_ban_times(&target).await.unwrap();
        assert_eq!(new_times, 1);

        let new_times = storage.increment_ban_times(&target).await.unwrap();
        assert_eq!(new_times, 2);
    }

    /// 测试 MemoryStorage 清理过期封禁
    #[tokio::test]
    async fn test_memory_storage_cleanup_expired_bans() {
        let storage = MemoryStorage::new();
        let target = BanTarget::Ip("192.168.1.400".to_string());

        // 添加一个已过期的封禁
        let record = BanRecord {
            target: target.clone(),
            ban_times: 1,
            duration: std::time::Duration::from_secs(1),
            banned_at: chrono::Utc::now() - chrono::Duration::seconds(10),
            expires_at: chrono::Utc::now() - chrono::Duration::seconds(5),
            is_manual: false,
            reason: "Expired ban".to_string(),
        };
        storage.add_ban(&record).await.unwrap();

        // 验证封禁存在
        let is_banned = storage.is_banned(&target).await.unwrap();
        assert!(is_banned.is_some());

        // 等待过期
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // 清理过期封禁
        let cleaned = storage.cleanup_expired_bans().await.unwrap();
        assert!(cleaned >= 1);

        // 验证封禁已清理
        let is_banned = storage.is_banned(&target).await.unwrap();
        assert!(is_banned.is_none());
    }

    /// 测试 MemoryStorage 并发访问
    #[tokio::test]
    async fn test_memory_storage_concurrent_access() {
        let storage = Arc::new(MemoryStorage::new());
        let mut handles = vec![];

        for i in 0..10 {
            let storage_clone = storage.clone();
            handles.push(tokio::spawn(async move {
                for j in 0..100 {
                    let key = format!("key_{}_{}", i, j);
                    let value = format!("value_{}_{}", i, j);
                    storage_clone.set(&key, &value, None).await.unwrap();
                    let retrieved = storage_clone.get(&key).await.unwrap();
                    assert_eq!(retrieved, Some(value));
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }
    }

    /// 测试 MemoryStorage 作为真实存储后端
    #[tokio::test]
    async fn test_memory_storage_as_production_storage() {
        // 这是一个集成测试，验证 MemoryStorage 可以作为生产存储使用
        let storage = Arc::new(MemoryStorage::new());

        // 并发添加封禁
        let mut handles = vec![];
        for i in 0..5 {
            let storage_clone = storage.clone();
            let target = BanTarget::Ip(format!("192.168.1.{}", i));
            handles.push(tokio::spawn(async move {
                let record = BanRecord {
                    target: target.clone(),
                    ban_times: 1,
                    duration: std::time::Duration::from_secs(3600),
                    banned_at: chrono::Utc::now(),
                    expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
                    is_manual: false,
                    reason: format!("Test ban {}", i),
                };
                storage_clone.add_ban(&record).await.unwrap();
                storage_clone.is_banned(&target).await.unwrap()
            }));
        }

        for result in handles {
            assert!(result.await.unwrap().is_some());
        }

        // 验证所有封禁都存在
        for i in 0..5 {
            let target = BanTarget::Ip(format!("192.168.1.{}", i));
            let is_banned = storage.is_banned(&target).await.unwrap();
            assert!(is_banned.is_some());
        }
    }
}
