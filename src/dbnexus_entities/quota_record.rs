// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! QuotaRecordEntity - DBNexus entity for quota records
//!
//! This entity is used by DBNexusQuotaStorageAdapter to store quota records
//! for tracking user resource consumption.

use dbnexus::db_entity;
use sea_orm::entity::prelude::DateTimeUtc;
use sea_orm::entity::prelude::*;

/// Quota record model
#[db_entity(table_name = "limiteron_quotas", primary_key = "id")]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "limiteron_quotas")]
pub struct Model {
    /// Primary key - unique quota ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// User identifier
    #[sea_orm(column_name = "user_id")]
    pub user_id: String,
    /// Resource being quota-controlled
    pub resource: String,
    /// Composite unique key for efficient lookups
    #[sea_orm(unique)]
    #[sea_orm(column_name = "quota_key")]
    pub quota_key: String,
    /// Total quota limit
    pub limit: u64,
    /// Amount of quota consumed
    pub consumed: u64,
    /// Quota window start time (UTC)
    #[sea_orm(column_name = "window_start")]
    pub window_start: DateTimeUtc,
    /// Quota window end time (UTC)
    #[sea_orm(column_name = "window_end")]
    pub window_end: DateTimeUtc,
    /// Creation timestamp (UTC)
    pub created_at: DateTimeUtc,
    /// Last update timestamp (UTC)
    pub updated_at: DateTimeUtc,
}

/// Relations for the entity
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

/// Create table DDL for QuotaRecordEntity
pub fn create_table_ddl() -> &'static str {
    r#"
    CREATE TABLE IF NOT EXISTS limiteron_quotas (
        id BIGSERIAL PRIMARY KEY,
        user_id VARCHAR(255) NOT NULL,
        resource VARCHAR(255) NOT NULL,
        quota_key VARCHAR(511) NOT NULL UNIQUE,
        limit BIGINT NOT NULL,
        consumed BIGINT NOT NULL DEFAULT 0,
        window_start TIMESTAMP WITH TIME ZONE NOT NULL,
        window_end TIMESTAMP WITH TIME ZONE NOT NULL,
        created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
        updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
    )
    "#
}

/// Helper to create quota key from user_id and resource
pub fn create_quota_key(user_id: &str, resource: &str) -> String {
    format!("{}:{}", user_id, resource)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_quota_key() {
        assert_eq!(
            create_quota_key("user1", "api_requests"),
            "user1:api_requests"
        );
        assert_eq!(create_quota_key("user2", "storage_mb"), "user2:storage_mb");
    }

    #[test]
    fn test_create_table_ddl() {
        let ddl = create_table_ddl();
        assert!(ddl.contains("limiteron_quotas"));
        assert!(ddl.contains("user_id"));
        assert!(ddl.contains("quota_key"));
        assert!(ddl.contains("consumed"));
    }
}
