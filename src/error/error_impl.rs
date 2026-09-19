// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Error 类型的 impl 块和单元测试
//!
//! 从 `mod.rs` 拆分而来，包含所有错误相关类型的实现逻辑。

use super::*;
use crate::i18n::LocalizedMsg;

// ============================================================================
// 错误双轨（dbnexus error_ext 模式）：Display 保持英文规范串（thiserror），
// 本地化输出经 `i18n::I18nExt::to_localized_string()` 查 FTL 目录。
// `LimiteronError::StorageError` 委托内层 `StorageError` 的键/参数，避免
// "Storage error: Connection error: x" 双层包装。
// ============================================================================

impl LocalizedMsg for StorageError {
    fn message_key(&self) -> &'static str {
        match self {
            StorageError::ConnectionError(_) => "error-storage-connection",
            StorageError::QueryError(_) => "error-storage-query",
            StorageError::TimeoutError(_) => "error-storage-timeout",
            StorageError::NotFound(_) => "error-storage-not-found",
            StorageError::AuthenticationError(_) => "error-storage-authentication",
            StorageError::PermissionError(_) => "error-storage-permission",
            StorageError::InvalidConfig(_) => "error-storage-invalid-config",
            StorageError::RateLimitError(_) => "error-storage-rate-limit",
            StorageError::ValidationError(_) => "error-storage-validation",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            StorageError::ConnectionError(message)
            | StorageError::QueryError(message)
            | StorageError::TimeoutError(message)
            | StorageError::NotFound(message)
            | StorageError::AuthenticationError(message)
            | StorageError::PermissionError(message)
            | StorageError::InvalidConfig(message)
            | StorageError::RateLimitError(message)
            | StorageError::ValidationError(message) => vec![("message", message.clone())],
        }
    }
}

impl LocalizedMsg for LimiteronError {
    fn message_key(&self) -> &'static str {
        match self {
            LimiteronError::ConfigError(_) => "error-config",
            LimiteronError::StorageError(inner) => inner.message_key(),
            LimiteronError::LimitError(_) => "error-limit",
            LimiteronError::BanError(_) => "error-ban",
            LimiteronError::CircuitBreakerError(_) => "error-circuit-breaker",
            LimiteronError::FallbackError(_) => "error-fallback",
            LimiteronError::AuditLogError(_) => "error-audit-log",
            LimiteronError::AuthorizationError(_) => "error-authorization",
            LimiteronError::IoError(_) => "error-io",
            LimiteronError::SerdeError(_) => "error-serde",
            LimiteronError::YamlError(_) => "error-yaml",
            LimiteronError::RateLimitExceeded(_) => "error-rate-limit-exceeded",
            LimiteronError::QuotaExceeded(_) => "error-quota-exceeded",
            LimiteronError::ConcurrencyLimitExceeded(_) => "error-concurrency-limit-exceeded",
            LimiteronError::Throttled(_) => "error-throttled",
            LimiteronError::ValidationError(_) => "error-validation",
            LimiteronError::LockError(_) => "error-lock",
            LimiteronError::TimeError(_) => "error-time",
            LimiteronError::DependencyError(_) => "error-dependency",
            LimiteronError::Other(_) => "error-other",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            LimiteronError::StorageError(inner) => inner.message_args(),
            LimiteronError::ConfigError(message)
            | LimiteronError::LimitError(message)
            | LimiteronError::BanError(message)
            | LimiteronError::CircuitBreakerError(message)
            | LimiteronError::FallbackError(message)
            | LimiteronError::AuditLogError(message)
            | LimiteronError::AuthorizationError(message)
            | LimiteronError::RateLimitExceeded(message)
            | LimiteronError::QuotaExceeded(message)
            | LimiteronError::ConcurrencyLimitExceeded(message)
            | LimiteronError::Throttled(message)
            | LimiteronError::ValidationError(message)
            | LimiteronError::LockError(message)
            | LimiteronError::TimeError(message)
            | LimiteronError::DependencyError(message)
            | LimiteronError::Other(message) => vec![("message", message.clone())],
            LimiteronError::IoError(err) => vec![("message", err.to_string())],
            LimiteronError::SerdeError(err) => vec![("message", err.to_string())],
            LimiteronError::YamlError(err) => vec![("message", err.to_string())],
        }
    }
}

// ============================================================================
// SafeErrorMessage 及其子枚举的错误双轨（T025）：Display 已是英文规范串，
// LocalizedMsg 将各变体映射到 FTL 目录 `safe-*` 键；经
// `i18n::I18nExt::to_localized_string()` 输出本地化文案。
// ============================================================================

impl LocalizedMsg for SafeErrorMessage {
    fn message_key(&self) -> &'static str {
        match self {
            SafeErrorMessage::ConfigError(_) => "safe-error-config",
            SafeErrorMessage::StorageError(_) => "safe-error-storage",
            SafeErrorMessage::LimitError(_) => "safe-error-limit",
            SafeErrorMessage::BanError(_) => "safe-error-ban",
            SafeErrorMessage::ValidationError(_) => "safe-error-validation",
            SafeErrorMessage::General(_) => "safe-error-general",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        // $message 取内层变体经目录渲染的文案（内层均为无参单元变体），
        // 使 zh 渲染为 "存储错误: 记录不存在" 式完整中文。
        let inner_key = match self {
            SafeErrorMessage::ConfigError(inner) => inner.message_key(),
            SafeErrorMessage::StorageError(inner) => inner.message_key(),
            SafeErrorMessage::LimitError(inner) => inner.message_key(),
            SafeErrorMessage::BanError(inner) => inner.message_key(),
            SafeErrorMessage::ValidationError(inner) => inner.message_key(),
            SafeErrorMessage::General(inner) => inner.message_key(),
        };
        vec![("message", crate::i18n::catalog::translate(inner_key, &[]))]
    }
}

impl LocalizedMsg for ConfigSafeError {
    fn message_key(&self) -> &'static str {
        match self {
            ConfigSafeError::InvalidFormat => "config-safe-invalid-format",
            ConfigSafeError::MissingRequiredField => "config-safe-missing-required-field",
            ConfigSafeError::DuplicateRuleId => "config-safe-duplicate-rule-id",
            ConfigSafeError::InvalidStorageType => "config-safe-invalid-storage-type",
            ConfigSafeError::InvalidCacheType => "config-safe-invalid-cache-type",
            ConfigSafeError::InvalidMetricsType => "config-safe-invalid-metrics-type",
            ConfigSafeError::InvalidVersion => "config-safe-invalid-version",
            ConfigSafeError::RuleNotFound => "config-safe-rule-not-found",
            ConfigSafeError::InvalidLimiterConfig => "config-safe-invalid-limiter-config",
            ConfigSafeError::InvalidMatcherConfig => "config-safe-invalid-matcher-config",
            ConfigSafeError::ValueOutOfRange => "config-safe-value-out-of-range",
            ConfigSafeError::MalformedPattern => "config-safe-malformed-pattern",
            ConfigSafeError::SecurityRisk => "config-safe-security-risk",
        }
    }
}

impl LocalizedMsg for StorageSafeError {
    fn message_key(&self) -> &'static str {
        match self {
            StorageSafeError::ConnectionFailed => "storage-safe-connection-failed",
            StorageSafeError::QueryFailed => "storage-safe-query-failed",
            StorageSafeError::Timeout => "storage-safe-timeout",
            StorageSafeError::NotFound => "storage-safe-not-found",
            StorageSafeError::ConcurrentModification => "storage-safe-concurrent-modification",
            StorageSafeError::StorageFull => "storage-safe-storage-full",
            StorageSafeError::InvalidDataFormat => "storage-safe-invalid-data-format",
        }
    }
}

impl LocalizedMsg for LimitSafeError {
    fn message_key(&self) -> &'static str {
        match self {
            LimitSafeError::RateLimitExceeded => "limit-safe-rate-limit-exceeded",
            LimitSafeError::QuotaExceeded => "limit-safe-quota-exceeded",
            LimitSafeError::ConcurrencyLimitExceeded => "limit-safe-concurrency-exceeded",
            LimitSafeError::TokenBucketEmpty => "limit-safe-token-bucket-empty",
            LimitSafeError::WindowFull => "limit-safe-window-full",
            LimitSafeError::TooManyRequests => "limit-safe-too-many-requests",
        }
    }
}

impl LocalizedMsg for BanSafeError {
    fn message_key(&self) -> &'static str {
        match self {
            BanSafeError::UserBanned => "ban-safe-user-banned",
            BanSafeError::IpBanned => "ban-safe-ip-banned",
            BanSafeError::DeviceBanned => "ban-safe-device-banned",
            BanSafeError::RateExceeded => "ban-safe-rate-exceeded",
            BanSafeError::SpamDetected => "ban-safe-spam-detected",
            BanSafeError::SecurityViolation => "ban-safe-security-violation",
        }
    }
}

impl LocalizedMsg for ValidationSafeError {
    fn message_key(&self) -> &'static str {
        match self {
            ValidationSafeError::InvalidInput => "validation-safe-invalid-input",
            ValidationSafeError::MalformedData => "validation-safe-malformed-data",
            ValidationSafeError::SecurityCheckFailed => "validation-safe-security-check-failed",
            ValidationSafeError::InputTooLong => "validation-safe-input-too-long",
            ValidationSafeError::InvalidFormat => "validation-safe-invalid-format",
            ValidationSafeError::SuspiciousPattern => "validation-safe-suspicious-pattern",
        }
    }
}

impl LocalizedMsg for GeneralSafeError {
    fn message_key(&self) -> &'static str {
        match self {
            GeneralSafeError::InternalError => "general-safe-internal-error",
            GeneralSafeError::ServiceUnavailable => "general-safe-service-unavailable",
            GeneralSafeError::InvalidRequest => "general-safe-invalid-request",
            GeneralSafeError::Unauthorized => "general-safe-unauthorized",
            GeneralSafeError::Forbidden => "general-safe-forbidden",
            GeneralSafeError::RateLimited => "general-safe-rate-limited",
        }
    }
}

impl StorageError {
    /// 判断是否为临时错误（可重试）
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            StorageError::TimeoutError(_)
                | StorageError::ConnectionError(_)
                | StorageError::RateLimitError(_)
        )
    }

    /// 判断是否为永久错误（不可重试）
    pub fn is_permanent(&self) -> bool {
        matches!(
            self,
            StorageError::AuthenticationError(_)
                | StorageError::PermissionError(_)
                | StorageError::InvalidConfig(_)
        )
    }
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitState::Closed => write!(f, "closed"),
            CircuitState::Open => write!(f, "open"),
            CircuitState::HalfOpen => write!(f, "half_open"),
        }
    }
}

impl RejectionMetadata {
    /// 创建新的拒绝元数据
    pub fn new(reason: String, retry_after: u64, limit: u64, reset_at: u64) -> Self {
        Self {
            reason,
            retry_after,
            limit,
            reset_at,
        }
    }
}

impl Decision {
    /// 创建默认的允许决策（向后兼容）
    pub fn allowed_default() -> Self {
        Decision::Allowed(RateLimitMetadata::default())
    }

    /// 创建允许决策
    pub fn allowed(metadata: RateLimitMetadata) -> Self {
        Decision::Allowed(metadata)
    }

    /// 创建拒绝决策
    pub fn rejected(metadata: RejectionMetadata) -> Self {
        Decision::Rejected(metadata)
    }

    /// 获取限流元数据（如果有）
    pub fn rate_limit_metadata(&self) -> Option<RateLimitMetadata> {
        match self {
            Decision::Allowed(metadata) => Some(metadata.clone()),
            Decision::Rejected(metadata) => {
                // 对于拒绝情况，我们也返回元数据信息
                Some(RateLimitMetadata {
                    limit: metadata.limit,
                    remaining: 0,
                    reset_at: metadata.reset_at,
                    retry_after: Some(metadata.retry_after),
                    policy: String::new(),
                })
            }
            Decision::Banned(_) => None,
        }
    }
}

impl BanInfo {
    /// 创建新的封禁信息
    pub fn new(
        reason: String,
        banned_until: chrono::DateTime<chrono::Utc>,
        ban_times: u32,
    ) -> Self {
        Self {
            reason,
            banned_until,
            ban_times,
        }
    }

    /// 获取封禁原因
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// 获取封禁到期时间
    pub fn banned_until(&self) -> chrono::DateTime<chrono::Utc> {
        self.banned_until
    }

    /// 获取封禁次数
    pub fn ban_times(&self) -> u32 {
        self.ban_times
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_message() {
        let error = LimiteronError::ConfigError("测试错误".to_string());
        assert_eq!(error.to_string(), "Configuration error: 测试错误");
    }

    /// 错误双轨：Display 恒英文规范串；to_localized_string 随 locale，
    /// message_en 恒英文。单测试内顺序完成全部 locale 断言（全局 override
    /// 态在并行测试下存在竞态，合并为单用例消除交叉污染）。
    #[test]
    fn test_localized_dual_track() {
        use crate::i18n::{I18nExt, clear_locale_override, set_locale};

        let err = LimiteronError::StorageError(StorageError::NotFound("k".into()));
        // Display 恒英文（规范轨；包装变体带前缀，内层经 #[from] 嵌套 Display）
        assert_eq!(err.to_string(), "Storage error: Not found: k");
        // message_en 恒英文（经目录，与 Display 对齐）
        assert_eq!(err.message_en(), "Not found: k");

        clear_locale_override();
        set_locale("en").expect("en is valid");
        assert_eq!(err.to_localized_string(), "Not found: k");

        set_locale("zh-CN").expect("zh-CN is valid");
        assert_eq!(err.to_localized_string(), "未找到: k");

        // 委托：包装变体直接落到内层 StorageError 的键
        let wrapped = LimiteronError::StorageError(StorageError::QueryError("q".into()));
        assert_eq!(wrapped.message_en(), "Query error: q");

        clear_locale_override();
    }

    #[test]
    fn test_storage_error_conversion() {
        let storage_error = StorageError::NotFound("test_key".to_string());
        let flowguard_error: LimiteronError = storage_error.into();
        assert!(matches!(flowguard_error, LimiteronError::StorageError(_)));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let flowguard_error: LimiteronError = io_error.into();
        assert!(matches!(flowguard_error, LimiteronError::IoError(_)));
    }

    #[test]
    fn test_decision_allowed() {
        let metadata = RateLimitMetadata {
            limit: 100,
            remaining: 99,
            reset_at: 1234567890,
            retry_after: None,
            policy: "token_bucket".to_string(),
        };
        let decision = Decision::Allowed(metadata.clone());
        assert_eq!(decision, Decision::Allowed(metadata));
        assert!(matches!(decision, Decision::Allowed(_)));
    }

    #[test]
    fn test_decision_rejected() {
        let metadata =
            RejectionMetadata::new("rate limit exceeded".to_string(), 60, 100, 1234567890);
        let decision = Decision::Rejected(metadata.clone());
        assert!(matches!(decision, Decision::Rejected(_)));
        assert_eq!(decision.rate_limit_metadata().unwrap().remaining, 0);
    }

    #[test]
    fn test_decision_banned() {
        let info = BanInfo::new("spam".to_string(), chrono::Utc::now(), 3);
        let decision = Decision::Banned(info);
        assert!(matches!(decision, Decision::Banned(_)));
        assert!(decision.rate_limit_metadata().is_none());
    }

    #[test]
    fn test_decision_allowed_default() {
        let decision = Decision::allowed_default();
        assert!(matches!(decision, Decision::Allowed(_)));
    }

    #[test]
    fn test_rate_limit_metadata_default() {
        let metadata = RateLimitMetadata::default();
        assert_eq!(metadata.limit, 0);
        assert_eq!(metadata.remaining, 0);
        assert_eq!(metadata.reset_at, 0);
        assert!(metadata.retry_after.is_none());
        assert!(metadata.policy.is_empty());
    }

    #[test]
    fn test_rejection_metadata() {
        let metadata = RejectionMetadata::new("test".to_string(), 30, 50, 1234567890);
        assert_eq!(metadata.reason, "test");
        assert_eq!(metadata.retry_after, 30);
        assert_eq!(metadata.limit, 50);
        assert_eq!(metadata.reset_at, 1234567890);
    }

    #[test]
    fn test_ban_info_equality() {
        let now = chrono::Utc::now();
        let info1 = BanInfo::new("test".to_string(), now, 1);
        let info2 = BanInfo::new("test".to_string(), now, 1);
        assert_eq!(info1, info2);
    }

    #[test]
    fn test_ban_info_accessors() {
        let until = chrono::Utc::now() + chrono::Duration::hours(1);
        let info = BanInfo::new("spam".to_string(), until, 5);
        assert_eq!(info.reason(), "spam");
        assert_eq!(info.banned_until(), until);
        assert_eq!(info.ban_times(), 5);
    }

    #[test]
    fn test_storage_error_is_transient() {
        assert!(StorageError::TimeoutError("t".into()).is_transient());
        assert!(StorageError::ConnectionError("c".into()).is_transient());
        assert!(StorageError::RateLimitError("r".into()).is_transient());
        assert!(!StorageError::NotFound("n".into()).is_transient());
        assert!(!StorageError::QueryError("q".into()).is_transient());
        assert!(!StorageError::AuthenticationError("a".into()).is_transient());
        assert!(!StorageError::PermissionError("p".into()).is_transient());
        assert!(!StorageError::InvalidConfig("i".into()).is_transient());
        assert!(!StorageError::ValidationError("v".into()).is_transient());
    }

    #[test]
    fn test_storage_error_is_permanent() {
        assert!(StorageError::AuthenticationError("a".into()).is_permanent());
        assert!(StorageError::PermissionError("p".into()).is_permanent());
        assert!(StorageError::InvalidConfig("i".into()).is_permanent());
        assert!(!StorageError::TimeoutError("t".into()).is_permanent());
        assert!(!StorageError::ConnectionError("c".into()).is_permanent());
        assert!(!StorageError::NotFound("n".into()).is_permanent());
        assert!(!StorageError::QueryError("q".into()).is_permanent());
        assert!(!StorageError::RateLimitError("r".into()).is_permanent());
        assert!(!StorageError::ValidationError("v".into()).is_permanent());
    }

    #[test]
    fn test_circuit_state_display() {
        assert_eq!(format!("{}", CircuitState::Closed), "closed");
        assert_eq!(format!("{}", CircuitState::Open), "open");
        assert_eq!(format!("{}", CircuitState::HalfOpen), "half_open");
    }

    #[test]
    fn test_decision_allowed_constructor() {
        let metadata = RateLimitMetadata {
            limit: 200,
            remaining: 150,
            reset_at: 999,
            retry_after: None,
            policy: "sliding".to_string(),
        };
        let decision = Decision::allowed(metadata.clone());
        let retrieved = decision
            .rate_limit_metadata()
            .expect("should have metadata");
        assert_eq!(retrieved.limit, 200);
        assert_eq!(retrieved.remaining, 150);
        assert_eq!(retrieved.reset_at, 999);
        assert_eq!(retrieved.policy, "sliding");
    }

    #[test]
    fn test_decision_rejected_constructor() {
        let metadata = RejectionMetadata::new("too many".to_string(), 30, 100, 12345);
        let decision = Decision::rejected(metadata);
        let retrieved = decision
            .rate_limit_metadata()
            .expect("should have metadata");
        assert_eq!(retrieved.limit, 100);
        assert_eq!(retrieved.remaining, 0);
        assert_eq!(retrieved.reset_at, 12345);
        assert_eq!(retrieved.retry_after, Some(30));
    }

    #[test]
    fn test_consume_result_construction() {
        let result = ConsumeResult {
            allowed: true,
            remaining: 50,
            alert_triggered: true,
            usage_percent: 50.0,
        };
        assert!(result.allowed);
        assert_eq!(result.remaining, 50);
        assert!(result.alert_triggered);
        assert!((result.usage_percent - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_flowguard_error_variants_display() {
        assert_eq!(
            LimiteronError::LimitError("x".into()).to_string(),
            "Rate limit error: x"
        );
        assert_eq!(
            LimiteronError::BanError("x".into()).to_string(),
            "Ban error: x"
        );
        assert_eq!(
            LimiteronError::CircuitBreakerError("x".into()).to_string(),
            "Circuit breaker error: x"
        );
        assert_eq!(
            LimiteronError::FallbackError("x".into()).to_string(),
            "Fallback error: x"
        );
        assert_eq!(
            LimiteronError::AuditLogError("x".into()).to_string(),
            "Audit log error: x"
        );
        assert_eq!(
            LimiteronError::AuthorizationError("x".into()).to_string(),
            "Authorization error: x"
        );
        assert_eq!(
            LimiteronError::RateLimitExceeded("x".into()).to_string(),
            "Rate limit exceeded: x"
        );
        assert_eq!(
            LimiteronError::QuotaExceeded("x".into()).to_string(),
            "Quota exceeded: x"
        );
        assert_eq!(
            LimiteronError::ConcurrencyLimitExceeded("x".into()).to_string(),
            "Concurrency limit exceeded: x"
        );
        assert_eq!(
            LimiteronError::ValidationError("x".into()).to_string(),
            "Validation error: x"
        );
        assert_eq!(
            LimiteronError::LockError("x".into()).to_string(),
            "Lock acquisition error: x"
        );
        assert_eq!(
            LimiteronError::TimeError("x".into()).to_string(),
            "Time error: x"
        );
        assert_eq!(
            LimiteronError::DependencyError("x".into()).to_string(),
            "Missing dependency: x"
        );
        assert_eq!(
            LimiteronError::Other("x".into()).to_string(),
            "Unknown error: x"
        );
    }

    #[test]
    fn test_storage_error_display() {
        assert_eq!(
            StorageError::ConnectionError("c".into()).to_string(),
            "Connection error: c"
        );
        assert_eq!(
            StorageError::QueryError("q".into()).to_string(),
            "Query error: q"
        );
        assert_eq!(
            StorageError::TimeoutError("t".into()).to_string(),
            "Timeout error: t"
        );
        assert_eq!(
            StorageError::NotFound("n".into()).to_string(),
            "Not found: n"
        );
        assert_eq!(
            StorageError::AuthenticationError("a".into()).to_string(),
            "Authentication error: a"
        );
        assert_eq!(
            StorageError::PermissionError("p".into()).to_string(),
            "Permission error: p"
        );
        assert_eq!(
            StorageError::InvalidConfig("i".into()).to_string(),
            "Invalid configuration: i"
        );
        assert_eq!(
            StorageError::RateLimitError("r".into()).to_string(),
            "Rate limit: r"
        );
        assert_eq!(
            StorageError::ValidationError("v".into()).to_string(),
            "Validation error: v"
        );
    }

    #[test]
    fn test_serde_error_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("{invalid}");
        let err: serde_json::Error = json_err.unwrap_err();
        let fg_err: LimiteronError = err.into();
        assert!(matches!(fg_err, LimiteronError::SerdeError(_)));
    }

    #[test]
    fn test_storage_error_from_into_flowguard() {
        let se = StorageError::QueryError("q".into());
        let fg: LimiteronError = se.into();
        assert!(matches!(fg, LimiteronError::StorageError(_)));
        let back = match fg {
            LimiteronError::StorageError(s) => s,
            _ => unreachable!(),
        };
        assert!(matches!(back, StorageError::QueryError(_)));
    }
}
