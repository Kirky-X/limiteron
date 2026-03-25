//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! Storage trait definitions
//!
//! This module provides the Storage, QuotaStorage, and BanStorage traits
//! that were previously defined in storage.rs.

// 子模块
#[cfg(feature = "parallel-checker")]
pub mod parallel_checker;

// 重新导出 parallel_checker 模块的公共类型
#[cfg(feature = "parallel-checker")]
pub use parallel_checker::ParallelBanChecker;

use crate::error::{ConsumeResult, StorageError};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

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

/// 配额信息
#[derive(Debug, Clone)]
pub struct QuotaInfo {
    /// 已消耗配额
    pub consumed: u64,
    /// 配额上限
    pub limit: u64,
    /// 窗口开始时间
    pub window_start: DateTime<Utc>,
    /// 窗口结束时间
    pub window_end: DateTime<Utc>,
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
        window: Duration,
    ) -> Result<ConsumeResult, StorageError>;

    /// 重置配额
    async fn reset(
        &self,
        user_id: &str,
        resource: &str,
        limit: u64,
        window: Duration,
    ) -> Result<(), StorageError>;
}

/// 封禁目标类型
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum BanTarget {
    /// IP地址封禁
    #[serde(rename = "ip")]
    Ip(String),
    /// 用户ID封禁
    #[serde(rename = "user")]
    UserId(String),
    /// MAC地址封禁
    #[serde(rename = "mac")]
    Mac(String),
}

/// 封禁记录
#[derive(Debug, Clone)]
pub struct BanRecord {
    /// 封禁目标
    pub target: BanTarget,
    /// 封禁次数
    pub ban_times: u32,
    /// 封禁时长
    pub duration: Duration,
    /// 封禁时间
    pub banned_at: DateTime<Utc>,
    /// 过期时间
    pub expires_at: DateTime<Utc>,
    /// 是否手动封禁
    pub is_manual: bool,
    /// 封禁原因
    pub reason: String,
}

/// 封禁历史
#[derive(Debug, Clone)]
pub struct BanHistory {
    /// 封禁次数
    pub ban_times: u32,
    /// 最后封禁时间
    pub last_banned_at: DateTime<Utc>,
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

    /// 列出所有封禁记录（支持分页）
    ///
    /// # 参数
    /// - `active_only`: 是否只返回未过期的封禁
    /// - `offset`: 分页偏移
    /// - `limit`: 每页数量限制
    ///
    /// # 返回
    /// - 封禁记录列表
    async fn list_bans(
        &self,
        active_only: bool,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BanRecord>, StorageError>;

    /// 获取Any引用（用于类型转换）
    fn as_any(&self) -> &dyn std::any::Any;
}

// ============================================================================
// In-Memory Storage Implementations (Default Dependencies)
// ============================================================================
// These implementations are provided for the "out-of-the-box" pattern (new())
// where components need default dependencies without external configuration.

/// Trait for creating Storage instances with default configuration
///
/// This trait enables the "out-of-the-box" pattern where components
/// can create default storage dependencies without external configuration.
pub trait StorageCreate: Send + Sync {
    /// Creates a new Storage instance with default configuration
    fn create_storage() -> Arc<dyn Storage>
    where
        Self: Sized,
    {
        Arc::new(MemoryStorage::new())
    }
}

/// Trait for creating BanStorage instances with default configuration
///
/// This trait enables the "out-of-the-box" pattern where components
/// can create default ban storage dependencies without external configuration.
pub trait BanStorageCreate: Send + Sync {
    /// Creates a new BanStorage instance with default configuration
    fn create_ban_storage() -> Arc<dyn BanStorage>
    where
        Self: Sized,
    {
        Arc::new(MemoryBanStorage::new())
    }
}

impl StorageCreate for MemoryStorage {}
impl BanStorageCreate for MemoryBanStorage {}

use ahash::AHashMap as HashMap;
use tokio::sync::RwLock;

/// In-memory storage implementation for Storage trait
///
/// This is a simple in-memory key-value store with TTL support.
/// It is suitable for testing, development, or single-instance deployments.
///
/// **Note**: This implementation is not suitable for production use with
/// multiple instances as data is not shared across processes.
pub struct MemoryStorage {
    /// Key-value data storage
    data: RwLock<HashMap<String, String>>,
    /// Expiration times (key -> expiration timestamp in seconds)
    expiration: RwLock<HashMap<String, u64>>,
}

impl MemoryStorage {
    /// Creates a new MemoryStorage instance
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
            expiration: RwLock::new(HashMap::new()),
        }
    }

    /// Creates a new MemoryStorage instance with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: RwLock::new(HashMap::with_capacity(capacity)),
            expiration: RwLock::new(HashMap::with_capacity(capacity)),
        }
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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let expiration = self.expiration.read().await.get(key).copied();

        // Check if key has expired
        if let Some(exp) = expiration {
            if exp <= now {
                // Remove expired key
                let _ = self.data.write().await.remove(key);
                let _ = self.expiration.write().await.remove(key);
                return Ok(None);
            }
        }

        Ok(self.data.read().await.get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError> {
        let mut data = self.data.write().await;
        let mut expiration = self.expiration.write().await;

        data.insert(key.to_string(), value.to_string());

        if let Some(ttl_seconds) = ttl {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            expiration.insert(key.to_string(), now + ttl_seconds);
        } else {
            expiration.remove(key);
        }

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.data.write().await.remove(key);
        self.expiration.write().await.remove(key);
        Ok(())
    }
}

/// In-memory ban storage implementation for BanStorage trait
///
/// This is a simple in-memory ban record store.
/// It is suitable for testing, development, or single-instance deployments.
///
/// **Note**: This implementation is not suitable for production use with
/// multiple instances as data is not shared across processes.
pub struct MemoryBanStorage {
    /// Ban records storage
    bans: RwLock<HashMap<BanTarget, BanRecord>>,
    /// Expiration tracking (target -> expires_at timestamp)
    expiration: RwLock<HashMap<BanTarget, i64>>,
}

impl MemoryBanStorage {
    /// Creates a new MemoryBanStorage instance
    pub fn new() -> Self {
        Self {
            bans: RwLock::new(HashMap::new()),
            expiration: RwLock::new(HashMap::new()),
        }
    }

    /// Creates a new MemoryBanStorage instance with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bans: RwLock::new(HashMap::with_capacity(capacity)),
            expiration: RwLock::new(HashMap::with_capacity(capacity)),
        }
    }
}

impl Default for MemoryBanStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BanStorage for MemoryBanStorage {
    async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
        let now = Utc::now().timestamp();

        // Check expiration first
        let expires_at = self.expiration.read().await.get(target).copied();

        if let Some(exp) = expires_at {
            if exp <= now {
                // Remove expired ban
                let _ = self.bans.write().await.remove(target);
                let _ = self.expiration.write().await.remove(target);
                return Ok(None);
            }
        }

        Ok(self.bans.read().await.get(target).cloned())
    }

    async fn save(&self, record: &BanRecord) -> Result<(), StorageError> {
        let mut bans = self.bans.write().await;
        let mut expiration = self.expiration.write().await;

        bans.insert(record.target.clone(), record.clone());
        expiration.insert(record.target.clone(), record.expires_at.timestamp());

        Ok(())
    }

    async fn get_history(&self, _target: &BanTarget) -> Result<Option<BanHistory>, StorageError> {
        // Memory storage doesn't track history by default
        Ok(None)
    }

    async fn increment_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let mut bans = self.bans.write().await;

        if let Some(record) = bans.get_mut(target) {
            record.ban_times += 1;
            Ok(record.ban_times as u64)
        } else {
            Ok(0)
        }
    }

    async fn get_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        Ok(self
            .bans
            .read()
            .await
            .get(target)
            .map(|r| r.ban_times as u64)
            .unwrap_or(0))
    }

    async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError> {
        self.bans.write().await.remove(target);
        self.expiration.write().await.remove(target);
        Ok(())
    }

    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        let now = Utc::now().timestamp();
        let mut removed = 0u64;

        let targets: Vec<_> = self.expiration.read().await.keys().map(Clone::clone).collect();
        for target in targets {
            if let Some(exp) = self.expiration.read().await.get(&target) {
                if *exp <= now {
                    let _ = self.bans.write().await.remove(&target);
                    let _ = self.expiration.write().await.remove(&target);
                    removed += 1;
                }
            }
        }

        Ok(removed)
    }

    async fn list_bans(
        &self,
        active_only: bool,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BanRecord>, StorageError> {
        let now = Utc::now().timestamp();

        // Cleanup expired bans first if active_only
        if active_only {
            let _ = self.cleanup_expired_bans().await;
        }

        let bans: Vec<_> = self.bans.read().await.values().map(Clone::clone).collect();

        let filtered: Vec<_> = if active_only {
            bans.into_iter()
                .filter(|b| b.expires_at.timestamp() > now)
                .collect()
        } else {
            bans
        };

        let total = filtered.len() as u64;
        let start = offset as usize;
        let end = (offset + limit) as usize;

        Ok(filtered
            .into_iter()
            .skip(start)
            .take(end.saturating_sub(start).min((total - offset) as usize))
            .collect())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// Blanket implementations for Arc<dyn XStorage>
#[async_trait]
impl<S: Storage + ?Sized> Storage for Arc<S> {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        (**self).get(key).await
    }

    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError> {
        (**self).set(key, value, ttl).await
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        (**self).delete(key).await
    }
}

#[async_trait]
impl<S: QuotaStorage + ?Sized> QuotaStorage for Arc<S> {
    async fn get_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError> {
        (**self).get_quota(user_id, resource).await
    }

    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
        limit: u64,
        window: Duration,
    ) -> Result<ConsumeResult, StorageError> {
        (**self)
            .consume(user_id, resource, cost, limit, window)
            .await
    }

    async fn reset(
        &self,
        user_id: &str,
        resource: &str,
        limit: u64,
        window: Duration,
    ) -> Result<(), StorageError> {
        (**self).reset(user_id, resource, limit, window).await
    }
}

#[async_trait]
impl<S: BanStorage + ?Sized> BanStorage for Arc<S> {
    async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
        (**self).is_banned(target).await
    }

    async fn save(&self, record: &BanRecord) -> Result<(), StorageError> {
        (**self).save(record).await
    }

    async fn get_history(&self, target: &BanTarget) -> Result<Option<BanHistory>, StorageError> {
        (**self).get_history(target).await
    }

    async fn increment_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        (**self).increment_ban_times(target).await
    }

    async fn get_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        (**self).get_ban_times(target).await
    }

    async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError> {
        (**self).remove_ban(target).await
    }

    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        (**self).cleanup_expired_bans().await
    }

    async fn list_bans(
        &self,
        active_only: bool,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BanRecord>, StorageError> {
        (**self).list_bans(active_only, offset, limit).await
    }

    fn as_any(&self) -> &dyn std::any::Any {
        (**self).as_any()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ahash::AHashMap as HashMap;
    use tokio::sync::RwLock;

    struct TestStorage {
        data: RwLock<HashMap<String, String>>,
    }

    #[async_trait]
    impl Storage for TestStorage {
        async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
            Ok(self.data.read().await.get(key).cloned())
        }

        async fn set(&self, key: &str, value: &str, _ttl: Option<u64>) -> Result<(), StorageError> {
            self.data
                .write()
                .await
                .insert(key.to_string(), value.to_string());
            Ok(())
        }

        async fn delete(&self, key: &str) -> Result<(), StorageError> {
            self.data.write().await.remove(key);
            Ok(())
        }
    }

    struct TestQuotaStorage {
        quotas: RwLock<HashMap<String, QuotaInfo>>,
    }

    #[async_trait]
    impl QuotaStorage for TestQuotaStorage {
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
            window: Duration,
        ) -> Result<ConsumeResult, StorageError> {
            let key = format!("{}:{}", user_id, resource);
            let now = Utc::now();
            let mut quotas = self.quotas.write().await;
            let entry = quotas.entry(key).or_insert(QuotaInfo {
                consumed: 0,
                limit,
                window_start: now,
                window_end: now + chrono::Duration::from_std(window).unwrap(),
            });
            if entry.window_end <= now {
                *entry = QuotaInfo {
                    consumed: 0,
                    limit,
                    window_start: now,
                    window_end: now + chrono::Duration::from_std(window).unwrap(),
                };
            }
            if entry.consumed + cost > entry.limit {
                let usage = if entry.limit > 0 {
                    (entry.consumed as f64 / entry.limit as f64) * 100.0
                } else {
                    0.0
                };
                return Ok(ConsumeResult {
                    allowed: false,
                    remaining: entry.limit.saturating_sub(entry.consumed),
                    alert_triggered: false,
                    usage_percent: usage,
                });
            }
            entry.consumed += cost;
            let usage = if entry.limit > 0 {
                (entry.consumed as f64 / entry.limit as f64) * 100.0
            } else {
                0.0
            };
            Ok(ConsumeResult {
                allowed: true,
                remaining: entry.limit - entry.consumed,
                alert_triggered: false,
                usage_percent: usage,
            })
        }

        async fn reset(
            &self,
            user_id: &str,
            resource: &str,
            limit: u64,
            window: Duration,
        ) -> Result<(), StorageError> {
            let key = format!("{}:{}", user_id, resource);
            let now = Utc::now();
            self.quotas.write().await.insert(
                key,
                QuotaInfo {
                    consumed: 0,
                    limit,
                    window_start: now,
                    window_end: now + chrono::Duration::from_std(window).unwrap(),
                },
            );
            Ok(())
        }
    }

    struct TestBanStorage {
        bans: RwLock<HashMap<BanTarget, BanRecord>>,
    }

    #[async_trait]
    impl BanStorage for TestBanStorage {
        async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
            Ok(self.bans.read().await.get(target).cloned())
        }

        async fn save(&self, record: &BanRecord) -> Result<(), StorageError> {
            self.bans
                .write()
                .await
                .insert(record.target.clone(), record.clone());
            Ok(())
        }

        async fn get_history(
            &self,
            _target: &BanTarget,
        ) -> Result<Option<BanHistory>, StorageError> {
            Ok(None)
        }

        async fn increment_ban_times(&self, _target: &BanTarget) -> Result<u64, StorageError> {
            Ok(0)
        }

        async fn get_ban_times(&self, _target: &BanTarget) -> Result<u64, StorageError> {
            Ok(0)
        }

        async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError> {
            self.bans.write().await.remove(target);
            Ok(())
        }

        async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
            Ok(0)
        }

        async fn list_bans(
            &self,
            _active_only: bool,
            offset: u64,
            limit: u64,
        ) -> Result<Vec<BanRecord>, StorageError> {
            let bans: Vec<_> = self.bans.read().await.values().map(Clone::clone).collect();
            let total = bans.len() as u64;
            let start = offset as usize;
            let end = (offset + limit) as usize;
            Ok(bans
                .into_iter()
                .skip(start)
                .take(end.saturating_sub(start).min((total - offset) as usize))
                .collect())
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[tokio::test]
    async fn test_arc_storage_blanket_impl() {
        let s = Arc::new(TestStorage {
            data: RwLock::new(HashMap::new()),
        });
        Storage::set(&s, "k", "v", None).await.unwrap();
        let v = Storage::get(&s, "k").await.unwrap();
        assert_eq!(v, Some("v".to_string()));
        Storage::delete(&s, "k").await.unwrap();
        let v2 = Storage::get(&s, "k").await.unwrap();
        assert!(v2.is_none());
    }

    #[tokio::test]
    async fn test_arc_quota_storage_blanket_impl() {
        let qs = Arc::new(TestQuotaStorage {
            quotas: RwLock::new(HashMap::new()),
        });
        let r = QuotaStorage::consume(&qs, "u", "res", 100, 1000, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(r.allowed);
        let q = QuotaStorage::get_quota(&qs, "u", "res")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(q.consumed, 100);
    }

    #[tokio::test]
    async fn test_arc_ban_storage_blanket_impl() {
        let bs = Arc::new(TestBanStorage {
            bans: RwLock::new(HashMap::new()),
        });
        let rec = BanRecord {
            target: BanTarget::UserId("u".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "r".to_string(),
        };
        BanStorage::save(&bs, &rec).await.unwrap();
        let found = BanStorage::is_banned(&bs, &rec.target).await.unwrap();
        assert!(found.is_some());
        BanStorage::remove_ban(&bs, &rec.target).await.unwrap();
        let none = BanStorage::is_banned(&bs, &rec.target).await.unwrap();
        assert!(none.is_none());
    }
}

// ============================================================================
// MemoryStorage and MemoryBanStorage Tests
// ============================================================================

#[cfg(test)]
mod memory_storage_tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_storage_basic_operations() {
        let storage = MemoryStorage::new();

        // Test set and get
        Storage::set(&storage, "key1", "value1", None)
            .await
            .unwrap();
        let value = Storage::get(&storage, "key1").await.unwrap();
        assert_eq!(value, Some("value1".to_string()));

        // Test delete
        Storage::delete(&storage, "key1").await.unwrap();
        let value = Storage::get(&storage, "key1").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_memory_storage_ttl() {
        let storage = MemoryStorage::new();

        // Set with TTL of 1 second
        Storage::set(&storage, "key1", "value1", Some(1))
            .await
            .unwrap();

        // Should exist immediately
        let value = Storage::get(&storage, "key1").await.unwrap();
        assert_eq!(value, Some("value1".to_string()));

        // Wait for TTL to expire
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Should be expired
        let value = Storage::get(&storage, "key1").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_memory_storage_update() {
        let storage = MemoryStorage::new();

        // Set initial value
        Storage::set(&storage, "key1", "value1", None)
            .await
            .unwrap();
        let value = Storage::get(&storage, "key1").await.unwrap();
        assert_eq!(value, Some("value1".to_string()));

        // Update value
        Storage::set(&storage, "key1", "value2", None)
            .await
            .unwrap();
        let value = Storage::get(&storage, "key1").await.unwrap();
        assert_eq!(value, Some("value2".to_string()));
    }

    #[tokio::test]
    async fn test_memory_storage_arc_wrapper() {
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());

        Storage::set(&storage, "key1", "value1", None)
            .await
            .unwrap();
        let value = Storage::get(&storage, "key1").await.unwrap();
        assert_eq!(value, Some("value1".to_string()));
    }

    #[tokio::test]
    async fn test_memory_storage_create_trait() {
        let storage = MemoryStorage::create_storage();
        Storage::set(&storage, "key1", "value1", None)
            .await
            .unwrap();
        let value = Storage::get(&storage, "key1").await.unwrap();
        assert_eq!(value, Some("value1".to_string()));
    }
}

#[cfg(test)]
mod memory_ban_storage_tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_ban_storage_basic_operations() {
        let storage = MemoryBanStorage::new();

        let rec = BanRecord {
            target: BanTarget::UserId("user1".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "test ban".to_string(),
        };

        // Save ban
        BanStorage::save(&storage, &rec).await.unwrap();

        // Check is_banned
        let found = BanStorage::is_banned(&storage, &rec.target).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().reason, "test ban");

        // Remove ban
        BanStorage::remove_ban(&storage, &rec.target).await.unwrap();

        // Check removed
        let found = BanStorage::is_banned(&storage, &rec.target).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_memory_ban_storage_increment_ban_times() {
        let storage = MemoryBanStorage::new();

        let rec = BanRecord {
            target: BanTarget::Ip("192.168.1.1".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "test".to_string(),
        };

        BanStorage::save(&storage, &rec).await.unwrap();

        let times = BanStorage::increment_ban_times(&storage, &rec.target)
            .await
            .unwrap();
        assert_eq!(times, 2);

        let times = BanStorage::get_ban_times(&storage, &rec.target)
            .await
            .unwrap();
        assert_eq!(times, 2);
    }

    #[tokio::test]
    async fn test_memory_ban_storage_list_bans() {
        let storage = MemoryBanStorage::new();

        // Add multiple bans
        for i in 0..5 {
            let rec = BanRecord {
                target: BanTarget::UserId(format!("user{}", i)),
                ban_times: 1,
                duration: Duration::from_secs(60),
                banned_at: Utc::now(),
                expires_at: Utc::now() + chrono::Duration::seconds(60),
                is_manual: false,
                reason: format!("ban {}", i),
            };
            BanStorage::save(&storage, &rec).await.unwrap();
        }

        let bans = BanStorage::list_bans(&storage, false, 0, 10).await.unwrap();
        assert_eq!(bans.len(), 5);
    }

    #[tokio::test]
    async fn test_memory_ban_storage_expired() {
        let storage = MemoryBanStorage::new();

        // Create an already expired ban
        let rec = BanRecord {
            target: BanTarget::UserId("expired_user".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: Utc::now() - chrono::Duration::seconds(120),
            expires_at: Utc::now() - chrono::Duration::seconds(60), // Already expired
            is_manual: false,
            reason: "expired".to_string(),
        };

        BanStorage::save(&storage, &rec).await.unwrap();

        // Should not find expired ban
        let found = BanStorage::is_banned(&storage, &rec.target).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_memory_ban_storage_arc_wrapper() {
        let storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let rec = BanRecord {
            target: BanTarget::UserId("user1".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "test".to_string(),
        };

        BanStorage::save(&storage, &rec).await.unwrap();
        let found = BanStorage::is_banned(&storage, &rec.target).await.unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn test_memory_ban_storage_create_trait() {
        let storage = MemoryBanStorage::create_ban_storage();

        let rec = BanRecord {
            target: BanTarget::UserId("user1".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "test".to_string(),
        };

        BanStorage::save(&storage, &rec).await.unwrap();
        let found = BanStorage::is_banned(&storage, &rec.target).await.unwrap();
        assert!(found.is_some());
    }
}
