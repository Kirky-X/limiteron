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

#[cfg(feature = "admin-api")]
pub mod admin;

pub mod authorization;
#[cfg(feature = "ban-manager")]
pub mod ban;
#[cfg(feature = "circuit-breaker")]
pub mod circuit;
pub mod clock;
pub mod config;

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
pub(crate) mod constants;
#[cfg(feature = "postgres")]
pub mod dbnexus_entities;
pub mod decision_chain;

// Event system (feature-gated)
#[cfg(feature = "event-system")]
pub mod events;

// Consolidated modules
pub mod error; // Contains error types and abstraction
pub(crate) mod limiters; // Contains limiters, factory, and manager
pub mod logging; // Contains audit_log and log_redaction
pub mod rules; // Contains rule_builder and stats_manager
pub(crate) mod storage; // Contains storage_trait and parallel_ban_checker

#[cfg(feature = "redis-storage")]
pub mod redis;

#[cfg(feature = "fallback")]
pub mod fallback;
pub mod governor;
pub(crate) mod l1_cache;
#[cfg(feature = "macros")]
pub mod macros;
pub mod matchers;
#[cfg(feature = "lua-script")]
pub mod oxcache_lua;
#[cfg(feature = "quota-control")]
pub mod quota;
#[cfg(any(feature = "telemetry", feature = "monitoring"))]
pub mod telemetry;
#[cfg(feature = "multi-tenant")]
pub mod tenant;
pub mod validation;
#[cfg(feature = "webhook")]
pub(crate) mod webhook_validator;

// Tower 中间件层 (feature-gated)
#[cfg(feature = "tower-middleware")]
pub mod middleware;

// 重新导出常用类型
#[cfg(feature = "ban-manager")]
pub use authorization::OperationAuthorizationProvider;
#[cfg(test)]
pub use authorization::{AllowAllAuthorizationProvider, DenyAllAuthorizationProvider};
pub use authorization::{AuthorizationProvider, SimpleAuthorizationProvider};
#[cfg(feature = "ban-manager")]
pub use ban::{
    BackoffConfig, BanDetail, BanFilter, BanManager, BanManagerConfig, BanPriority, BanSource,
};
#[cfg(feature = "circuit-breaker")]
pub use circuit::{CircuitBreaker, CircuitBreakerConfig};
#[cfg(feature = "audit-log")]
pub use logging::audit::{AuditEvent, AuditLogConfig, AuditLogStats, AuditLogger};
// 导出配置相关类型
#[cfg(feature = "cache-service")]
pub use cache::{cache_service::CacheService, Cache, CacheKey, Cacheable};
pub use config::{
    ActionConfig, ChangeSource, ConfigChangeRecord, ConfigHistory, ConfigMatcher,
    FlowControlConfig, LimiterConfig, Rule as ConfigRule,
};
// 导出 confers ConfigBuilder（当启用 confers feature 时）
#[cfg(feature = "confers")]
pub use config::{ConfigBuilder, ConfigLoader};
pub use decision_chain::{ChainStats, DecisionChain, DecisionChainBuilder, DecisionNode};
// AtomicChainStats 改为 pub(crate)，不再公开导出
pub use error::{
    BanInfo, CircuitBreakerStats, CircuitState, ConsumeResult, Decision, FlowGuardError,
    StorageError,
};
// Event system types (feature-gated)
#[cfg(feature = "event-system")]
pub use events::{Event, EventConfig, EventDispatcher, EventEmitter, EventHandler, EventType};
// Error abstraction types
pub use error::{
    BanSafeError, ConfigSafeError, ErrorMessageAbstraction, GeneralSafeError, LimitSafeError,
    SafeErrorMessage, StorageSafeError, ValidationSafeError,
};
#[cfg(feature = "fallback")]
pub use fallback::{ComponentType, FallbackConfig, FallbackManager, FallbackStrategy};
pub use governor::{Governor, GovernorStats};
pub use l1_cache::{L1Cache, L1CacheConfig, RateLimitCacheKey};
pub use limiters::Limiter;
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

// Re-export storage traits for compatibility (internal implementations are pub(crate))
pub use storage::{
    BanHistory, BanRecord, BanStorage, BanStorageCreate, BanTarget, QuotaInfo, QuotaStorage,
    Storage, StorageCreate,
};

#[cfg(feature = "parallel-checker")]
pub use storage::ParallelBanChecker;

// Re-export Redis types (feature-gated)
#[cfg(feature = "redis-storage")]
pub use redis::{
    execute_gcra, execute_gcra_with_sha, load_gcra_script, GcraResult, RedisStorage, ScriptManager,
    ScriptType,
};

// Re-export GCRA limiter (feature-gated)
#[cfg(feature = "gcra")]
pub use limiters::GcraLimiter;

// Re-export logging types for compatibility
pub use logging::{redact_basic, redact_email, redact_ip, redact_user_id};

#[cfg(feature = "log-redaction")]
pub use logging::{contains_sensitive_info, redact_advanced, redact_http_content, RedactionConfig};

// Re-export rule builder
pub use rules::RuleBuilder;

// Re-export stats manager
pub use rules::{StatsManager, StatsSnapshot};

// Re-export clock types
pub use clock::{Clock, MockClock, SystemClock};

// Re-export tenant types (feature-gated)
#[cfg(feature = "multi-tenant")]
pub use tenant::{Namespace, TenantResolver};

// Re-export middleware types (feature-gated)
#[cfg(feature = "tower-middleware")]
pub use middleware::{
    inject_rate_limit_headers, IntoRequestContext, RateLimitConfig, RateLimitHeaderValues,
    RateLimitLayer, RateLimitService,
};
