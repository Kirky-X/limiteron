// Copyright (c) 2026, Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! BanRecordEntity - DBNexus entity for ban records
//!
//! This entity is used by DBNexusBanStorageAdapter to store ban records
//! with support for IP bans, user ID bans, and MAC address bans.

use dbnexus::db_crud;
use sea_orm::entity::prelude::DateTimeUtc;
use sea_orm::entity::prelude::*;

/// Ban record model
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "limiteron_bans")]
#[db_crud(table_name = "limiteron_bans")]
pub struct Model {
    /// Primary key - unique ban ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Target type: "ip", "user", "mac"
    #[sea_orm(column_type = "Text")]
    pub target_type: String,
    /// Target value (IP address, user ID, or MAC address)
    #[sea_orm(column_name = "target_value")]
    pub target_value: String,
    /// Composite unique key for efficient lookups
    #[sea_orm(unique)]
    #[sea_orm(column_name = "target_key")]
    pub target_key: String,
    /// Number of times this target has been banned
    pub ban_times: u32,
    /// Ban duration in seconds
    pub duration: i64,
    /// When the ban was applied (UTC)
    pub banned_at: DateTimeUtc,
    /// When the ban expires (UTC)
    pub expires_at: DateTimeUtc,
    /// Whether this is a manual ban (true) or automatic (false)
    pub is_manual: bool,
    /// Reason for the ban
    pub reason: String,
    /// Creation timestamp (UTC)
    pub created_at: DateTimeUtc,
    /// Last update timestamp (UTC)
    pub updated_at: DateTimeUtc,
}

/// Relations for the entity
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl sea_orm::ActiveModelBehavior for ActiveModel {}

/// Create table DDL for BanRecordEntity
pub fn create_table_ddl() -> &'static str {
    r#"
    CREATE TABLE IF NOT EXISTS limiteron_bans (
        id BIGSERIAL PRIMARY KEY,
        target_type VARCHAR(50) NOT NULL,
        target_value TEXT NOT NULL,
        target_key VARCHAR(511) NOT NULL UNIQUE,
        ban_times INTEGER NOT NULL DEFAULT 1,
        duration BIGINT NOT NULL,
        banned_at TIMESTAMP WITH TIME ZONE NOT NULL,
        expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
        is_manual BOOLEAN NOT NULL DEFAULT FALSE,
        reason TEXT NOT NULL,
        created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
        updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
    )
    "#
}

/// Helper to create target key from type and value
pub fn create_target_key(target_type: &str, target_value: &str) -> String {
    format!("{}:{}", target_type, target_value)
}
