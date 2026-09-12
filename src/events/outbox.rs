// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 事件 Outbox：封禁/配额事件 outbox 表化。
//!
//! Transactional Outbox 模式的 MVP：
//! - 业务路径（封禁/配额变更）把事件写入 `limiteron_event_outbox`
//!   （status = `pending`）。**当前** [`EventOutboxStore::append_event`]
//!   是独立的自动提交 INSERT，尚未接入调用方事务——严格同事务原子性
//!   （领域变更与事件行同生共死）需业务侧在自己的事务内写 outbox 行，
//!   本方法适用于「先提交领域变更、再登记事件」的可接受顺序；
//! - 后台投递器经 [`EventOutboxStore::pending`] 批量取出，投递（webhook /
//!   ban-sync 总线）成功后 [`EventOutboxStore::mark_published`]；
//! - 崩溃恢复：pending 行天然留在表中，重启后续投（at-least-once，
//!   投递方须按事件幂等消费）。
//!
//! 与 dbnexus saga 持久化同一测试口径：sqlite 本地 DSN 全链路读写。
//!
//! # Example
//!
//! ```rust,ignore
//! use limiteron::events::outbox::{EventOutboxStore, OutboxEventKind, OutboxDialect};
//!
//! let store = EventOutboxStore::new(pool.clone());
//! store.init(OutboxDialect::Sqlite).await?;
//! store.append_event(OutboxEventKind::BanApplied, "ip:192.0.2.1",
//!                    serde_json::json!({"reason": "abuse"})).await?;
//! for entry in store.pending(100).await? {
//!     // ... 投递 ...
//!     store.mark_published(entry.id).await?;
//! }
//! ```

use crate::dbnexus_entities::{
    EventOutboxActiveModel, EventOutboxColumn, EventOutboxEntity, EventOutboxModel,
};
use crate::error::{LimiteronError, StorageError};
use chrono::Utc;
use dbnexus::{DbPool, Session};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use std::sync::Arc;

/// 建表方言
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxDialect {
    /// PostgreSQL（BIGSERIAL / TIMESTAMPTZ）
    Postgres,
    /// SQLite（INTEGER AUTOINCREMENT / TEXT 时间）
    Sqlite,
}

/// outbox 事件类型（封禁/配额域 MVP 集合）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxEventKind {
    /// 封禁生效
    BanApplied,
    /// 封禁解除
    BanRemoved,
    /// 配额告警
    QuotaAlert,
}

impl OutboxEventKind {
    /// 事件类型字符串（wire/表存储形态）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BanApplied => "ban_applied",
            Self::BanRemoved => "ban_removed",
            Self::QuotaAlert => "quota_alert",
        }
    }
}

/// pending 行（投递器消费单元）
#[derive(Debug, Clone, PartialEq)]
pub struct OutboxEntry {
    pub id: i64,
    pub event_type: String,
    pub aggregate_id: String,
    pub payload: serde_json::Value,
}

/// 事件 Outbox 存储（`limiteron_event_outbox` 表）
pub struct EventOutboxStore {
    pool: Arc<DbPool>,
}

impl EventOutboxStore {
    /// 基于既有连接池创建 store
    pub fn new(pool: Arc<DbPool>) -> Self {
        Self { pool }
    }

    /// 从池中取会话（连接借用与会话同生命周期，方法内完成读写）
    async fn session(&self) -> Result<Session, LimiteronError> {
        Ok(self
            .pool
            .get_session("admin")
            .await
            .map_err(|e| StorageError::ConnectionError(e.to_string()))?)
    }

    fn conn(session: &Session) -> Result<&sea_orm::DatabaseConnection, StorageError> {
        session
            .connection()
            .map_err(|e| StorageError::ConnectionError(e.to_string()))
    }

    /// 建表（IF NOT EXISTS，可重复执行）
    pub async fn init(&self, dialect: OutboxDialect) -> Result<(), LimiteronError> {
        let ddl = match dialect {
            OutboxDialect::Postgres => crate::dbnexus_entities::event_outbox::create_table_ddl(),
            OutboxDialect::Sqlite => {
                crate::dbnexus_entities::event_outbox::create_table_ddl_sqlite()
            }
        };
        let session = self.session().await?;
        let conn = Self::conn(&session)?;
        conn.execute_unprepared(ddl)
            .await
            .map_err(|e| StorageError::QueryError(e.to_string()))?;
        Ok(())
    }

    /// 追加一条 pending 事件（业务路径调用）
    pub async fn append_event(
        &self,
        kind: OutboxEventKind,
        aggregate_id: &str,
        payload: serde_json::Value,
    ) -> Result<i64, LimiteronError> {
        let active = EventOutboxActiveModel {
            event_type: Set(kind.as_str().to_string()),
            aggregate_id: Set(aggregate_id.to_string()),
            payload: Set(payload.to_string()),
            status: Set("pending".to_string()),
            created_at: Set(Utc::now()),
            published_at: Set(None),
            ..Default::default()
        };
        let session = self.session().await?;
        let conn = Self::conn(&session)?;
        let inserted = EventOutboxEntity::insert(active)
            .exec(conn)
            .await
            .map_err(|e| StorageError::QueryError(e.to_string()))?;
        Ok(inserted.last_insert_id)
    }

    /// 取出 pending 行（FIFO，上限 `limit`）
    pub async fn pending(&self, limit: u64) -> Result<Vec<OutboxEntry>, LimiteronError> {
        let session = self.session().await?;
        let conn = Self::conn(&session)?;
        let rows: Vec<EventOutboxModel> = EventOutboxEntity::find()
            .filter(EventOutboxColumn::Status.eq("pending"))
            .order_by_asc(EventOutboxColumn::Id)
            .limit(limit)
            .all(conn)
            .await
            .map_err(|e| StorageError::QueryError(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|m| {
                // 损坏 payload（截断写入/迁移失配）不能静默变成 Null——
                // 那会以合法形态投递错误数据；至少留下可定位的告警
                let payload = match serde_json::from_str(&m.payload) {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!(
                            target: "limiteron",
                            "outbox entry {} has corrupt payload, delivering as null: {}",
                            m.id,
                            e
                        );
                        serde_json::Value::Null
                    }
                };
                OutboxEntry {
                    id: m.id,
                    event_type: m.event_type,
                    aggregate_id: m.aggregate_id,
                    payload,
                }
            })
            .collect())
    }

    /// 标记投递完成
    pub async fn mark_published(&self, id: i64) -> Result<(), LimiteronError> {
        let session = self.session().await?;
        let conn = Self::conn(&session)?;
        let mut active: EventOutboxActiveModel = EventOutboxEntity::find_by_id(id)
            .one(conn)
            .await
            .map_err(|e| StorageError::QueryError(e.to_string()))?
            .ok_or_else(|| StorageError::NotFound(format!("outbox entry {id}")))?
            .into();
        active.status = Set("published".to_string());
        active.published_at = Set(Some(Utc::now()));
        active
            .update(conn)
            .await
            .map_err(|e| StorageError::QueryError(e.to_string()))?;
        Ok(())
    }

    /// pending 行数（监控/排空断言）
    pub async fn pending_count(&self) -> Result<u64, LimiteronError> {
        let session = self.session().await?;
        let conn = Self::conn(&session)?;
        let count = EventOutboxEntity::find()
            .filter(EventOutboxColumn::Status.eq("pending"))
            .count(conn)
            .await
            .map_err(|e| StorageError::QueryError(e.to_string()))?;
        Ok(count)
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use super::*;

    /// sqlite 本地 DSN（dbnexus saga 测试同款口径）
    fn temp_db_url(tag: &str) -> (String, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "limiteron_outbox_{}_{}.db",
            tag,
            std::process::id()
        ));
        (format!("sqlite:{}?mode=rwc", path.display()), path)
    }

    async fn make_store(tag: &str) -> (EventOutboxStore, std::path::PathBuf) {
        let (url, path) = temp_db_url(tag);
        let pool = Arc::new(DbPool::new(&url).await.expect("sqlite pool"));
        let store = EventOutboxStore::new(pool);
        store.init(OutboxDialect::Sqlite).await.expect("init ddl");
        (store, path)
    }

    /// 全链路：append → pending FIFO → mark_published → pending 排空
    #[tokio::test]
    async fn test_t616_outbox_append_pending_publish_lifecycle() {
        let (store, _path) = make_store("lifecycle").await;
        assert_eq!(store.pending_count().await.unwrap(), 0);

        let id1 = store
            .append_event(
                OutboxEventKind::BanApplied,
                "ip:192.0.2.1",
                serde_json::json!({"reason": "abuse", "duration_secs": 600}),
            )
            .await
            .unwrap();
        let id2 = store
            .append_event(
                OutboxEventKind::QuotaAlert,
                "tenant:t1:user:u1",
                serde_json::json!({"usage_percent": 92}),
            )
            .await
            .unwrap();
        assert_ne!(id1, id2, "inserted rows get distinct ids");
        assert_eq!(store.pending_count().await.unwrap(), 2);

        // FIFO：先插入的先投递
        let pending = store.pending(10).await.unwrap();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].id, id1);
        assert_eq!(pending[0].event_type, "ban_applied");
        assert_eq!(pending[0].aggregate_id, "ip:192.0.2.1");
        assert_eq!(pending[0].payload["reason"], serde_json::json!("abuse"));

        // 投递完成后 pending 排空
        store.mark_published(id1).await.unwrap();
        store.mark_published(id2).await.unwrap();
        assert_eq!(store.pending_count().await.unwrap(), 0);
        assert!(store.pending(10).await.unwrap().is_empty());
    }

    /// limit 语义：一次只取一批，未投递的保持 pending（崩溃恢复语义）
    #[tokio::test]
    async fn test_t616_outbox_pending_limit_and_crash_recovery() {
        let (store, _path) = make_store("limit").await;
        for i in 0..5 {
            store
                .append_event(
                    OutboxEventKind::BanRemoved,
                    &format!("user:u{i}"),
                    serde_json::json!({"seq": i}),
                )
                .await
                .unwrap();
        }
        // 批量取 2 → 3 条仍 pending（崩溃后可续投）
        let batch = store.pending(2).await.unwrap();
        assert_eq!(batch.len(), 2);
        assert_eq!(store.pending_count().await.unwrap(), 5);
        store.mark_published(batch[0].id).await.unwrap();
        assert_eq!(store.pending_count().await.unwrap(), 4);

        // 重复标记已发布行：幂等（状态保持 published，不产生副作用）
        store.mark_published(batch[0].id).await.unwrap();
        assert_eq!(store.pending_count().await.unwrap(), 4);
    }

    /// init 幂等：重复建表不报错
    #[tokio::test]
    async fn test_t616_outbox_init_is_idempotent() {
        let (store, _path) = make_store("idem").await;
        store.init(OutboxDialect::Sqlite).await.expect("re-init ok");
    }
}
