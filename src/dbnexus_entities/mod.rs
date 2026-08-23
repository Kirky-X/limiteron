// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! DBNexus Entity Definitions for Limiteron
//!
//! This module contains all DBNexus entity definitions used by Limiteron's
//! storage adapters. Each entity corresponds to a database table for storing
//! rate limiting, ban management, and quota control data.

pub mod ban_record;
pub mod key_value;
pub mod quota_record;
pub mod rate_limit;

// Re-exports for adapter implementations (non-test code)
pub use ban_record::{
    ActiveModel as BanRecordActiveModel, Column as BanColumn, Entity as BanRecordEntity,
    Model as BanRecordModel, create_target_key,
};
pub use key_value::{ActiveModel as KeyValueActiveModel, Entity as KeyValueEntity};
pub use quota_record::{
    ActiveModel as QuotaRecordActiveModel, Column as QuotaColumn, Model as QuotaRecordModel,
    create_quota_key,
};

// Re-exports for test code only (DDL helpers and unused entity types)
#[cfg(test)]
pub use ban_record::create_table_ddl as ban_table_ddl;
#[cfg(test)]
pub use key_value::create_table_ddl as key_value_table_ddl;
#[cfg(test)]
pub use quota_record::create_table_ddl as quota_table_ddl;
#[cfg(test)]
pub use rate_limit::{
    Model as RateLimitModel, create_rate_key, create_table_ddl as rate_limit_table_ddl,
};

mod dbnexus_entities_impl;
pub use dbnexus_entities_impl::create_all_tables_ddl;
