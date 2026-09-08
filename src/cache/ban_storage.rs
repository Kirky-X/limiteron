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
    /// 进程内 RMW 串行锁
    ///
    /// 本文件全部索引/记录写入均为 read-modify-write。oxcache 0.5 的
    /// `CacheBackend` 不提供 CAS/事务/Lua 原语（按 AGENTS.md 不修改外部
    /// 依赖），跨进程原子性无法在本层实现；此锁保证**单实例内**的并发
    /// 不再互相覆盖（丢索引 key、丢 ban_times），多实例部署仍需依赖
    /// 后端自身的写入粒度。
    rw_lock: tokio::sync::Mutex<()>,
}

impl CacheBanStorage {
    pub fn new(backend: Arc<dyn CacheBackend>) -> Self {
        Self {
            backend,
            rw_lock: tokio::sync::Mutex::new(()),
        }
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

    /// 后端原子写能力（Redis/Moka/Mock 实现；DashMap 等返回 None）
    fn atomic(&self) -> Option<&dyn oxcache::backend::AtomicCacheWriter> {
        self.backend.as_atomic_writer()
    }

    /// 写入索引（须已持有 `rw_lock`；调用方负责串行化）
    ///
    /// 后端支持原子写时优先 CAS 乐观锁（跨实例安全，冲突重试）；
    /// 否则退化为普通 RMW。
    async fn add_to_index_locked(&self, key: &str) -> Result<(), StorageError> {
        if let Some(atomic) = self.atomic() {
            for _ in 0..8 {
                let raw = self.backend.get(BAN_INDEX_KEY).await.map_err(map_error)?;
                let mut idx: Vec<String> = raw
                    .as_deref()
                    .and_then(|d| serde_json::from_slice(d).ok())
                    .unwrap_or_default();
                if idx.iter().any(|k| k == key) {
                    return Ok(());
                }
                idx.push(key.to_string());
                let new = serde_json::to_vec(&idx)
                    .map_err(|e| StorageError::QueryError(format!("{e}")))?;
                if atomic
                    .compare_and_swap(BAN_INDEX_KEY, raw.as_deref(), new, None)
                    .await
                    .map_err(map_error)?
                {
                    return Ok(());
                }
            }
            return Err(StorageError::QueryError(
                "ban index CAS 重试耗尽（并发冲突过高）".to_string(),
            ));
        }

        let mut idx = self.get_index().await?;
        if !idx.contains(&key.to_string()) {
            idx.push(key.to_string());
            self.set_index(&idx).await?;
        }
        Ok(())
    }

    /// 移除索引（须已持有 `rw_lock`；调用方负责串行化）
    ///
    /// CAS 语义同 [`Self::add_to_index_locked`]。
    async fn remove_from_index_locked(&self, key: &str) -> Result<(), StorageError> {
        if let Some(atomic) = self.atomic() {
            for _ in 0..8 {
                let raw = self.backend.get(BAN_INDEX_KEY).await.map_err(map_error)?;
                let Some(data) = raw else {
                    return Ok(());
                };
                let mut idx: Vec<String> = serde_json::from_slice(&data)
                    .map_err(|e| StorageError::QueryError(format!("{e}")))?;
                let before = idx.len();
                idx.retain(|k| k != key);
                if idx.len() == before {
                    return Ok(());
                }
                let new = serde_json::to_vec(&idx)
                    .map_err(|e| StorageError::QueryError(format!("{e}")))?;
                if atomic
                    .compare_and_swap(BAN_INDEX_KEY, Some(&data), new, None)
                    .await
                    .map_err(map_error)?
                {
                    return Ok(());
                }
            }
            return Err(StorageError::QueryError(
                "ban index CAS 重试耗尽（并发冲突过高）".to_string(),
            ));
        }

        let mut idx = self.get_index().await?;
        idx.retain(|k| k != key);
        self.set_index(&idx).await
    }

    // read-modify-write：后端支持原子写时以 CAS 乐观锁重试（跨实例安全）；
    // 否则经进程内 rw_lock 串行化（单实例并发安全，多实例已文档化限制）。
    async fn modify_ban<F>(&self, target: &BanTarget, f: F) -> Result<(), StorageError>
    where
        F: FnMut(&mut BanRecord),
    {
        let mut f = f;
        let _guard = self.rw_lock.lock().await;
        let key = target_key(target);

        if let Some(atomic) = self.atomic() {
            for _ in 0..8 {
                let raw = self.backend.get(&key).await.map_err(map_error)?;
                let Some(data) = raw else {
                    return Ok(()); // 记录不存在，无需修改
                };
                let Ok(v) = serde_json::from_slice::<serde_json::Value>(&data) else {
                    return Ok(()); // 损坏条目按不存在处理（与旧行为一致）
                };
                let Some(mut record) = record_from_json(&v) else {
                    return Ok(());
                };
                f(&mut record);
                let ttl = record
                    .expires_at
                    .signed_duration_since(chrono::Utc::now())
                    .num_seconds()
                    .max(1) as u64;
                let new_data = serde_json::to_vec(&record_to_json(&record))
                    .map_err(|e| StorageError::QueryError(format!("{e}")))?;
                if atomic
                    .compare_and_swap(
                        &key,
                        Some(&data),
                        new_data,
                        Some(std::time::Duration::from_secs(ttl)),
                    )
                    .await
                    .map_err(map_error)?
                {
                    return Ok(());
                }
            }
            return Err(StorageError::QueryError(
                "ban record CAS 重试耗尽（并发冲突过高）".to_string(),
            ));
        }

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
        let _guard = self.rw_lock.lock().await;
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
        self.add_to_index_locked(&key).await
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

    /// 插入或更新封禁记录，`ban_times` 在已存值上原子 +1（ban-5）
    ///
    /// 后端支持原子写时以 CAS 乐观锁重试（与 `modify_ban` 同模式），
    /// 修复 trait 默认实现「读计数 → 整条保存」两步在 cache 后端的
    /// 丢计数缺口；无原子后端退回默认两步（进程内另有 rw_lock 串行化）。
    async fn upsert_ban_record(&self, record: &BanRecord) -> Result<u64, StorageError> {
        let key = target_key(&record.target);

        if let Some(atomic) = self.atomic() {
            let _guard = self.rw_lock.lock().await;
            for _ in 0..8 {
                let raw = self.backend.get(&key).await.map_err(map_error)?;
                let mut is_new = false;
                let new_times = match raw.as_deref().and_then(|d| {
                    serde_json::from_slice::<serde_json::Value>(d)
                        .ok()
                        .and_then(|v| record_from_json(&v))
                }) {
                    Some(existing) => existing.ban_times.saturating_add(1),
                    None => {
                        is_new = true;
                        record.ban_times.max(1)
                    }
                };
                let mut stored = record.clone();
                stored.ban_times = new_times;
                let ttl = stored
                    .expires_at
                    .signed_duration_since(chrono::Utc::now())
                    .num_seconds()
                    .max(1) as u64;
                let data = serde_json::to_vec(&record_to_json(&stored))
                    .map_err(|e| StorageError::QueryError(format!("{e}")))?;
                if atomic
                    .compare_and_swap(
                        &key,
                        raw.as_deref(),
                        data,
                        Some(std::time::Duration::from_secs(ttl)),
                    )
                    .await
                    .map_err(map_error)?
                {
                    // 新记录需登记索引（save() 的职责在此路径的手工等价物）
                    if is_new {
                        self.add_to_index_locked(&key).await?;
                    }
                    return Ok(u64::from(new_times));
                }
            }
            return Err(StorageError::QueryError(
                "ban record CAS 重试耗尽（并发冲突过高）".to_string(),
            ));
        }

        // 无原子后端：读计数 → 整条保存（进程内 rw_lock 已串行化）
        let current = self.get_ban_times(&record.target).await?;
        let mut stored = record.clone();
        stored.ban_times = stored
            .ban_times
            .max(u32::try_from(current).unwrap_or(u32::MAX).saturating_add(1));
        self.save(&stored).await?;
        Ok(u64::from(stored.ban_times))
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
        let _guard = self.rw_lock.lock().await;
        self.backend.delete(&key).await.map_err(map_error)?;
        self.remove_from_index_locked(&key).await
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
