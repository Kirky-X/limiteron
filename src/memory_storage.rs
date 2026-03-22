//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! In-memory storage implementations for testing and development.
//!
//! # Warning
//!
//! These implementations store all data in memory. Data will be lost when
//! the application restarts. Not suitable for production use.

use ahash::AHashMap as HashMap;
use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use parking_lot::RwLock;
use std::time::Duration;

use crate::error::{ConsumeResult, StorageError};
use crate::storage_trait::{BanHistory, BanRecord, BanStorage, BanTarget, QuotaInfo, QuotaStorage};

/// In-memory quota storage for testing and development.
///
/// # Warning
///
/// This implementation stores all data in memory. Data will be lost when
/// the application restarts. Not suitable for production use.
///
/// # Example
///
/// ```rust
/// use limiteron::memory_storage::MemoryQuotaStorage;
/// use std::sync::Arc;
///
/// let storage = Arc::new(MemoryQuotaStorage::new());
/// ```
pub struct MemoryQuotaStorage {
    quotas: RwLock<HashMap<String, QuotaInfo>>,
}

impl MemoryQuotaStorage {
    /// Creates a new in-memory quota storage instance.
    pub fn new() -> Self {
        Self {
            quotas: RwLock::new(HashMap::new()),
        }
    }

    fn now_window_end(now: DateTime<Utc>, window: Duration) -> DateTime<Utc> {
        now + ChronoDuration::seconds(window.as_secs() as i64)
    }
}

impl Default for MemoryQuotaStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl QuotaStorage for MemoryQuotaStorage {
    async fn get_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError> {
        let key = format!("{}:{}", user_id, resource);
        Ok(self.quotas.read().get(&key).cloned())
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
        let mut quotas = self.quotas.write();
        let entry = quotas.entry(key).or_insert_with(|| QuotaInfo {
            consumed: 0,
            limit,
            window_start: now,
            window_end: Self::now_window_end(now, window),
        });

        if now > entry.window_end {
            entry.consumed = 0;
            entry.limit = limit;
            entry.window_start = now;
            entry.window_end = Self::now_window_end(now, window);
        }

        let next_consumed = entry.consumed.saturating_add(cost);
        let allowed = next_consumed <= limit;
        if allowed {
            entry.consumed = next_consumed;
        }

        let remaining = limit.saturating_sub(entry.consumed);
        let usage_percent = if limit == 0 {
            0.0
        } else {
            (entry.consumed as f64 / limit as f64) * 100.0
        };

        Ok(ConsumeResult {
            allowed,
            remaining,
            alert_triggered: false,
            usage_percent,
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
        let mut quotas = self.quotas.write();
        quotas.insert(
            key,
            QuotaInfo {
                consumed: 0,
                limit,
                window_start: now,
                window_end: Self::now_window_end(now, window),
            },
        );
        Ok(())
    }
}

/// In-memory ban storage for testing and development.
///
/// # Warning
///
/// This implementation stores all data in memory. Data will be lost when
/// the application restarts. Not suitable for production use.
///
/// # Example
///
/// ```rust
/// use limiteron::memory_storage::MemoryBanStorage;
/// use std::sync::Arc;
///
/// let storage = Arc::new(MemoryBanStorage::new());
/// ```
pub struct MemoryBanStorage {
    bans: RwLock<HashMap<BanTarget, BanRecord>>,
    history: RwLock<HashMap<BanTarget, BanHistory>>,
}

impl MemoryBanStorage {
    /// Creates a new in-memory ban storage instance.
    pub fn new() -> Self {
        Self {
            bans: RwLock::new(HashMap::new()),
            history: RwLock::new(HashMap::new()),
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
        let now = Utc::now();
        let mut bans = self.bans.write();
        if let Some(record) = bans.get(target) {
            if record.expires_at > now {
                return Ok(Some(record.clone()));
            }
            bans.remove(target);
        }
        Ok(None)
    }

    async fn save(&self, record: &BanRecord) -> Result<(), StorageError> {
        let mut bans = self.bans.write();
        bans.insert(record.target.clone(), record.clone());
        let mut history = self.history.write();
        history.insert(
            record.target.clone(),
            BanHistory {
                ban_times: record.ban_times,
                last_banned_at: record.banned_at,
            },
        );
        Ok(())
    }

    async fn get_history(&self, target: &BanTarget) -> Result<Option<BanHistory>, StorageError> {
        Ok(self.history.read().get(target).cloned())
    }

    async fn increment_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let mut history = self.history.write();
        let next = match history.get(target) {
            Some(value) => value.ban_times.saturating_add(1),
            None => 1,
        };
        history.insert(
            target.clone(),
            BanHistory {
                ban_times: next,
                last_banned_at: Utc::now(),
            },
        );
        Ok(next as u64)
    }

    async fn get_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let history = self.history.read();
        Ok(history.get(target).map(|v| v.ban_times as u64).unwrap_or(0))
    }

    async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError> {
        self.bans.write().remove(target);
        Ok(())
    }

    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        let now = Utc::now();
        let mut bans = self.bans.write();
        let before = bans.len();
        bans.retain(|_, record| record.expires_at > now);
        let removed = before.saturating_sub(bans.len());
        Ok(removed as u64)
    }

    #[allow(clippy::map_clone)]
    async fn list_bans(
        &self,
        active_only: bool,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BanRecord>, StorageError> {
        let bans = self.bans.read();
        let now = Utc::now();
        let mut records: Vec<_> = bans.values().map(|v| v.clone()).collect();

        if active_only {
            records.retain(|r| r.expires_at > now);
        }

        let start = offset as usize;
        let end = (offset.saturating_add(limit)) as usize;

        if start >= records.len() {
            return Ok(vec![]);
        }

        Ok(records.into_iter().skip(start).take(end - start).collect())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_quota_storage_consume() {
        let storage = MemoryQuotaStorage::new();
        let result = storage
            .consume("user1", "resource1", 10, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 90);
    }

    #[tokio::test]
    async fn test_memory_quota_storage_limit_exceeded() {
        let storage = MemoryQuotaStorage::new();
        let result = storage
            .consume("user1", "resource1", 100, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 0);

        let result = storage
            .consume("user1", "resource1", 1, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_memory_ban_storage_is_banned() {
        let storage = MemoryBanStorage::new();
        let target = BanTarget::UserId("user1".to_string());

        let is_banned = storage.is_banned(&target).await.unwrap();
        assert!(is_banned.is_none());

        let record = BanRecord {
            target: target.clone(),
            ban_times: 1,
            duration: Duration::from_secs(3600),
            banned_at: Utc::now(),
            expires_at: Utc::now() + ChronoDuration::hours(1),
            is_manual: false,
            reason: "test".to_string(),
        };
        storage.save(&record).await.unwrap();

        let is_banned = storage.is_banned(&target).await.unwrap();
        assert!(is_banned.is_some());
    }

    #[tokio::test]
    async fn test_memory_ban_storage_remove_ban() {
        let storage = MemoryBanStorage::new();
        let target = BanTarget::UserId("user1".to_string());

        let record = BanRecord {
            target: target.clone(),
            ban_times: 1,
            duration: Duration::from_secs(3600),
            banned_at: Utc::now(),
            expires_at: Utc::now() + ChronoDuration::hours(1),
            is_manual: false,
            reason: "test".to_string(),
        };
        storage.save(&record).await.unwrap();

        storage.remove_ban(&target).await.unwrap();
        let is_banned = storage.is_banned(&target).await.unwrap();
        assert!(is_banned.is_none());
    }
}
