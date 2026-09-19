// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 错误消息抽象模块
//!
//! 提供安全的错误消息生成，防止内部结构泄露。
//! 所有对外暴露的错误消息都经过脱敏处理。

/// 安全的错误消息类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafeErrorMessage {
    /// 配置错误
    ConfigError(ConfigSafeError),
    /// 存储错误
    StorageError(StorageSafeError),
    /// 限流错误
    LimitError(LimitSafeError),
    /// 封禁错误
    BanError(BanSafeError),
    /// 验证错误
    ValidationError(ValidationSafeError),
    /// 通用错误
    General(GeneralSafeError),
}

/// 配置安全错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSafeError {
    InvalidFormat,
    MissingRequiredField,
    DuplicateRuleId,
    InvalidStorageType,
    InvalidCacheType,
    InvalidMetricsType,
    InvalidVersion,
    RuleNotFound,
    InvalidLimiterConfig,
    InvalidMatcherConfig,
    ValueOutOfRange,
    MalformedPattern,
    SecurityRisk,
}

/// 存储安全错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageSafeError {
    ConnectionFailed,
    QueryFailed,
    Timeout,
    NotFound,
    ConcurrentModification,
    StorageFull,
    InvalidDataFormat,
}

/// 限流安全错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimitSafeError {
    RateLimitExceeded,
    QuotaExceeded,
    ConcurrencyLimitExceeded,
    TokenBucketEmpty,
    WindowFull,
    TooManyRequests,
}

/// 封禁安全错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BanSafeError {
    UserBanned,
    IpBanned,
    DeviceBanned,
    RateExceeded,
    SpamDetected,
    SecurityViolation,
}

/// 验证安全错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationSafeError {
    InvalidInput,
    MalformedData,
    SecurityCheckFailed,
    InputTooLong,
    InvalidFormat,
    SuspiciousPattern,
}

/// 通用安全错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneralSafeError {
    InternalError,
    ServiceUnavailable,
    InvalidRequest,
    Unauthorized,
    Forbidden,
    RateLimited,
}

impl std::fmt::Display for SafeErrorMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SafeErrorMessage::ConfigError(e) => write!(f, "Configuration error: {}", e),
            SafeErrorMessage::StorageError(e) => write!(f, "Storage error: {}", e),
            SafeErrorMessage::LimitError(e) => write!(f, "Rate limit error: {}", e),
            SafeErrorMessage::BanError(e) => write!(f, "Ban error: {}", e),
            SafeErrorMessage::ValidationError(e) => write!(f, "Validation error: {}", e),
            SafeErrorMessage::General(e) => write!(f, "Error: {}", e),
        }
    }
}

impl std::fmt::Display for ConfigSafeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigSafeError::InvalidFormat => write!(f, "Invalid configuration format"),
            ConfigSafeError::MissingRequiredField => write!(f, "Missing required field"),
            ConfigSafeError::DuplicateRuleId => write!(f, "Duplicate rule ID"),
            ConfigSafeError::InvalidStorageType => write!(f, "Invalid storage type"),
            ConfigSafeError::InvalidCacheType => write!(f, "Invalid cache type"),
            ConfigSafeError::InvalidMetricsType => write!(f, "Invalid metrics type"),
            ConfigSafeError::InvalidVersion => write!(f, "Invalid version"),
            ConfigSafeError::RuleNotFound => write!(f, "Rule not found"),
            ConfigSafeError::InvalidLimiterConfig => write!(f, "Invalid limiter configuration"),
            ConfigSafeError::InvalidMatcherConfig => write!(f, "Invalid matcher configuration"),
            ConfigSafeError::ValueOutOfRange => write!(f, "Value out of allowed range"),
            ConfigSafeError::MalformedPattern => write!(f, "Malformed pattern"),
            ConfigSafeError::SecurityRisk => write!(f, "Security risk detected"),
        }
    }
}

impl std::fmt::Display for StorageSafeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageSafeError::ConnectionFailed => write!(f, "Connection failed"),
            StorageSafeError::QueryFailed => write!(f, "Query failed"),
            StorageSafeError::Timeout => write!(f, "Operation timed out"),
            StorageSafeError::NotFound => write!(f, "Record not found"),
            StorageSafeError::ConcurrentModification => {
                write!(f, "Data was concurrently modified")
            }
            StorageSafeError::StorageFull => write!(f, "Storage full"),
            StorageSafeError::InvalidDataFormat => write!(f, "Invalid data format"),
        }
    }
}

impl std::fmt::Display for LimitSafeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LimitSafeError::RateLimitExceeded => write!(f, "Request rate exceeded"),
            LimitSafeError::QuotaExceeded => write!(f, "Quota exhausted"),
            LimitSafeError::ConcurrencyLimitExceeded => write!(f, "Concurrency limit exceeded"),
            LimitSafeError::TokenBucketEmpty => write!(f, "Tokens exhausted"),
            LimitSafeError::WindowFull => write!(f, "Time window is full"),
            LimitSafeError::TooManyRequests => write!(f, "Too many requests"),
        }
    }
}

impl std::fmt::Display for BanSafeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BanSafeError::UserBanned => write!(f, "User is banned"),
            BanSafeError::IpBanned => write!(f, "IP address is banned"),
            BanSafeError::DeviceBanned => write!(f, "Device is banned"),
            BanSafeError::RateExceeded => write!(f, "Request rate exceeded"),
            BanSafeError::SpamDetected => write!(f, "Suspicious behavior detected"),
            BanSafeError::SecurityViolation => write!(f, "Security check failed"),
        }
    }
}

impl std::fmt::Display for ValidationSafeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationSafeError::InvalidInput => write!(f, "Invalid input"),
            ValidationSafeError::MalformedData => write!(f, "Malformed data"),
            ValidationSafeError::SecurityCheckFailed => write!(f, "Security check failed"),
            ValidationSafeError::InputTooLong => write!(f, "Input too long"),
            ValidationSafeError::InvalidFormat => write!(f, "Invalid format"),
            ValidationSafeError::SuspiciousPattern => write!(f, "Suspicious pattern detected"),
        }
    }
}

impl std::fmt::Display for GeneralSafeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeneralSafeError::InternalError => write!(f, "Internal error"),
            GeneralSafeError::ServiceUnavailable => write!(f, "Service unavailable"),
            GeneralSafeError::InvalidRequest => write!(f, "Invalid request"),
            GeneralSafeError::Unauthorized => write!(f, "Unauthorized"),
            GeneralSafeError::Forbidden => write!(f, "Forbidden"),
            GeneralSafeError::RateLimited => write!(f, "Rate limited"),
        }
    }
}

impl std::error::Error for SafeErrorMessage {}
impl std::error::Error for ConfigSafeError {}
impl std::error::Error for StorageSafeError {}
impl std::error::Error for LimitSafeError {}
impl std::error::Error for BanSafeError {}
impl std::error::Error for ValidationSafeError {}
impl std::error::Error for GeneralSafeError {}

/// 错误消息抽象器
pub struct ErrorMessageAbstraction;

impl ErrorMessageAbstraction {
    /// 从详细错误生成安全错误消息
    pub fn abstract_storage_error(detailed_error: &str) -> SafeErrorMessage {
        let lower_error = detailed_error.to_lowercase();

        if lower_error.contains("connection") || lower_error.contains("connect") {
            SafeErrorMessage::StorageError(StorageSafeError::ConnectionFailed)
        } else if lower_error.contains("timeout") || lower_error.contains("timed out") {
            SafeErrorMessage::StorageError(StorageSafeError::Timeout)
        } else if lower_error.contains("not found") {
            SafeErrorMessage::StorageError(StorageSafeError::NotFound)
        } else if lower_error.contains("duplicate") {
            SafeErrorMessage::StorageError(StorageSafeError::ConcurrentModification)
        } else {
            SafeErrorMessage::StorageError(StorageSafeError::QueryFailed)
        }
    }

    /// 从详细配置错误生成安全错误消息
    pub fn abstract_config_error(detailed_error: &str) -> SafeErrorMessage {
        let lower_error = detailed_error.to_lowercase();

        if lower_error.contains("duplicate") {
            SafeErrorMessage::ConfigError(ConfigSafeError::DuplicateRuleId)
        } else if lower_error.contains("storage") && lower_error.contains("invalid") {
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidStorageType)
        } else if lower_error.contains("cache") && lower_error.contains("invalid") {
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidCacheType)
        } else if lower_error.contains("version") {
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidVersion)
        } else if lower_error.contains("missing") || lower_error.contains("empty") {
            SafeErrorMessage::ConfigError(ConfigSafeError::MissingRequiredField)
        } else if lower_error.contains("format") || lower_error.contains("parse") {
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidFormat)
        } else if lower_error.contains("limiter") {
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidLimiterConfig)
        } else if lower_error.contains("matcher") {
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidMatcherConfig)
        } else if lower_error.contains("range") || lower_error.contains("out of") {
            SafeErrorMessage::ConfigError(ConfigSafeError::ValueOutOfRange)
        } else if lower_error.contains("<script") || lower_error.contains("injection") {
            SafeErrorMessage::ConfigError(ConfigSafeError::SecurityRisk)
        } else {
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidFormat)
        }
    }

    /// 从详细限流错误生成安全错误消息
    pub fn abstract_limit_error(detailed_error: &str) -> SafeErrorMessage {
        let lower_error = detailed_error.to_lowercase();

        if lower_error.contains("rate") || lower_error.contains("rate limit") {
            SafeErrorMessage::LimitError(LimitSafeError::RateLimitExceeded)
        } else if lower_error.contains("quota") {
            SafeErrorMessage::LimitError(LimitSafeError::QuotaExceeded)
        } else if lower_error.contains("concurrency") || lower_error.contains("concurrent") {
            SafeErrorMessage::LimitError(LimitSafeError::ConcurrencyLimitExceeded)
        } else if lower_error.contains("token") {
            SafeErrorMessage::LimitError(LimitSafeError::TokenBucketEmpty)
        } else if lower_error.contains("window") {
            SafeErrorMessage::LimitError(LimitSafeError::WindowFull)
        } else {
            SafeErrorMessage::LimitError(LimitSafeError::TooManyRequests)
        }
    }

    /// 从详细验证错误生成安全错误消息
    pub fn abstract_validation_error(detailed_error: &str) -> SafeErrorMessage {
        let lower_error = detailed_error.to_lowercase();

        if lower_error.contains("empty") || lower_error.contains("null") {
            SafeErrorMessage::ValidationError(ValidationSafeError::InvalidInput)
        } else if lower_error.contains("length") || lower_error.contains("too long") {
            SafeErrorMessage::ValidationError(ValidationSafeError::InputTooLong)
        } else if lower_error.contains("format") || lower_error.contains("parse") {
            SafeErrorMessage::ValidationError(ValidationSafeError::InvalidFormat)
        } else if lower_error.contains("<script")
            || lower_error.contains("sql")
            || lower_error.contains("injection")
        {
            SafeErrorMessage::ValidationError(ValidationSafeError::SuspiciousPattern)
        } else if lower_error.contains("security") {
            SafeErrorMessage::ValidationError(ValidationSafeError::SecurityCheckFailed)
        } else {
            SafeErrorMessage::ValidationError(ValidationSafeError::MalformedData)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_error_abstraction() {
        let err1 = ErrorMessageAbstraction::abstract_storage_error("Connection refused");
        assert_eq!(
            err1,
            SafeErrorMessage::StorageError(StorageSafeError::ConnectionFailed)
        );

        let err2 = ErrorMessageAbstraction::abstract_storage_error("Query timeout");
        assert_eq!(
            err2,
            SafeErrorMessage::StorageError(StorageSafeError::Timeout)
        );

        let err3 = ErrorMessageAbstraction::abstract_storage_error("Key not found");
        assert_eq!(
            err3,
            SafeErrorMessage::StorageError(StorageSafeError::NotFound)
        );
    }

    #[test]
    fn test_config_error_abstraction() {
        let err1 = ErrorMessageAbstraction::abstract_config_error("Duplicate rule ID: test");
        assert_eq!(
            err1,
            SafeErrorMessage::ConfigError(ConfigSafeError::DuplicateRuleId)
        );

        let err2 = ErrorMessageAbstraction::abstract_config_error("Invalid storage type: mysql");
        assert_eq!(
            err2,
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidStorageType)
        );

        let err3 = ErrorMessageAbstraction::abstract_config_error("Version is empty");
        assert_eq!(
            err3,
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidVersion)
        );
    }

    #[test]
    fn test_limit_error_abstraction() {
        let err1 = ErrorMessageAbstraction::abstract_limit_error("Rate limit exceeded");
        assert_eq!(
            err1,
            SafeErrorMessage::LimitError(LimitSafeError::RateLimitExceeded)
        );

        let err2 = ErrorMessageAbstraction::abstract_limit_error("Quota exceeded for user");
        assert_eq!(
            err2,
            SafeErrorMessage::LimitError(LimitSafeError::QuotaExceeded)
        );

        let err3 = ErrorMessageAbstraction::abstract_limit_error("Too many concurrent requests");
        assert_eq!(
            err3,
            SafeErrorMessage::LimitError(LimitSafeError::ConcurrencyLimitExceeded)
        );
    }

    #[test]
    fn test_validation_error_abstraction() {
        let err1 = ErrorMessageAbstraction::abstract_validation_error("Input is empty");
        assert_eq!(
            err1,
            SafeErrorMessage::ValidationError(ValidationSafeError::InvalidInput)
        );

        let err2 = ErrorMessageAbstraction::abstract_validation_error("Input too long: 1000 chars");
        assert_eq!(
            err2,
            SafeErrorMessage::ValidationError(ValidationSafeError::InputTooLong)
        );

        let err3 =
            ErrorMessageAbstraction::abstract_validation_error("Detected SQL injection pattern");
        assert_eq!(
            err3,
            SafeErrorMessage::ValidationError(ValidationSafeError::SuspiciousPattern)
        );
    }

    #[test]
    fn test_safe_error_display() {
        let err = SafeErrorMessage::ConfigError(ConfigSafeError::DuplicateRuleId);
        assert_eq!(err.to_string(), "Configuration error: Duplicate rule ID");

        let err = SafeErrorMessage::StorageError(StorageSafeError::ConnectionFailed);
        assert_eq!(err.to_string(), "Storage error: Connection failed");

        let err = SafeErrorMessage::LimitError(LimitSafeError::RateLimitExceeded);
        assert_eq!(err.to_string(), "Rate limit error: Request rate exceeded");
    }

    #[test]
    fn test_storage_error_abstraction_all_branches() {
        let err = ErrorMessageAbstraction::abstract_storage_error("Duplicate key error");
        assert_eq!(
            err,
            SafeErrorMessage::StorageError(StorageSafeError::ConcurrentModification)
        );

        let err = ErrorMessageAbstraction::abstract_storage_error("Unknown database error");
        assert_eq!(
            err,
            SafeErrorMessage::StorageError(StorageSafeError::QueryFailed)
        );
    }

    #[test]
    fn test_storage_error_abstraction_edge_cases() {
        let err = ErrorMessageAbstraction::abstract_storage_error("");
        assert_eq!(
            err,
            SafeErrorMessage::StorageError(StorageSafeError::QueryFailed)
        );

        let err = ErrorMessageAbstraction::abstract_storage_error("timed out");
        assert_eq!(
            err,
            SafeErrorMessage::StorageError(StorageSafeError::Timeout)
        );
    }

    #[test]
    fn test_config_error_abstraction_all_branches() {
        let err = ErrorMessageAbstraction::abstract_config_error("Invalid cache type");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidCacheType)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("Missing required field: name");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::MissingRequiredField)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("Invalid format for field");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidFormat)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("Invalid limiter configuration");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidLimiterConfig)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("Invalid matcher config");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidMatcherConfig)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("Value out of range");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::ValueOutOfRange)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("<script>alert('xss')</script>");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::SecurityRisk)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("Duplicate rule ID");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::DuplicateRuleId)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("Invalid storage type");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidStorageType)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("Invalid cache type");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidCacheType)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("Invalid version");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidVersion)
        );
    }

    #[test]
    fn test_config_error_abstraction_edge_cases() {
        let err = ErrorMessageAbstraction::abstract_config_error("");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidFormat)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("injection attempt detected");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::SecurityRisk)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("parse error");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidFormat)
        );
    }

    #[test]
    fn test_limit_error_abstraction_all_branches() {
        let err = ErrorMessageAbstraction::abstract_limit_error("Request rate exceeded");
        assert_eq!(
            err,
            SafeErrorMessage::LimitError(LimitSafeError::RateLimitExceeded)
        );

        let err = ErrorMessageAbstraction::abstract_limit_error("Quota exhausted for user");
        assert_eq!(
            err,
            SafeErrorMessage::LimitError(LimitSafeError::QuotaExceeded)
        );

        let err = ErrorMessageAbstraction::abstract_limit_error("concurrent request limit");
        assert_eq!(
            err,
            SafeErrorMessage::LimitError(LimitSafeError::ConcurrencyLimitExceeded)
        );

        let err = ErrorMessageAbstraction::abstract_limit_error("No tokens available");
        assert_eq!(
            err,
            SafeErrorMessage::LimitError(LimitSafeError::TokenBucketEmpty)
        );

        let err = ErrorMessageAbstraction::abstract_limit_error("Window is full");
        assert_eq!(
            err,
            SafeErrorMessage::LimitError(LimitSafeError::WindowFull)
        );
    }

    #[test]
    fn test_limit_error_abstraction_edge_cases() {
        let err = ErrorMessageAbstraction::abstract_limit_error("");
        assert_eq!(
            err,
            SafeErrorMessage::LimitError(LimitSafeError::TooManyRequests)
        );

        let err = ErrorMessageAbstraction::abstract_limit_error("rate concurrent");
        assert_eq!(
            err,
            SafeErrorMessage::LimitError(LimitSafeError::RateLimitExceeded)
        );

        let err = ErrorMessageAbstraction::abstract_limit_error("Some unknown limit error");
        assert_eq!(
            err,
            SafeErrorMessage::LimitError(LimitSafeError::TooManyRequests)
        );
    }

    #[test]
    fn test_validation_error_abstraction_all_branches() {
        let err = ErrorMessageAbstraction::abstract_validation_error("null value");
        assert_eq!(
            err,
            SafeErrorMessage::ValidationError(ValidationSafeError::InvalidInput)
        );

        let err = ErrorMessageAbstraction::abstract_validation_error("Input too long: 1000 chars");
        assert_eq!(
            err,
            SafeErrorMessage::ValidationError(ValidationSafeError::InputTooLong)
        );

        let err = ErrorMessageAbstraction::abstract_validation_error("Parse error at line 1");
        assert_eq!(
            err,
            SafeErrorMessage::ValidationError(ValidationSafeError::InvalidFormat)
        );

        let err = ErrorMessageAbstraction::abstract_validation_error("<script>alert(1)</script>");
        assert_eq!(
            err,
            SafeErrorMessage::ValidationError(ValidationSafeError::SuspiciousPattern)
        );

        let err = ErrorMessageAbstraction::abstract_validation_error("Security check failed");
        assert_eq!(
            err,
            SafeErrorMessage::ValidationError(ValidationSafeError::SecurityCheckFailed)
        );
    }

    #[test]
    fn test_validation_error_abstraction_edge_cases() {
        let err = ErrorMessageAbstraction::abstract_validation_error("");
        assert_eq!(
            err,
            SafeErrorMessage::ValidationError(ValidationSafeError::MalformedData)
        );

        let err = ErrorMessageAbstraction::abstract_validation_error("SQL injection detected");
        assert_eq!(
            err,
            SafeErrorMessage::ValidationError(ValidationSafeError::SuspiciousPattern)
        );

        let err = ErrorMessageAbstraction::abstract_validation_error("Unknown validation error");
        assert_eq!(
            err,
            SafeErrorMessage::ValidationError(ValidationSafeError::MalformedData)
        );

        let err = ErrorMessageAbstraction::abstract_validation_error("Format error");
        assert_eq!(
            err,
            SafeErrorMessage::ValidationError(ValidationSafeError::InvalidFormat)
        );
    }

    #[test]
    fn test_display_config_safe_error_all_variants() {
        assert_eq!(
            ConfigSafeError::InvalidFormat.to_string(),
            "Invalid configuration format"
        );
        assert_eq!(
            ConfigSafeError::MissingRequiredField.to_string(),
            "Missing required field"
        );
        assert_eq!(
            ConfigSafeError::DuplicateRuleId.to_string(),
            "Duplicate rule ID"
        );
        assert_eq!(
            ConfigSafeError::InvalidStorageType.to_string(),
            "Invalid storage type"
        );
        assert_eq!(
            ConfigSafeError::InvalidCacheType.to_string(),
            "Invalid cache type"
        );
        assert_eq!(
            ConfigSafeError::InvalidMetricsType.to_string(),
            "Invalid metrics type"
        );
        assert_eq!(
            ConfigSafeError::InvalidVersion.to_string(),
            "Invalid version"
        );
        assert_eq!(ConfigSafeError::RuleNotFound.to_string(), "Rule not found");
        assert_eq!(
            ConfigSafeError::InvalidLimiterConfig.to_string(),
            "Invalid limiter configuration"
        );
        assert_eq!(
            ConfigSafeError::InvalidMatcherConfig.to_string(),
            "Invalid matcher configuration"
        );
        assert_eq!(
            ConfigSafeError::ValueOutOfRange.to_string(),
            "Value out of allowed range"
        );
        assert_eq!(
            ConfigSafeError::MalformedPattern.to_string(),
            "Malformed pattern"
        );
        assert_eq!(
            ConfigSafeError::SecurityRisk.to_string(),
            "Security risk detected"
        );
    }

    #[test]
    fn test_display_storage_safe_error_all_variants() {
        assert_eq!(
            StorageSafeError::ConnectionFailed.to_string(),
            "Connection failed"
        );
        assert_eq!(StorageSafeError::QueryFailed.to_string(), "Query failed");
        assert_eq!(StorageSafeError::Timeout.to_string(), "Operation timed out");
        assert_eq!(StorageSafeError::NotFound.to_string(), "Record not found");
        assert_eq!(
            StorageSafeError::ConcurrentModification.to_string(),
            "Data was concurrently modified"
        );
        assert_eq!(StorageSafeError::StorageFull.to_string(), "Storage full");
        assert_eq!(
            StorageSafeError::InvalidDataFormat.to_string(),
            "Invalid data format"
        );
    }

    #[test]
    fn test_display_limit_safe_error_all_variants() {
        assert_eq!(
            LimitSafeError::RateLimitExceeded.to_string(),
            "Request rate exceeded"
        );
        assert_eq!(LimitSafeError::QuotaExceeded.to_string(), "Quota exhausted");
        assert_eq!(
            LimitSafeError::ConcurrencyLimitExceeded.to_string(),
            "Concurrency limit exceeded"
        );
        assert_eq!(
            LimitSafeError::TokenBucketEmpty.to_string(),
            "Tokens exhausted"
        );
        assert_eq!(
            LimitSafeError::WindowFull.to_string(),
            "Time window is full"
        );
        assert_eq!(
            LimitSafeError::TooManyRequests.to_string(),
            "Too many requests"
        );
    }

    #[test]
    fn test_display_ban_safe_error_all_variants() {
        assert_eq!(BanSafeError::UserBanned.to_string(), "User is banned");
        assert_eq!(BanSafeError::IpBanned.to_string(), "IP address is banned");
        assert_eq!(BanSafeError::DeviceBanned.to_string(), "Device is banned");
        assert_eq!(
            BanSafeError::RateExceeded.to_string(),
            "Request rate exceeded"
        );
        assert_eq!(
            BanSafeError::SpamDetected.to_string(),
            "Suspicious behavior detected"
        );
        assert_eq!(
            BanSafeError::SecurityViolation.to_string(),
            "Security check failed"
        );
    }

    #[test]
    fn test_display_validation_safe_error_all_variants() {
        assert_eq!(
            ValidationSafeError::InvalidInput.to_string(),
            "Invalid input"
        );
        assert_eq!(
            ValidationSafeError::MalformedData.to_string(),
            "Malformed data"
        );
        assert_eq!(
            ValidationSafeError::SecurityCheckFailed.to_string(),
            "Security check failed"
        );
        assert_eq!(
            ValidationSafeError::InputTooLong.to_string(),
            "Input too long"
        );
        assert_eq!(
            ValidationSafeError::InvalidFormat.to_string(),
            "Invalid format"
        );
        assert_eq!(
            ValidationSafeError::SuspiciousPattern.to_string(),
            "Suspicious pattern detected"
        );
    }

    #[test]
    fn test_display_general_safe_error_all_variants() {
        assert_eq!(
            GeneralSafeError::InternalError.to_string(),
            "Internal error"
        );
        assert_eq!(
            GeneralSafeError::ServiceUnavailable.to_string(),
            "Service unavailable"
        );
        assert_eq!(
            GeneralSafeError::InvalidRequest.to_string(),
            "Invalid request"
        );
        assert_eq!(GeneralSafeError::Unauthorized.to_string(), "Unauthorized");
        assert_eq!(GeneralSafeError::Forbidden.to_string(), "Forbidden");
        assert_eq!(GeneralSafeError::RateLimited.to_string(), "Rate limited");
    }

    #[test]
    fn test_display_safe_error_message_all_variants() {
        assert_eq!(
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidCacheType).to_string(),
            "Configuration error: Invalid cache type"
        );
        assert_eq!(
            SafeErrorMessage::StorageError(StorageSafeError::StorageFull).to_string(),
            "Storage error: Storage full"
        );
        assert_eq!(
            SafeErrorMessage::LimitError(LimitSafeError::WindowFull).to_string(),
            "Rate limit error: Time window is full"
        );
        assert_eq!(
            SafeErrorMessage::BanError(BanSafeError::UserBanned).to_string(),
            "Ban error: User is banned"
        );
        assert_eq!(
            SafeErrorMessage::ValidationError(ValidationSafeError::InputTooLong).to_string(),
            "Validation error: Input too long"
        );
        assert_eq!(
            SafeErrorMessage::General(GeneralSafeError::InternalError).to_string(),
            "Error: Internal error"
        );
    }

    /// 错误双轨：Display 恒英文规范串；to_localized_string 随 locale，
    /// message_en 恒英文（经目录，与 Display 逐字对齐）。
    #[test]
    fn test_safe_error_dual_track() {
        use crate::i18n::{I18nExt, clear_locale_override, set_locale};

        let err = SafeErrorMessage::StorageError(StorageSafeError::NotFound);
        assert_eq!(err.to_string(), "Storage error: Record not found");
        assert_eq!(err.message_en(), "Storage error: Record not found");

        clear_locale_override();
        set_locale("en").expect("en is valid");
        assert_eq!(err.to_localized_string(), "Storage error: Record not found");

        set_locale("zh-CN").expect("zh-CN is valid");
        assert_eq!(err.to_localized_string(), "存储错误: 记录不存在");

        // 内层变体同样具备双轨能力
        let inner = StorageSafeError::ConcurrentModification;
        assert_eq!(inner.to_string(), "Data was concurrently modified");
        assert_eq!(inner.message_en(), "Data was concurrently modified");

        clear_locale_override();
    }

    #[test]
    fn test_safe_error_message_construction() {
        let err = SafeErrorMessage::BanError(BanSafeError::IpBanned);
        assert_eq!(err, SafeErrorMessage::BanError(BanSafeError::IpBanned));

        let err = SafeErrorMessage::General(GeneralSafeError::ServiceUnavailable);
        assert_eq!(
            err,
            SafeErrorMessage::General(GeneralSafeError::ServiceUnavailable)
        );

        let err = SafeErrorMessage::ConfigError(ConfigSafeError::RuleNotFound);
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::RuleNotFound)
        );

        let err = SafeErrorMessage::ValidationError(ValidationSafeError::SuspiciousPattern);
        assert_eq!(
            err,
            SafeErrorMessage::ValidationError(ValidationSafeError::SuspiciousPattern)
        );
    }

    #[test]
    fn test_error_types_clone_eq() {
        assert_eq!(
            ConfigSafeError::InvalidFormat.clone(),
            ConfigSafeError::InvalidFormat
        );
        assert_eq!(
            StorageSafeError::StorageFull.clone(),
            StorageSafeError::StorageFull
        );
        assert_eq!(
            LimitSafeError::QuotaExceeded.clone(),
            LimitSafeError::QuotaExceeded
        );
        assert_eq!(
            BanSafeError::SpamDetected.clone(),
            BanSafeError::SpamDetected
        );
        assert_eq!(
            ValidationSafeError::MalformedData.clone(),
            ValidationSafeError::MalformedData
        );
        assert_eq!(
            GeneralSafeError::Forbidden.clone(),
            GeneralSafeError::Forbidden
        );
    }

    #[test]
    fn test_error_types_inequality() {
        assert_ne!(
            ConfigSafeError::InvalidFormat,
            ConfigSafeError::MissingRequiredField
        );
        assert_ne!(
            StorageSafeError::ConnectionFailed,
            StorageSafeError::QueryFailed
        );
        assert_ne!(
            LimitSafeError::RateLimitExceeded,
            LimitSafeError::QuotaExceeded
        );
        assert_ne!(BanSafeError::UserBanned, BanSafeError::IpBanned);
        assert_ne!(
            ValidationSafeError::InvalidInput,
            ValidationSafeError::MalformedData
        );
        assert_ne!(
            GeneralSafeError::InternalError,
            GeneralSafeError::ServiceUnavailable
        );
    }

    #[test]
    fn test_safe_error_message_inequality() {
        assert_ne!(
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidFormat),
            SafeErrorMessage::StorageError(StorageSafeError::ConnectionFailed)
        );
        assert_ne!(
            SafeErrorMessage::LimitError(LimitSafeError::RateLimitExceeded),
            SafeErrorMessage::BanError(BanSafeError::UserBanned)
        );
    }
}
