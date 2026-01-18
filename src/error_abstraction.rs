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
}
