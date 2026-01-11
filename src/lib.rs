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
pub mod ban_manager;
pub mod circuit_breaker;
pub mod code_review;
pub mod config;
pub mod config_watcher;
pub mod custom_limiter;
pub mod custom_matcher;
pub mod decision_chain;
pub mod device_matcher;
pub mod error;
pub mod factory;
pub mod fallback;
pub mod geo_matcher;
pub mod governor;
pub mod l2_cache;
pub mod l3_cache;
pub mod limiter_manager;
pub mod limiters;
pub mod lua_scripts;
pub mod macros;
pub mod matchers;
pub mod parallel_ban_checker;
pub mod postgres_storage;
pub mod quota_controller;
pub mod redis_storage;
pub mod storage;
pub mod telemetry;

// 重新导出常用类型
pub use audit_log::{AuditEvent, AuditLogConfig, AuditLogStats, AuditLogger};
pub use ban_manager::{
    BackoffConfig, BanDetail, BanFilter, BanManager, BanManagerConfig, BanPriority, BanSource,
};
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
pub use code_review::{
    CodeReviewConfig, CodeReviewIssue, CodeReviewManager, CodeReviewReport, CodeReviewStats,
    IssueCategory, ReviewConclusion, ReviewStatus, ReviewSummary, Severity,
};
pub use config::{
    ActionConfig, ChangeSource, ConfigChangeRecord, ConfigHistory, FlowControlConfig,
    LimiterConfig, Matcher as ConfigMatcher, Rule as ConfigRule,
};
pub use config_watcher::{ConfigChangeCallback, ConfigWatcher, PostgresConfigStorage, WatchMode};
pub use custom_limiter::{
    CustomLimiter, CustomLimiterRegistry, LeakyBucketLimiter, LimiterStats, TokenBucketLimiter,
};
pub use custom_matcher::{CustomMatcher, CustomMatcherRegistry, HeaderMatcher, TimeWindowMatcher};
pub use decision_chain::{ChainStats, DecisionChain, DecisionChainBuilder, DecisionNode};
pub use device_matcher::{
    DeviceCacheStats, DeviceCondition, DeviceInfo, DeviceMatcher, DeviceType,
};
pub use error::{
    BanInfo, CircuitBreakerStats, CircuitState, ConsumeResult, Decision, FlowGuardError,
    StorageError,
};
pub use factory::LimiterFactory;
pub use fallback::{ComponentType, FallbackConfig, FallbackManager, FallbackStrategy};
pub use geo_matcher::{GeoCacheStats, GeoCondition, GeoInfo, GeoMatcher};
pub use governor::{Governor, GovernorStats};
pub use l2_cache::{CacheEntry, CacheStats, L2Cache, L2CacheConfig};
pub use l3_cache::{L3Cache, L3CacheConfig, L3CacheStats};
pub use limiter_manager::GLOBAL_LIMITER_MANAGER;
pub use lua_scripts::{LuaScriptInfo, LuaScriptManager, LuaScriptType};
pub use macros::{
    flow_control, parse_quota_limit, parse_rate_limit, FlowControlConfig as MacroFlowControlConfig,
    QuotaLimit, RateLimit,
};
pub use matchers::{
    ApiKeyExtractor, CompositeCondition, CompositeExtractor, ConditionEvaluator, CustomExtractor,
    DeviceIdExtractor, Identifier, IdentifierExtractor, IpExtractor, IpRange, LogicalOperator,
    MacExtractor, MatchCondition, MatcherStats, RequestContext, Rule, RuleMatcher, UserIdExtractor,
};
pub use postgres_storage::{PostgresStorage, PostgresStorageConfig};
pub use quota_controller::{
    AlertChannel, AlertConfig, AlertInfo, QuotaConfig, QuotaController, QuotaState, QuotaType,
};
pub use redis_storage::{RedisConfig, RedisStorage, RetryStats};
pub use storage::{BanConfig, BanRecord, BanScope, BanStorage, BanTarget, QuotaStorage, Storage};
pub use telemetry::{
    init_telemetry, set_global_metrics, try_global, Metrics, TelemetryConfig, Tracer,
};
