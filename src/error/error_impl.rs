// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Error 类型的 impl 块和单元测试
//!
//! 从 `mod.rs` 拆分而来，包含所有错误相关类型的实现逻辑。

use super::*;

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
        assert_eq!(error.to_string(), "配置错误: 测试错误");
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
            "限流错误: x"
        );
        assert_eq!(
            LimiteronError::BanError("x".into()).to_string(),
            "封禁错误: x"
        );
        assert_eq!(
            LimiteronError::CircuitBreakerError("x".into()).to_string(),
            "熔断器错误: x"
        );
        assert_eq!(
            LimiteronError::FallbackError("x".into()).to_string(),
            "降级错误: x"
        );
        assert_eq!(
            LimiteronError::AuditLogError("x".into()).to_string(),
            "审计日志错误: x"
        );
        assert_eq!(
            LimiteronError::AuthorizationError("x".into()).to_string(),
            "授权错误: x"
        );
        assert_eq!(
            LimiteronError::RateLimitExceeded("x".into()).to_string(),
            "速率限制超出: x"
        );
        assert_eq!(
            LimiteronError::QuotaExceeded("x".into()).to_string(),
            "配额超出: x"
        );
        assert_eq!(
            LimiteronError::ConcurrencyLimitExceeded("x".into()).to_string(),
            "并发限制超出: x"
        );
        assert_eq!(
            LimiteronError::ValidationError("x".into()).to_string(),
            "验证错误: x"
        );
        assert_eq!(
            LimiteronError::LockError("x".into()).to_string(),
            "锁获取错误: x"
        );
        assert_eq!(
            LimiteronError::TimeError("x".into()).to_string(),
            "时间错误: x"
        );
        assert_eq!(
            LimiteronError::DependencyError("x".into()).to_string(),
            "依赖缺失: x"
        );
        assert_eq!(LimiteronError::Other("x".into()).to_string(), "未知错误: x");
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
