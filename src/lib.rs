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
pub mod authorization;
#[cfg(feature = "ban-manager")]
pub mod ban_manager;
#[cfg(feature = "circuit-breaker")]
pub mod circuit_breaker;
#[cfg(feature = "code-review")]
pub mod code_review;
pub mod config;
#[cfg(feature = "config-security")]
pub mod config_security;
#[cfg(feature = "config-watcher")]
pub mod config_watcher;
#[cfg(feature = "config-security")]
pub use config_security::{ConfigSecurityReport, ConfigSecurityValidator};
// config_loader 需要 confers 特性
#[cfg(feature = "confers")]
pub mod config_loader;

#[cfg(feature = "postgres")]
pub mod adapters;

// DBNexus Storage Adapters (requires postgres feature)
#[cfg(feature = "postgres")]
pub use adapters::{
    create_ban_storage_from_dsn, create_quota_storage_from_dsn, create_storage_from_dsn,
    DBNexusBanStorageAdapter, DBNexusQuotaStorageAdapter, DBNexusStorageAdapter, StorageFactory,
    StorageFactoryConfig, StorageType,
};

#[cfg(feature = "cache-service")]
pub mod cache;
pub mod constants;
#[cfg(feature = "custom-limiter")]
pub mod custom_limiter;
#[cfg(feature = "postgres")]
pub mod dbnexus_entities;
pub mod decision_chain;
pub mod error;
pub mod error_abstraction;
pub mod factory;
#[cfg(feature = "fallback")]
pub mod fallback;
pub mod governor;
pub mod l1_cache;
pub mod limiter_manager;
pub mod limiters;
pub mod log_redaction;
#[cfg(feature = "macros")]
pub mod macros;
pub mod matchers;
#[cfg(feature = "lua-script")]
pub mod oxcache_lua;
#[cfg(feature = "parallel-checker")]
pub mod parallel_ban_checker;
#[cfg(feature = "quota-control")]
pub mod quota_controller;
pub mod rule_builder;
pub mod stats_manager;
pub mod storage_trait;
#[cfg(any(feature = "telemetry", feature = "monitoring"))]
pub mod telemetry;
pub mod validation;

// 重新导出常用类型
#[cfg(feature = "audit-log")]
pub use audit_log::{AuditEvent, AuditLogConfig, AuditLogStats, AuditLogger};
#[cfg(feature = "ban-manager")]
pub use authorization::OperationAuthorizationProvider;
pub use authorization::{
    AllowAllAuthorizationProvider, AuthorizationProvider, DenyAllAuthorizationProvider,
    SimpleAuthorizationProvider,
};
#[cfg(feature = "ban-manager")]
pub use ban_manager::{
    BackoffConfig, BanDetail, BanFilter, BanManager, BanManagerConfig, BanPriority, BanSource,
};
#[cfg(feature = "circuit-breaker")]
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
// 导出配置相关类型和加载API
#[cfg(feature = "cache-service")]
pub use cache::{cache_service::CacheService, Cache, CacheKey, Cacheable};
pub use config::{
    ActionConfig, ChangeSource, ConfigBuilder, ConfigChangeRecord, ConfigHistory,
    FlowControlConfig, LimiterConfig, Matcher as ConfigMatcher, Rule as ConfigRule,
};
#[cfg(feature = "confers")]
pub use config_loader::ConfigLoader;
#[cfg(feature = "config-watcher")]
pub use config_watcher::{ConfigChangeCallback, ConfigWatcher, WatchMode};
#[cfg(feature = "custom-limiter")]
pub use custom_limiter::{
    CustomLimiter, CustomLimiterRegistry, CustomTokenBucketLimiter, LeakyBucketLimiter,
    LimiterStats,
};
pub use decision_chain::{ChainStats, DecisionChain, DecisionChainBuilder, DecisionNode};
// AtomicChainStats 改为 pub(crate)，不再公开导出
pub use error::{
    BanInfo, CircuitBreakerStats, CircuitState, ConsumeResult, Decision, FlowGuardError,
    StorageError,
};
pub use factory::LimiterFactory;
#[cfg(feature = "fallback")]
pub use fallback::{ComponentType, FallbackConfig, FallbackManager, FallbackStrategy};
pub use governor::{Governor, GovernorStats};
pub use l1_cache::{
    CacheableBanInfo, CacheableDecision, L1Cache, L1CacheConfig, L1CacheStats, RateLimitCacheKey,
};
// GLOBAL_LIMITER_MANAGER 改为 pub(crate)，不再公开导出
#[cfg(feature = "quota-control")]
pub use limiters::QuotaLimiter;
#[cfg(feature = "macros")]
pub use macros::{
    flow_control, parse_quota_limit, parse_rate_limit, FlowControlConfig as MacroFlowControlConfig,
    QuotaLimit, RateLimit,
};
pub use matchers::custom::{
    CustomMatcher, CustomMatcherRegistry, CustomMatcherRegistryBuilder, HeaderMatcher,
    HeaderMatcherBuilder, TimeWindowMatcher, TimeWindowMatcherBuilder,
};
#[cfg(feature = "device-matching")]
pub use matchers::device::{
    DeviceCacheStats, DeviceCondition, DeviceInfo, DeviceMatcher, DeviceMatcherBuilder, DeviceType,
};
pub use matchers::{
    ApiKeyExtractor, ApiKeyExtractorBuilder, CompositeCondition, CompositeExtractor,
    ConditionEvaluator, CustomExtractor, DeviceIdExtractor, DeviceIdExtractorBuilder, Identifier,
    IdentifierExtractor, IpExtractor, IpExtractorBuilder, IpRange, LogicalOperator, MacExtractor,
    MacExtractorBuilder, MatchCondition, MatcherStats, RequestContext, Rule, RuleMatcher,
    UserIdExtractor, UserIdExtractorBuilder,
};
#[cfg(feature = "geo-matching")]
pub use matchers::{GeoCacheStats, GeoCondition, GeoInfo, GeoMatcher};
#[cfg(feature = "quota-control")]
#[cfg(feature = "telemetry")]
pub use telemetry::{init_telemetry, TelemetryConfig, Tracer};
#[cfg(feature = "monitoring")]
pub use telemetry::{set_global_metrics, try_global, Metrics};
#[cfg(feature = "validation")]
pub use validation::{
    validate_api_key, validate_ban_reason, validate_ban_target, validate_header_value,
    validate_ip_address, validate_length, validate_mac_address, validate_path, validate_user_id,
};

#[cfg(feature = "lua-script")]
pub use oxcache_lua::{
    execute_cached_script, execute_lua_script, load_script, LuaScriptInfo, LuaScriptType,
    OxcacheLuaManager, FIXED_WINDOW_SCRIPT, QUOTA_CONSUME_SCRIPT, QUOTA_RESET_SCRIPT,
    SLIDING_WINDOW_SCRIPT, TOKEN_BUCKET_SCRIPT,
};

// Re-export storage traits for compatibility
pub use storage_trait::{
    BanHistory, BanRecord, BanStorage, BanTarget, QuotaInfo, QuotaStorage, Storage,
};

// Re-export rule builder
pub use rule_builder::RuleBuilder;

// Re-export stats manager
pub use stats_manager::{StatsManager, StatsSnapshot};
