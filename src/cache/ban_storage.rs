// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
use crate::error::StorageError;
use crate::storage::{BanHistory, BanRecord, BanStorage, BanTarget};
use async_trait::async_trait;
use oxcache::backend::CacheBackend;
use oxcache::error::OxCacheError;
use serde_json::json;
use std::sync::Arc;

fn map_error(e: OxCacheError) -> StorageError {
    match e {
        OxCacheError::Connection(_) | OxCacheError::Timeout(_) => {
            StorageError::ConnectionError(e.to_string())
        }
        _ => StorageError::QueryError(e.to_string()),
    }
}

fn target_key(target: &BanTarget) -> String {
    let tag = match target {
        BanTarget::Ip(_) => "ip",
        BanTarget::UserId(_) => "uid",
        BanTarget::Mac(_) => "mac",
        BanTarget::Geo { .. } => "geo",
    };
    format!(
        "ban:{tag}:{}",
        match target {
            BanTarget::Ip(v) | BanTarget::UserId(v) | BanTarget::Mac(v) => v,
            BanTarget::Geo { country_code } => country_code,
        }
    )
}

fn record_to_json(r: &BanRecord) -> serde_json::Value {
    json!({
        "target": r.target,
        "ban_times": r.ban_times,
        "duration_secs": r.duration.as_secs(),
        "banned_at": r.banned_at.timestamp(),
        "expires_at": r.expires_at.timestamp(),
        "is_manual": r.is_manual,
        "reason": r.reason,
    })
}

fn record_from_json(v: &serde_json::Value) -> Option<BanRecord> {
    let target: BanTarget = serde_json::from_value(v.get("target")?.clone()).ok()?;
    // 显式 u32 范围检查，避免 `as u32` 静默截断（如 u64::MAX → u32 截断为 0）
    let ban_times = u32::try_from(v.get("ban_times")?.as_u64()?).ok()?;
    let duration_secs = v.get("duration_secs")?.as_u64()?;
    let banned_at_ts = v.get("banned_at")?.as_i64()?;
    let expires_at_ts = v.get("expires_at")?.as_i64()?;
    let is_manual = v.get("is_manual")?.as_bool()?;
    let reason = v.get("reason")?.as_str()?.to_string();
    Some(BanRecord {
        target,
        ban_times,
        duration: std::time::Duration::from_secs(duration_secs),
        banned_at: chrono::DateTime::from_timestamp(banned_at_ts, 0)?,
        expires_at: chrono::DateTime::from_timestamp(expires_at_ts, 0)?,
        is_manual,
        reason,
    })
}

fn target_json_str(target: &BanTarget) -> String {
    serde_json::to_string(target).unwrap_or_default()
}

const BAN_INDEX_KEY: &str = "_ban_idx";
const BAN_HISTORY_PREFIX: &str = "ban:hist:";

pub struct CacheBanStorage {
    backend: Arc<dyn CacheBackend>,
}

impl CacheBanStorage {
    pub fn new(backend: Arc<dyn CacheBackend>) -> Self {
        Self { backend }
    }

    async fn get_index(&self) -> Result<Vec<String>, StorageError> {
        let raw = self.backend.get(BAN_INDEX_KEY).await.map_err(map_error)?;
        match raw {
            Some(data) => serde_json::from_slice(&data)
                .map_err(|e| StorageError::QueryError(format!("index deserialize: {e}"))),
            None => Ok(Vec::new()),
        }
    }

    async fn set_index(&self, keys: &[String]) -> Result<(), StorageError> {
        let data =
            serde_json::to_vec(keys).map_err(|e| StorageError::QueryError(format!("{e}")))?;
        self.backend
            .set(Arc::from(BAN_INDEX_KEY), Arc::new(data), None)
            .await
            .map_err(map_error)
    }

    async fn add_to_index(&self, key: &str) -> Result<(), StorageError> {
        let mut idx = self.get_index().await?;
        if !idx.contains(&key.to_string()) {
            idx.push(key.to_string());
            self.set_index(&idx).await?;
        }
        Ok(())
    }

    async fn remove_from_index(&self, key: &str) -> Result<(), StorageError> {
        let mut idx = self.get_index().await?;
        idx.retain(|k| k != key);
        self.set_index(&idx).await
    }

    // ponytail: read-modify-write, not atomic across distributed backends
    async fn modify_ban<F>(&self, target: &BanTarget, f: F) -> Result<(), StorageError>
    where
        F: FnOnce(&mut BanRecord),
    {
        let key = target_key(target);
        let raw = self.backend.get(&key).await.map_err(map_error)?;
        if let Some(data) = raw {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&data) {
                if let Some(mut record) = record_from_json(&v) {
                    f(&mut record);
                    let ttl = record
                        .expires_at
                        .signed_duration_since(chrono::Utc::now())
                        .num_seconds()
                        .max(1) as u64;
                    let data = serde_json::to_vec(&record_to_json(&record))
                        .map_err(|e| StorageError::QueryError(format!("{e}")))?;
                    self.backend
                        .set(
                            Arc::from(key.as_str()),
                            Arc::new(data),
                            Some(std::time::Duration::from_secs(ttl)),
                        )
                        .await
                        .map_err(map_error)?;
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl BanStorage for CacheBanStorage {
    async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
        let key = target_key(target);
        let raw = self.backend.get(&key).await.map_err(map_error)?;
        match raw {
            Some(data) => {
                let v: serde_json::Value = serde_json::from_slice(&data)
                    .map_err(|e| StorageError::QueryError(format!("{e}")))?;
                Ok(record_from_json(&v))
            }
            None => Ok(None),
        }
    }

    async fn save(&self, record: &BanRecord) -> Result<(), StorageError> {
        let key = target_key(&record.target);
        let ttl = record
            .expires_at
            .signed_duration_since(chrono::Utc::now())
            .num_seconds()
            .max(1) as u64;
        let data = serde_json::to_vec(&record_to_json(record))
            .map_err(|e| StorageError::QueryError(format!("{e}")))?;
        self.backend
            .set(
                Arc::from(key.as_str()),
                Arc::new(data),
                Some(std::time::Duration::from_secs(ttl)),
            )
            .await
            .map_err(map_error)?;
        self.add_to_index(&key).await
    }

    async fn get_history(&self, target: &BanTarget) -> Result<Option<BanHistory>, StorageError> {
        let key = format!("{}{}", BAN_HISTORY_PREFIX, target_json_str(target));
        let raw = self.backend.get(&key).await.map_err(map_error)?;
        match raw {
            Some(data) => {
                let v: serde_json::Value = serde_json::from_slice(&data)
                    .map_err(|e| StorageError::QueryError(format!("{e}")))?;
                let ban_times_u64 = v.get("ban_times").and_then(|n| n.as_u64()).unwrap_or(0);
                let ban_times = u32::try_from(ban_times_u64).map_err(|e| {
                    StorageError::QueryError(format!("ban_times 超出 u32 范围: {}", e))
                })?;
                let ts = v
                    .get("last_banned_at")
                    .and_then(|n| n.as_i64())
                    .unwrap_or(0);
                Ok(Some(BanHistory {
                    ban_times,
                    last_banned_at: chrono::DateTime::from_timestamp(ts, 0).unwrap_or_default(),
                }))
            }
            None => Ok(None),
        }
    }

    async fn increment_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let mut times = 0u64;
        self.modify_ban(target, |r| {
            r.ban_times += 1;
            times = r.ban_times as u64;
        })
        .await?;
        Ok(times)
    }

    async fn get_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let key = target_key(target);
        let raw = self.backend.get(&key).await.map_err(map_error)?;
        match raw {
            Some(data) => {
                let v: serde_json::Value = serde_json::from_slice(&data)
                    .map_err(|e| StorageError::QueryError(format!("{e}")))?;
                Ok(v.get("ban_times").and_then(|n| n.as_u64()).unwrap_or(0))
            }
            None => Ok(0),
        }
    }

    async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError> {
        let key = target_key(target);
        self.backend.delete(&key).await.map_err(map_error)?;
        self.remove_from_index(&key).await
    }

    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        // cache backend handles TTL-based eviction automatically
        Ok(0)
    }

    async fn list_bans(
        &self,
        active_only: bool,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BanRecord>, StorageError> {
        let index = self.get_index().await?;
        let mut records = Vec::new();
        for key in &index {
            if let Some(data) = self.backend.get(key).await.map_err(map_error)? {
                let v: serde_json::Value = serde_json::from_slice(&data)
                    .map_err(|e| StorageError::QueryError(format!("{e}")))?;
                if let Some(record) = record_from_json(&v) {
                    if active_only {
                        let now = chrono::Utc::now().timestamp();
                        if record.expires_at.timestamp() > now {
                            records.push(record);
                        }
                    } else {
                        records.push(record);
                    }
                }
            }
        }
        let total = records.len() as u64;
        let start = offset as usize;
        let end = (offset + limit) as usize;
        // 防止 offset > total 时的整数下溢（debug panic / release wraparound）
        let take_count = end
            .saturating_sub(start)
            .min(total.saturating_sub(offset) as usize);
        Ok(records.into_iter().skip(start).take(take_count).collect())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxcache::backend::memory::DashMapMemoryBackend;
    use std::time::Duration;

    fn make_backend() -> Arc<dyn CacheBackend> {
        Arc::new(DashMapMemoryBackend::new())
    }

    fn make_record(target: BanTarget, expires_in_secs: i64) -> BanRecord {
        let now = chrono::Utc::now();
        BanRecord {
            target,
            ban_times: 1,
            duration: Duration::from_secs(expires_in_secs.max(1) as u64),
            banned_at: now,
            expires_at: now + chrono::Duration::seconds(expires_in_secs),
            is_manual: false,
            reason: "test".into(),
        }
    }

    #[tokio::test]
    async fn test_save_and_is_banned() {
        let bs = CacheBanStorage::new(make_backend());
        let rec = make_record(BanTarget::UserId("u1".into()), 3600);
        bs.save(&rec).await.unwrap();
        let found = bs.is_banned(&rec.target).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().reason, "test");
    }

    #[tokio::test]
    async fn test_is_banned_nonexistent() {
        let bs = CacheBanStorage::new(make_backend());
        let found = bs
            .is_banned(&BanTarget::Ip("1.2.3.4".into()))
            .await
            .unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_remove_ban() {
        let bs = CacheBanStorage::new(make_backend());
        let rec = make_record(BanTarget::UserId("rm".into()), 3600);
        bs.save(&rec).await.unwrap();
        bs.remove_ban(&rec.target).await.unwrap();
        let found = bs.is_banned(&rec.target).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_increment_ban_times() {
        let bs = CacheBanStorage::new(make_backend());
        let rec = make_record(BanTarget::Ip("10.0.0.1".into()), 3600);
        bs.save(&rec).await.unwrap();
        let n = bs.increment_ban_times(&rec.target).await.unwrap();
        assert_eq!(n, 2);
        assert_eq!(bs.get_ban_times(&rec.target).await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_increment_nonexistent() {
        let bs = CacheBanStorage::new(make_backend());
        let n = bs
            .increment_ban_times(&BanTarget::UserId("nonexistent".into()))
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn test_get_ban_times_nonexistent() {
        let bs = CacheBanStorage::new(make_backend());
        let n = bs
            .get_ban_times(&BanTarget::Mac("00:11".into()))
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn test_list_bans() {
        let bs = CacheBanStorage::new(make_backend());
        for i in 0..5 {
            let rec = make_record(BanTarget::UserId(format!("u{i}")), 3600);
            bs.save(&rec).await.unwrap();
        }
        let bans = bs.list_bans(false, 0, 100).await.unwrap();
        assert_eq!(bans.len(), 5);
    }

    #[tokio::test]
    async fn test_list_bans_active_only() {
        let bs = CacheBanStorage::new(make_backend());
        let rec = make_record(BanTarget::UserId("active".into()), 3600);
        bs.save(&rec).await.unwrap();
        let expired_rec = make_record(BanTarget::UserId("expired".into()), -10);
        bs.save(&expired_rec).await.unwrap();

        let all = bs.list_bans(false, 0, 100).await.unwrap();
        assert_eq!(all.len(), 2);
        let active = bs.list_bans(true, 0, 100).await.unwrap();
        assert_eq!(active.len(), 1);
    }

    #[tokio::test]
    async fn test_list_bans_pagination() {
        let bs = CacheBanStorage::new(make_backend());
        for i in 0..3 {
            let rec = make_record(BanTarget::UserId(format!("u{i}")), 3600);
            bs.save(&rec).await.unwrap();
        }
        let page1 = bs.list_bans(false, 0, 2).await.unwrap();
        assert_eq!(page1.len(), 2);
        let page2 = bs.list_bans(false, 2, 2).await.unwrap();
        assert_eq!(page2.len(), 1);
    }

    #[tokio::test]
    async fn test_list_bans_offset_eq_total() {
        let bs = CacheBanStorage::new(make_backend());
        for i in 0..3 {
            let rec = make_record(BanTarget::UserId(format!("u{i}")), 3600);
            bs.save(&rec).await.unwrap();
        }
        let bans = bs.list_bans(false, 3, 5).await.unwrap();
        assert!(bans.is_empty());
    }

    #[tokio::test]
    async fn test_cleanup_expired_bans_delegated() {
        let bs = CacheBanStorage::new(make_backend());
        let n = bs.cleanup_expired_bans().await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn test_get_history() {
        let bs = CacheBanStorage::new(make_backend());
        let h = bs
            .get_history(&BanTarget::UserId("hist".into()))
            .await
            .unwrap();
        assert!(h.is_none());
    }

    #[tokio::test]
    async fn test_as_any() {
        let bs = CacheBanStorage::new(make_backend());
        let any = BanStorage::as_any(&bs);
        assert!(any.downcast_ref::<CacheBanStorage>().is_some());
    }

    #[tokio::test]
    async fn test_save_overwrite() {
        let bs = CacheBanStorage::new(make_backend());
        let rec1 = make_record(BanTarget::Ip("10.0.0.1".into()), 3600);
        bs.save(&rec1).await.unwrap();
        let rec2 = BanRecord {
            ban_times: 5,
            reason: "overwritten".into(),
            ..rec1
        };
        bs.save(&rec2).await.unwrap();
        let found = bs.is_banned(&rec2.target).await.unwrap().unwrap();
        assert_eq!(found.ban_times, 5);
        assert_eq!(found.reason, "overwritten");
    }

    #[tokio::test]
    async fn test_arc_trait_object() {
        let bs: Arc<dyn BanStorage> = Arc::new(CacheBanStorage::new(make_backend()));
        let rec = make_record(BanTarget::UserId("arc".into()), 3600);
        bs.save(&rec).await.unwrap();
        let found = bs.is_banned(&rec.target).await.unwrap();
        assert!(found.is_some());
    }

    // map_error 直接调用覆盖（私有函数，通过 use super::* 可访问）
    #[test]
    fn test_map_error_connection() {
        let err = map_error(OxCacheError::Connection("conn fail".to_string()));
        assert!(matches!(err, StorageError::ConnectionError(_)));
    }

    #[test]
    fn test_map_error_timeout() {
        let err = map_error(OxCacheError::Timeout("timed out".to_string()));
        assert!(matches!(err, StorageError::ConnectionError(_)));
    }

    #[test]
    fn test_map_error_other() {
        let err = map_error(OxCacheError::NotFound("not found".to_string()));
        assert!(matches!(err, StorageError::QueryError(_)));
    }

    // get_history Some 分支（数据存在时的反序列化路径）
    #[tokio::test]
    async fn test_get_history_with_data() {
        let backend = make_backend();
        let bs = CacheBanStorage::new(backend.clone());
        let target = BanTarget::UserId("hist_data".into());
        let key = format!("{}{}", BAN_HISTORY_PREFIX, target_json_str(&target));
        let history_data = json!({
            "ban_times": 3u64,
            "last_banned_at": 1234567890i64,
        });
        let data = serde_json::to_vec(&history_data).unwrap();
        backend
            .set(Arc::from(key.as_str()), Arc::new(data), None)
            .await
            .unwrap();
        let h = bs.get_history(&target).await.unwrap().unwrap();
        assert_eq!(h.ban_times, 3);
    }

    // get_history Some 分支：JSON 字段缺失时走 unwrap_or 默认值
    #[tokio::test]
    async fn test_get_history_with_partial_data() {
        let backend = make_backend();
        let bs = CacheBanStorage::new(backend.clone());
        let target = BanTarget::Ip("10.0.0.99".into());
        let key = format!("{}{}", BAN_HISTORY_PREFIX, target_json_str(&target));
        // 只含部分字段，触发 unwrap_or(0) 路径
        let history_data = json!({ "ban_times": 5u64 });
        let data = serde_json::to_vec(&history_data).unwrap();
        backend
            .set(Arc::from(key.as_str()), Arc::new(data), None)
            .await
            .unwrap();
        let h = bs.get_history(&target).await.unwrap().unwrap();
        assert_eq!(h.ban_times, 5);
        // last_banned_at 走 unwrap_or_default() → epoch
        assert_eq!(h.last_banned_at.timestamp(), 0);
    }

    // get_history Some 分支：JSON 反序列化失败
    #[tokio::test]
    async fn test_get_history_invalid_json() {
        let backend = make_backend();
        let bs = CacheBanStorage::new(backend.clone());
        let target = BanTarget::UserId("hist_invalid".into());
        let key = format!("{}{}", BAN_HISTORY_PREFIX, target_json_str(&target));
        backend
            .set(
                Arc::from(key.as_str()),
                Arc::new(b"not valid json".to_vec()),
                None,
            )
            .await
            .unwrap();
        let result = bs.get_history(&target).await;
        assert!(result.is_err());
    }
}
