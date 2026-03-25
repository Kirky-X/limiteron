//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
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

/// 决策结果
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// 允许
    Allowed(Option<String>),
    /// 拒绝
    Rejected(String),
    /// 封禁
    Banned(BanInfo),
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
        let decision = Decision::Allowed(None);
        assert_eq!(decision, Decision::Allowed(None));
        assert!(matches!(decision, Decision::Allowed(_)));
    }

    #[test]
    fn test_decision_rejected() {
        let decision = Decision::Rejected("rate limit exceeded".to_string());
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    #[test]
    fn test_decision_banned() {
        let info = BanInfo::new("spam".to_string(), chrono::Utc::now(), 3);
        let decision = Decision::Banned(info);
        assert!(matches!(decision, Decision::Banned(_)));
    }

    #[test]
    fn test_ban_info_equality() {
        let now = chrono::Utc::now();
        let info1 = BanInfo::new("test".to_string(), now, 1);
        let info2 = BanInfo::new("test".to_string(), now, 1);
        assert_eq!(info1, info2);
    }
}
