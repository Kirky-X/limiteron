// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! EventOutbox — DBNexus entity for the event outbox table
//!
//! 封禁/配额事件的 outbox 表：事务内写入 pending 行，后台投递成功后标记
//! published（Transactional Outbox 模式，跨实例最终一致）。

use dbnexus::db_entity;
use sea_orm::entity::prelude::*;

/// Event outbox model
#[db_entity(table_name = "limiteron_event_outbox", primary_key = "id")]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "limiteron_event_outbox")]
pub struct Model {
    /// Primary key
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 事件类型：ban_applied / ban_removed / quota_alert
    #[sea_orm(column_type = "Text")]
    pub event_type: String,
    /// 聚合标识（封禁 target key / 配额 key）
    #[sea_orm(column_name = "aggregate_id")]
    #[sea_orm(column_type = "Text")]
    pub aggregate_id: String,
    /// 事件负载（JSON）
    #[sea_orm(column_type = "Text")]
    pub payload: String,
    /// 投递状态：pending / published
    #[sea_orm(column_name = "delivery_status")]
    pub status: String,
    /// 创建时间（UTC）
    pub created_at: DateTimeUtc,
    /// 投递完成时间（UTC；pending 时为 NULL）
    pub published_at: Option<DateTimeUtc>,
}

/// Relations for the entity
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

/// Postgres 方言建表 DDL
pub fn create_table_ddl() -> &'static str {
    r#"
    CREATE TABLE IF NOT EXISTS limiteron_event_outbox (
        id BIGSERIAL PRIMARY KEY,
        event_type TEXT NOT NULL,
        aggregate_id TEXT NOT NULL,
        payload TEXT NOT NULL,
        delivery_status VARCHAR(20) NOT NULL DEFAULT 'pending',
        created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
        published_at TIMESTAMP WITH TIME ZONE
    )
    "#
}

/// SQLite 方言建表 DDL（单测/本地后端）
pub fn create_table_ddl_sqlite() -> &'static str {
    r#"
    CREATE TABLE IF NOT EXISTS limiteron_event_outbox (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        event_type TEXT NOT NULL,
        aggregate_id TEXT NOT NULL,
        payload TEXT NOT NULL,
        delivery_status TEXT NOT NULL DEFAULT 'pending',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        published_at TEXT
    )
    "#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outbox_ddl_contains_table() {
        assert!(create_table_ddl().contains("limiteron_event_outbox"));
        assert!(create_table_ddl_sqlite().contains("limiteron_event_outbox"));
        assert!(create_table_ddl_sqlite().contains("delivery_status"));
    }
}
