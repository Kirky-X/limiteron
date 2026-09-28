// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! DBNexusQuotaStorageAdapter - DBNexus-based implementation of QuotaStorage trait
//!
//! This adapter provides a complete QuotaStorage trait implementation using DBNexus
//! for all quota management operations.

use crate::dbnexus_entities::{QuotaColumn, QuotaRecordModel, create_quota_key};
use crate::error::{ConsumeResult, StorageError};
use crate::i18n::t;
use crate::storage::{QuotaInfo, QuotaStorage};
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use dbnexus::{Condition, DbPool, Session};
use sea_orm::entity::prelude::*;
use sea_orm::{DatabaseBackend, Statement};
use std::sync::Arc;
use std::time::Duration as StdDuration;

/// Read the `RETURNING consumed` column from an atomic quota update
fn row_consumed(row: &sea_orm::QueryResult) -> Result<u64, StorageError> {
    row.try_get::<i64>("", "consumed")
        .map(|v| v as u64)
        .map_err(|e| StorageError::QueryError(format!("Failed to read consumed: {}", e)))
}

/// DBNexus-based quota storage adapter
pub struct DBNexusQuotaStorageAdapter {
    pool: Arc<DbPool>,
}

impl DBNexusQuotaStorageAdapter {
    /// Create a new DBNexusQuotaStorageAdapter
    pub fn new(pool: Arc<DbPool>) -> Self {
        Self { pool }
    }

    /// Get a session from the pool
    async fn get_session(&self) -> Result<Session, StorageError> {
        self.pool
            .get_session("admin")
            .await
            .map_err(|e| StorageError::ConnectionError(e.to_string()))
    }

    /// Get a connection from the session (for direct sea-orm operations)
    fn get_conn(session: &Session) -> Result<&DatabaseConnection, StorageError> {
        session
            .connection()
            .map_err(|e| StorageError::ConnectionError(e.to_string()))
    }

    /// Convert model to QuotaInfo
    fn model_to_info(model: &QuotaRecordModel) -> QuotaInfo {
        QuotaInfo {
            consumed: model.consumed as u64,
            limit: model.limit as u64,
            window_start: model.window_start,
            window_end: model.window_end,
        }
    }

    /// Map dbnexus DbError to StorageError
    fn map_err(e: dbnexus::DbError) -> StorageError {
        StorageError::QueryError(e.to_string())
    }

    /// Execute a statement that returns at most one row (None if no match)
    async fn query_optional(
        conn: &DatabaseConnection,
        sql: &str,
        values: impl IntoIterator<Item = sea_orm::Value>,
    ) -> Result<Option<sea_orm::QueryResult>, StorageError> {
        conn.query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            sql,
            values,
        ))
        .await
        .map_err(|e| {
            StorageError::QueryError(format!("Failed to execute atomic quota update: {}", e))
        })
    }

    /// Build an allowed ConsumeResult from the post-consumption ledger value
    /// （简化：改用共享的 `ConsumeResult::allowed` 构造器）
    fn allowed_result(new_consumed: u64, limit: u64) -> ConsumeResult {
        ConsumeResult::allowed(new_consumed, limit)
    }
}

#[async_trait]
impl QuotaStorage for DBNexusQuotaStorageAdapter {
    /// Get quota info for a user and resource
    async fn get_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError> {
        let session = self.get_session().await?;
        let quota_key = create_quota_key(user_id, resource);
        let now = Utc::now();

        let condition = Condition::all()
            .add(QuotaColumn::QuotaKey.eq(quota_key))
            .add(QuotaColumn::WindowEnd.gt(now));

        let records = QuotaRecordModel::find_by_condition(&session, condition)
            .await
            .map_err(Self::map_err)?;

        Ok(records.into_iter().next().map(|m| Self::model_to_info(&m)))
    }

    /// Consume quota
    ///
    /// 原子条件 UPDATE（A3）：限额检查与累加在单条 SQL 内由数据库完成
    /// （`WHERE consumed + cost <= "limit"` + `RETURNING`），消除
    /// read-check-write 竞态下的静默超额放行。未命中时区分「超限拒绝」
    /// 与「无活跃记录」；后者经新窗口原子重启（复用过期行）或守卫式
    /// 插入处理，两条路径都以 `ON CONFLICT` 兜底并发首触（
    /// 旧实现并发 INSERT 撞 quota_key UNIQUE 会向请求返回错误）。
    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
        limit: u64,
        window: StdDuration,
    ) -> Result<ConsumeResult, StorageError> {
        // 类型边界：cost 超出 i64 表示域时 SQL 参数绑定强转会回绕为负
        //（consumed + 负数 <= limit 恒真、账本反减）。该量级永远超出任何
        // limit，直接拒绝。
        if cost > i64::MAX as u64 {
            return Ok(ConsumeResult::rejected(0, limit));
        }

        let session = self.get_session().await?;
        let conn = Self::get_conn(&session)?;
        let quota_key = create_quota_key(user_id, resource);
        let now = Utc::now();
        let chrono_window =
            ChronoDuration::from_std(window).unwrap_or_else(|_| ChronoDuration::days(365));
        // 窗口锚定 epoch 对齐（与 cache 后端一致）：整窗边界全局一致，
        // 不随首次消费时刻漂移
        let window_secs = chrono_window.num_seconds().max(1);
        let window_start = Utc
            .timestamp_opt((now.timestamp().div_euclid(window_secs)) * window_secs, 0)
            .unwrap();
        let window_end = window_start + chrono_window;

        // 活跃窗口原子累加：命中即允许并返回累加后的 consumed。
        // 限额取调用方参数（历史教训：曾取 LEAST(存储列, 参数)——列值仅在
        // 窗口重启时更新，活跃窗口内升额 10→20 不生效，恒按旧小值裁决）。
        // 参数即传即生效：升额与降额在活跃窗口内均即时生效。
        const CONSUME_SQL: &str = r#"
            UPDATE limiteron_quotas
            SET consumed = consumed + $2, updated_at = $4
            WHERE quota_key = $1 AND window_end > $4 AND consumed + $2 <= $3
            RETURNING consumed
        "#;
        // 守卫式插入：仅当冲突行确实过期时以全新窗口覆盖（并发首触时
        // 输家不再撞 UNIQUE 报错，而是回到循环顶部走原子累加路径）
        const INSERT_SQL: &str = r#"
            INSERT INTO limiteron_quotas
                (user_id, resource, quota_key, "limit", consumed,
                 window_start, window_end, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $6, $6)
            ON CONFLICT (quota_key) DO UPDATE SET
                consumed = EXCLUDED.consumed,
                "limit" = EXCLUDED."limit",
                window_start = EXCLUDED.window_start,
                window_end = EXCLUDED.window_end,
                updated_at = EXCLUDED.updated_at
            WHERE limiteron_quotas.window_end <= EXCLUDED.window_start
            RETURNING consumed
        "#;
        // 新窗口原子重启：同 key 的过期行在「窗口确实过期 + 不超限」
        // 条件下原子复用（保留行身份与创建时间）
        const RESTART_SQL: &str = r#"
            UPDATE limiteron_quotas
            SET consumed = $2, "limit" = $3, window_start = $4, window_end = $5, updated_at = $6
            WHERE quota_key = $1 AND window_end <= $4 AND $2 <= $3
            RETURNING consumed
        "#;
        // 注:$4 = 新窗口起点(epoch 对齐),过期判定 window_end <= $4 与
        // 窗口推进语义一致

        // 有界重试：并发首触时败方的 INSERT 冲突由守卫吸收（0 行返回），
        // 回到顶部重跑原子累加即可命中胜方建立的活跃行
        for _ in 0..3 {
            if let Some(row) = Self::query_optional(
                conn,
                CONSUME_SQL,
                [
                    quota_key.clone().into(),
                    (cost as i64).into(),
                    (limit as i64).into(),
                    now.into(),
                ],
            )
            .await?
            {
                return Ok(Self::allowed_result(row_consumed(&row)?, limit));
            }

            // 未命中：区分超限拒绝与无活跃记录（新窗口/首次消费）
            let condition = Condition::all()
                .add(QuotaColumn::QuotaKey.eq(quota_key.clone()))
                .add(QuotaColumn::WindowEnd.gt(now));

            if let Some(record) = QuotaRecordModel::find_by_condition(&session, condition)
                .await
                .map_err(Self::map_err)?
                .into_iter()
                .next()
            {
                // 超限拒绝：给出基于当前账本的准确 remaining/usage
                let consumed = record.consumed as u64;
                return Ok(ConsumeResult::rejected(consumed, limit));
            }

            if cost > limit {
                return Ok(ConsumeResult::rejected(0, limit));
            }

            // 先试新窗口原子重启（复用过期行，保留行身份与创建时间）
            if let Some(row) = Self::query_optional(
                conn,
                RESTART_SQL,
                [
                    quota_key.clone().into(),
                    (cost as i64).into(),
                    (limit as i64).into(),
                    window_start.into(),
                    window_end.into(),
                    now.into(),
                ],
            )
            .await?
            {
                let _ = row_consumed(&row)?;
                return Ok(Self::allowed_result(cost, limit));
            }

            // 全新 key：守卫式插入。并发首触时败方 INSERT 撞 quota_key
            // UNIQUE，但 ON CONFLICT 的 WHERE 过期守卫使其成为 0 行更新
            // （不再向请求抛 UNIQUE 错误），回到循环顶部走原子累加路径。
            if let Some(row) = Self::query_optional(
                conn,
                INSERT_SQL,
                [
                    user_id.into(),
                    resource.into(),
                    quota_key.clone().into(),
                    (limit as i64).into(),
                    (cost as i64).into(),
                    window_start.into(),
                    window_end.into(),
                ],
            )
            .await?
            {
                let _ = row_consumed(&row)?;
                return Ok(Self::allowed_result(cost, limit));
            }
            // 0 行返回 = 冲突行仍活跃（竞态），循环重试
        }

        Err(StorageError::QueryError(t(
            "quota-init-conflict-retries-exhausted",
            &[],
        )))
    }

    /// Reset quota
    ///
    /// 单语句守卫式 upsert（历史教训：曾是 find→save/insert 两步——
    /// SELECT 与全行覆盖之间并发 consume 被覆盖（lost update，刚扣的账
    /// 被抹成 0），并发双 INSERT 撞 quota_key UNIQUE 直接报错）。
    /// ON CONFLICT 消灭 UNIQUE 冲突；DO UPDATE 不触碰 created_at
    /// （保留行身份），reset 语义 = 显式清零账本并开新窗口。
    async fn reset(
        &self,
        user_id: &str,
        resource: &str,
        limit: u64,
        window: StdDuration,
    ) -> Result<(), StorageError> {
        let session = self.get_session().await?;
        let conn = Self::get_conn(&session)?;
        let quota_key = create_quota_key(user_id, resource);
        let now = Utc::now();
        let chrono_window =
            ChronoDuration::from_std(window).unwrap_or_else(|_| ChronoDuration::days(365));
        let window_end = now + chrono_window;

        const RESET_SQL: &str = r#"
            INSERT INTO limiteron_quotas
                (user_id, resource, quota_key, "limit", consumed,
                 window_start, window_end, created_at, updated_at)
            VALUES ($1, $2, $3, $4, 0, $5, $6, $5, $5)
            ON CONFLICT (quota_key) DO UPDATE SET
                consumed = 0,
                "limit" = EXCLUDED."limit",
                window_start = EXCLUDED.window_start,
                window_end = EXCLUDED.window_end,
                updated_at = EXCLUDED.updated_at
            RETURNING consumed
        "#;

        let _row = Self::query_optional(
            conn,
            RESET_SQL,
            [
                user_id.into(),
                resource.into(),
                quota_key.into(),
                (limit as i64).into(),
                now.into(),
                window_end.into(),
            ],
        )
        .await?
        .ok_or_else(|| {
            StorageError::QueryError("quota reset upsert returned no rows".to_string())
        })?;

        Ok(())
    }
    /// 退还配额（业务失败补偿）
    ///
    /// 条件原子 UPDATE：`GREATEST(consumed - n, 0)` 钳制账本不为负，
    /// 仅活跃窗口参与（过期记录 no-op），单条 SQL 消除读改写竞态。
    async fn refund(
        &self,
        user_id: &str,
        resource: &str,
        amount: u64,
        _limit: u64,
        _window: StdDuration,
    ) -> Result<u64, StorageError> {
        let session = self.get_session().await?;
        let conn = Self::get_conn(&session)?;
        let quota_key = create_quota_key(user_id, resource);
        let now = Utc::now();

        const REFUND_SQL: &str = r#"
            UPDATE limiteron_quotas
            SET consumed = GREATEST(consumed - $2, 0), updated_at = $3
            WHERE quota_key = $1 AND window_end > $3
            RETURNING consumed
        "#;

        match Self::query_optional(
            conn,
            REFUND_SQL,
            [quota_key.into(), (amount as i64).into(), now.into()],
        )
        .await?
        {
            Some(row) => Ok(row_consumed(&row)?),
            None => Ok(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_model(user_id: &str, resource: &str, limit: u64, consumed: u64) -> QuotaRecordModel {
        let window_start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let window_end = window_start + ChronoDuration::hours(1);
        QuotaRecordModel {
            id: 1,
            user_id: user_id.to_string(),
            resource: resource.to_string(),
            quota_key: create_quota_key(user_id, resource),
            limit: limit as i64,
            consumed: consumed as i64,
            window_start,
            window_end,
            created_at: window_start,
            updated_at: window_start,
        }
    }

    #[test]
    fn test_model_to_info_basic() {
        let model = make_model("user1", "api_requests", 1000, 250);
        let info = DBNexusQuotaStorageAdapter::model_to_info(&model);
        assert_eq!(info.consumed, 250);
        assert_eq!(info.limit, 1000);
        assert_eq!(info.window_start, model.window_start);
        assert_eq!(info.window_end, model.window_end);
    }

    #[test]
    fn test_model_to_info_zero_consumed() {
        let model = make_model("user2", "storage_mb", 500, 0);
        let info = DBNexusQuotaStorageAdapter::model_to_info(&model);
        assert_eq!(info.consumed, 0);
        assert_eq!(info.limit, 500);
    }

    #[test]
    fn test_model_to_info_full_consumed() {
        let model = make_model("user3", "api_calls", 100, 100);
        let info = DBNexusQuotaStorageAdapter::model_to_info(&model);
        assert_eq!(info.consumed, 100);
        assert_eq!(info.limit, 100);
    }

    #[test]
    fn test_model_to_info_preserves_window() {
        let model = make_model("user4", "resource", 50, 25);
        let info = DBNexusQuotaStorageAdapter::model_to_info(&model);
        assert_eq!(info.window_start, model.window_start);
        assert_eq!(info.window_end, model.window_end);
    }

    #[test]
    fn test_map_err_db_error_to_storage_error() {
        let db_err = dbnexus::DbError::new(sea_orm::DbErr::Custom("quota query error".to_string()));
        let storage_err = DBNexusQuotaStorageAdapter::map_err(db_err);
        match storage_err {
            StorageError::QueryError(msg) => {
                assert!(msg.contains("quota query error"));
            }
            other => panic!("expected QueryError, got {:?}", other),
        }
    }
}
