// Copyright (c) 2026, Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! DBNexusBanStorageAdapter - DBNexus-based implementation of BanStorage trait
//!
//! This adapter provides a complete BanStorage trait implementation using DBNexus
//! for all ban management operations.

use crate::dbnexus_entities::ban_record::{
    create_target_key, ActiveModel as BanRecordActiveModel, Column as BanColumn,
    Entity as BanRecordEntity, Model as BanRecordModel,
};
use crate::error::StorageError;
use crate::storage::{BanHistory, BanRecord, BanStorage, BanTarget};
use async_trait::async_trait;
use chrono::Utc;
use dbnexus::{Condition, DbPool, Session};
use sea_orm::entity::prelude::*;
use sea_orm::{Order, QueryOrder, QuerySelect, Set};
use std::sync::Arc;
use std::time::Duration as StdDuration;

/// DBNexus-based ban storage adapter
pub struct DBNexusBanStorageAdapter {
    pool: Arc<DbPool>,
}

impl DBNexusBanStorageAdapter {
    /// Create a new DBNexusBanStorageAdapter
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

    /// Convert BanTarget to target key
    fn target_to_key(target: &BanTarget) -> String {
        match target {
            BanTarget::Ip(ip) => create_target_key("ip", ip),
            BanTarget::UserId(user_id) => create_target_key("user", user_id),
            BanTarget::Mac(mac) => create_target_key("mac", mac),
            BanTarget::Geo { country_code } => create_target_key("geo", country_code),
        }
    }

    /// Convert BanTarget to type and value
    fn target_to_type_value(target: &BanTarget) -> (String, String) {
        match target {
            BanTarget::Ip(ip) => ("ip".to_string(), ip.clone()),
            BanTarget::UserId(user_id) => ("user".to_string(), user_id.clone()),
            BanTarget::Mac(mac) => ("mac".to_string(), mac.clone()),
            BanTarget::Geo { country_code } => ("geo".to_string(), country_code.clone()),
        }
    }

    /// Convert model to BanRecord
    fn model_to_record(model: &BanRecordModel) -> BanRecord {
        let (target_type, _target_value) = match model.target_type.as_str() {
            "ip" => (
                BanTarget::Ip(model.target_value.clone()),
                model.target_value.clone(),
            ),
            "user" => (
                BanTarget::UserId(model.target_value.clone()),
                model.target_value.clone(),
            ),
            "geo" => (
                BanTarget::Geo {
                    country_code: model.target_value.clone(),
                },
                model.target_value.clone(),
            ),
            _ => (
                BanTarget::Mac(model.target_value.clone()),
                model.target_value.clone(),
            ),
        };

        // Convert i64 seconds to std::time::Duration
        let duration = StdDuration::from_secs(model.duration as u64);

        BanRecord {
            target: target_type,
            ban_times: model.ban_times,
            duration,
            banned_at: model.banned_at,
            expires_at: model.expires_at,
            is_manual: model.is_manual,
            reason: model.reason.clone(),
        }
    }

    /// Map dbnexus DbError to StorageError
    fn map_err(e: dbnexus::DbError) -> StorageError {
        StorageError::QueryError(e.to_string())
    }
}

#[async_trait]
impl BanStorage for DBNexusBanStorageAdapter {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// Check if a target is currently banned
    async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
        let session = self.get_session().await?;
        let target_key = Self::target_to_key(target);
        let now = Utc::now();

        let condition = Condition::all()
            .add(BanColumn::TargetKey.eq(target_key))
            .add(BanColumn::ExpiresAt.gt(now));

        let records = BanRecordModel::find_by_condition(&session, condition)
            .await
            .map_err(Self::map_err)?;

        Ok(records
            .into_iter()
            .next()
            .map(|m| Self::model_to_record(&m)))
    }

    /// Save a ban record
    async fn save(&self, record: &BanRecord) -> Result<(), StorageError> {
        let session = self.get_session().await?;
        let conn = Self::get_conn(&session)?;
        let now = Utc::now();
        let (target_type, target_value) = Self::target_to_type_value(&record.target);
        let target_key = create_target_key(&target_type, &target_value);

        // Check if record exists
        let existing = BanRecordEntity::find()
            .filter(BanColumn::TargetKey.eq(target_key.clone()))
            .one(conn)
            .await
            .map_err(|e| StorageError::QueryError(e.to_string()))?;

        // Calculate duration in seconds from expires_at - banned_at
        let duration_secs = record
            .expires_at
            .signed_duration_since(record.banned_at)
            .num_seconds()
            .max(0);

        let model_id = existing.as_ref().map(|m| m.id).unwrap_or(0);
        let model = BanRecordModel {
            id: model_id,
            target_type,
            target_value,
            target_key,
            ban_times: record.ban_times,
            duration: duration_secs,
            banned_at: record.banned_at,
            expires_at: record.expires_at,
            is_manual: record.is_manual,
            reason: record.reason.clone(),
            created_at: existing.as_ref().map(|m| m.created_at).unwrap_or(now),
            updated_at: now,
        };

        if existing.is_some() {
            // Update existing record
            let mut active_model: BanRecordActiveModel = model.into();
            active_model.id = Set(model_id);
            active_model.save(conn).await.map_err(|e| {
                StorageError::QueryError(format!("Failed to update ban record: {}", e))
            })?;
        } else {
            // Insert new record
            let active_model: BanRecordActiveModel = model.into();
            active_model.insert(conn).await.map_err(|e| {
                StorageError::QueryError(format!("Failed to insert ban record: {}", e))
            })?;
        }

        Ok(())
    }

    /// Get ban history for a target
    async fn get_history(&self, target: &BanTarget) -> Result<Option<BanHistory>, StorageError> {
        let session = self.get_session().await?;
        let target_key = Self::target_to_key(target);

        let condition = Condition::all().add(BanColumn::TargetKey.eq(target_key));

        let mut records = BanRecordModel::find_by_condition(&session, condition)
            .await
            .map_err(Self::map_err)?;

        if records.is_empty() {
            return Ok(None);
        }

        // Sort by banned_at descending (most recent first)
        records.sort_by_key(|r| std::cmp::Reverse(r.banned_at));

        let first = records.remove(0);
        Ok(Some(BanHistory {
            ban_times: first.ban_times,
            last_banned_at: first.banned_at,
        }))
    }

    /// Increment ban times for a target
    async fn increment_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let session = self.get_session().await?;
        let conn = Self::get_conn(&session)?;
        let target_key = Self::target_to_key(target);

        // Find existing record
        let existing_opt = BanRecordEntity::find()
            .filter(BanColumn::TargetKey.eq(target_key))
            .one(conn)
            .await
            .map_err(|e| StorageError::QueryError(e.to_string()))?;

        if let Some(existing) = existing_opt {
            let new_ban_times = existing.ban_times + 1;
            let mut active_model: BanRecordActiveModel = existing.into();
            active_model.ban_times = Set(new_ban_times);
            active_model.updated_at = Set(Utc::now());
            active_model.save(conn).await.map_err(|e| {
                StorageError::QueryError(format!("Failed to increment ban times: {}", e))
            })?;
            Ok(new_ban_times as u64)
        } else {
            Err(StorageError::NotFound("Ban record not found".to_string()))
        }
    }

    /// Get ban times for a target
    async fn get_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let session = self.get_session().await?;
        let target_key = Self::target_to_key(target);

        let condition = Condition::all().add(BanColumn::TargetKey.eq(target_key));

        let records = BanRecordModel::find_by_condition(&session, condition)
            .await
            .map_err(Self::map_err)?;

        Ok(records
            .into_iter()
            .next()
            .map(|m| m.ban_times as u64)
            .unwrap_or(0))
    }

    /// Remove a ban record
    async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError> {
        let session = self.get_session().await?;
        let target_key = Self::target_to_key(target);

        let condition = Condition::all().add(BanColumn::TargetKey.eq(target_key));

        BanRecordModel::delete_many(&session, condition)
            .await
            .map_err(Self::map_err)?;

        Ok(())
    }

    /// Clean up expired bans
    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        let session = self.get_session().await?;
        let now = Utc::now();

        let condition = Condition::all().add(BanColumn::ExpiresAt.lt(now));

        let deleted = BanRecordModel::delete_many(&session, condition)
            .await
            .map_err(Self::map_err)?;

        Ok(deleted)
    }

    async fn list_bans(
        &self,
        active_only: bool,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BanRecord>, StorageError> {
        let session = self.get_session().await?;
        let conn = Self::get_conn(&session)?;
        let now = Utc::now();

        let mut condition = Condition::all();
        if active_only {
            condition = condition.add(BanColumn::ExpiresAt.gt(now));
        }

        let records: Vec<BanRecordModel> = BanRecordEntity::find()
            .filter(condition)
            .order_by(BanColumn::CreatedAt, Order::Desc)
            .offset(Some(offset))
            .limit(Some(limit))
            .all(conn)
            .await
            .map_err(|e| StorageError::QueryError(e.to_string()))?;

        Ok(records
            .into_iter()
            .map(|m| Self::model_to_record(&m))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_model(
        target_type: &str,
        target_value: &str,
        ban_times: u32,
        duration: i64,
        is_manual: bool,
        reason: &str,
    ) -> BanRecordModel {
        let banned_at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let expires_at = banned_at + chrono::Duration::seconds(duration);
        BanRecordModel {
            id: 1,
            target_type: target_type.to_string(),
            target_value: target_value.to_string(),
            target_key: create_target_key(target_type, target_value),
            ban_times,
            duration,
            banned_at,
            expires_at,
            is_manual,
            reason: reason.to_string(),
            created_at: banned_at,
            updated_at: banned_at,
        }
    }

    #[test]
    fn test_target_to_key_ip() {
        let target = BanTarget::Ip("192.168.1.1".to_string());
        assert_eq!(
            DBNexusBanStorageAdapter::target_to_key(&target),
            "ip:192.168.1.1"
        );
    }

    #[test]
    fn test_target_to_key_user_id() {
        let target = BanTarget::UserId("user123".to_string());
        assert_eq!(
            DBNexusBanStorageAdapter::target_to_key(&target),
            "user:user123"
        );
    }

    #[test]
    fn test_target_to_key_mac() {
        let target = BanTarget::Mac("aa:bb:cc:dd:ee:ff".to_string());
        assert_eq!(
            DBNexusBanStorageAdapter::target_to_key(&target),
            "mac:aa:bb:cc:dd:ee:ff"
        );
    }

    #[test]
    fn test_target_to_type_value_ip() {
        let target = BanTarget::Ip("192.168.1.1".to_string());
        let (t, v) = DBNexusBanStorageAdapter::target_to_type_value(&target);
        assert_eq!(t, "ip");
        assert_eq!(v, "192.168.1.1");
    }

    #[test]
    fn test_target_to_type_value_user_id() {
        let target = BanTarget::UserId("user123".to_string());
        let (t, v) = DBNexusBanStorageAdapter::target_to_type_value(&target);
        assert_eq!(t, "user");
        assert_eq!(v, "user123");
    }

    #[test]
    fn test_target_to_type_value_mac() {
        let target = BanTarget::Mac("aa:bb:cc:dd:ee:ff".to_string());
        let (t, v) = DBNexusBanStorageAdapter::target_to_type_value(&target);
        assert_eq!(t, "mac");
        assert_eq!(v, "aa:bb:cc:dd:ee:ff");
    }

    #[test]
    fn test_model_to_record_ip() {
        let model = make_model("ip", "192.168.1.1", 3, 3600, true, "excessive requests");
        let record = DBNexusBanStorageAdapter::model_to_record(&model);
        assert_eq!(record.target, BanTarget::Ip("192.168.1.1".to_string()));
        assert_eq!(record.ban_times, 3);
        assert_eq!(record.duration, StdDuration::from_secs(3600));
        assert!(record.is_manual);
        assert_eq!(record.reason, "excessive requests");
    }

    #[test]
    fn test_model_to_record_user() {
        let model = make_model("user", "user123", 1, 7200, false, "abuse");
        let record = DBNexusBanStorageAdapter::model_to_record(&model);
        assert_eq!(record.target, BanTarget::UserId("user123".to_string()));
        assert_eq!(record.ban_times, 1);
        assert_eq!(record.duration, StdDuration::from_secs(7200));
        assert!(!record.is_manual);
        assert_eq!(record.reason, "abuse");
    }

    #[test]
    fn test_model_to_record_unknown_type_falls_back_to_mac() {
        // 未知 target_type 应回退到 Mac
        let model = make_model("unknown", "aa:bb:cc:dd:ee:ff", 2, 1800, true, "test");
        let record = DBNexusBanStorageAdapter::model_to_record(&model);
        assert_eq!(
            record.target,
            BanTarget::Mac("aa:bb:cc:dd:ee:ff".to_string())
        );
        assert_eq!(record.ban_times, 2);
        assert_eq!(record.duration, StdDuration::from_secs(1800));
    }

    #[test]
    fn test_model_to_record_preserves_timestamps() {
        let model = make_model("ip", "10.0.0.1", 1, 3600, false, "test");
        let record = DBNexusBanStorageAdapter::model_to_record(&model);
        assert_eq!(record.banned_at, model.banned_at);
        assert_eq!(record.expires_at, model.expires_at);
    }

    #[test]
    fn test_map_err_db_error_to_storage_error() {
        // 通过构造一个 DbError 来测试 map_err
        let db_err = dbnexus::DbError::new(sea_orm::DbErr::Custom("test query error".to_string()));
        let storage_err = DBNexusBanStorageAdapter::map_err(db_err);
        match storage_err {
            StorageError::QueryError(msg) => {
                assert!(msg.contains("test query error"));
            }
            other => panic!("expected QueryError, got {:?}", other),
        }
    }
}
