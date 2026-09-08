// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
use crate::error::{ConsumeResult, StorageError};
use crate::storage::{QuotaInfo, QuotaStorage};
use async_trait::async_trait;
use chrono::Utc;
use oxcache::backend::AtomicCacheWriter;
use oxcache::backend::CacheBackend;
use oxcache::error::OxCacheError;
use std::sync::Arc;
use std::time::Duration;

fn map_error(e: OxCacheError) -> StorageError {
    match e {
        OxCacheError::Connection(_) | OxCacheError::Timeout(_) => {
            StorageError::ConnectionError(e.to_string())
        }
        _ => StorageError::QueryError(e.to_string()),
    }
}

/// 计数器键：`quota:{user}:{resource}:{bucket}`（bucket = 当前时间对窗口取整）
fn counter_key(user_id: &str, resource: &str, bucket: u64) -> String {
    format!("quota:{user_id}:{resource}:{bucket}")
}

/// 元数据键：记录 limit 与当前窗口起止（get_quota 重建 QuotaInfo 用）
fn meta_key(user_id: &str, resource: &str) -> String {
    format!("quota:{user_id}:{resource}:meta")
}

fn window_secs(window: Duration) -> u64 {
    window.as_secs().max(1)
}

pub struct CacheQuotaStorage {
    backend: Arc<dyn CacheBackend>,
    /// 回退路径（后端不支持原子操作时）的进程内串行锁
    ///
    /// 后端实现 `AtomicCacheWriter`（Redis/Moka/Mock）时走 `INCR` 原子
    /// 计数路径，本锁不参与；仅回退 RMW 路径用它保证**单实例内**并发
    /// 不互相覆盖。多实例部署请使用支持原子写的后端。
    rw_lock: tokio::sync::Mutex<()>,
}

impl CacheQuotaStorage {
    pub fn new(backend: Arc<dyn CacheBackend>) -> Self {
        Self {
            backend,
            rw_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// 原子 consume：`INCR` 计数后检查限额，超限即原子回滚。
    ///
    /// 单条 `INCRBY` 由后端原子执行，并发消费者的放行决策互不交错
    /// （修复 A4 跨实例 RMW 覆盖）；超限者回滚自己的增量，计数收敛为
    /// 已放行总量。
    async fn consume_atomic(
        &self,
        atomic: &dyn AtomicCacheWriter,
        key: &str,
        cost: u64,
        limit: u64,
        window: Duration,
    ) -> Result<ConsumeResult, StorageError> {
        let new_total = atomic
            .incr(key, cost as i64, Some(window))
            .await
            .map_err(map_error)?;

        if new_total > limit as i64 {
            // 超限：回滚本次增量，按扣除后的已用量给出准确 remaining
            let rolled_back = atomic
                .incr(key, -(cost as i64), Some(window))
                .await
                .map_err(map_error)?;
            let used = rolled_back.max(0) as u64;
            return Ok(ConsumeResult::rejected(used, limit));
        }

        Ok(ConsumeResult::allowed(new_total.max(0) as u64, limit))
    }

    /// 回退 consume（后端无原子写时）：进程内锁串行化的 RMW
    async fn consume_fallback(
        &self,
        key: &str,
        cost: u64,
        limit: u64,
        window: Duration,
    ) -> Result<ConsumeResult, StorageError> {
        let _guard = self.rw_lock.lock().await;

        let current = self.read_counter(key).await?;
        let new_total = current.saturating_add(cost);

        if new_total > limit {
            return Ok(ConsumeResult::rejected(current, limit));
        }

        self.write_counter(key, new_total, window).await?;
        Ok(ConsumeResult::allowed(new_total, limit))
    }

    async fn read_counter(&self, key: &str) -> Result<u64, StorageError> {
        match self.backend.get(key).await.map_err(map_error)? {
            Some(data) => std::str::from_utf8(&data)
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
                .ok_or_else(|| StorageError::QueryError("quota counter 解析失败".to_string())),
            None => Ok(0),
        }
    }

    async fn write_counter(
        &self,
        key: &str,
        value: u64,
        window: Duration,
    ) -> Result<(), StorageError> {
        self.backend
            .set(
                Arc::from(key),
                Arc::new(value.to_string().into_bytes()),
                Some(window),
            )
            .await
            .map_err(map_error)
    }
}

#[async_trait]
impl QuotaStorage for CacheQuotaStorage {
    async fn get_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError> {
        let meta_key = meta_key(user_id, resource);
        let raw = self.backend.get(&meta_key).await.map_err(map_error)?;
        let Some(data) = raw else {
            return Ok(None);
        };
        let v: serde_json::Value =
            serde_json::from_slice(&data).map_err(|e| StorageError::QueryError(format!("{e}")))?;
        let limit = v
            .get("limit")
            .and_then(|n| n.as_u64())
            .ok_or_else(|| StorageError::QueryError("meta missing limit".to_string()))?;
        let window_start = v
            .get("window_start")
            .and_then(|n| n.as_i64())
            .ok_or_else(|| StorageError::QueryError("meta missing window_start".to_string()))?;
        let window_end = v
            .get("window_end")
            .and_then(|n| n.as_i64())
            .ok_or_else(|| StorageError::QueryError("meta missing window_end".to_string()))?;

        // 读取当前 bucket 计数（元数据缺 counter 视为 0）
        let bucket = (window_start as u64)
            / window_secs(Duration::from_secs(
                (window_end - window_start).max(1) as u64
            ));
        let consumed = self
            .read_counter(&counter_key(user_id, resource, bucket))
            .await?;

        let window_start = chrono::DateTime::from_timestamp(window_start, 0)
            .ok_or_else(|| StorageError::QueryError("invalid window_start".to_string()))?;
        let window_end = chrono::DateTime::from_timestamp(window_end, 0)
            .ok_or_else(|| StorageError::QueryError("invalid window_end".to_string()))?;

        Ok(Some(QuotaInfo {
            consumed,
            limit,
            window_start,
            window_end,
        }))
    }

    /// Consume quota
    ///
    /// 原子计数方案（修复 A4）：后端支持原子写时，`INCRBY` 单条命令完成
    /// 「计数 + 限额裁决」，超限回滚本次增量——并发下既不丢更新也不超额。
    /// 无原子能力的后端回退到进程内锁串行化的 RMW。
    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
        limit: u64,
        window: Duration,
    ) -> Result<ConsumeResult, StorageError> {
        // cost=0 不改变账本，直接按当前用量放行
        if cost == 0 {
            let info = self.get_quota(user_id, resource).await?;
            let consumed = info.as_ref().map(|i| i.consumed).unwrap_or(0);
            return Ok(ConsumeResult::allowed(consumed, limit));
        }

        let now = Utc::now();
        let bucket_secs = window_secs(window);
        let bucket = now.timestamp() as u64 / bucket_secs;
        let key = counter_key(user_id, resource, bucket);
        let mkey = meta_key(user_id, resource);

        // 元数据：get_quota/reports 重建 QuotaInfo 用（普通 set，键级原子）
        let meta = serde_json::json!({
            "limit": limit,
            "window_start": bucket * bucket_secs,
            "window_end": (bucket + 1) * bucket_secs,
        });
        let meta_bytes =
            serde_json::to_vec(&meta).map_err(|e| StorageError::QueryError(format!("{e}")))?;
        self.backend
            .set(
                Arc::from(mkey.as_str()),
                Arc::new(meta_bytes),
                Some(window + window),
            )
            .await
            .map_err(map_error)?;

        if cost > limit {
            return Ok(ConsumeResult::rejected(0, limit));
        }

        // 原子路径
        if let Some(atomic) = self.backend.as_atomic_writer() {
            return self.consume_atomic(atomic, &key, cost, limit, window).await;
        }

        // 回退路径：无原子写的后端（进程内锁串行化）
        self.consume_fallback(&key, cost, limit, window).await
    }

    async fn reset(
        &self,
        user_id: &str,
        resource: &str,
        limit: u64,
        window: Duration,
    ) -> Result<(), StorageError> {
        let _guard = self.rw_lock.lock().await;
        let now = Utc::now();
        let bucket_secs = window_secs(window);
        let bucket = now.timestamp() as u64 / bucket_secs;

        // 元数据与计数器均为整键覆盖写（键级原子）
        let meta = serde_json::json!({
            "limit": limit,
            "window_start": bucket * bucket_secs,
            "window_end": (bucket + 1) * bucket_secs,
        });
        let meta_bytes =
            serde_json::to_vec(&meta).map_err(|e| StorageError::QueryError(format!("{e}")))?;
        self.backend
            .set(
                Arc::from(meta_key(user_id, resource).as_str()),
                Arc::new(meta_bytes),
                Some(window + window),
            )
            .await
            .map_err(map_error)?;
        self.write_counter(&counter_key(user_id, resource, bucket), 0, window)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use oxcache::backend::memory::DashMapMemoryBackend;
    use std::sync::Arc;

    fn make_backend() -> Arc<dyn CacheBackend> {
        Arc::new(DashMapMemoryBackend::new())
    }

    #[tokio::test]
    async fn test_consume_allowed() {
        let qs = CacheQuotaStorage::new(make_backend());
        let r = qs
            .consume("u1", "api", 10, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(r.allowed);
        assert_eq!(r.remaining, 90);
    }

    #[tokio::test]
    async fn test_consume_denied() {
        let qs = CacheQuotaStorage::new(make_backend());
        let r = qs
            .consume("u1", "api", 100, 50, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!r.allowed);
    }

    #[tokio::test]
    async fn test_consume_limit_exceeded() {
        let qs = CacheQuotaStorage::new(make_backend());
        qs.consume("u1", "api", 40, 50, Duration::from_secs(60))
            .await
            .unwrap();
        let r = qs
            .consume("u1", "api", 20, 50, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!r.allowed);
    }

    #[tokio::test]
    async fn test_consume_window_reset() {
        let qs = CacheQuotaStorage::new(make_backend());
        qs.consume("u1", "api", 40, 50, Duration::from_secs(1))
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let r = qs
            .consume("u1", "api", 30, 50, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(r.allowed);
    }

    #[tokio::test]
    async fn test_get_quota_none() {
        let qs = CacheQuotaStorage::new(make_backend());
        let q = qs.get_quota("u1", "nonexistent").await.unwrap();
        assert!(q.is_none());
    }

    #[tokio::test]
    async fn test_get_quota_after_consume() {
        let qs = CacheQuotaStorage::new(make_backend());
        qs.consume("u1", "api", 25, 100, Duration::from_secs(60))
            .await
            .unwrap();
        let q = qs.get_quota("u1", "api").await.unwrap().unwrap();
        assert_eq!(q.consumed, 25);
        assert_eq!(q.limit, 100);
    }

    #[tokio::test]
    async fn test_reset() {
        let qs = CacheQuotaStorage::new(make_backend());
        qs.consume("u1", "api", 80, 100, Duration::from_secs(60))
            .await
            .unwrap();
        qs.reset("u1", "api", 200, Duration::from_secs(120))
            .await
            .unwrap();
        let q = qs.get_quota("u1", "api").await.unwrap().unwrap();
        assert_eq!(q.consumed, 0);
        assert_eq!(q.limit, 200);
    }

    #[tokio::test]
    async fn test_consume_zero_cost() {
        let qs = CacheQuotaStorage::new(make_backend());
        let r = qs
            .consume("u1", "api", 0, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(r.allowed);
        assert_eq!(r.remaining, 100);
    }

    #[tokio::test]
    async fn test_consume_exact_limit() {
        let qs = CacheQuotaStorage::new(make_backend());
        let r = qs
            .consume("u1", "api", 100, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(r.allowed);
        assert_eq!(r.remaining, 0);
    }

    #[tokio::test]
    async fn test_multiple_users() {
        let qs = CacheQuotaStorage::new(make_backend());
        qs.consume("u1", "api", 30, 100, Duration::from_secs(60))
            .await
            .unwrap();
        qs.consume("u2", "api", 80, 100, Duration::from_secs(60))
            .await
            .unwrap();
        let q1 = qs.get_quota("u1", "api").await.unwrap().unwrap();
        let q2 = qs.get_quota("u2", "api").await.unwrap().unwrap();
        assert_eq!(q1.consumed, 30);
        assert_eq!(q2.consumed, 80);
    }

    #[tokio::test]
    async fn test_arc_trait_object() {
        let qs: Arc<dyn QuotaStorage> = Arc::new(CacheQuotaStorage::new(make_backend()));
        let r = qs
            .consume("u1", "api", 10, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(r.allowed);
    }

    #[tokio::test]
    async fn test_consume_zero_limit_allowed_zero_cost() {
        // limit=0 路径: usage = 0.0，cost=0 不超限
        let qs = CacheQuotaStorage::new(make_backend());
        let r = qs
            .consume("u_zero", "api", 0, 0, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(r.allowed);
        assert_eq!(r.remaining, 0);
        assert_eq!(r.usage_percent, 0.0);
    }

    #[tokio::test]
    async fn test_consume_zero_limit_denied_nonzero_cost() {
        // limit=0 with non-zero cost: cost > limit → denied
        let qs = CacheQuotaStorage::new(make_backend());
        let r = qs
            .consume("u_zero2", "api", 5, 0, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!r.allowed);
        assert_eq!(r.remaining, 0);
        assert_eq!(r.usage_percent, 0.0);
    }

    #[tokio::test]
    async fn test_consume_zero_limit_after_existing_consumption() {
        // 先 cost=0（放行，不落账），再 cost>0（拒绝）
        let qs = CacheQuotaStorage::new(make_backend());
        let r1 = qs
            .consume("u_zero3", "api", 0, 0, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(r1.allowed);
        let r2 = qs
            .consume("u_zero3", "api", 1, 0, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!r2.allowed);
        assert_eq!(r2.usage_percent, 0.0);
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

    // 覆盖 get_quota 读取元数据的正常路径
    #[tokio::test]
    async fn test_get_quota_with_existing_data() {
        let backend = make_backend();
        let qs = CacheQuotaStorage::new(backend.clone());
        qs.consume("u_get", "api", 30, 200, Duration::from_secs(60))
            .await
            .unwrap();
        let q = qs.get_quota("u_get", "api").await.unwrap().unwrap();
        assert_eq!(q.consumed, 30);
        assert_eq!(q.limit, 200);
    }

    // 覆盖 get_quota 元数据 JSON 损坏路径
    #[tokio::test]
    async fn test_get_quota_invalid_meta() {
        let backend = make_backend();
        let qs = CacheQuotaStorage::new(backend.clone());
        let key = meta_key("u_bad", "api");
        backend
            .set(
                Arc::from(key.as_str()),
                Arc::new(b"not json".to_vec()),
                Some(Duration::from_secs(60)),
            )
            .await
            .unwrap();
        let result = qs.get_quota("u_bad", "api").await;
        assert!(result.is_err());
    }

    // 覆盖 consume 计数器损坏路径（fallback 读取非数字计数）：
    // 直接向当前 bucket 计数键写入垃圾数据（window=3600 → bucket=小时）
    #[tokio::test]
    async fn test_consume_with_invalid_counter() {
        let backend = make_backend();
        let qs = CacheQuotaStorage::new(backend.clone());
        let bucket = Utc::now().timestamp() as u64 / 3600;
        let key = counter_key("u_invjson", "api", bucket);
        backend
            .set(
                Arc::from(key.as_str()),
                Arc::new(b"not-a-number".to_vec()),
                Some(Duration::from_secs(7200)),
            )
            .await
            .unwrap();
        let result = qs
            .consume("u_invjson", "api", 10, 100, Duration::from_secs(3600))
            .await;
        assert!(result.is_err());
    }

    // 并发原子性回归（A4 单实例层）：64 个并发 consume，放行总数精确
    // 不超过 limit（回退路径经 rw_lock 串行化保证）
    #[tokio::test]
    async fn test_consume_fallback_no_overshoot_under_concurrency() {
        let qs = Arc::new(CacheQuotaStorage::new(make_backend()));
        let limit = 50u64;
        let tasks = 20usize;
        let per_task = 10u64;

        let mut handles = Vec::with_capacity(tasks);
        for _ in 0..tasks {
            let qs = Arc::clone(&qs);
            handles.push(tokio::spawn(async move {
                let mut allowed = 0u64;
                for _ in 0..per_task {
                    if qs
                        .consume("u_conc", "api", 1, limit, Duration::from_secs(60))
                        .await
                        .unwrap()
                        .allowed
                    {
                        allowed += 1;
                    }
                }
                allowed
            }));
        }
        let total: u64 = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .sum();

        assert!(total <= limit, "并发放行数 {total} 超过上限 {limit}");
    }
}
