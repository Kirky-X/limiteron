//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 错误类型定义
//!
//! 使用thiserror定义所有错误类型。

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
}

#[cfg(feature = "postgres")]
impl From<sqlx::Error> for StorageError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::Database(db_err) => StorageError::QueryError(db_err.to_string()),
            sqlx::Error::PoolTimedOut => StorageError::TimeoutError("连接池超时".to_string()),
            sqlx::Error::PoolClosed => StorageError::ConnectionError("连接池已关闭".to_string()),
            sqlx::Error::RowNotFound => StorageError::NotFound("记录未找到".to_string()),
            _ => StorageError::QueryError(err.to_string()),
        }
    }
}

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
    pub reason: String,
    pub banned_until: chrono::DateTime<chrono::Utc>,
    pub ban_times: u32,
}

/// 配额消费结果
#[derive(Debug, Clone)]
pub struct ConsumeResult {
    pub allowed: bool,
    pub remaining: u64,
    pub alert_triggered: bool,
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
        let info = BanInfo {
            reason: "spam".to_string(),
            banned_until: chrono::Utc::now(),
            ban_times: 3,
        };
        let decision = Decision::Banned(info);
        assert!(matches!(decision, Decision::Banned(_)));
    }

    #[test]
    fn test_ban_info_equality() {
        let info1 = BanInfo {
            reason: "test".to_string(),
            banned_until: chrono::Utc::now(),
            ban_times: 1,
        };
        let info2 = BanInfo {
            reason: "test".to_string(),
            banned_until: info1.banned_until,
            ban_times: 1,
        };
        assert_eq!(info1, info2);
    }
}
