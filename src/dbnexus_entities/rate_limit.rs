// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! RateLimitEntity - DBNexus entity for rate limit counters
//!
//! This entity is used by DBNexusStorageAdapter to store rate limit
//! counter state for sliding window and fixed window algorithms.

use dbnexus::db_entity;
use sea_orm::entity::prelude::DateTimeUtc;
use sea_orm::entity::prelude::*;

/// Rate limit counter model
#[db_entity(table_name = "limiteron_rate_limits", primary_key = "id")]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "limiteron_rate_limits")]
pub struct Model {
    /// Primary key - unique rate limit ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Rate limit key (identifier + limiter type + parameters)
    #[sea_orm(column_name = "rate_key")]
    pub rate_key: String,
    /// Token bucket: current tokens, or
    /// Fixed/Sliding window: current count
    pub count: u64,
    /// Token bucket: tokens per refill, or
    /// Window: refill rate
    pub rate: u64,
    /// Token bucket: max capacity, or
    /// Window: max count
    pub capacity: u64,
    /// Last refill or window start timestamp (UTC)
    #[sea_orm(column_name = "last_update")]
    pub last_update: DateTimeUtc,
    /// Creation timestamp (UTC)
    pub created_at: DateTimeUtc,
    /// Last update timestamp (UTC)
    pub updated_at: DateTimeUtc,
}

/// Relations for the entity
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

/// Create table DDL for RateLimitEntity
pub fn create_table_ddl() -> &'static str {
    r#"
    CREATE TABLE IF NOT EXISTS limiteron_rate_limits (
        id BIGSERIAL PRIMARY KEY,
        rate_key VARCHAR(511) NOT NULL UNIQUE,
        count BIGINT NOT NULL DEFAULT 0,
        rate BIGINT NOT NULL,
        capacity BIGINT NOT NULL,
        last_update TIMESTAMP WITH TIME ZONE NOT NULL,
        created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
        updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
    )
    "#
}

/// Helper to create rate limit key
pub fn create_rate_key(identifier: &str, limiter_type: &str, params: &str) -> String {
    format!("{}:{}:{}", identifier, limiter_type, params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_rate_key() {
        assert_eq!(
            create_rate_key("user1", "token_bucket", "100/1s"),
            "user1:token_bucket:100/1s"
        );
        assert_eq!(
            create_rate_key("192.168.1.1", "fixed_window", "60/1m"),
            "192.168.1.1:fixed_window:60/1m"
        );
    }

    #[test]
    fn test_create_table_ddl() {
        let ddl = create_table_ddl();
        assert!(ddl.contains("limiteron_rate_limits"));
        assert!(ddl.contains("rate_key"));
        assert!(ddl.contains("capacity"));
    }
}
