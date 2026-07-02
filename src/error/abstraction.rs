//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
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
            SafeErrorMessage::ConfigError(e) => write!(f, "配置错误: {}", e),
            SafeErrorMessage::StorageError(e) => write!(f, "存储错误: {}", e),
            SafeErrorMessage::LimitError(e) => write!(f, "限流错误: {}", e),
            SafeErrorMessage::BanError(e) => write!(f, "封禁错误: {}", e),
            SafeErrorMessage::ValidationError(e) => write!(f, "验证错误: {}", e),
            SafeErrorMessage::General(e) => write!(f, "错误: {}", e),
        }
    }
}

impl std::fmt::Display for ConfigSafeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigSafeError::InvalidFormat => write!(f, "配置格式无效"),
            ConfigSafeError::MissingRequiredField => write!(f, "缺少必需字段"),
            ConfigSafeError::DuplicateRuleId => write!(f, "规则ID重复"),
            ConfigSafeError::InvalidStorageType => write!(f, "无效的存储类型"),
            ConfigSafeError::InvalidCacheType => write!(f, "无效的缓存类型"),
            ConfigSafeError::InvalidMetricsType => write!(f, "无效的指标类型"),
            ConfigSafeError::InvalidVersion => write!(f, "版本号无效"),
            ConfigSafeError::RuleNotFound => write!(f, "规则不存在"),
            ConfigSafeError::InvalidLimiterConfig => write!(f, "限流器配置无效"),
            ConfigSafeError::InvalidMatcherConfig => write!(f, "匹配器配置无效"),
            ConfigSafeError::ValueOutOfRange => write!(f, "值超出允许范围"),
            ConfigSafeError::MalformedPattern => write!(f, "模式格式错误"),
            ConfigSafeError::SecurityRisk => write!(f, "检测到安全风险"),
        }
    }
}

impl std::fmt::Display for StorageSafeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageSafeError::ConnectionFailed => write!(f, "连接失败"),
            StorageSafeError::QueryFailed => write!(f, "查询失败"),
            StorageSafeError::Timeout => write!(f, "操作超时"),
            StorageSafeError::NotFound => write!(f, "记录不存在"),
            StorageSafeError::ConcurrentModification => write!(f, "数据被并发修改"),
            StorageSafeError::StorageFull => write!(f, "存储空间不足"),
            StorageSafeError::InvalidDataFormat => write!(f, "数据格式无效"),
        }
    }
}

impl std::fmt::Display for LimitSafeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LimitSafeError::RateLimitExceeded => write!(f, "请求频率超出限制"),
            LimitSafeError::QuotaExceeded => write!(f, "配额已用尽"),
            LimitSafeError::ConcurrencyLimitExceeded => write!(f, "并发请求数超出限制"),
            LimitSafeError::TokenBucketEmpty => write!(f, "令牌已用尽"),
            LimitSafeError::WindowFull => write!(f, "时间窗口已满"),
            LimitSafeError::TooManyRequests => write!(f, "请求过于频繁"),
        }
    }
}

impl std::fmt::Display for BanSafeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BanSafeError::UserBanned => write!(f, "用户已被封禁"),
            BanSafeError::IpBanned => write!(f, "IP地址已被封禁"),
            BanSafeError::DeviceBanned => write!(f, "设备已被封禁"),
            BanSafeError::RateExceeded => write!(f, "请求频率超出限制"),
            BanSafeError::SpamDetected => write!(f, "检测到可疑行为"),
            BanSafeError::SecurityViolation => write!(f, "安全检查未通过"),
        }
    }
}

impl std::fmt::Display for ValidationSafeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationSafeError::InvalidInput => write!(f, "输入无效"),
            ValidationSafeError::MalformedData => write!(f, "数据格式错误"),
            ValidationSafeError::SecurityCheckFailed => write!(f, "安全检查失败"),
            ValidationSafeError::InputTooLong => write!(f, "输入过长"),
            ValidationSafeError::InvalidFormat => write!(f, "格式无效"),
            ValidationSafeError::SuspiciousPattern => write!(f, "检测到可疑模式"),
        }
    }
}

impl std::fmt::Display for GeneralSafeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeneralSafeError::InternalError => write!(f, "内部错误"),
            GeneralSafeError::ServiceUnavailable => write!(f, "服务不可用"),
            GeneralSafeError::InvalidRequest => write!(f, "请求无效"),
            GeneralSafeError::Unauthorized => write!(f, "未授权"),
            GeneralSafeError::Forbidden => write!(f, "禁止访问"),
            GeneralSafeError::RateLimited => write!(f, "请求被限流"),
        }
    }
}

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
        } else if lower_error.contains("not found") || lower_error.contains("不存在") {
            SafeErrorMessage::StorageError(StorageSafeError::NotFound)
        } else if lower_error.contains("duplicate") || lower_error.contains("冲突") {
            SafeErrorMessage::StorageError(StorageSafeError::ConcurrentModification)
        } else {
            SafeErrorMessage::StorageError(StorageSafeError::QueryFailed)
        }
    }

    /// 从详细配置错误生成安全错误消息
    pub fn abstract_config_error(detailed_error: &str) -> SafeErrorMessage {
        let lower_error = detailed_error.to_lowercase();

        if lower_error.contains("duplicate") || lower_error.contains("重复") {
            SafeErrorMessage::ConfigError(ConfigSafeError::DuplicateRuleId)
        } else if lower_error.contains("storage")
            || lower_error.contains("存储")
                && (lower_error.contains("invalid") || lower_error.contains("无效"))
        {
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidStorageType)
        } else if lower_error.contains("cache")
            || lower_error.contains("缓存")
                && (lower_error.contains("invalid") || lower_error.contains("无效"))
        {
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidCacheType)
        } else if lower_error.contains("version") || lower_error.contains("版本") {
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidVersion)
        } else if lower_error.contains("missing")
            || lower_error.contains("empty")
            || lower_error.contains("缺少")
            || lower_error.contains("为空")
        {
            SafeErrorMessage::ConfigError(ConfigSafeError::MissingRequiredField)
        } else if lower_error.contains("format")
            || lower_error.contains("格式")
            || lower_error.contains("parse")
        {
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidFormat)
        } else if lower_error.contains("limiter") || lower_error.contains("限流器") {
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidLimiterConfig)
        } else if lower_error.contains("matcher") || lower_error.contains("匹配器") {
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidMatcherConfig)
        } else if lower_error.contains("range")
            || lower_error.contains("范围")
            || lower_error.contains("out of")
        {
            SafeErrorMessage::ConfigError(ConfigSafeError::ValueOutOfRange)
        } else if lower_error.contains("<script")
            || lower_error.contains("注入")
            || lower_error.contains("injection")
        {
            SafeErrorMessage::ConfigError(ConfigSafeError::SecurityRisk)
        } else {
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidFormat)
        }
    }

    /// 从详细限流错误生成安全错误消息
    pub fn abstract_limit_error(detailed_error: &str) -> SafeErrorMessage {
        let lower_error = detailed_error.to_lowercase();

        if lower_error.contains("rate")
            || lower_error.contains("频率")
            || lower_error.contains("rate limit")
        {
            SafeErrorMessage::LimitError(LimitSafeError::RateLimitExceeded)
        } else if lower_error.contains("quota") || lower_error.contains("配额") {
            SafeErrorMessage::LimitError(LimitSafeError::QuotaExceeded)
        } else if lower_error.contains("concurrency")
            || lower_error.contains("并发")
            || lower_error.contains("concurrent")
        {
            SafeErrorMessage::LimitError(LimitSafeError::ConcurrencyLimitExceeded)
        } else if lower_error.contains("token") || lower_error.contains("令牌") {
            SafeErrorMessage::LimitError(LimitSafeError::TokenBucketEmpty)
        } else if lower_error.contains("window") || lower_error.contains("窗口") {
            SafeErrorMessage::LimitError(LimitSafeError::WindowFull)
        } else {
            SafeErrorMessage::LimitError(LimitSafeError::TooManyRequests)
        }
    }

    /// 从详细验证错误生成安全错误消息
    pub fn abstract_validation_error(detailed_error: &str) -> SafeErrorMessage {
        let lower_error = detailed_error.to_lowercase();

        if lower_error.contains("empty")
            || lower_error.contains("null")
            || lower_error.contains("为空")
            || lower_error.contains("空")
        {
            SafeErrorMessage::ValidationError(ValidationSafeError::InvalidInput)
        } else if lower_error.contains("length")
            || lower_error.contains("too long")
            || lower_error.contains("过长")
        {
            SafeErrorMessage::ValidationError(ValidationSafeError::InputTooLong)
        } else if lower_error.contains("format")
            || lower_error.contains("格式")
            || lower_error.contains("parse")
        {
            SafeErrorMessage::ValidationError(ValidationSafeError::InvalidFormat)
        } else if lower_error.contains("<script")
            || lower_error.contains("sql")
            || lower_error.contains("injection")
            || lower_error.contains("注入")
        {
            SafeErrorMessage::ValidationError(ValidationSafeError::SuspiciousPattern)
        } else if lower_error.contains("security") || lower_error.contains("安全") {
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
        assert_eq!(err.to_string(), "配置错误: 规则ID重复");

        let err = SafeErrorMessage::StorageError(StorageSafeError::ConnectionFailed);
        assert_eq!(err.to_string(), "存储错误: 连接失败");

        let err = SafeErrorMessage::LimitError(LimitSafeError::RateLimitExceeded);
        assert_eq!(err.to_string(), "限流错误: 请求频率超出限制");
    }

    #[test]
    fn test_storage_error_abstraction_all_branches() {
        let err = ErrorMessageAbstraction::abstract_storage_error("Duplicate key error");
        assert_eq!(
            err,
            SafeErrorMessage::StorageError(StorageSafeError::ConcurrentModification)
        );

        let err = ErrorMessageAbstraction::abstract_storage_error("数据冲突");
        assert_eq!(
            err,
            SafeErrorMessage::StorageError(StorageSafeError::ConcurrentModification)
        );

        let err = ErrorMessageAbstraction::abstract_storage_error("记录不存在");
        assert_eq!(
            err,
            SafeErrorMessage::StorageError(StorageSafeError::NotFound)
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

        let err = ErrorMessageAbstraction::abstract_config_error("缺少必需字段");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::MissingRequiredField)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("Invalid format for field");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidFormat)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("配置格式无效");
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

        let err = ErrorMessageAbstraction::abstract_config_error("检测到注入攻击");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::SecurityRisk)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("规则ID重复");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::DuplicateRuleId)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("无效的存储类型");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidStorageType)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("无效的缓存类型");
        assert_eq!(
            err,
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidCacheType)
        );

        let err = ErrorMessageAbstraction::abstract_config_error("版本号无效");
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
        let err = ErrorMessageAbstraction::abstract_limit_error("请求频率超出限制");
        assert_eq!(
            err,
            SafeErrorMessage::LimitError(LimitSafeError::RateLimitExceeded)
        );

        let err = ErrorMessageAbstraction::abstract_limit_error("配额已用尽");
        assert_eq!(
            err,
            SafeErrorMessage::LimitError(LimitSafeError::QuotaExceeded)
        );

        let err = ErrorMessageAbstraction::abstract_limit_error("concurrent request limit");
        assert_eq!(
            err,
            SafeErrorMessage::LimitError(LimitSafeError::ConcurrencyLimitExceeded)
        );

        let err = ErrorMessageAbstraction::abstract_limit_error("并发请求数超出限制");
        assert_eq!(
            err,
            SafeErrorMessage::LimitError(LimitSafeError::ConcurrencyLimitExceeded)
        );

        let err = ErrorMessageAbstraction::abstract_limit_error("No tokens available");
        assert_eq!(
            err,
            SafeErrorMessage::LimitError(LimitSafeError::TokenBucketEmpty)
        );

        let err = ErrorMessageAbstraction::abstract_limit_error("令牌已用尽");
        assert_eq!(
            err,
            SafeErrorMessage::LimitError(LimitSafeError::TokenBucketEmpty)
        );

        let err = ErrorMessageAbstraction::abstract_limit_error("Window is full");
        assert_eq!(
            err,
            SafeErrorMessage::LimitError(LimitSafeError::WindowFull)
        );

        let err = ErrorMessageAbstraction::abstract_limit_error("时间窗口已满");
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

        let err = ErrorMessageAbstraction::abstract_validation_error("输入为空");
        assert_eq!(
            err,
            SafeErrorMessage::ValidationError(ValidationSafeError::InvalidInput)
        );

        let err = ErrorMessageAbstraction::abstract_validation_error("输入过长");
        assert_eq!(
            err,
            SafeErrorMessage::ValidationError(ValidationSafeError::InputTooLong)
        );

        let err = ErrorMessageAbstraction::abstract_validation_error("Parse error at line 1");
        assert_eq!(
            err,
            SafeErrorMessage::ValidationError(ValidationSafeError::InvalidFormat)
        );

        let err = ErrorMessageAbstraction::abstract_validation_error("格式错误");
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

        let err = ErrorMessageAbstraction::abstract_validation_error("安全检查失败");
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

        let err = ErrorMessageAbstraction::abstract_validation_error("检测到注入攻击");
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
        assert_eq!(ConfigSafeError::InvalidFormat.to_string(), "配置格式无效");
        assert_eq!(
            ConfigSafeError::MissingRequiredField.to_string(),
            "缺少必需字段"
        );
        assert_eq!(ConfigSafeError::DuplicateRuleId.to_string(), "规则ID重复");
        assert_eq!(
            ConfigSafeError::InvalidStorageType.to_string(),
            "无效的存储类型"
        );
        assert_eq!(
            ConfigSafeError::InvalidCacheType.to_string(),
            "无效的缓存类型"
        );
        assert_eq!(
            ConfigSafeError::InvalidMetricsType.to_string(),
            "无效的指标类型"
        );
        assert_eq!(ConfigSafeError::InvalidVersion.to_string(), "版本号无效");
        assert_eq!(ConfigSafeError::RuleNotFound.to_string(), "规则不存在");
        assert_eq!(
            ConfigSafeError::InvalidLimiterConfig.to_string(),
            "限流器配置无效"
        );
        assert_eq!(
            ConfigSafeError::InvalidMatcherConfig.to_string(),
            "匹配器配置无效"
        );
        assert_eq!(
            ConfigSafeError::ValueOutOfRange.to_string(),
            "值超出允许范围"
        );
        assert_eq!(
            ConfigSafeError::MalformedPattern.to_string(),
            "模式格式错误"
        );
        assert_eq!(ConfigSafeError::SecurityRisk.to_string(), "检测到安全风险");
    }

    #[test]
    fn test_display_storage_safe_error_all_variants() {
        assert_eq!(StorageSafeError::ConnectionFailed.to_string(), "连接失败");
        assert_eq!(StorageSafeError::QueryFailed.to_string(), "查询失败");
        assert_eq!(StorageSafeError::Timeout.to_string(), "操作超时");
        assert_eq!(StorageSafeError::NotFound.to_string(), "记录不存在");
        assert_eq!(
            StorageSafeError::ConcurrentModification.to_string(),
            "数据被并发修改"
        );
        assert_eq!(StorageSafeError::StorageFull.to_string(), "存储空间不足");
        assert_eq!(
            StorageSafeError::InvalidDataFormat.to_string(),
            "数据格式无效"
        );
    }

    #[test]
    fn test_display_limit_safe_error_all_variants() {
        assert_eq!(
            LimitSafeError::RateLimitExceeded.to_string(),
            "请求频率超出限制"
        );
        assert_eq!(LimitSafeError::QuotaExceeded.to_string(), "配额已用尽");
        assert_eq!(
            LimitSafeError::ConcurrencyLimitExceeded.to_string(),
            "并发请求数超出限制"
        );
        assert_eq!(LimitSafeError::TokenBucketEmpty.to_string(), "令牌已用尽");
        assert_eq!(LimitSafeError::WindowFull.to_string(), "时间窗口已满");
        assert_eq!(LimitSafeError::TooManyRequests.to_string(), "请求过于频繁");
    }

    #[test]
    fn test_display_ban_safe_error_all_variants() {
        assert_eq!(BanSafeError::UserBanned.to_string(), "用户已被封禁");
        assert_eq!(BanSafeError::IpBanned.to_string(), "IP地址已被封禁");
        assert_eq!(BanSafeError::DeviceBanned.to_string(), "设备已被封禁");
        assert_eq!(BanSafeError::RateExceeded.to_string(), "请求频率超出限制");
        assert_eq!(BanSafeError::SpamDetected.to_string(), "检测到可疑行为");
        assert_eq!(
            BanSafeError::SecurityViolation.to_string(),
            "安全检查未通过"
        );
    }

    #[test]
    fn test_display_validation_safe_error_all_variants() {
        assert_eq!(ValidationSafeError::InvalidInput.to_string(), "输入无效");
        assert_eq!(
            ValidationSafeError::MalformedData.to_string(),
            "数据格式错误"
        );
        assert_eq!(
            ValidationSafeError::SecurityCheckFailed.to_string(),
            "安全检查失败"
        );
        assert_eq!(ValidationSafeError::InputTooLong.to_string(), "输入过长");
        assert_eq!(ValidationSafeError::InvalidFormat.to_string(), "格式无效");
        assert_eq!(
            ValidationSafeError::SuspiciousPattern.to_string(),
            "检测到可疑模式"
        );
    }

    #[test]
    fn test_display_general_safe_error_all_variants() {
        assert_eq!(GeneralSafeError::InternalError.to_string(), "内部错误");
        assert_eq!(
            GeneralSafeError::ServiceUnavailable.to_string(),
            "服务不可用"
        );
        assert_eq!(GeneralSafeError::InvalidRequest.to_string(), "请求无效");
        assert_eq!(GeneralSafeError::Unauthorized.to_string(), "未授权");
        assert_eq!(GeneralSafeError::Forbidden.to_string(), "禁止访问");
        assert_eq!(GeneralSafeError::RateLimited.to_string(), "请求被限流");
    }

    #[test]
    fn test_display_safe_error_message_all_variants() {
        assert_eq!(
            SafeErrorMessage::ConfigError(ConfigSafeError::InvalidCacheType).to_string(),
            "配置错误: 无效的缓存类型"
        );
        assert_eq!(
            SafeErrorMessage::StorageError(StorageSafeError::StorageFull).to_string(),
            "存储错误: 存储空间不足"
        );
        assert_eq!(
            SafeErrorMessage::LimitError(LimitSafeError::WindowFull).to_string(),
            "限流错误: 时间窗口已满"
        );
        assert_eq!(
            SafeErrorMessage::BanError(BanSafeError::UserBanned).to_string(),
            "封禁错误: 用户已被封禁"
        );
        assert_eq!(
            SafeErrorMessage::ValidationError(ValidationSafeError::InputTooLong).to_string(),
            "验证错误: 输入过长"
        );
        assert_eq!(
            SafeErrorMessage::General(GeneralSafeError::InternalError).to_string(),
            "错误: 内部错误"
        );
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
