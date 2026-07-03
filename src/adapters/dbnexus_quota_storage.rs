// Copyright (c) 2026, Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! DBNexusQuotaStorageAdapter - DBNexus-based implementation of QuotaStorage trait
//!
//! This adapter provides a complete QuotaStorage trait implementation using DBNexus
//! for all quota management operations.

use crate::dbnexus_entities::quota_record::{
    create_quota_key, ActiveModel as QuotaRecordActiveModel, Column as QuotaColumn,
    Model as QuotaRecordModel,
};
use crate::error::{ConsumeResult, StorageError};
use crate::storage::{QuotaInfo, QuotaStorage};
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use dbnexus::{Condition, DbPool, Session};
use sea_orm::entity::prelude::*;
use sea_orm::Set;
use std::sync::Arc;
use std::time::Duration as StdDuration;

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
            consumed: model.consumed,
            limit: model.limit,
            window_start: model.window_start,
            window_end: model.window_end,
        }
    }

    /// Map dbnexus DbError to StorageError
    fn map_err(e: dbnexus::DbError) -> StorageError {
        StorageError::QueryError(e.to_string())
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
    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
        limit: u64,
        window: StdDuration,
    ) -> Result<ConsumeResult, StorageError> {
        let session = self.get_session().await?;
        let conn = Self::get_conn(&session)?;
        let quota_key = create_quota_key(user_id, resource);
        let now = Utc::now();
        let chrono_window =
            ChronoDuration::from_std(window).unwrap_or_else(|_| ChronoDuration::days(365));
        let window_end = now + chrono_window;

        // Try to find existing quota record
        let condition = Condition::all()
            .add(QuotaColumn::QuotaKey.eq(quota_key.clone()))
            .add(QuotaColumn::WindowEnd.gt(now));

        let existing_record = QuotaRecordModel::find_by_condition(&session, condition)
            .await
            .map_err(Self::map_err)?
            .into_iter()
            .next();

        if let Some(record) = existing_record {
            // Check if within limit
            let new_consumed = record.consumed.saturating_add(cost);
            if new_consumed > limit {
                let usage = if limit > 0 {
                    (record.consumed as f64 / limit as f64) * 100.0
                } else {
                    0.0
                };
                return Ok(ConsumeResult {
                    allowed: false,
                    remaining: limit.saturating_sub(record.consumed),
                    alert_triggered: false,
                    usage_percent: usage,
                });
            }

            // Update existing record
            let mut active_model: QuotaRecordActiveModel = record.into();
            active_model.consumed = Set(new_consumed);
            active_model.updated_at = Set(now);

            active_model
                .save(conn)
                .await
                .map_err(|e| StorageError::QueryError(format!("Failed to update quota: {}", e)))?;

            let remaining = limit.saturating_sub(new_consumed);
            let usage = if limit > 0 {
                (new_consumed as f64 / limit as f64) * 100.0
            } else {
                0.0
            };
            return Ok(ConsumeResult {
                allowed: true,
                remaining,
                alert_triggered: false,
                usage_percent: usage,
            });
        }

        // No existing record - check if initial cost exceeds limit
        if cost > limit {
            let usage = if limit > 0 {
                (cost as f64 / limit as f64) * 100.0
            } else {
                0.0
            };
            return Ok(ConsumeResult {
                allowed: false,
                remaining: 0,
                alert_triggered: false,
                usage_percent: usage,
            });
        }

        // Create new quota record
        let model = QuotaRecordModel {
            id: 0,
            user_id: user_id.to_string(),
            resource: resource.to_string(),
            quota_key,
            limit,
            consumed: cost,
            window_start: now,
            window_end,
            created_at: now,
            updated_at: now,
        };

        let active_model: QuotaRecordActiveModel = model.into();
        active_model
            .insert(conn)
            .await
            .map_err(|e| StorageError::QueryError(format!("Failed to create quota: {}", e)))?;

        let remaining = limit.saturating_sub(cost);
        let usage = if limit > 0 {
            (cost as f64 / limit as f64) * 100.0
        } else {
            0.0
        };
        Ok(ConsumeResult {
            allowed: true,
            remaining,
            alert_triggered: false,
            usage_percent: usage,
        })
    }

    /// Reset quota
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

        let model = QuotaRecordModel {
            id: 0,
            user_id: user_id.to_string(),
            resource: resource.to_string(),
            quota_key,
            limit,
            consumed: 0,
            window_start: now,
            window_end,
            created_at: now,
            updated_at: now,
        };

        let active_model: QuotaRecordActiveModel = model.into();
        active_model
            .insert(conn)
            .await
            .map_err(|e| StorageError::QueryError(format!("Failed to reset quota: {}", e)))?;

        Ok(())
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
            limit,
            consumed,
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
