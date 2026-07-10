// Copyright (c) 2026, Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! KeyValueEntity - DBNexus entity for simple key-value storage
//!
//! This entity is used by DBNexusStorageAdapter to store key-value pairs
//! with optional TTL support.
//!
//! Note: No #\[db_entity\] attribute because the primary key is String (not i64, which #\[db_entity\] requires).
//! The storage adapter uses session.connection() directly for sea-orm operations.

use sea_orm::entity::prelude::DateTimeUtc;
use sea_orm::entity::prelude::*;

/// Key-Value storage model
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "limiteron_kv")]
pub struct Model {
    /// Primary key - the storage key
    #[sea_orm(primary_key)]
    pub key: String,
    /// The stored value
    pub value: String,
    /// Optional expiration timestamp (UTC)
    pub expires_at: Option<DateTimeUtc>,
    /// Creation timestamp (UTC)
    pub created_at: DateTimeUtc,
    /// Last update timestamp (UTC)
    pub updated_at: DateTimeUtc,
}

/// Relations for the entity
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl sea_orm::ActiveModelBehavior for ActiveModel {}

/// Create table DDL for KeyValueEntity
pub fn create_table_ddl() -> &'static str {
    r#"
    CREATE TABLE IF NOT EXISTS limiteron_kv (
        key VARCHAR(255) PRIMARY KEY,
        value TEXT NOT NULL,
        expires_at TIMESTAMP WITH TIME ZONE,
        created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
        updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
    )
    "#
}
