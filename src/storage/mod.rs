// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
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
    /// 地理位置封禁（国家代码，ISO 3166-1 alpha-2）
    #[serde(rename = "geo")]
    Geo { country_code: String },
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

    /// 创建默认 Storage 实例（out-of-the-box 模式）
    ///
    /// 返回 `Arc<dyn Storage>` 以便直接注入到需要存储依赖的组件中。
    pub fn create_storage() -> Arc<dyn Storage> {
        Arc::new(MemoryStorage::new())
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

    /// 创建默认 BanStorage 实例（out-of-the-box 模式）
    ///
    /// 返回 `Arc<dyn BanStorage>` 以便直接注入到需要封禁存储依赖的组件中。
    pub fn create_ban_storage() -> Arc<dyn BanStorage> {
        Arc::new(MemoryBanStorage::new())
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

        // 单次读锁：收集所有已过期的 target
        let expired_targets: Vec<BanTarget> = self
            .expiration
            .read()
            .await
            .iter()
            .filter_map(|(target, exp)| {
                if *exp <= now {
                    Some(target.clone())
                } else {
                    None
                }
            })
            .collect();

        let removed = expired_targets.len() as u64;

        if removed == 0 {
            return Ok(0);
        }

        // 批量写锁：同时持有 bans 和 expiration 写锁，一次性删除所有过期项
        {
            let mut bans = self.bans.write().await;
            let mut expiration = self.expiration.write().await;
            for target in &expired_targets {
                bans.remove(target);
                expiration.remove(target);
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

        #[allow(clippy::map_clone)]
        let bans: Vec<_> = self.bans.read().await.values().map(|x| x.clone()).collect();

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
        // 防止 offset > total 时的整数下溢（debug panic / release wraparound）
        let take_count = end
            .saturating_sub(start)
            .min(total.saturating_sub(offset) as usize);

        Ok(filtered.into_iter().skip(start).take(take_count).collect())
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

    #[test]
    fn test_ban_target_geo_serialization() {
        let geo = BanTarget::Geo {
            country_code: "CN".to_string(),
        };
        let json = serde_json::to_string(&geo).unwrap();
        assert_eq!(json, r#"{"type":"geo","value":{"country_code":"CN"}}"#);
    }

    #[test]
    fn test_ban_target_geo_deserialization() {
        let json = r#"{"type":"geo","value":{"country_code":"CN"}}"#;
        let target: BanTarget = serde_json::from_str(json).unwrap();
        assert_eq!(
            target,
            BanTarget::Geo {
                country_code: "CN".to_string()
            }
        );
    }

    #[test]
    fn test_ban_target_geo_roundtrip() {
        let original = BanTarget::Geo {
            country_code: "US".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: BanTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_ban_target_geo_equality() {
        let a = BanTarget::Geo {
            country_code: "CN".to_string(),
        };
        let b = BanTarget::Geo {
            country_code: "CN".to_string(),
        };
        let c = BanTarget::Geo {
            country_code: "US".to_string(),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_ban_target_geo_hash() {
        let mut map: HashMap<BanTarget, i32> = HashMap::new();
        let geo = BanTarget::Geo {
            country_code: "CN".to_string(),
        };
        map.insert(geo.clone(), 1);
        assert_eq!(map.get(&geo), Some(&1));
    }

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
            #[allow(clippy::map_clone)]
            let bans: Vec<_> = self.bans.read().await.values().map(|r| r.clone()).collect();
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

    #[tokio::test]
    async fn test_arc_quota_storage_reset() {
        let qs = Arc::new(TestQuotaStorage {
            quotas: RwLock::new(HashMap::new()),
        });
        QuotaStorage::consume(&qs, "u", "res", 100, 1000, Duration::from_secs(60))
            .await
            .unwrap();
        QuotaStorage::reset(&qs, "u", "res", 500, Duration::from_secs(120))
            .await
            .unwrap();
        let q = QuotaStorage::get_quota(&qs, "u", "res")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(q.consumed, 0);
        assert_eq!(q.limit, 500);
    }

    #[tokio::test]
    async fn test_arc_ban_storage_get_history() {
        let bs = Arc::new(TestBanStorage {
            bans: RwLock::new(HashMap::new()),
        });
        let target = BanTarget::UserId("u".to_string());
        let h = BanStorage::get_history(&bs, &target).await.unwrap();
        assert!(h.is_none());
    }

    #[tokio::test]
    async fn test_arc_ban_storage_increment_ban_times() {
        let bs = Arc::new(TestBanStorage {
            bans: RwLock::new(HashMap::new()),
        });
        let target = BanTarget::UserId("u".to_string());
        let n = BanStorage::increment_ban_times(&bs, &target).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn test_arc_ban_storage_get_ban_times() {
        let bs = Arc::new(TestBanStorage {
            bans: RwLock::new(HashMap::new()),
        });
        let target = BanTarget::UserId("u".to_string());
        let n = BanStorage::get_ban_times(&bs, &target).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn test_arc_ban_storage_cleanup_expired_bans() {
        let bs = Arc::new(TestBanStorage {
            bans: RwLock::new(HashMap::new()),
        });
        let n = BanStorage::cleanup_expired_bans(&bs).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn test_arc_ban_storage_list_bans() {
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
        let bans = BanStorage::list_bans(&bs, false, 0, 10).await.unwrap();
        assert_eq!(bans.len(), 1);
    }

    #[tokio::test]
    async fn test_arc_ban_storage_as_any() {
        let bs: Arc<dyn BanStorage> = Arc::new(TestBanStorage {
            bans: RwLock::new(HashMap::new()),
        });
        let any = BanStorage::as_any(&bs);
        assert!(any.downcast_ref::<TestBanStorage>().is_some());
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

    #[tokio::test]
    async fn test_memory_storage_default() {
        let storage: MemoryStorage = Default::default();
        Storage::set(&storage, "k", "v", None).await.unwrap();
        let v = Storage::get(&storage, "k").await.unwrap();
        assert_eq!(v, Some("v".to_string()));
    }

    #[tokio::test]
    async fn test_memory_storage_with_capacity() {
        let storage = MemoryStorage::with_capacity(100);
        Storage::set(&storage, "k", "v", None).await.unwrap();
        let v = Storage::get(&storage, "k").await.unwrap();
        assert_eq!(v, Some("v".to_string()));
    }

    #[tokio::test]
    async fn test_memory_storage_get_nonexistent() {
        let storage = MemoryStorage::new();
        let v = Storage::get(&storage, "nonexistent").await.unwrap();
        assert!(v.is_none());
    }

    #[tokio::test]
    async fn test_memory_storage_delete_nonexistent() {
        let storage = MemoryStorage::new();
        Storage::delete(&storage, "nonexistent").await.unwrap();
    }

    #[tokio::test]
    async fn test_memory_storage_update_ttl() {
        let storage = MemoryStorage::new();

        Storage::set(&storage, "k", "v1", None).await.unwrap();
        Storage::set(&storage, "k", "v2", Some(1)).await.unwrap();
        let v = Storage::get(&storage, "k").await.unwrap();
        assert_eq!(v, Some("v2".to_string()));

        Storage::set(&storage, "k", "v3", Some(3600)).await.unwrap();
        Storage::set(&storage, "k", "v4", None).await.unwrap();
        let v = Storage::get(&storage, "k").await.unwrap();
        assert_eq!(v, Some("v4".to_string()));
    }

    #[tokio::test]
    async fn test_memory_storage_overwrite_expired_key() {
        let storage = MemoryStorage::new();

        Storage::set(&storage, "k", "v1", Some(1)).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let v = Storage::get(&storage, "k").await.unwrap();
        assert!(v.is_none());

        Storage::set(&storage, "k", "v2", None).await.unwrap();
        let v = Storage::get(&storage, "k").await.unwrap();
        assert_eq!(v, Some("v2".to_string()));
    }

    #[tokio::test]
    async fn test_arc_memory_storage_blanket_all() {
        let s: Arc<MemoryStorage> = Arc::new(MemoryStorage::new());
        Storage::set(&s, "k", "v", None).await.unwrap();
        assert_eq!(Storage::get(&s, "k").await.unwrap(), Some("v".to_string()));
        Storage::delete(&s, "k").await.unwrap();
        assert!(Storage::get(&s, "k").await.unwrap().is_none());
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

    #[tokio::test]
    async fn test_memory_ban_storage_default() {
        let storage: MemoryBanStorage = Default::default();
        let rec = BanRecord {
            target: BanTarget::UserId("default_user".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "default test".to_string(),
        };
        BanStorage::save(&storage, &rec).await.unwrap();
        let found = BanStorage::is_banned(&storage, &rec.target).await.unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn test_memory_ban_storage_with_capacity() {
        let storage = MemoryBanStorage::with_capacity(100);
        let rec = BanRecord {
            target: BanTarget::Ip("10.0.0.1".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "capacity test".to_string(),
        };
        BanStorage::save(&storage, &rec).await.unwrap();
        let found = BanStorage::is_banned(&storage, &rec.target).await.unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn test_memory_ban_storage_get_ban_alias() {
        let storage = MemoryBanStorage::new();
        let rec = BanRecord {
            target: BanTarget::Mac("00:11:22:33:44:55".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "mac ban".to_string(),
        };
        BanStorage::save(&storage, &rec).await.unwrap();
        let found = BanStorage::get_ban(&storage, &rec.target).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().reason, "mac ban");
    }

    #[tokio::test]
    async fn test_memory_ban_storage_add_ban_alias() {
        let storage = MemoryBanStorage::new();
        let rec = BanRecord {
            target: BanTarget::UserId("alias_test".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "add_ban test".to_string(),
        };
        BanStorage::add_ban(&storage, &rec).await.unwrap();
        let found = BanStorage::is_banned(&storage, &rec.target).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().reason, "add_ban test");
    }

    #[tokio::test]
    async fn test_memory_ban_storage_get_history() {
        let storage = MemoryBanStorage::new();
        let rec = BanRecord {
            target: BanTarget::UserId("history_test".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "history".to_string(),
        };
        BanStorage::save(&storage, &rec).await.unwrap();
        let history = BanStorage::get_history(&storage, &rec.target)
            .await
            .unwrap();
        assert!(history.is_none());
    }

    #[tokio::test]
    async fn test_memory_ban_storage_cleanup_expired_bans_empty() {
        let storage = MemoryBanStorage::new();
        let removed = BanStorage::cleanup_expired_bans(&storage).await.unwrap();
        assert_eq!(removed, 0);
    }

    #[tokio::test]
    async fn test_memory_ban_storage_list_bans_active_only() {
        let storage = MemoryBanStorage::new();

        let active = BanRecord {
            target: BanTarget::UserId("active".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(3600),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(3600),
            is_manual: false,
            reason: "active".to_string(),
        };
        BanStorage::save(&storage, &active).await.unwrap();

        let active_only = BanStorage::list_bans(&storage, true, 0, 10).await.unwrap();
        assert_eq!(active_only.len(), 1);
        assert_eq!(active_only[0].reason, "active");
    }

    #[tokio::test]
    async fn test_memory_ban_storage_list_bans_pagination() {
        let storage = MemoryBanStorage::new();

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

        let bans = BanStorage::list_bans(&storage, false, 5, 10).await.unwrap();
        assert!(bans.is_empty());

        let bans = BanStorage::list_bans(&storage, false, 3, 10).await.unwrap();
        assert_eq!(bans.len(), 2);
    }

    #[tokio::test]
    async fn test_memory_ban_storage_increment_nonexistent() {
        let storage = MemoryBanStorage::new();
        let target = BanTarget::UserId("nonexistent".to_string());
        let times = BanStorage::increment_ban_times(&storage, &target)
            .await
            .unwrap();
        assert_eq!(times, 0);
    }

    #[tokio::test]
    async fn test_memory_ban_storage_as_any() {
        let storage = MemoryBanStorage::new();
        let any = BanStorage::as_any(&storage);
        assert!(any.downcast_ref::<MemoryBanStorage>().is_some());
    }

    #[tokio::test]
    async fn test_memory_ban_storage_list_bans_offset_eq_total() {
        let storage = MemoryBanStorage::new();

        for i in 0..3 {
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

        let bans = BanStorage::list_bans(&storage, false, 3, 5).await.unwrap();
        assert!(bans.is_empty());
    }

    #[tokio::test]
    async fn test_memory_ban_storage_list_bans_limit_zero() {
        let storage = MemoryBanStorage::new();

        for i in 0..3 {
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

        let bans = BanStorage::list_bans(&storage, false, 0, 0).await.unwrap();
        assert!(bans.is_empty());
    }

    #[tokio::test]
    async fn test_memory_ban_storage_save_overwrite() {
        let storage = MemoryBanStorage::new();

        let rec1 = BanRecord {
            target: BanTarget::Ip("10.0.0.1".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "first".to_string(),
        };
        BanStorage::save(&storage, &rec1).await.unwrap();

        let rec2 = BanRecord {
            target: BanTarget::Ip("10.0.0.1".to_string()),
            ban_times: 2,
            duration: Duration::from_secs(120),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(120),
            is_manual: true,
            reason: "overwritten".to_string(),
        };
        BanStorage::save(&storage, &rec2).await.unwrap();

        let found = BanStorage::is_banned(&storage, &rec2.target)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.ban_times, 2);
        assert_eq!(found.reason, "overwritten");
        assert!(found.is_manual);
    }

    #[tokio::test]
    async fn test_memory_ban_storage_remove_ban_cleans_expiration() {
        let storage = MemoryBanStorage::new();

        let rec = BanRecord {
            target: BanTarget::UserId("remove_me".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "to remove".to_string(),
        };
        BanStorage::save(&storage, &rec).await.unwrap();
        BanStorage::remove_ban(&storage, &rec.target).await.unwrap();

        let removed = BanStorage::cleanup_expired_bans(&storage).await.unwrap();
        assert_eq!(removed, 0);
    }

    #[tokio::test]
    async fn test_arc_memory_ban_storage_blanket() {
        let bs: Arc<MemoryBanStorage> = Arc::new(MemoryBanStorage::new());
        let rec = BanRecord {
            target: BanTarget::UserId("arc_blanket".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "arc blanket".to_string(),
        };
        BanStorage::save(&bs, &rec).await.unwrap();
        let found = BanStorage::is_banned(&bs, &rec.target).await.unwrap();
        assert!(found.is_some());
        BanStorage::remove_ban(&bs, &rec.target).await.unwrap();
        assert!(
            BanStorage::is_banned(&bs, &rec.target)
                .await
                .unwrap()
                .is_none()
        );
    }

    /// 覆盖 cleanup_expired_bans 中无过期 ban 的路径
    #[tokio::test]
    async fn test_memory_ban_storage_cleanup_no_expired() {
        let storage = MemoryBanStorage::new();

        // 只添加未过期的 ban
        let active_rec = BanRecord {
            target: BanTarget::UserId("active_user".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(3600),
            is_manual: false,
            reason: "active".to_string(),
        };
        BanStorage::save(&storage, &active_rec).await.unwrap();

        // cleanup_expired_bans 应返回 0（无过期 ban）
        let removed = BanStorage::cleanup_expired_bans(&storage).await.unwrap();
        assert_eq!(removed, 0, "should remove 0 bans");
    }

    /// 验证 cleanup_expired_bans 批量删除过期 ban 并保留 active ban
    #[tokio::test]
    async fn test_memory_ban_storage_cleanup_expired_batch() {
        let storage = MemoryBanStorage::new();

        // 添加 3 个已过期 + 2 个未过期
        let expired_target_1 = BanTarget::Ip("10.0.0.1".to_string());
        let expired_target_2 = BanTarget::Ip("10.0.0.2".to_string());
        let expired_target_3 = BanTarget::UserId("expired_user".to_string());
        let active_target_1 = BanTarget::Ip("10.0.0.3".to_string());
        let active_target_2 = BanTarget::UserId("active_user".to_string());

        let now = Utc::now();
        for target in [
            expired_target_1.clone(),
            expired_target_2.clone(),
            expired_target_3.clone(),
        ] {
            let rec = BanRecord {
                target: target.clone(),
                ban_times: 1,
                duration: Duration::from_secs(60),
                banned_at: now - chrono::Duration::seconds(120),
                expires_at: now - chrono::Duration::seconds(60),
                is_manual: false,
                reason: "expired".to_string(),
            };
            BanStorage::save(&storage, &rec).await.unwrap();
        }
        for target in [active_target_1.clone(), active_target_2.clone()] {
            let rec = BanRecord {
                target: target.clone(),
                ban_times: 1,
                duration: Duration::from_secs(60),
                banned_at: now,
                expires_at: now + chrono::Duration::seconds(3600),
                is_manual: false,
                reason: "active".to_string(),
            };
            BanStorage::save(&storage, &rec).await.unwrap();
        }

        // 执行批量清理
        let removed = BanStorage::cleanup_expired_bans(&storage).await.unwrap();
        assert_eq!(removed, 3, "should remove 3 expired bans");

        // 验证过期 ban 已删除
        assert!(
            BanStorage::is_banned(&storage, &expired_target_1)
                .await
                .unwrap()
                .is_none(),
            "expired ban 1 should be removed"
        );
        assert!(
            BanStorage::is_banned(&storage, &expired_target_2)
                .await
                .unwrap()
                .is_none(),
            "expired ban 2 should be removed"
        );
        assert!(
            BanStorage::is_banned(&storage, &expired_target_3)
                .await
                .unwrap()
                .is_none(),
            "expired ban 3 should be removed"
        );

        // 验证 active ban 仍存在
        assert!(
            BanStorage::is_banned(&storage, &active_target_1)
                .await
                .unwrap()
                .is_some(),
            "active ban 1 should remain"
        );
        assert!(
            BanStorage::is_banned(&storage, &active_target_2)
                .await
                .unwrap()
                .is_some(),
            "active ban 2 should remain"
        );
    }

    /// 验证并发 add_ban + cleanup_expired_bans 不会 hang（5s 超时）
    #[tokio::test]
    async fn test_cleanup_expired_bans_no_deadlock() {
        let storage = Arc::new(MemoryBanStorage::new());

        // 预填充一些已过期 ban
        let now = Utc::now();
        for i in 0..20 {
            let rec = BanRecord {
                target: BanTarget::Ip(format!("10.0.0.{}", i)),
                ban_times: 1,
                duration: Duration::from_secs(60),
                banned_at: now - chrono::Duration::seconds(120),
                expires_at: now - chrono::Duration::seconds(60),
                is_manual: false,
                reason: "expired".to_string(),
            };
            BanStorage::save(&storage, &rec).await.unwrap();
        }

        // 并发：一个任务持续 add_ban，另一个任务持续 cleanup
        let storage_clone = Arc::clone(&storage);
        let add_handle = tokio::spawn(async move {
            for i in 0..50 {
                let rec = BanRecord {
                    target: BanTarget::UserId(format!("concurrent_user_{}", i)),
                    ban_times: 1,
                    duration: Duration::from_secs(60),
                    banned_at: Utc::now(),
                    expires_at: Utc::now() + chrono::Duration::seconds(3600),
                    is_manual: false,
                    reason: "concurrent".to_string(),
                };
                BanStorage::save(&storage_clone, &rec).await.unwrap();
            }
        });

        let storage_clone2 = Arc::clone(&storage);
        let cleanup_handle = tokio::spawn(async move {
            for _ in 0..10 {
                let _ = BanStorage::cleanup_expired_bans(&storage_clone2).await;
            }
        });

        // 5s 超时验证无 hang
        let result = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            futures::future::join(add_handle, cleanup_handle),
        )
        .await;

        assert!(
            result.is_ok(),
            "concurrent add_ban + cleanup should not hang within 5s"
        );
    }
}
