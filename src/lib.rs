// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
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
//! - [`LimiteronError`] - Error types
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
//! - Distributed rate limiting (requires `distributed` feature)
//! - trait-kit AsyncKit integration (requires `kit` feature)
//! - Internationalization (requires `i18n` feature)
//! - inklog structured logging (requires `inklog` feature)
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
//! - **Distributed rate limiting**: `DistributedLimiter` trait + `InMemoryDistributedLimiter` implementation (requires `distributed` feature)
//! - **trait-kit integration**: AsyncKit `LimiteronModule` (requires `kit` feature)
//! - **Internationalization**: Locale-aware number/date/plural/sort formatting (requires `i18n` feature)
//! - **inklog logging**: inklog structured logging integration (requires `inklog` feature)

#![allow(clippy::collapsible_if)]

pub mod prelude;

#[cfg(feature = "admin-api")]
pub mod admin;

pub mod authorization;
#[cfg(feature = "ban-manager")]
pub mod ban;
#[cfg(feature = "circuit-breaker")]
pub mod circuit;
mod clock;
pub mod config;

#[cfg(feature = "postgres")]
pub mod adapters;

// DBNexus Storage Adapters (requires postgres feature)
#[cfg(feature = "postgres")]
pub use adapters::{
    DBNexusBanStorageAdapter, DBNexusQuotaStorageAdapter, DBNexusStorageAdapter, StorageFactory,
    StorageFactoryConfig, StorageType, create_ban_storage_from_dsn, create_quota_storage_from_dsn,
    create_storage_from_dsn,
};

#[cfg(feature = "cache-service")]
pub mod cache;
pub(crate) mod constants;
#[cfg(feature = "postgres")]
mod dbnexus_entities;
#[cfg(feature = "postgres")]
pub use dbnexus_entities::create_all_tables_ddl;
pub mod decision_chain;

// Event system (feature-gated)
#[cfg(feature = "event-system")]
mod events;

// Consolidated modules
pub mod error; // Contains error types and abstraction
pub mod limiters; // Contains limiters, factory, and manager
pub mod logging; // Contains audit_log and log_redaction
mod rules; // Contains rule_builder and stats_manager
pub mod storage; // Contains storage_trait and parallel_ban_checker

#[cfg(feature = "fallback")]
pub mod fallback;
mod governor;
mod l1_cache;
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
mod tenant;
pub mod validation;
#[cfg(feature = "webhook")]
pub(crate) mod webhook_validator;

// ICU4X 国际化格式化 (feature-gated). 提供 locale 感知的限流消息/数字/日期/复数/排序格式化。
// Mirrors trait-kit/oxcache i18n pattern.
#[cfg(feature = "i18n")]
pub mod i18n;

// External integrations (feature-gated). Each integration lives under
// `integrations/` and is gated by its own feature so the core limiteron
// library stays dependency-free when integrations are not needed.
#[cfg(any(feature = "kit", feature = "inklog"))]
pub mod integrations;

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
    BackoffConfig, BanDetail, BanFile, BanFileEntry, BanFileLoader, BanFilter, BanLoadError,
    BanManager, BanManagerBuilder, BanManagerConfig, BanPriority, BanSource, LoadResult,
};
#[cfg(feature = "circuit-breaker")]
pub use circuit::{CircuitBreaker, CircuitBreakerConfig};
#[cfg(feature = "audit-log")]
pub use logging::audit::{AuditEvent, AuditLogConfig, AuditLogStats, AuditLogger};
// 导出配置相关类型
#[cfg(feature = "cache-service")]
pub use cache::{Cache, CacheKey, CacheService};
pub use config::ConfigLoader;
pub use config::{
    ActionConfig, ChangeSource, ConfigChangeRecord, ConfigHistory, ConfigMatcher,
    FlowControlConfig, LimiterConfig, Rule as ConfigRule,
};
pub use decision_chain::{ChainStats, DecisionChain, DecisionChainBuilder, DecisionNode};
// AtomicChainStats 改为 pub(crate)，不再公开导出
pub use error::{
    BanInfo, CircuitBreakerStats, CircuitState, ConsumeResult, Decision, LimiteronError,
    LimiteronResult, StorageError,
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
pub use governor::{Governor, GovernorStats, HealthStatus};
pub use l1_cache::{L1Cache, L1CacheConfig, RateLimitCacheKey};
pub use limiters::Limiter;
#[cfg(feature = "quota-control")]
pub use limiters::QuotaLimiter;
#[cfg(feature = "macros")]
pub use macros::{
    FlowControlConfig as MacroFlowControlConfig, QuotaLimit, RateLimit, flow_control,
    parse_quota_limit, parse_rate_limit,
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
pub use quota::QuotaController;
#[cfg(feature = "monitoring")]
pub use telemetry::{Metrics, set_global_metrics, try_global};
#[cfg(feature = "telemetry")]
pub use telemetry::{TelemetryConfig, Tracer, init_telemetry};
#[cfg(feature = "validation")]
pub use validation::{
    validate_api_key, validate_ban_reason, validate_ban_target, validate_header_value,
    validate_ip_address, validate_length, validate_mac_address, validate_path, validate_user_id,
};

#[cfg(feature = "lua-script")]
pub use oxcache_lua::{
    FIXED_WINDOW_SCRIPT, LuaScriptInfo, LuaScriptType, OxcacheLuaManager, QUOTA_CONSUME_SCRIPT,
    QUOTA_RESET_SCRIPT, SLIDING_WINDOW_SCRIPT, TOKEN_BUCKET_SCRIPT, execute_cached_script,
    execute_lua_script, load_script,
};

// Re-export storage traits for compatibility (internal implementations are pub(crate))
pub use storage::{BanHistory, BanRecord, BanStorage, BanTarget, QuotaInfo, QuotaStorage, Storage};

#[cfg(feature = "parallel-checker")]
pub use storage::ParallelBanChecker;

// Re-export CacheStorage (feature-gated)
#[cfg(feature = "cache-storage")]
pub use cache::CacheBanStorage;
#[cfg(feature = "cache-storage")]
pub use cache::CacheQuotaStorage;
#[cfg(feature = "cache-storage")]
pub use cache::CacheStorage;

// Re-export GCRA limiter (feature-gated)
#[cfg(feature = "gcra")]
pub use limiters::GcraLimiter;

// Re-export logging types for compatibility
pub use logging::{redact_basic, redact_email, redact_ip, redact_user_id};

#[cfg(feature = "log-redaction")]
pub use logging::{RedactionConfig, contains_sensitive_info, redact_advanced, redact_http_content};

// Re-export rule builder
pub use rules::RuleBuilder;

// Re-export stats manager
pub use rules::{StatsManager, StatsSnapshot};

// Re-export clock types
pub use clock::{Clock, MockClock, SystemClock};

// Re-export tenant types (feature-gated)
#[cfg(feature = "multi-tenant")]
pub use tenant::{DefaultTenantResolver, HeaderTenantResolver, Namespace, TenantResolver};

// Re-export middleware types (feature-gated)
#[cfg(feature = "tower-middleware")]
pub use middleware::{
    IntoRequestContext, RateLimitConfig, RateLimitHeaderValues, RateLimitLayer, RateLimitService,
    inject_rate_limit_headers,
};

// Re-export underlying dependencies used in public API and trait definitions.
//
// Scope: only type references (L1) — e.g. `use limiteron::oxcache::Cache`,
// `use limiteron::tokio::sync::Mutex`, `use limiteron::async_trait::async_trait`.
// Macro attributes that expand to canonical crate paths (L2, e.g.
// `#[tokio::main]` expands to `tokio::runtime::...`, `#[derive(serde::Serialize)]`
// expands to `impl serde::Serialize`) and macro invocations (L3, e.g.
// `tokio::spawn`) reference absolute crate paths at expansion time and cannot
// be routed through a re-export alias; downstream crates must still declare
// direct dependencies for those uses (e.g. benches/regression.rs keeps
// `use serde::{Deserialize, Serialize}` because serde is not re-exported and
// derive macros need the canonical path). Note: `#[async_trait]` is an
// attribute macro whose expansion does NOT reference `async_trait::` paths, so
// it can be used via the re-export (`use limiteron::async_trait::async_trait`).
// This re-export narrows the direct-dependency surface to the macro path only;
// it does not eliminate it.
//
// Each re-export is feature-gated to features that actually pull it in.

// oxcache is a non-optional core dependency.
pub use oxcache;

// async_trait is non-optional; always available.
pub use async_trait;

// chrono is non-optional; always available.
pub use chrono;

// tokio is non-optional core dependency.
pub use tokio;

// tower is optional; re-export only when a feature pulling it in is enabled.
#[cfg(any(feature = "tower-middleware", feature = "admin-api"))]
pub use tower;
