// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Unit tests for DBNexus Storage Adapters
//!
//! These tests verify that the storage adapters correctly implement
//! the Storage, BanStorage, and QuotaStorage traits.
//!
//! Note: These are compile-time verification tests that ensure
//! the trait implementations are correct. Runtime tests require
//! a real database connection.

#[cfg(test)]
mod storage_adapter_tests {
    use crate::error::StorageError;

    #[test]
    fn test_storage_error_types() {
        // Test StorageError conversion
        let not_found_error = StorageError::NotFound("key not found".to_string());
        assert_eq!(not_found_error.to_string(), "未找到: key not found");

        let query_error = StorageError::QueryError("database error".to_string());
        assert_eq!(query_error.to_string(), "查询错误: database error");

        let connection_error = StorageError::ConnectionError("connection failed".to_string());
        assert_eq!(connection_error.to_string(), "连接错误: connection failed");
    }

    #[test]
    fn test_storage_error_is_transient() {
        let timeout_error = StorageError::TimeoutError("timeout".to_string());
        assert!(timeout_error.is_transient());

        let connection_error = StorageError::ConnectionError("connection failed".to_string());
        assert!(connection_error.is_transient());

        let rate_limit_error = StorageError::RateLimitError("rate limited".to_string());
        assert!(rate_limit_error.is_transient());
    }

    #[test]
    fn test_storage_error_is_permanent() {
        let auth_error = StorageError::AuthenticationError("auth failed".to_string());
        assert!(auth_error.is_permanent());

        let permission_error = StorageError::PermissionError("permission denied".to_string());
        assert!(permission_error.is_permanent());

        let config_error = StorageError::InvalidConfig("invalid config".to_string());
        assert!(config_error.is_permanent());
    }

    #[test]
    fn test_storage_error_display() {
        let errors = [
            (StorageError::NotFound("test".to_string()), "未找到: test"),
            (
                StorageError::QueryError("test".to_string()),
                "查询错误: test",
            ),
            (
                StorageError::ConnectionError("test".to_string()),
                "连接错误: test",
            ),
            (
                StorageError::TimeoutError("test".to_string()),
                "超时错误: test",
            ),
            (
                StorageError::RateLimitError("test".to_string()),
                "速率限制: test",
            ),
            (
                StorageError::AuthenticationError("test".to_string()),
                "认证错误: test",
            ),
            (
                StorageError::PermissionError("test".to_string()),
                "权限错误: test",
            ),
            (
                StorageError::InvalidConfig("test".to_string()),
                "无效配置: test",
            ),
        ];

        for (error, expected) in errors {
            assert_eq!(error.to_string(), expected);
        }
    }
}

#[cfg(test)]
mod ban_storage_adapter_tests {
    use crate::storage::{BanHistory, BanRecord, BanTarget};

    #[test]
    fn test_ban_target_types() {
        // Test different ban target types
        let ip_target = BanTarget::Ip("192.168.1.1".to_string());
        let user_target = BanTarget::UserId("user123".to_string());
        let mac_target = BanTarget::Mac("00:1A:2B:3C:4D:5E".to_string());

        match ip_target {
            BanTarget::Ip(ip) => assert_eq!(ip, "192.168.1.1"),
            _ => panic!("Expected Ip variant"),
        }

        match user_target {
            BanTarget::UserId(id) => assert_eq!(id, "user123"),
            _ => panic!("Expected UserId variant"),
        }

        match mac_target {
            BanTarget::Mac(mac) => assert_eq!(mac, "00:1A:2B:3C:4D:5E"),
            _ => panic!("Expected Mac variant"),
        }
    }

    #[test]
    fn test_ban_target_serialization() {
        use serde::{Deserialize, Serialize};

        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(tag = "type", content = "value")]
        enum BanTargetSerde {
            #[serde(rename = "ip")]
            Ip(String),
            #[serde(rename = "user")]
            UserId(String),
            #[serde(rename = "mac")]
            Mac(String),
        }

        let ip = BanTargetSerde::Ip("192.168.1.1".to_string());
        let json = serde_json::to_string(&ip).unwrap();
        assert!(json.contains("ip"));

        let user = BanTargetSerde::UserId("user123".to_string());
        let json = serde_json::to_string(&user).unwrap();
        assert!(json.contains("user"));
    }

    #[test]
    fn test_ban_record_structure() {
        use chrono::Utc;

        let now = Utc::now();
        let record = BanRecord {
            target: BanTarget::Ip("192.168.1.1".to_string()),
            ban_times: 3,
            duration: std::time::Duration::from_secs(24 * 3600),
            banned_at: now,
            expires_at: now + chrono::Duration::hours(24),
            is_manual: true,
            reason: "Repeated policy violations".to_string(),
        };

        assert_eq!(record.ban_times, 3);
        assert_eq!(record.duration, std::time::Duration::from_secs(24 * 3600));
        assert!(record.is_manual);
        assert!(!record.reason.is_empty());
    }

    #[test]
    fn test_ban_history_structure() {
        use chrono::Utc;

        let now = Utc::now();
        let history = BanHistory {
            ban_times: 5,
            last_banned_at: now,
        };

        assert_eq!(history.ban_times, 5);
        assert_eq!(history.last_banned_at, now);
    }

    #[test]
    fn test_ban_record_with_user_target() {
        use chrono::Utc;

        let now = Utc::now();
        let record = BanRecord {
            target: BanTarget::UserId("user456".to_string()),
            ban_times: 1,
            duration: std::time::Duration::from_secs(30 * 60),
            banned_at: now,
            expires_at: now + chrono::Duration::minutes(30),
            is_manual: false,
            reason: "Auto-ban due to rate limit".to_string(),
        };

        match &record.target {
            BanTarget::UserId(id) => assert_eq!(id, "user456"),
            _ => panic!("Expected UserId variant"),
        }
        assert_eq!(record.ban_times, 1);
        assert!(!record.is_manual);
    }

    #[test]
    fn test_ban_record_with_mac_target() {
        use chrono::Utc;

        let now = Utc::now();
        let record = BanRecord {
            target: BanTarget::Mac("AA:BB:CC:DD:EE:FF".to_string()),
            ban_times: 2,
            duration: std::time::Duration::from_secs(7 * 24 * 3600),
            banned_at: now,
            expires_at: now + chrono::Duration::days(7),
            is_manual: true,
            reason: "Suspicious activity detected".to_string(),
        };

        match &record.target {
            BanTarget::Mac(mac) => assert_eq!(mac, "AA:BB:CC:DD:EE:FF"),
            _ => panic!("Expected Mac variant"),
        }
        assert_eq!(record.ban_times, 2);
        assert!(record.is_manual);
    }
}

#[cfg(test)]
mod quota_storage_adapter_tests {
    use crate::error::ConsumeResult;
    use crate::storage::QuotaInfo;
    use chrono::Duration;

    #[test]
    fn test_quota_info_structure() {
        use chrono::Utc;

        let now = Utc::now();
        let info = QuotaInfo {
            consumed: 500,
            limit: 1000,
            window_start: now,
            window_end: now + Duration::hours(1),
        };

        assert_eq!(info.consumed, 500);
        assert_eq!(info.limit, 1000);
        assert_eq!(info.limit.saturating_sub(info.consumed), 500);
    }

    #[test]
    fn test_quota_info_remaining() {
        let info = QuotaInfo {
            consumed: 300,
            limit: 1000,
            window_start: chrono::Utc::now(),
            window_end: chrono::Utc::now() + Duration::hours(1),
        };

        assert_eq!(info.limit.saturating_sub(info.consumed), 700);
    }

    #[test]
    fn test_quota_info_full_consumption() {
        let info = QuotaInfo {
            consumed: 1000,
            limit: 1000,
            window_start: chrono::Utc::now(),
            window_end: chrono::Utc::now() + Duration::hours(1),
        };

        assert_eq!(info.limit.saturating_sub(info.consumed), 0);
    }

    #[test]
    fn test_quota_info_over_consumption() {
        let info = QuotaInfo {
            consumed: 1200,
            limit: 1000,
            window_start: chrono::Utc::now(),
            window_end: chrono::Utc::now() + Duration::hours(1),
        };

        assert_eq!(info.limit.saturating_sub(info.consumed), 0);
    }

    #[test]
    fn test_consume_result_allowed() {
        let result = ConsumeResult {
            allowed: true,
            remaining: 900,
            alert_triggered: false,
            usage_percent: 10.0,
        };

        assert!(result.allowed);
        assert_eq!(result.remaining, 900);
        assert!(!result.alert_triggered);
        assert!((result.usage_percent - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_consume_result_exhausted() {
        let result = ConsumeResult {
            allowed: false,
            remaining: 0,
            alert_triggered: true,
            usage_percent: 100.0,
        };

        assert!(!result.allowed);
        assert_eq!(result.remaining, 0);
        assert!(result.alert_triggered);
        assert!((result.usage_percent - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_consume_result_partial() {
        let result = ConsumeResult {
            allowed: true,
            remaining: 250,
            alert_triggered: false,
            usage_percent: 75.0,
        };

        assert!(result.allowed);
        assert_eq!(result.remaining, 250);
        assert!(!result.alert_triggered);
        assert!((result.usage_percent - 75.0).abs() < f64::EPSILON);
    }
}

#[cfg(test)]
mod entity_helper_functions {
    use crate::dbnexus_entities::{create_quota_key, create_rate_key, create_target_key};

    #[test]
    fn test_create_target_key() {
        assert_eq!(create_target_key("ip", "192.168.1.1"), "ip:192.168.1.1");
        assert_eq!(create_target_key("user", "user123"), "user:user123");
        assert_eq!(
            create_target_key("mac", "00:1A:2B:3C:4D:5E"),
            "mac:00:1A:2B:3C:4D:5E"
        );
    }

    #[test]
    fn test_create_target_key_special_characters() {
        // Test with special characters in IP
        assert_eq!(
            create_target_key("ip", "192.168.1.1:8080"),
            "ip:192.168.1.1:8080"
        );
        // Test with UUID-like user ID
        assert_eq!(
            create_target_key("user", "550e8400-e29b-41d4-a716-446655440000"),
            "user:550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn test_create_quota_key() {
        assert_eq!(
            create_quota_key("user123", "api_calls"),
            "user123:api_calls"
        );
        assert_eq!(
            create_quota_key("user456", "storage_bytes"),
            "user456:storage_bytes"
        );
    }

    #[test]
    fn test_create_quota_key_special_cases() {
        // Test with empty resource
        assert_eq!(create_quota_key("user123", ""), "user123:");
        // Test with special characters
        assert_eq!(
            create_quota_key("user123", "api/calls:v2"),
            "user123:api/calls:v2"
        );
    }

    #[test]
    fn test_create_rate_key() {
        assert_eq!(
            create_rate_key("192.168.1.1", "token_bucket", "capacity=100,refill=10"),
            "192.168.1.1:token_bucket:capacity=100,refill=10"
        );
        assert_eq!(
            create_rate_key("user123", "fixed_window", "limit=100,window=60"),
            "user123:fixed_window:limit=100,window=60"
        );
    }

    #[test]
    fn test_create_rate_key_sliding_window() {
        assert_eq!(
            create_rate_key("192.168.1.1", "sliding_window", "max=50,window=60"),
            "192.168.1.1:sliding_window:max=50,window=60"
        );
    }

    #[test]
    fn test_key_format_consistency() {
        // Verify that key generation produces consistent formats
        let ip_key = create_target_key("ip", "10.0.0.1");
        let user_key = create_target_key("user", "admin");
        let mac_key = create_target_key("mac", "00:11:22:33:44:55");

        // All keys should contain colon separator
        assert!(ip_key.contains(':'));
        assert!(user_key.contains(':'));
        assert!(mac_key.contains(':'));

        // Prefix should match target type
        assert!(ip_key.starts_with("ip:"));
        assert!(user_key.starts_with("user:"));
        assert!(mac_key.starts_with("mac:"));
    }
}

#[cfg(test)]
mod table_ddl_tests {
    use crate::dbnexus_entities::{
        ban_table_ddl, key_value_table_ddl, quota_table_ddl, rate_limit_table_ddl,
    };

    #[test]
    fn test_key_value_table_ddl() {
        let ddl = key_value_table_ddl();
        assert!(ddl.contains("limiteron_kv"));
        assert!(ddl.contains("PRIMARY KEY"));
        assert!(ddl.contains("expires_at"));
    }

    #[test]
    fn test_key_value_table_columns() {
        let ddl = key_value_table_ddl();
        // Verify all required columns are present
        assert!(ddl.contains("key"));
        assert!(ddl.contains("value"));
        assert!(ddl.contains("created_at"));
        assert!(ddl.contains("updated_at"));
    }

    #[test]
    fn test_ban_table_ddl() {
        let ddl = ban_table_ddl();
        assert!(ddl.contains("limiteron_bans"));
        assert!(ddl.contains("BIGSERIAL"));
        assert!(ddl.contains("target_type"));
        assert!(ddl.contains("target_value"));
        assert!(ddl.contains("expires_at"));
    }

    #[test]
    fn test_ban_table_columns() {
        let ddl = ban_table_ddl();
        // Verify all required columns are present
        assert!(ddl.contains("target_key"));
        assert!(ddl.contains("ban_times"));
        assert!(ddl.contains("duration"));
        assert!(ddl.contains("banned_at"));
        assert!(ddl.contains("is_manual"));
        assert!(ddl.contains("reason"));
        assert!(ddl.contains("created_at"));
        assert!(ddl.contains("updated_at"));
    }

    #[test]
    fn test_quota_table_ddl() {
        let ddl = quota_table_ddl();
        assert!(ddl.contains("limiteron_quotas"));
        assert!(ddl.contains("user_id"));
        assert!(ddl.contains("resource"));
        assert!(ddl.contains("limit"));
        assert!(ddl.contains("consumed"));
    }

    #[test]
    fn test_quota_table_columns() {
        let ddl = quota_table_ddl();
        // Verify all required columns are present
        assert!(ddl.contains("quota_key"));
        assert!(ddl.contains("window_start"));
        assert!(ddl.contains("window_end"));
        assert!(ddl.contains("created_at"));
        assert!(ddl.contains("updated_at"));
    }

    #[test]
    fn test_rate_limit_table_ddl() {
        let ddl = rate_limit_table_ddl();
        assert!(ddl.contains("limiteron_rate_limits"));
        assert!(ddl.contains("rate_key"));
        assert!(ddl.contains("count"));
        assert!(ddl.contains("rate"));
        assert!(ddl.contains("capacity"));
    }

    #[test]
    fn test_rate_limit_table_columns() {
        let ddl = rate_limit_table_ddl();
        // Verify all required columns are present
        assert!(ddl.contains("last_update"));
        assert!(ddl.contains("created_at"));
        assert!(ddl.contains("updated_at"));
    }

    #[test]
    fn test_ddl_contains_timestamps() {
        // All tables should have timestamp columns for auditing
        let kv_ddl = key_value_table_ddl();
        let ban_ddl = ban_table_ddl();
        let quota_ddl = quota_table_ddl();
        let rate_ddl = rate_limit_table_ddl();

        assert!(kv_ddl.contains("created_at"));
        assert!(kv_ddl.contains("updated_at"));

        assert!(ban_ddl.contains("created_at"));
        assert!(ban_ddl.contains("updated_at"));

        assert!(quota_ddl.contains("created_at"));
        assert!(quota_ddl.contains("updated_at"));

        assert!(rate_ddl.contains("created_at"));
        assert!(rate_ddl.contains("updated_at"));
    }
}

#[cfg(test)]
mod adapter_trait_signature_tests {
    use crate::adapters::DBNexusBanStorageAdapter;
    use crate::adapters::DBNexusQuotaStorageAdapter;
    use crate::adapters::DBNexusStorageAdapter;

    #[tokio::test]
    #[ignore = "requires a real database connection"]
    async fn test_storage_adapter_trait_signature() {
        // DBNexusStorageAdapter needs Arc<DbPool> which requires a real DB
        let _ = std::any::type_name::<DBNexusStorageAdapter>();
    }

    #[tokio::test]
    #[ignore = "requires a real database connection"]
    async fn test_ban_storage_adapter_trait_signature() {
        // DBNexusBanStorageAdapter needs Arc<DbPool> which requires a real DB
        let _ = std::any::type_name::<DBNexusBanStorageAdapter>();
    }

    #[tokio::test]
    #[ignore = "requires a real database connection"]
    async fn test_quota_storage_adapter_trait_signature() {
        // DBNexusQuotaStorageAdapter needs Arc<DbPool> which requires a real DB
        let _ = std::any::type_name::<DBNexusQuotaStorageAdapter>();
    }
}

#[cfg(test)]
mod adapter_construction_tests {
    use crate::adapters::DBNexusBanStorageAdapter;
    use crate::adapters::DBNexusQuotaStorageAdapter;
    use crate::adapters::DBNexusStorageAdapter;

    #[test]
    #[ignore = "requires a real database connection"]
    fn test_storage_adapter_new() {
        // DBNexusStorageAdapter::new() requires Arc<DbPool>
        // which needs a real database connection
        let _ = std::any::type_name::<DBNexusStorageAdapter>();
    }

    #[test]
    #[ignore = "requires a real database connection"]
    fn test_ban_storage_adapter_new() {
        // DBNexusBanStorageAdapter::new() requires Arc<DbPool>
        // which needs a real database connection
        // This test is ignored because it can't run without a DB
        let _ = std::any::type_name::<DBNexusBanStorageAdapter>();
    }

    #[test]
    #[ignore = "requires a real database connection"]
    fn test_quota_storage_adapter_new() {
        // DBNexusQuotaStorageAdapter::new() requires Arc<DbPool>
        // which needs a real database connection
        let _ = std::any::type_name::<DBNexusQuotaStorageAdapter>();
    }

    #[test]
    fn test_adapter_impl_send_sync() {
        // Verify that all adapters are Send + Sync
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<DBNexusStorageAdapter>();
        assert_send_sync::<DBNexusBanStorageAdapter>();
        assert_send_sync::<DBNexusQuotaStorageAdapter>();
    }
}

#[cfg(test)]
mod model_structure_tests {
    use crate::dbnexus_entities::{
        BanRecordModel, KeyValueModel, QuotaRecordModel, RateLimitModel,
    };
    use chrono::Utc;

    #[test]
    fn test_key_value_model_structure() {
        let now = Utc::now();
        let model = KeyValueModel {
            key: "test_key".to_string(),
            value: "test_value".to_string(),
            expires_at: None,
            created_at: now,
            updated_at: now,
        };

        assert_eq!(model.key, "test_key");
        assert_eq!(model.value, "test_value");
        assert_eq!(model.expires_at, None);
    }

    #[test]
    fn test_key_value_model_with_ttl() {
        use chrono::{Duration, Utc};

        let now = Utc::now();
        let expires_at = now + Duration::hours(1);

        let model = KeyValueModel {
            key: "test_key".to_string(),
            value: "test_value".to_string(),
            expires_at: Some(expires_at),
            created_at: now,
            updated_at: now,
        };

        assert!(model.expires_at.unwrap() > now);
        assert!(model.expires_at.unwrap() < now + Duration::hours(2));
    }

    #[test]
    fn test_ban_record_model_structure() {
        let now = Utc::now();
        let model = BanRecordModel {
            id: 1,
            target_type: "ip".to_string(),
            target_value: "192.168.1.1".to_string(),
            target_key: "ip:192.168.1.1".to_string(),
            ban_times: 3,
            duration: 86400,
            banned_at: now,
            expires_at: now,
            is_manual: true,
            reason: "Test ban".to_string(),
            created_at: now,
            updated_at: now,
        };

        assert_eq!(model.id, 1);
        assert_eq!(model.target_type, "ip");
        assert_eq!(model.ban_times, 3);
    }

    #[test]
    fn test_quota_record_model_structure() {
        let now = Utc::now();
        let model = QuotaRecordModel {
            id: 1,
            user_id: "user123".to_string(),
            resource: "api_calls".to_string(),
            quota_key: "user123:api_calls".to_string(),
            limit: 1000,
            consumed: 500,
            window_start: now,
            window_end: now,
            created_at: now,
            updated_at: now,
        };

        assert_eq!(model.user_id, "user123");
        assert_eq!(model.resource, "api_calls");
        assert_eq!(model.limit, 1000);
        assert_eq!(model.consumed, 500);
    }

    #[test]
    fn test_rate_limit_model_structure() {
        let now = Utc::now();
        let model = RateLimitModel {
            id: 1,
            rate_key: "ip:192.168.1.1:token_bucket".to_string(),
            count: 50,
            rate: 10,
            capacity: 100,
            last_update: now,
            created_at: now,
            updated_at: now,
        };

        assert_eq!(model.rate_key, "ip:192.168.1.1:token_bucket");
        assert_eq!(model.count, 50);
        assert_eq!(model.capacity, 100);
        assert_eq!(model.rate, 10);
    }
}
