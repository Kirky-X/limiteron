// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
use crate::error::{ConsumeResult, StorageError};
use crate::storage::{QuotaInfo, QuotaStorage};
use async_trait::async_trait;
use chrono::Utc;
use oxcache::backend::CacheBackend;
use oxcache::error::CacheError;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

fn map_error(e: CacheError) -> StorageError {
    match e {
        CacheError::Connection(_) | CacheError::Timeout(_) => {
            StorageError::ConnectionError(e.to_string())
        }
        _ => StorageError::QueryError(e.to_string()),
    }
}

fn quota_key(user_id: &str, resource: &str) -> String {
    format!("quota:{user_id}:{resource}")
}

fn info_to_json(info: &QuotaInfo) -> serde_json::Value {
    json!({
        "consumed": info.consumed,
        "limit": info.limit,
        "window_start": info.window_start.timestamp(),
        "window_end": info.window_end.timestamp(),
    })
}

fn info_from_json(v: &serde_json::Value) -> Option<QuotaInfo> {
    let consumed = v.get("consumed")?.as_u64()?;
    let limit = v.get("limit")?.as_u64()?;
    let ws = v.get("window_start")?.as_i64()?;
    let we = v.get("window_end")?.as_i64()?;
    Some(QuotaInfo {
        consumed,
        limit,
        window_start: chrono::DateTime::from_timestamp(ws, 0)?,
        window_end: chrono::DateTime::from_timestamp(we, 0)?,
    })
}

pub struct CacheQuotaStorage {
    backend: Arc<dyn CacheBackend>,
}

impl CacheQuotaStorage {
    pub fn new(backend: Arc<dyn CacheBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl QuotaStorage for CacheQuotaStorage {
    async fn get_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError> {
        let key = quota_key(user_id, resource);
        let raw = self.backend.get(&key).await.map_err(map_error)?;
        match raw {
            Some(data) => {
                let v: serde_json::Value = serde_json::from_slice(&data)
                    .map_err(|e| StorageError::QueryError(format!("{e}")))?;
                Ok(info_from_json(&v))
            }
            None => Ok(None),
        }
    }

    // ponytail: read-modify-write, not atomic across distributed backends
    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
        limit: u64,
        window: Duration,
    ) -> Result<ConsumeResult, StorageError> {
        let key = quota_key(user_id, resource);
        let now = Utc::now();
        let window_end = now
            + chrono::Duration::from_std(window)
                .map_err(|e| StorageError::QueryError(format!("invalid Duration: {}", e)))?;

        let raw = self.backend.get(&key).await.map_err(map_error)?;
        let mut info = match raw {
            Some(data) => {
                let v: serde_json::Value = serde_json::from_slice(&data)
                    .map_err(|e| StorageError::QueryError(format!("{e}")))?;
                info_from_json(&v).unwrap_or(QuotaInfo {
                    consumed: 0,
                    limit,
                    window_start: now,
                    window_end,
                })
            }
            None => QuotaInfo {
                consumed: 0,
                limit,
                window_start: now,
                window_end,
            },
        };

        // Reset window if expired
        if info.window_end <= now {
            info = QuotaInfo {
                consumed: 0,
                limit,
                window_start: now,
                window_end,
            };
        }

        let usage = if info.limit > 0 {
            (info.consumed as f64 / info.limit as f64) * 100.0
        } else {
            0.0
        };

        if info.consumed + cost > info.limit {
            return Ok(ConsumeResult {
                allowed: false,
                remaining: info.limit.saturating_sub(info.consumed),
                alert_triggered: false,
                usage_percent: usage,
            });
        }

        info.consumed += cost;
        let remaining = info.limit - info.consumed;
        let usage = if info.limit > 0 {
            (info.consumed as f64 / info.limit as f64) * 100.0
        } else {
            0.0
        };

        let data = serde_json::to_vec(&info_to_json(&info))
            .map_err(|e| StorageError::QueryError(format!("{e}")))?;
        let ttl = info
            .window_end
            .signed_duration_since(Utc::now())
            .num_seconds()
            .max(1) as u64;
        self.backend
            .set(&key, data, Some(Duration::from_secs(ttl)))
            .await
            .map_err(map_error)?;

        Ok(ConsumeResult {
            allowed: true,
            remaining,
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
        let key = quota_key(user_id, resource);
        let now = Utc::now();
        let window_end = now
            + chrono::Duration::from_std(window)
                .map_err(|e| StorageError::QueryError(format!("invalid Duration: {}", e)))?;
        let info = QuotaInfo {
            consumed: 0,
            limit,
            window_start: now,
            window_end,
        };
        let data = serde_json::to_vec(&info_to_json(&info))
            .map_err(|e| StorageError::QueryError(format!("{e}")))?;
        let ttl = info
            .window_end
            .signed_duration_since(Utc::now())
            .num_seconds()
            .max(1) as u64;
        self.backend
            .set(&key, data, Some(Duration::from_secs(ttl)))
            .await
            .map_err(map_error)
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
        // limit=0 path: usage = 0.0, but consumed+cost (0) > limit (0) is false,
        // so allowed with cost 0
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
        // limit=0 with non-zero cost: consumed (0) + cost (5) > limit (0) -> denied
        let qs = CacheQuotaStorage::new(make_backend());
        let r = qs
            .consume("u_zero2", "api", 5, 0, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!r.allowed);
        // remaining = limit.saturating_sub(consumed) = 0.saturating_sub(0) = 0
        assert_eq!(r.remaining, 0);
        assert_eq!(r.usage_percent, 0.0);
    }

    #[tokio::test]
    async fn test_consume_zero_limit_after_existing_consumption() {
        // First consume with limit=0 and cost=0 (allowed, consumed stays 0),
        // then consume with cost>0 (denied). usage_percent should be 0.0
        // because limit > 0 is false on both branches.
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
        let err = map_error(CacheError::Connection("conn fail".to_string()));
        assert!(matches!(err, StorageError::ConnectionError(_)));
    }

    #[test]
    fn test_map_error_timeout() {
        let err = map_error(CacheError::Timeout("timed out".to_string()));
        assert!(matches!(err, StorageError::ConnectionError(_)));
    }

    #[test]
    fn test_map_error_other() {
        let err = map_error(CacheError::NotFound("not found".to_string()));
        assert!(matches!(err, StorageError::QueryError(_)));
    }

    // 覆盖 window 过期重置路径（lines 109-116）
    // 通过直接注入过期数据避免依赖 TTL 驱逐时序
    #[tokio::test]
    async fn test_consume_window_reset_via_pre_populated() {
        let backend = make_backend();
        let qs = CacheQuotaStorage::new(backend.clone());
        let key = quota_key("u_reset", "api");
        let past = Utc::now() - chrono::Duration::seconds(3600);
        let info = QuotaInfo {
            consumed: 50,
            limit: 100,
            window_start: past,
            window_end: past + chrono::Duration::seconds(60),
        };
        let data = serde_json::to_vec(&info_to_json(&info)).unwrap();
        backend
            .set(&key, data, Some(Duration::from_secs(3600)))
            .await
            .unwrap();
        let r = qs
            .consume("u_reset", "api", 10, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(r.allowed);
        assert_eq!(r.remaining, 90);
    }

    // 覆盖 consume 读取已有数据但 info_from_json 返回 None 的兜底路径
    // （lines 92-98：JSON 有效但字段不匹配时使用默认 QuotaInfo）
    #[tokio::test]
    async fn test_consume_with_corrupted_existing_data() {
        let backend = make_backend();
        let qs = CacheQuotaStorage::new(backend.clone());
        let key = quota_key("u_corrupt", "api");
        // 写入有效 JSON 但缺少必需字段，使 info_from_json 返回 None
        let bad_data = serde_json::to_vec(&json!({ "foo": "bar" })).unwrap();
        backend
            .set(&key, bad_data, Some(Duration::from_secs(3600)))
            .await
            .unwrap();
        let r = qs
            .consume("u_corrupt", "api", 10, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(r.allowed);
        assert_eq!(r.remaining, 90);
    }

    // 覆盖 get_quota 读取已有数据的反序列化路径（lines 66-69）
    #[tokio::test]
    async fn test_get_quota_with_existing_data() {
        let backend = make_backend();
        let qs = CacheQuotaStorage::new(backend.clone());
        let key = quota_key("u_get", "api");
        let info = QuotaInfo {
            consumed: 30,
            limit: 200,
            window_start: Utc::now(),
            window_end: Utc::now() + chrono::Duration::seconds(3600),
        };
        let data = serde_json::to_vec(&info_to_json(&info)).unwrap();
        backend
            .set(&key, data, Some(Duration::from_secs(3600)))
            .await
            .unwrap();
        let q = qs.get_quota("u_get", "api").await.unwrap().unwrap();
        assert_eq!(q.consumed, 30);
        assert_eq!(q.limit, 200);
    }

    // 覆盖 get_quota JSON 反序列化失败路径（line 68）
    #[tokio::test]
    async fn test_get_quota_invalid_json() {
        let backend = make_backend();
        let qs = CacheQuotaStorage::new(backend.clone());
        let key = quota_key("u_bad", "api");
        backend
            .set(&key, b"not json".to_vec(), Some(Duration::from_secs(60)))
            .await
            .unwrap();
        let result = qs.get_quota("u_bad", "api").await;
        assert!(result.is_err());
    }

    // 覆盖 consume 时 backend.get 返回 Some 但 JSON 解析失败（line 92）
    #[tokio::test]
    async fn test_consume_with_invalid_json_existing() {
        let backend = make_backend();
        let qs = CacheQuotaStorage::new(backend.clone());
        let key = quota_key("u_invjson", "api");
        backend
            .set(&key, b"invalid".to_vec(), Some(Duration::from_secs(60)))
            .await
            .unwrap();
        let result = qs
            .consume("u_invjson", "api", 10, 100, Duration::from_secs(60))
            .await;
        assert!(result.is_err());
    }
}
