//! Limiteron - 统一流量治理框架
//!
//! 提供限流、配额、速率控制和封禁管理功能。
//!
//! # 特性
//!
//! - **多种限流算法**：令牌桶、滑动窗口、固定窗口、并发控制
//! - **配额管理**：支持周期性配额和配额告警
//! - **封禁管理**：自动封禁和手动封禁
//! - **声明式宏**：使用 `#[flow_control]` 宏简化限流配置
//! - **监控追踪**：集成Prometheus指标和OpenTelemetry追踪
//! - **高性能**：零运行时开销（编译期优化）
//!
//! # 快速开始
//!
//! ```rust
//! use limiteron::flow_control;
//!
//! #[flow_control(rate = "100/s")]
//! async fn my_api_function(user_id: &str) -> String {
//!     format!("Hello, {}", user_id)
//! }
//! ```
//!
//! # 模块
//!
//! - `ban_manager`: 封禁管理器
//! - `config`: 配置管理
//! - `decision_chain`: 决策链
//! - `error`: 错误类型
//! - `governor`: 主控制器
//! - `limiters`: 限流器实现
//! - `l2_cache`: L2缓存
//! - `macros`: 宏定义
//! - `matchers`: 标识符匹配
//! - `postgres_storage`: PostgreSQL存储
//! - `quota_controller`: 配额控制
//! - `storage`: 存储接口
//! - `telemetry`: 监控和追踪

pub mod audit_log;
#[cfg(feature = "ban-manager")]
pub mod ban_manager;
pub mod cache;
#[cfg(feature = "circuit-breaker")]
pub mod circuit_breaker;
pub mod code_review;
pub mod config;
pub mod config_security;
#[cfg(feature = "config-watcher")]
pub mod config_watcher;
pub mod custom_limiter;
pub mod decision_chain;
pub mod error;
pub mod error_abstraction;
pub mod factory;
pub mod fallback;
pub mod governor;
pub mod limiter_manager;
pub mod limiters;
pub mod log_redaction;
#[cfg(feature = "redis")]
pub mod lua_scripts;
#[cfg(feature = "macros")]
pub mod macros;
pub mod matchers;
pub mod parallel_ban_checker;
#[cfg(feature = "postgres")]
pub mod postgres_storage;
#[cfg(feature = "quota-control")]
pub mod quota_controller;
#[cfg(feature = "redis")]
pub mod redis_storage;
pub mod storage;
#[cfg(any(feature = "telemetry", feature = "monitoring"))]
pub mod telemetry;

// 重新导出常用类型
#[cfg(feature = "audit-log")]
pub use audit_log::{AuditEvent, AuditLogConfig, AuditLogStats, AuditLogger};
#[cfg(feature = "ban-manager")]
pub use ban_manager::{
    BackoffConfig, BanDetail, BanFilter, BanManager, BanManagerConfig, BanPriority, BanSource,
};
pub use cache::{L2Cache, L2CacheConfig, SmartCacheStrategy};
#[cfg(feature = "redis")]
pub use cache::{L3Cache, L3CacheConfig, L3CacheStats};
#[cfg(feature = "circuit-breaker")]
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
pub use code_review::{
    CodeReviewConfig, CodeReviewIssue, CodeReviewManager, CodeReviewReport, CodeReviewStats,
    IssueCategory, ReviewConclusion, ReviewStatus, ReviewSummary, Severity,
};
pub use config::{
    ActionConfig, ChangeSource, ConfigChangeRecord, ConfigHistory, FlowControlConfig,
    LimiterConfig, Matcher as ConfigMatcher, Rule as ConfigRule,
};
#[cfg(feature = "config-watcher")]
pub use config_watcher::{ConfigChangeCallback, ConfigWatcher, PostgresConfigStorage, WatchMode};
pub use custom_limiter::{
    CustomLimiter, CustomLimiterRegistry, LeakyBucketLimiter, LimiterStats, TokenBucketLimiter,
};
pub use decision_chain::{ChainStats, DecisionChain, DecisionChainBuilder, DecisionNode};
pub use error::{
    BanInfo, CircuitBreakerStats, CircuitState, ConsumeResult, Decision, FlowGuardError,
    StorageError,
};
pub use factory::LimiterFactory;
pub use fallback::{ComponentType, FallbackConfig, FallbackManager, FallbackStrategy};
pub use governor::{Governor, GovernorStats};
pub use limiter_manager::GLOBAL_LIMITER_MANAGER;
#[cfg(feature = "redis")]
pub use lua_scripts::{LuaScriptInfo, LuaScriptManager, LuaScriptType};
#[cfg(feature = "macros")]
pub use macros::{
    flow_control, parse_quota_limit, parse_rate_limit, FlowControlConfig as MacroFlowControlConfig,
    QuotaLimit, RateLimit,
};
pub use matchers::{
    ApiKeyExtractor, CompositeCondition, CompositeExtractor, ConditionEvaluator, CustomExtractor,
    DeviceIdExtractor, Identifier, IdentifierExtractor, IpExtractor, IpRange, LogicalOperator,
    MacExtractor, MatchCondition, MatcherStats, RequestContext, Rule, RuleMatcher, UserIdExtractor,
};
pub use matchers::{CustomMatcher, CustomMatcherRegistry, HeaderMatcher, TimeWindowMatcher};
#[cfg(feature = "device-matching")]
pub use matchers::{DeviceCacheStats, DeviceCondition, DeviceInfo, DeviceMatcher, DeviceType};
#[cfg(feature = "geo-matching")]
pub use matchers::{GeoCacheStats, GeoCondition, GeoInfo, GeoMatcher};
#[cfg(feature = "postgres")]
pub use postgres_storage::{PostgresStorage, PostgresStorageConfig};
#[cfg(feature = "quota-control")]
pub use quota_controller::{
    AlertChannel, AlertConfig, AlertInfo, QuotaConfig, QuotaController, QuotaState, QuotaType,
};
#[cfg(feature = "redis")]
pub use redis_storage::{RedisConfig, RedisStorage, RetryStats};
pub use storage::{BanConfig, BanRecord, BanScope, BanStorage, BanTarget, QuotaStorage, Storage};
#[cfg(feature = "telemetry")]
pub use telemetry::{init_telemetry, TelemetryConfig, Tracer};
#[cfg(feature = "monitoring")]
pub use telemetry::{set_global_metrics, try_global, Metrics};
