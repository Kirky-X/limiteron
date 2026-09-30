// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT

use super::*;

/// Create all Limiteron tables
pub fn create_all_tables_ddl() -> String {
    let ddl = [
        key_value::create_table_ddl(),
        ban_record::create_table_ddl(),
        quota_record::create_table_ddl(),
        rate_limit::create_table_ddl(),
        event_outbox::create_table_ddl(),
    ];
    ddl.join(";\n")
}

/// Create all Limiteron tables（SQLite 方言，嵌入式后端）
pub fn create_all_tables_ddl_sqlite() -> String {
    let ddl = [
        key_value::create_table_ddl_sqlite(),
        ban_record::create_table_ddl_sqlite(),
        quota_record::create_table_ddl_sqlite(),
        rate_limit::create_table_ddl_sqlite(),
        event_outbox::create_table_ddl_sqlite(),
    ];
    ddl.join(";\n")
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
        assert!(ddl.contains("limiteron_event_outbox"));
    }
}
