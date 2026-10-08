// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! DBNexus Entity Definitions for Limiteron
//!
//! This module contains all DBNexus entity definitions used by Limiteron's
//! storage adapters. Each entity corresponds to a database table for storing
//! rate limiting, ban management, and quota control data.

pub mod ban_record;
pub mod event_outbox;
pub mod key_value;
pub mod quota_record;
pub mod rate_limit;

// Re-exports for adapter implementations (non-test code)
pub use ban_record::{
    ActiveModel as BanRecordActiveModel, Column as BanColumn, Entity as BanRecordEntity,
    Model as BanRecordModel, create_target_key,
};
// EventOutbox 实体仅供 event-system 的 outbox 存储适配器消费；
// 该特性关闭时 re-export 会成为 unused import，故与其唯一消费者同门控。
#[cfg(feature = "event-system")]
pub use event_outbox::{
    ActiveModel as EventOutboxActiveModel, Column as EventOutboxColumn,
    Entity as EventOutboxEntity, Model as EventOutboxModel,
};
pub use key_value::{ActiveModel as KeyValueActiveModel, Entity as KeyValueEntity};
pub use quota_record::{Column as QuotaColumn, Model as QuotaRecordModel, create_quota_key};

mod dbnexus_entities_impl;
pub use dbnexus_entities_impl::{create_all_tables_ddl, create_all_tables_ddl_sqlite};
