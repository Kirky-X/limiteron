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

// Re-export entities for convenient access
pub use ban_record::{
    Entity as BanRecordEntity, Model as BanRecordModel, create_table_ddl as ban_table_ddl,
    create_target_key,
};
pub use key_value::{
    Entity as KeyValueEntity, Model as KeyValueModel, create_table_ddl as key_value_table_ddl,
};
pub use quota_record::{
    Entity as QuotaRecordEntity, Model as QuotaRecordModel, create_quota_key,
    create_table_ddl as quota_table_ddl,
};
pub use rate_limit::{
    Entity as RateLimitEntity, Model as RateLimitModel, create_rate_key,
    create_table_ddl as rate_limit_table_ddl,
};

mod dbnexus_entities_impl;
pub use dbnexus_entities_impl::create_all_tables_ddl;
