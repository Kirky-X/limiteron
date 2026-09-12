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

// 实现模块
mod error_impl;

/// FlowGuard 错误类型
#[derive(Error, Debug)]
pub enum LimiteronError {
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
    YamlError(#[from] serde_yaml_ng::Error),

    /// 速率限制超出
    #[error("速率限制超出: {0}")]
    RateLimitExceeded(String),

    /// 配额超出
    #[error("配额超出: {0}")]
    QuotaExceeded(String),

    /// 并发限制超出
    #[error("并发限制超出: {0}")]
    ConcurrencyLimitExceeded(String),

    /// 排队超时：`on_exceed = "throttle"` 模式下，请求在限流队列中
    /// 等待超过队列时限仍未获得令牌时返回
    #[error("排队超时: {0}")]
    Throttled(String),

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

impl ConsumeResult {
    fn usage_percent(consumed: u64, limit: u64) -> f64 {
        if limit > 0 {
            (consumed as f64 / limit as f64) * 100.0
        } else {
            0.0
        }
    }

    /// 构造放行结果（`consumed` 为放行后的账本值）
    ///
    /// 供各存储后端统一结果构造（diting 简化：usage/remaining 推导
    /// 此前在 cache 与 dbnexus 适配器中各写一份）。
    pub fn allowed(consumed: u64, limit: u64) -> Self {
        Self {
            allowed: true,
            remaining: limit.saturating_sub(consumed),
            alert_triggered: false,
            usage_percent: Self::usage_percent(consumed, limit),
        }
    }

    /// 构造拒绝结果（`consumed` 为当前已用量）
    pub fn rejected(consumed: u64, limit: u64) -> Self {
        Self {
            allowed: false,
            remaining: limit.saturating_sub(consumed),
            alert_triggered: false,
            usage_percent: Self::usage_percent(consumed, limit),
        }
    }
}

/// Limiteron 结果类型别名
pub type LimiteronResult<T> = std::result::Result<T, LimiteronError>;
