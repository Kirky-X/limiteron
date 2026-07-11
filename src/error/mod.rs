// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 错误类型定义
//!
//! 使用thiserror定义所有错误类型。

// 子模块
pub mod abstraction;

// 重新导出 abstraction 模块的公共类型
pub use abstraction::{
    BanSafeError, ConfigSafeError, ErrorMessageAbstraction, GeneralSafeError, LimitSafeError,
    SafeErrorMessage, StorageSafeError, ValidationSafeError,
};

use thiserror::Error;

/// FlowGuard 错误类型
#[derive(Error, Debug)]
pub enum FlowGuardError {
    /// 配置错误
    #[error("配置错误: {0}")]
    ConfigError(String),

    /// 存储错误
    #[error("存储错误: {0}")]
    StorageError(#[from] StorageError),

    /// 限流错误
    #[error("限流错误: {0}")]
    LimitError(String),

    /// 封禁错误
    #[error("封禁错误: {0}")]
    BanError(String),

    /// 熔断器错误
    #[error("熔断器错误: {0}")]
    CircuitBreakerError(String),

    /// 降级错误
    #[error("降级错误: {0}")]
    FallbackError(String),

    /// 审计日志错误
    #[error("审计日志错误: {0}")]
    AuditLogError(String),

    /// 授权错误
    #[error("授权错误: {0}")]
    AuthorizationError(String),

    /// IO错误
    #[error("IO错误: {0}")]
    IoError(#[from] std::io::Error),

    /// 序列化错误
    #[error("序列化错误: {0}")]
    SerdeError(#[from] serde_json::Error),

    /// YAML解析错误
    #[error("YAML解析错误: {0}")]
    YamlError(#[from] serde_yaml::Error),

    /// 速率限制超出
    #[error("速率限制超出: {0}")]
    RateLimitExceeded(String),

    /// 配额超出
    #[error("配额超出: {0}")]
    QuotaExceeded(String),

    /// 并发限制超出
    #[error("并发限制超出: {0}")]
    ConcurrencyLimitExceeded(String),

    /// 验证错误
    #[error("验证错误: {0}")]
    ValidationError(String),

    /// 锁获取错误
    #[error("锁获取错误: {0}")]
    LockError(String),

    /// 时间错误
    #[error("时间错误: {0}")]
    TimeError(String),

    /// 依赖缺失错误
    #[error("依赖缺失: {0}")]
    DependencyError(String),

    /// 其他错误
    #[error("未知错误: {0}")]
    Other(String),
}

/// 存储错误
#[derive(Error, Debug, Clone)]
pub enum StorageError {
    /// 连接错误
    #[error("连接错误: {0}")]
    ConnectionError(String),

    /// 查询错误
    #[error("查询错误: {0}")]
    QueryError(String),

    /// 超时错误
    #[error("超时错误: {0}")]
    TimeoutError(String),

    /// 未找到
    #[error("未找到: {0}")]
    NotFound(String),

    /// 认证错误
    #[error("认证错误: {0}")]
    AuthenticationError(String),

    /// 权限错误
    #[error("权限错误: {0}")]
    PermissionError(String),

    /// 无效配置
    #[error("无效配置: {0}")]
    InvalidConfig(String),

    /// 速率限制
    #[error("速率限制: {0}")]
    RateLimitError(String),

    /// 验证错误
    #[error("验证错误: {0}")]
    ValidationError(String),
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

// sqlx error conversion removed - using DBNexus for database operations
// Error conversion is now handled by DBNexusStorageAdapter

/// 熔断器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CircuitState {
    /// 关闭状态（正常）
    Closed,
    /// 打开状态（熔断）
    Open,
    /// 半开状态（探测）
    HalfOpen,
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

/// 熔断器统计信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CircuitBreakerStats {
    /// 当前状态
    pub state: CircuitState,
    /// 失败次数
    pub failure_count: u64,
    /// 成功次数
    pub success_count: u64,
    /// 总调用次数
    pub total_calls: u64,
    /// 最后失败时间
    pub last_failure_time: Option<chrono::DateTime<chrono::Utc>>,
    /// 最后状态变更时间
    pub last_state_change: Option<chrono::DateTime<chrono::Utc>>,
}

/// 限流元数据信息
///
/// 用于标准限流响应头，包含当前限流状态信息。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RateLimitMetadata {
    /// 限流上限
    pub limit: u64,
    /// 剩余可用次数
    pub remaining: u64,
    /// 重置时间戳（Unix 秒）
    pub reset_at: u64,
    /// 重试等待时间（秒，仅在超限时）
    pub retry_after: Option<u64>,
    /// 限流策略名称
    pub policy: String,
}

/// 拒绝元数据信息
///
/// 包含请求被拒绝的详细信息。
#[derive(Debug, Clone, PartialEq)]
pub struct RejectionMetadata {
    /// 拒绝原因
    pub reason: String,
    /// 重试等待时间（秒）
    pub retry_after: u64,
    /// 限流上限
    pub limit: u64,
    /// 重置时间戳（Unix 秒）
    pub reset_at: u64,
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

/// 决策结果
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// 允许（携带限流元数据）
    Allowed(RateLimitMetadata),
    /// 拒绝（携带拒绝元数据）
    Rejected(RejectionMetadata),
    /// 封禁
    Banned(BanInfo),
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

/// 封禁信息
#[derive(Debug, Clone, PartialEq)]
pub struct BanInfo {
    /// 封禁原因
    reason: String,
    /// 封禁到期时间
    banned_until: chrono::DateTime<chrono::Utc>,
    /// 封禁次数
    ban_times: u32,
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

/// 配额消费结果
#[derive(Debug, Clone)]
pub struct ConsumeResult {
    /// 是否允许继续消费
    pub allowed: bool,
    /// 剩余配额
    pub remaining: u64,
    /// 是否触发告警（基于使用率阈值判断）
    pub alert_triggered: bool,
    /// 当前使用率（百分比 0-100）
    pub usage_percent: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_message() {
        let error = FlowGuardError::ConfigError("测试错误".to_string());
        assert_eq!(error.to_string(), "配置错误: 测试错误");
    }

    #[test]
    fn test_storage_error_conversion() {
        let storage_error = StorageError::NotFound("test_key".to_string());
        let flowguard_error: FlowGuardError = storage_error.into();
        assert!(matches!(flowguard_error, FlowGuardError::StorageError(_)));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let flowguard_error: FlowGuardError = io_error.into();
        assert!(matches!(flowguard_error, FlowGuardError::IoError(_)));
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
            FlowGuardError::LimitError("x".into()).to_string(),
            "限流错误: x"
        );
        assert_eq!(
            FlowGuardError::BanError("x".into()).to_string(),
            "封禁错误: x"
        );
        assert_eq!(
            FlowGuardError::CircuitBreakerError("x".into()).to_string(),
            "熔断器错误: x"
        );
        assert_eq!(
            FlowGuardError::FallbackError("x".into()).to_string(),
            "降级错误: x"
        );
        assert_eq!(
            FlowGuardError::AuditLogError("x".into()).to_string(),
            "审计日志错误: x"
        );
        assert_eq!(
            FlowGuardError::AuthorizationError("x".into()).to_string(),
            "授权错误: x"
        );
        assert_eq!(
            FlowGuardError::RateLimitExceeded("x".into()).to_string(),
            "速率限制超出: x"
        );
        assert_eq!(
            FlowGuardError::QuotaExceeded("x".into()).to_string(),
            "配额超出: x"
        );
        assert_eq!(
            FlowGuardError::ConcurrencyLimitExceeded("x".into()).to_string(),
            "并发限制超出: x"
        );
        assert_eq!(
            FlowGuardError::ValidationError("x".into()).to_string(),
            "验证错误: x"
        );
        assert_eq!(
            FlowGuardError::LockError("x".into()).to_string(),
            "锁获取错误: x"
        );
        assert_eq!(
            FlowGuardError::TimeError("x".into()).to_string(),
            "时间错误: x"
        );
        assert_eq!(
            FlowGuardError::DependencyError("x".into()).to_string(),
            "依赖缺失: x"
        );
        assert_eq!(FlowGuardError::Other("x".into()).to_string(), "未知错误: x");
    }

    #[test]
    fn test_storage_error_display() {
        assert_eq!(
            StorageError::ConnectionError("c".into()).to_string(),
            "连接错误: c"
        );
        assert_eq!(
            StorageError::QueryError("q".into()).to_string(),
            "查询错误: q"
        );
        assert_eq!(
            StorageError::TimeoutError("t".into()).to_string(),
            "超时错误: t"
        );
        assert_eq!(StorageError::NotFound("n".into()).to_string(), "未找到: n");
        assert_eq!(
            StorageError::AuthenticationError("a".into()).to_string(),
            "认证错误: a"
        );
        assert_eq!(
            StorageError::PermissionError("p".into()).to_string(),
            "权限错误: p"
        );
        assert_eq!(
            StorageError::InvalidConfig("i".into()).to_string(),
            "无效配置: i"
        );
        assert_eq!(
            StorageError::RateLimitError("r".into()).to_string(),
            "速率限制: r"
        );
        assert_eq!(
            StorageError::ValidationError("v".into()).to_string(),
            "验证错误: v"
        );
    }

    #[test]
    fn test_serde_error_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("{invalid}");
        let err: serde_json::Error = json_err.unwrap_err();
        let fg_err: FlowGuardError = err.into();
        assert!(matches!(fg_err, FlowGuardError::SerdeError(_)));
    }

    #[test]
    fn test_storage_error_from_into_flowguard() {
        let se = StorageError::QueryError("q".into());
        let fg: FlowGuardError = se.into();
        assert!(matches!(fg, FlowGuardError::StorageError(_)));
        let back = match fg {
            FlowGuardError::StorageError(s) => s,
            _ => unreachable!(),
        };
        assert!(matches!(back, StorageError::QueryError(_)));
    }
}
