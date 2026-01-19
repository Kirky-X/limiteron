//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! Limiteron - Unified Flow Control Framework
//!
//! Provides rate limiting, quota management, circuit breaking, and ban control.
//!
//! # API Layers
//!
//! ## Prelude (Quick Start)
//!
//! Use `use limiteron::prelude::*;` to import all commonly used types.
//!
//! ## Core API
//!
//! - [`Governor`] - Main controller for flow control
//! - [`FlowControlConfig`] - Configuration for flow control
//! - [`Decision`] - Decision result from flow control checks
//! - [`FlowGuardError`] - Error types
//!
//! ## Matchers
//!
//! Identifier extractors: IP, User ID, Device ID, API Key, etc.
//!
//! ## Limiters
//!
//! Low-level rate limiting algorithms: Token bucket, sliding window, fixed window.
//!
//! ## Extensions (feature-gated)
//!
//! - Ban management (requires `ban-manager` feature)
//! - Circuit breaker (requires `circuit-breaker` feature)
//! - Quota control (requires `quota-control` feature)
//! - Macros (requires `macros` feature)
//!
//! # Examples
//!
//! ```rust
//! use limiteron::prelude::*;
//! use limiteron::limiters::{TokenBucketLimiter, Limiter};
//!
//! #[tokio::main]
//! async fn main() {
//!     // 创建一个简单的令牌桶限流器
//!     let limiter = TokenBucketLimiter::new(100, 10);
//!
//!     // 检查请求是否被允许
//!     let decision = limiter.allow(1).await.unwrap();
//!     assert!(decision);
//! }
//! ```
//!
//! # Features
//!
//! - **Multiple rate limiting algorithms**: Token bucket, sliding window, fixed window, concurrency control
//! - **Ban management**: Automatic and manual ban management with priority support
//! - **Quota control**: Periodic quota allocation and alerting
//! - **Circuit breaker**: Automatic failover and state recovery
//! - **Declarative macros**: Use `#[flow_control]` macro to simplify rate limiting configuration
//! - **Monitoring**: Integrated Prometheus metrics and OpenTelemetry tracing
//! - **High performance**: Zero runtime overhead through compile-time optimization

pub mod prelude;

pub mod audit_log;
#[cfg(feature = "ban-manager")]
pub mod ban_manager;
pub mod cache;
#[cfg(feature = "circuit-breaker")]
pub mod circuit_breaker;
#[cfg(feature = "code-review")]
pub mod code_review;
pub mod config;
#[cfg(feature = "config-security")]
pub mod config_security;
#[cfg(feature = "config-security")]
pub use config_security::{ConfigSecurityReport, ConfigSecurityValidator};
#[cfg(feature = "config-watcher")]
pub mod config_watcher;
pub mod constants;
#[cfg(feature = "custom-limiter")]
pub mod custom_limiter;
pub mod decision_chain;
pub mod error;
pub mod error_abstraction;
pub mod factory;
#[cfg(feature = "fallback")]
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
#[cfg(feature = "parallel-checker")]
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
#[cfg(feature = "code-review")]
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
#[cfg(feature = "custom-limiter")]
pub use custom_limiter::{
    CustomLimiter, CustomLimiterRegistry, LeakyBucketLimiter, LimiterStats, TokenBucketLimiter,
};
pub use decision_chain::{ChainStats, DecisionChain, DecisionChainBuilder, DecisionNode};
pub use error::{
    BanInfo, CircuitBreakerStats, CircuitState, ConsumeResult, Decision, FlowGuardError,
    StorageError,
};
pub use factory::LimiterFactory;
#[cfg(feature = "fallback")]
pub use fallback::{ComponentType, FallbackConfig, FallbackManager, FallbackStrategy};
pub use governor::{Governor, GovernorStats};
pub use limiter_manager::GLOBAL_LIMITER_MANAGER;
#[cfg(feature = "quota-control")]
pub use limiters::QuotaLimiter;
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
