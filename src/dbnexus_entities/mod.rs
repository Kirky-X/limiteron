// Copyright (c) 2026, Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

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

/// Create all Limiteron tables
pub fn create_all_tables_ddl() -> &'static str {
    // Combine all table creation DDLs
    let ddl = [
        key_value::create_table_ddl(),
        ban_record::create_table_ddl(),
        quota_record::create_table_ddl(),
        rate_limit::create_table_ddl(),
    ];
    // Leak the string intentionally to return &'static str
    // This is safe because the DDL strings are &'static str and never need deallocation
    Box::leak(ddl.join(";\n").into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_all_tables_ddl() {
        let ddl = create_all_tables_ddl();
        assert!(ddl.contains("limiteron_bans"));
        assert!(ddl.contains("limiteron_quotas"));
        assert!(ddl.contains("limiteron_rate_limits"));
        assert!(ddl.contains("limiteron_kv"));
    }
}
