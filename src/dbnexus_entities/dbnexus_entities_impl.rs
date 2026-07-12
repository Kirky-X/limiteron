// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT

use super::*;

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
