//! 审计日志模块集成测试
//!
//! 测试审计日志模块的完整功能（需要 audit-log 特性）

#[cfg(feature = "audit-log")]
mod tests {
    use chrono::Utc;
    use limiteron::logging::{AuditEvent, AuditLogEntry, AuditLogStats};

    // ============================================================================
    // AuditEvent Tests
    // ============================================================================

    #[tokio::test]
    async fn test_audit_event_decision() {
        let now = Utc::now();
        let event = AuditEvent::Decision {
            timestamp: now,
            identifier: "192.168.1.1".to_string(),
            decision: "allowed".to_string(),
            reason: "within_limit".to_string(),
            request_id: Some("req-123".to_string()),
        };
        assert_eq!(event.timestamp(), now);
        assert_eq!(event.operation(), "decision");
        assert_eq!(event.target(), "192.168.1.1");
        assert_eq!(event.result(), "allowed");
    }

    #[tokio::test]
    async fn test_audit_event_config_change() {
        let now = Utc::now();
        let event = AuditEvent::ConfigChange {
            timestamp: now,
            old_version: "v1".to_string(),
            new_version: "v2".to_string(),
            changes: vec!["rate_limit: 100->200".to_string()],
            operator: Some("admin".to_string()),
        };
        assert_eq!(event.operation(), "config_change");
        assert_eq!(event.target(), "v1->v2");
    }

    #[tokio::test]
    async fn test_audit_event_ban_operation() {
        let now = Utc::now();
        let event = AuditEvent::BanOperation {
            timestamp: now,
            target: "user-123".to_string(),
            action: "ban".to_string(),
            reason: "abuse".to_string(),
            operator: "admin".to_string(),
            expires_at: Some(now),
        };
        assert_eq!(event.operation(), "ban_operation");
        assert_eq!(event.target(), "user-123");
        assert_eq!(event.result(), "ban");
    }

    #[tokio::test]
    async fn test_audit_event_system_event() {
        let now = Utc::now();
        let event = AuditEvent::SystemEvent {
            timestamp: now,
            level: "info".to_string(),
            name: "startup".to_string(),
            details: "system started".to_string(),
        };
        assert_eq!(event.operation(), "system_event");
        assert_eq!(event.target(), "startup");
    }

    #[tokio::test]
    async fn test_audit_event_error_event() {
        let now = Utc::now();
        let event = AuditEvent::ErrorEvent {
            timestamp: now,
            error_type: "RateLimitError".to_string(),
            message: "rate limit exceeded".to_string(),
            stack_trace: None,
        };
        assert_eq!(event.operation(), "error_event");
        assert_eq!(event.target(), "RateLimitError");
    }

    // ============================================================================
    // AuditLogEntry Tests
    // ============================================================================

    #[tokio::test]
    async fn test_audit_log_entry_new() {
        let now = Utc::now();
        let event = AuditEvent::Decision {
            timestamp: now,
            identifier: "10.0.0.1".to_string(),
            decision: "allowed".to_string(),
            reason: "ok".to_string(),
            request_id: None,
        };
        let entry = AuditLogEntry::new(event);
        assert!(entry.signature.is_none());
        assert!(entry.signature_version.is_none());
    }

    #[tokio::test]
    async fn test_audit_log_entry_with_signature() {
        let now = Utc::now();
        let event = AuditEvent::Decision {
            timestamp: now,
            identifier: "10.0.0.1".to_string(),
            decision: "rejected".to_string(),
            reason: "over_limit".to_string(),
            request_id: None,
        };
        let entry = AuditLogEntry::with_signature(event, "secret-key-123");
        assert!(entry.signature.is_some());
        assert_eq!(entry.signature_version, Some(1));
    }

    #[tokio::test]
    async fn test_audit_log_entry_sign() {
        let now = Utc::now();
        let event = AuditEvent::BanOperation {
            timestamp: now,
            target: "192.168.1.1".to_string(),
            action: "ban".to_string(),
            reason: "spam".to_string(),
            operator: "admin".to_string(),
            expires_at: None,
        };
        let mut entry = AuditLogEntry::new(event);
        entry.sign("my-signing-key");

        assert!(entry.signature.is_some());
        let sig = entry.signature.as_ref().unwrap();
        // HMAC-SHA256 hex output is 64 characters
        assert_eq!(sig.len(), 64);
    }

    #[tokio::test]
    async fn test_audit_log_entry_verify_valid() {
        let now = Utc::now();
        let event = AuditEvent::Decision {
            timestamp: now,
            identifier: "10.0.0.1".to_string(),
            decision: "allowed".to_string(),
            reason: "ok".to_string(),
            request_id: None,
        };
        let key = "test-secret-key";
        let entry = AuditLogEntry::with_signature(event, key);
        assert!(entry.verify(key).is_ok());
    }

    #[tokio::test]
    async fn test_audit_log_entry_verify_invalid_key() {
        let now = Utc::now();
        let event = AuditEvent::Decision {
            timestamp: now,
            identifier: "10.0.0.1".to_string(),
            decision: "allowed".to_string(),
            reason: "ok".to_string(),
            request_id: None,
        };
        let entry = AuditLogEntry::with_signature(event, "correct-key");
        let result = entry.verify("wrong-key");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_audit_log_entry_verify_no_signature() {
        let now = Utc::now();
        let event = AuditEvent::Decision {
            timestamp: now,
            identifier: "10.0.0.1".to_string(),
            decision: "allowed".to_string(),
            reason: "ok".to_string(),
            request_id: None,
        };
        let entry = AuditLogEntry::new(event);
        let result = entry.verify("any-key");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_audit_log_entry_signature_deterministic() {
        let now = Utc::now();
        let event = AuditEvent::Decision {
            timestamp: now,
            identifier: "10.0.0.1".to_string(),
            decision: "allowed".to_string(),
            reason: "ok".to_string(),
            request_id: None,
        };
        let entry1 = AuditLogEntry::with_signature(event.clone(), "key");
        let entry2 = AuditLogEntry::with_signature(event, "key");
        // Same event + same key = same signature
        assert_eq!(entry1.signature, entry2.signature);
    }

    #[tokio::test]
    async fn test_audit_log_entry_different_events_different_signatures() {
        let now = Utc::now();
        let event1 = AuditEvent::Decision {
            timestamp: now,
            identifier: "10.0.0.1".to_string(),
            decision: "allowed".to_string(),
            reason: "ok".to_string(),
            request_id: None,
        };
        let event2 = AuditEvent::Decision {
            timestamp: now,
            identifier: "10.0.0.2".to_string(),
            decision: "allowed".to_string(),
            reason: "ok".to_string(),
            request_id: None,
        };
        let entry1 = AuditLogEntry::with_signature(event1, "key");
        let entry2 = AuditLogEntry::with_signature(event2, "key");
        assert_ne!(entry1.signature, entry2.signature);
    }

    // ============================================================================
    // AuditLogStats Tests
    // ============================================================================

    #[test]
    fn test_audit_log_stats_default() {
        let stats = AuditLogStats::default();
        assert_eq!(stats.total_events(), 0);
        assert_eq!(stats.decision_events(), 0);
        assert_eq!(stats.config_change_events(), 0);
        assert_eq!(stats.ban_operation_events(), 0);
        assert_eq!(stats.system_events(), 0);
        assert_eq!(stats.error_events(), 0);
    }
}
