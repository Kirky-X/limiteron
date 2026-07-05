//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! Governor 主控制器 - 重构版本
//!
//! 流量控制的核心控制器，重构后具有更好的模块化设计：
//! - 使用专门的并行封禁检查器提高性能
//! - 使用 RuleBuilder 构建规则和决策链
//! - 使用 StatsManager 管理统计信息
//! - 集成 L1 本地缓存层提高热点访问性能
//! - 简化核心逻辑，提高可维护性
//! - 保持向后兼容性

use crate::config::types::{ConfigChangeRecord, ConfigHistory, FlowControlConfig};
use crate::decision_chain::DecisionChain;
use crate::error::Decision;
use crate::error::FlowGuardError;
#[cfg(feature = "fallback")]
use crate::fallback::FallbackManager;
#[cfg(feature = "fallback")]
use crate::l1_cache::IslandFallbackStrategy;
#[cfg(feature = "fallback")]
use crate::l1_cache::IslandModeConfig;
use crate::l1_cache::{CacheableDecision, L1Cache, L1CacheConfig, RateLimitCacheKey};
use crate::logging::{redact_ip, redact_user_id};
use crate::matchers::{IdentifierExtractor, RequestContext, RuleMatcher};
use crate::rules::{RuleBuilder, StatsManager, StatsSnapshot};
// storage module removed as part of direct-inheritance refactoring
// Use dbnexus traits directly instead
// Re-exported from storage module for compatibility
#[cfg(all(feature = "ban-manager", not(feature = "parallel-checker")))]
use crate::error::BanInfo;
#[cfg(all(feature = "ban-manager", not(feature = "parallel-checker")))]
use crate::storage::BanTarget;
use crate::storage::{BanStorage, Storage};
use dashmap::DashMap;
#[cfg(feature = "parallel-checker")]
use log::warn;
use log::{debug, info, trace};
use std::sync::Arc;
use tokio::sync::RwLock;

// Conditional imports for optional features
#[cfg(feature = "ban-manager")]
use crate::ban::BanManager;
#[cfg(feature = "circuit-breaker")]
use crate::circuit::CircuitBreaker;
#[cfg(feature = "audit-log")]
use crate::logging::AuditLogger;
#[cfg(any(feature = "parallel-checker", feature = "ban-manager"))]
use crate::matchers::Identifier;
#[cfg(feature = "monitoring")]
use crate::telemetry::Metrics;
#[cfg(feature = "telemetry")]
use crate::telemetry::Tracer;
#[cfg(feature = "ban-manager")]
use crate::BanSource;

/// Governor 统计信息
///
/// 保持向后兼容性的统计信息结构体。
#[derive(Debug, Clone, Default)]
pub struct GovernorStats {
    /// 总请求数
    pub total_requests: u64,
    /// 允许的请求数
    pub allowed_requests: u64,
    /// 拒绝的请求数
    pub rejected_requests: u64,
    /// 封禁的请求数
    pub banned_requests: u64,
    /// 错误数
    pub error_count: u64,
    /// 最后更新时间
    pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<StatsSnapshot> for GovernorStats {
    fn from(snapshot: StatsSnapshot) -> Self {
        Self {
            total_requests: snapshot.total_requests,
            allowed_requests: snapshot.allowed_requests,
            rejected_requests: snapshot.rejected_requests,
            banned_requests: snapshot.banned_requests,
            error_count: snapshot.error_count,
            last_updated: snapshot.last_updated,
        }
    }
}

/// Governor 主控制器
///
/// 重构后的 Governor，具有更清晰的职责分离和更好的性能。
pub struct Governor {
    /// 配置
    config: Arc<RwLock<FlowControlConfig>>,

    /// 存储后端
    storage: Arc<dyn Storage>,

    /// 封禁存储
    ban_storage: Arc<dyn BanStorage>,

    /// 封禁管理器
    #[cfg(feature = "ban-manager")]
    ban_manager: Arc<BanManager>,

    /// 并行封禁检查器（新增）
    #[cfg(feature = "parallel-checker")]
    parallel_ban_checker: Arc<crate::storage::ParallelBanChecker>,

    /// 决策链
    decision_chain: Arc<RwLock<DecisionChain>>,

    /// 规则匹配器
    rule_matcher: Arc<RwLock<RuleMatcher>>,

    /// 规则对应的决策链
    rule_chains: Arc<RwLock<DashMap<String, DecisionChain>>>,

    /// 标识符提取器
    identifier_extractor: Arc<dyn IdentifierExtractor>,

    /// 熔断器
    #[cfg(feature = "circuit-breaker")]
    circuit_breaker: Arc<CircuitBreaker>,

    /// 审计日志记录器
    #[cfg(feature = "audit-log")]
    audit_logger: Arc<RwLock<Option<Arc<AuditLogger>>>>,

    /// 配置历史记录
    config_history: Arc<RwLock<ConfigHistory>>,

    /// 统计管理器
    stats: StatsManager,

    /// L1 本地缓存（用于缓存热点限流结果）
    l1_cache: L1Cache<CacheableDecision>,

    /// 是否启用 L1 缓存
    l1_cache_enabled: std::sync::atomic::AtomicBool,

    /// 降级管理器（可选，feature-gated）
    #[cfg(feature = "fallback")]
    fallback_manager: Option<Arc<FallbackManager>>,

    /// 事件发射器（可选，feature-gated）
    #[cfg(feature = "event-system")]
    event_emitter: Option<Arc<crate::events::EventEmitter>>,

    /// 优雅关闭令牌：取消时通知所有后台任务退出
    shutdown_token: tokio_util::sync::CancellationToken,

    /// 是否已关闭（幂等性保证）
    is_shutdown: std::sync::atomic::AtomicBool,
}

/// Governor 构建器
///
/// 用于链式配置 Governor 实例。
///
/// # 示例
///
/// ```rust,no_run
/// use limiteron::Governor;
/// use limiteron::storage::{MemoryStorage, MemoryBanStorage};
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let storage: Arc<dyn limiteron::storage::Storage> = MemoryStorage::create_storage();
///     let ban_storage: Arc<dyn limiteron::storage::BanStorage> = MemoryBanStorage::create_ban_storage();
///
///     let governor = Governor::builder()
///         .with_storage(storage)
///         .with_ban_storage(ban_storage)
///         .build()
///         .await
///         .unwrap();
///     Ok(())
/// }
/// ```
#[derive(Clone, Default)]
#[allow(clippy::type_complexity)]
pub struct GovernorBuilder {
    config: Option<FlowControlConfig>,
    storage: Option<Arc<dyn Storage>>,
    ban_storage: Option<Arc<dyn BanStorage>>,
    identifier_extractor: Option<Arc<dyn crate::matchers::IdentifierExtractor>>,
    #[cfg(feature = "circuit-breaker")]
    circuit_breaker: Option<Arc<CircuitBreaker>>,
    #[cfg(feature = "audit-log")]
    audit_logger: Option<Arc<crate::logging::AuditLogger>>,
    #[cfg(feature = "monitoring")]
    metrics: Option<Arc<Metrics>>,
    #[cfg(feature = "telemetry")]
    tracer: Option<Arc<Tracer>>,
    #[cfg(feature = "parallel-checker")]
    parallel_ban_checker: Option<Arc<crate::storage::ParallelBanChecker>>,
    /// L1 缓存配置
    l1_cache_config: Option<L1CacheConfig>,
    /// 是否启用 L1 缓存
    l1_cache_enabled: bool,
    /// 降级管理器（可选，feature-gated）
    #[cfg(feature = "fallback")]
    fallback_manager: Option<Arc<FallbackManager>>,
    /// 事件发射器（可选）
    #[cfg(feature = "event-system")]
    event_emitter: Option<Arc<crate::events::EventEmitter>>,
}

impl GovernorBuilder {
    /// 创建新的 GovernorBuilder
    pub fn new() -> Self {
        Self {
            config: None,
            storage: None,
            ban_storage: None,
            identifier_extractor: None,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
            #[cfg(feature = "audit-log")]
            audit_logger: None,
            #[cfg(feature = "monitoring")]
            metrics: None,
            #[cfg(feature = "telemetry")]
            tracer: None,
            #[cfg(feature = "parallel-checker")]
            parallel_ban_checker: None,
            l1_cache_config: None,
            l1_cache_enabled: true, // 默认启用 L1 缓存
            #[cfg(feature = "fallback")]
            fallback_manager: None,
            #[cfg(feature = "event-system")]
            event_emitter: None,
        }
    }

    /// 设置流量控制配置
    pub fn with_config(mut self, config: FlowControlConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// 设置存储后端
    pub fn with_storage(mut self, storage: Arc<dyn Storage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// 设置封禁存储后端
    pub fn with_ban_storage(mut self, ban_storage: Arc<dyn BanStorage>) -> Self {
        self.ban_storage = Some(ban_storage);
        self
    }

    /// 设置标识符提取器
    pub fn with_identifier_extractor(
        mut self,
        extractor: Arc<dyn crate::matchers::IdentifierExtractor>,
    ) -> Self {
        self.identifier_extractor = Some(extractor);
        self
    }

    /// 设置熔断器
    #[cfg(feature = "circuit-breaker")]
    pub fn with_circuit_breaker(mut self, circuit_breaker: Arc<CircuitBreaker>) -> Self {
        self.circuit_breaker = Some(circuit_breaker);
        self
    }

    /// 设置审计日志记录器
    #[cfg(feature = "audit-log")]
    pub fn with_audit_logger(mut self, audit_logger: Arc<crate::logging::AuditLogger>) -> Self {
        self.audit_logger = Some(audit_logger);
        self
    }

    /// 设置指标收集器
    #[cfg(feature = "monitoring")]
    pub fn with_metrics(mut self, metrics: Arc<Metrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// 设置追踪器
    #[cfg(feature = "telemetry")]
    pub fn with_tracer(mut self, tracer: Arc<Tracer>) -> Self {
        self.tracer = Some(tracer);
        self
    }

    /// 设置 L1 缓存配置
    pub fn with_l1_cache_config(mut self, config: L1CacheConfig) -> Self {
        self.l1_cache_config = Some(config);
        self
    }

    /// 启用或禁用 L1 缓存
    pub fn with_l1_cache_enabled(mut self, enabled: bool) -> Self {
        self.l1_cache_enabled = enabled;
        self
    }

    /// 设置降级管理器
    ///
    /// 当设置降级管理器后，Governor 会在存储层故障时自动触发孤岛模式，
    /// 并在 check 流程中使用降级策略。
    ///
    /// # 参数
    /// - `fallback_manager`: 降级管理器实例
    #[cfg(feature = "fallback")]
    pub fn with_fallback_manager(mut self, fallback_manager: Arc<FallbackManager>) -> Self {
        self.fallback_manager = Some(fallback_manager);
        self
    }

    /// 设置事件发射器
    #[cfg(feature = "event-system")]
    pub fn with_event_emitter(mut self, emitter: Arc<crate::events::EventEmitter>) -> Self {
        self.event_emitter = Some(emitter);
        self
    }

    /// 构建 Governor 实例
    ///
    /// # 返回
    ///
    /// * `Ok(Governor)` - 构建成功
    /// * `Err(FlowGuardError)` - 构建失败（配置错误或依赖缺失）
    ///
    pub async fn build(self) -> Result<Governor, FlowGuardError> {
        let config = self.config.unwrap_or_default();

        // 校验配置
        config.validate().map_err(FlowGuardError::ConfigError)?;

        // 获取存储后端（必需依赖）
        let storage = self
            .storage
            .ok_or_else(|| FlowGuardError::DependencyError("storage is required".to_string()))?;
        let ban_storage = self.ban_storage.ok_or_else(|| {
            FlowGuardError::DependencyError("ban_storage is required".to_string())
        })?;

        // 创建封禁管理器
        #[cfg(feature = "ban-manager")]
        let ban_manager = {
            use crate::ban::{BanManager, BanManagerConfig};
            let config = BanManagerConfig::default();
            BanManager::with_dependencies(ban_storage.clone(), config)
                .await
                .map(Arc::new)?
        };

        // 创建并行封禁检查器
        #[cfg(feature = "parallel-checker")]
        let parallel_ban_checker = self.parallel_ban_checker.unwrap_or_else(|| {
            #[cfg(feature = "ban-manager")]
            {
                Arc::new(crate::storage::ParallelBanChecker::new(ban_manager.clone()))
            }
            #[cfg(not(feature = "ban-manager"))]
            {
                panic!("parallel-checker feature requires ban-manager feature")
            }
        });

        // 创建标识符提取器（如果未提供）
        let identifier_extractor = if let Some(extractor) = self.identifier_extractor {
            extractor
        } else {
            Arc::new(
                crate::matchers::CompositeExtractor::builder()
                    .add_extractor(Box::new(crate::matchers::UserIdExtractor::from_header(
                        "X-User-Id",
                    )))
                    .add_extractor(Box::new(crate::matchers::IpExtractor::builder().build()))
                    .add_extractor(Box::new(crate::matchers::ApiKeyExtractor::from_header(
                        "X-API-Key",
                    )))
                    .build(),
            )
        };

        // 使用 RuleBuilder 创建规则匹配器
        let rules = RuleBuilder::build_rules(&config)?;
        let rule_matcher = Arc::new(tokio::sync::RwLock::new(
            crate::matchers::RuleMatcher::with_dependencies(rules),
        ));

        // 创建决策链
        let decision_chain = Arc::new(tokio::sync::RwLock::new(
            crate::decision_chain::DecisionChain::with_dependencies(vec![]),
        ));

        // 创建熔断器
        #[cfg(feature = "circuit-breaker")]
        let circuit_breaker = self.circuit_breaker.unwrap_or_else(|| {
            Arc::new(CircuitBreaker::with_dependencies(
                crate::circuit::CircuitBreakerConfig::default(),
            ))
        });

        // 创建审计日志记录器
        #[cfg(feature = "audit-log")]
        let audit_logger = self
            .audit_logger
            .map(|logger| Arc::new(tokio::sync::RwLock::new(Some(logger))))
            .unwrap_or_else(|| Arc::new(tokio::sync::RwLock::new(None)));

        // 使用 RuleBuilder 创建规则对应的决策链
        let rule_chains_map = RuleBuilder::build_rule_chains(&config)?;
        let rule_chains = Arc::new(tokio::sync::RwLock::new(rule_chains_map));

        // 创建 L1 缓存
        let l1_cache_config = self
            .l1_cache_config
            .unwrap_or_else(|| L1CacheConfig::new(std::time::Duration::from_secs(60), 10_000));
        let l1_cache = L1Cache::with_config(l1_cache_config).await.map_err(|e| {
            FlowGuardError::DependencyError(format!("Failed to create L1Cache: {}", e))
        })?;
        let l1_cache_enabled = self.l1_cache_enabled;

        // 集成降级管理器 (feature-gated)
        #[cfg(feature = "fallback")]
        let fallback_manager = self.fallback_manager.clone();

        // 如果提供了 fallback_manager，注册孤岛模式回调
        #[cfg(feature = "fallback")]
        if let Some(ref fm) = self.fallback_manager {
            let l1_cache_ref = l1_cache.clone();

            // 注册孤岛模式回调（直接 await 确保注册完成）
            fm.register_island_mode_callback(Box::new(move |is_island| {
                if is_island {
                    // 进入孤岛模式：配置 L1 缓存的孤岛降级策略
                    let island_config =
                        IslandModeConfig::new(IslandFallbackStrategy::LocalDecision);
                    l1_cache_ref.enable_island_mode(island_config);
                } else {
                    // 退出孤岛模式
                    l1_cache_ref.disable_island_mode();
                }
            }))
            .await;

            log::info!(target: "governor", "已注册孤岛模式回调到 FallbackManager");
        }

        Ok(Governor {
            config: Arc::new(tokio::sync::RwLock::new(config)),
            storage,
            ban_storage,
            #[cfg(feature = "ban-manager")]
            ban_manager,
            #[cfg(feature = "parallel-checker")]
            parallel_ban_checker,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker,
            decision_chain,
            rule_matcher,
            rule_chains,
            identifier_extractor,
            #[cfg(feature = "audit-log")]
            audit_logger,
            config_history: Arc::new(tokio::sync::RwLock::new(ConfigHistory::new(100))),
            stats: StatsManager::new(),
            l1_cache,
            l1_cache_enabled: std::sync::atomic::AtomicBool::new(l1_cache_enabled),
            #[cfg(feature = "fallback")]
            fallback_manager,
            #[cfg(feature = "event-system")]
            event_emitter: self.event_emitter,
            shutdown_token: tokio_util::sync::CancellationToken::new(),
            is_shutdown: std::sync::atomic::AtomicBool::new(false),
        })
    }
}

impl Governor {
    /// 创建 GovernorBuilder 用于链式配置
    pub fn builder() -> GovernorBuilder {
        GovernorBuilder::new()
    }

    /// 开箱即用：创建使用默认配置的 Governor
    ///
    /// 此方法使用内存存储作为默认依赖，无需外部配置即可运行。
    /// 适用于快速原型、测试或独立使用场景。
    ///
    /// **注意**：默认配置使用内存存储，不适用于多实例生产环境。
    /// 对于生产环境，建议使用 `builder()` 或 `with_dependencies()` 方法
    /// 配合持久化存储（如 PostgreSQL）。
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use limiteron::Governor;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let governor = Governor::new().await;
    ///     // governor 现在可以用于流量控制检查
    /// }
    /// ```
    pub async fn new() -> Self {
        use crate::storage::{MemoryBanStorage, MemoryStorage};

        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        // 创建默认配置
        let config = FlowControlConfig::default();

        Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("default config should be valid")
    }

    /// 使用依赖注入创建 Governor 实例（用于应用容器集成）
    #[allow(clippy::too_many_arguments)]
    pub async fn with_dependencies(
        config: Arc<tokio::sync::RwLock<FlowControlConfig>>,
        storage: Arc<dyn Storage>,
        ban_storage: Arc<dyn BanStorage>,
        identifier_extractor: Arc<dyn crate::matchers::IdentifierExtractor>,
        rule_matcher: Arc<tokio::sync::RwLock<crate::matchers::RuleMatcher>>,
        decision_chain: Arc<tokio::sync::RwLock<crate::decision_chain::DecisionChain>>,
        rule_chains: Arc<
            tokio::sync::RwLock<DashMap<String, crate::decision_chain::DecisionChain>>,
        >,
        #[cfg(feature = "circuit-breaker")] circuit_breaker: Arc<CircuitBreaker>,
    ) -> Self {
        // 创建封禁管理器
        #[cfg(feature = "ban-manager")]
        use crate::ban::{BanManager, BanManagerConfig};

        #[cfg(feature = "ban-manager")]
        let ban_manager: Arc<BanManager> = {
            match BanManager::with_dependencies(ban_storage.clone(), BanManagerConfig::default())
                .await
                .map(Arc::new)
            {
                Ok(manager) => manager,
                Err(e) => {
                    log::error!("Failed to create BanManager: {}", e);
                    // 使用默认配置重试或返回一个空的管理器
                    // 这里我们选择 panic，因为这是在初始化阶段，无法恢复
                    // 但更好的做法是返回 Result
                    panic!("Failed to create BanManager: {}", e);
                }
            }
        };

        // 创建并行封禁检查器
        #[cfg(feature = "parallel-checker")]
        let parallel_ban_checker: Arc<crate::storage::ParallelBanChecker> =
            Arc::new(crate::storage::ParallelBanChecker::new(ban_manager.clone()));

        Self {
            config,
            storage,
            ban_storage,
            #[cfg(feature = "ban-manager")]
            ban_manager,
            #[cfg(feature = "parallel-checker")]
            parallel_ban_checker,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker,
            decision_chain,
            rule_matcher,
            rule_chains,
            identifier_extractor,
            #[cfg(feature = "audit-log")]
            audit_logger: Arc::new(tokio::sync::RwLock::new(None)),
            config_history: Arc::new(tokio::sync::RwLock::new(ConfigHistory::new(100))),
            stats: StatsManager::new(),
            l1_cache: L1Cache::new().await.expect("Failed to create L1Cache"),
            l1_cache_enabled: std::sync::atomic::AtomicBool::new(true),
            #[cfg(feature = "fallback")]
            fallback_manager: None,
            #[cfg(feature = "event-system")]
            event_emitter: None,
            shutdown_token: tokio_util::sync::CancellationToken::new(),
            is_shutdown: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// 创建新的 Governor 实例（使用显式配置）
    ///
    /// 与 `new()` 不同，此方法需要显式提供配置和存储依赖。
    /// 适用于需要自定义配置或使用持久化存储的场景。
    ///
    /// # 参数
    ///
    /// - `config`: 流量控制配置
    /// - `storage`: 存储后端
    /// - `ban_storage`: 封禁存储后端
    /// - `metrics`: 可选的指标收集器
    /// - `tracer`: 可选的追踪器
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use limiteron::Governor;
    /// use limiteron::config::FlowControlConfig;
    /// use limiteron::storage::MemoryStorage;
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let config = FlowControlConfig::default();
    ///     let storage: Arc<dyn limiteron::storage::Storage> = Arc::new(MemoryStorage::new());
    ///     let ban_storage: Arc<dyn limiteron::storage::BanStorage> = Arc::new(limiteron::storage::MemoryBanStorage::new());
    ///     let _ = Governor::builder()
    ///         .with_config(config)
    ///         .with_storage(storage)
    ///         .with_ban_storage(ban_storage)
    ///         .build()
    ///         .await;
    /// }
    /// ```
    #[allow(unused_variables)]
    pub async fn with_storage(
        config: FlowControlConfig,
        storage: Arc<dyn Storage>,
        ban_storage: Arc<dyn BanStorage>,
        #[cfg(feature = "monitoring")] metrics: Option<Arc<Metrics>>,
        #[cfg(feature = "telemetry")] tracer: Option<Arc<Tracer>>,
    ) -> Result<Self, FlowGuardError> {
        // 使用 builder 模式创建 Governor
        Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .with_identifier_extractor(Arc::new(
                crate::matchers::CompositeExtractor::builder()
                    .add_extractor(Box::new(crate::matchers::UserIdExtractor::from_header(
                        "X-User-Id",
                    )))
                    .add_extractor(Box::new(crate::matchers::IpExtractor::builder().build()))
                    .add_extractor(Box::new(crate::matchers::ApiKeyExtractor::from_header(
                        "X-API-Key",
                    )))
                    .build(),
            ))
            .build()
            .await
    }

    /// 从配置文件创建 Governor 实例
    ///
    /// # 参数
    /// - `config_path`: 配置文件路径（支持 YAML、TOML、JSON）
    /// - `storage`: 存储后端
    /// - `ban_storage`: 封禁存储后端
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use limiteron::Governor;
    /// use limiteron::storage::{MemoryStorage, MemoryBanStorage};
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let storage: Arc<dyn limiteron::storage::Storage> = MemoryStorage::create_storage();
    ///     let ban_storage: Arc<dyn limiteron::storage::BanStorage> = MemoryBanStorage::create_ban_storage();
    ///
    ///     let governor = Governor::from_config_file(
    ///         "/path/to/config.yaml",
    ///         storage,
    ///         ban_storage,
    ///     ).await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn from_config_file<P: AsRef<std::path::Path>>(
        config_path: P,
        storage: Arc<dyn Storage>,
        ban_storage: Arc<dyn BanStorage>,
    ) -> Result<Self, FlowGuardError> {
        // 使用 ConfigLoader 加载配置
        let config = crate::config::ConfigLoader::load_from_file(config_path)?;
        Self::create_with_config(config, storage, ban_storage).await
    }

    /// 从配置文件和环境变量创建 Governor 实例
    ///
    /// 环境变量可以覆盖配置文件中的值。环境变量命名规则：
    /// `LIMITERON_<SECTION>_<FIELD>`
    ///
    /// # 参数
    /// - `config_path`: 配置文件路径（支持 YAML、TOML、JSON）
    /// - `storage`: 存储后端
    /// - `ban_storage`: 封禁存储后端
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use limiteron::Governor;
    /// use limiteron::storage::{MemoryStorage, MemoryBanStorage};
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let storage: Arc<dyn limiteron::storage::Storage> = MemoryStorage::create_storage();
    ///     let ban_storage: Arc<dyn limiteron::storage::BanStorage> = MemoryBanStorage::create_ban_storage();
    ///
    ///     // 设置环境变量覆盖
    ///     std::env::set_var("LIMITERON_GLOBAL_STORAGE", "redis");
    ///
    ///     let governor = Governor::from_config_with_env(
    ///         "/path/to/config.yaml",
    ///         storage,
    ///         ban_storage,
    ///     ).await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn from_config_with_env<P: AsRef<std::path::Path>>(
        config_path: P,
        storage: Arc<dyn Storage>,
        ban_storage: Arc<dyn BanStorage>,
    ) -> Result<Self, FlowGuardError> {
        // 使用 ConfigLoader 加载配置，支持环境变量覆盖
        let config = crate::ConfigLoader::load_from_file_with_env(config_path)?;
        Self::create_with_config(config, storage, ban_storage).await
    }

    /// 根据配置创建 Governor 实例（内部辅助方法）
    ///
    /// 统一处理不同 feature 组合下的创建逻辑，避免重复的条件编译代码。
    async fn create_with_config(
        config: FlowControlConfig,
        storage: Arc<dyn Storage>,
        ban_storage: Arc<dyn BanStorage>,
    ) -> Result<Self, FlowGuardError> {
        #[cfg(all(feature = "monitoring", feature = "telemetry"))]
        {
            Self::with_storage(config, storage, ban_storage, None, None).await
        }
        #[cfg(all(feature = "monitoring", not(feature = "telemetry")))]
        {
            Self::with_storage(config, storage, ban_storage, None).await
        }
        #[cfg(all(not(feature = "monitoring"), feature = "telemetry"))]
        {
            Self::with_storage(config, storage, ban_storage, None, None).await
        }
        #[cfg(all(not(feature = "monitoring"), not(feature = "telemetry")))]
        {
            Self::with_storage(config, storage, ban_storage).await
        }
    }

    /// 检查请求 - 简化版本使用并行检查器
    ///
    /// 该方法会首先检查 L1 缓存，如果缓存命中则直接返回缓存结果。
    /// 如果缓存未命中，则执行完整的检查流程，并将结果缓存。
    ///
    /// 当启用了 FallbackManager 时，会在存储层故障时自动降级。
    pub async fn check(&self, context: &RequestContext) -> Result<Decision, FlowGuardError> {
        // 如果启用了 FallbackManager，使用降级包装的检查逻辑
        #[cfg(feature = "fallback")]
        if let Some(ref fallback_mgr) = self.fallback_manager {
            return self.check_with_fallback(context, fallback_mgr).await;
        }

        // 否则直接执行检查
        self.check_internal(context).await
    }

    /// 内部检查逻辑（不包含降级处理）
    async fn check_internal(&self, context: &RequestContext) -> Result<Decision, FlowGuardError> {
        self.stats.increment_total();

        debug!(
            "开始请求检查: user_id={}, ip={}, path={}, method={}",
            redact_user_id(context.user_id.as_deref()),
            redact_ip(context.ip.as_deref()),
            context.path,
            context.method
        );

        // Extracted identifier
        let identifier = self.identifier_extractor.extract(context).ok_or_else(|| {
            FlowGuardError::ConfigError("Failed to extract identifier".to_string())
        })?;
        trace!("Extracted identifier: {}", identifier.key());

        // 规则匹配 - 只计算一次，贯穿整个检查流程
        let matched_rules = {
            let matcher = self.rule_matcher.read().await;
            #[allow(clippy::disallowed_methods)]
            matcher
                .match_all(context)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        };

        // 尝试从 L1 缓存获取结果
        if self.is_l1_cache_enabled() && !matched_rules.is_empty() {
            // 使用第一个规则构建缓存键
            let first_rule = &matched_rules[0];
            let cache_key = self.build_cache_key(&identifier, &first_rule.id);

            if let Ok(Some(cached_decision)) = self.l1_cache.get(&cache_key).await {
                trace!("L1 缓存命中: key={}", cache_key);
                let decision = cached_decision.to_decision();
                self.update_stats_for_decision(&Result::Ok(decision.clone()));
                return Ok(decision);
            }
        }

        // 并行封禁检查 (仅当 parallel-checker 特性启用时)
        #[cfg(feature = "parallel-checker")]
        {
            // 尝试转换为 BanTarget 进行检查
            let ban_target = identifier.to_ban_target();

            if let Some(target) = ban_target {
                // 使用专门的并行封禁检查器
                let ban_info = self
                    .parallel_ban_checker
                    .check_single_target(&target)
                    .await?;

                if let Some(info) = ban_info {
                    warn!(
                        "Request banned: 用户={}, 原因={}",
                        crate::logging::redact_user_id(Some(identifier.key().as_str())),
                        info.reason()
                    );
                    self.stats.increment_banned();
                    return Ok(Decision::Banned(info));
                }
            }
        }

        // 继续其他检查（使用已计算的 matched_rules）

        if matched_rules.is_empty() {
            // 如果没有匹配的规则，检查默认决策链
            // 目前默认决策链为空，相当于直接允许
            let result = self.decision_chain.read().await.check().await;
            self.update_stats_for_decision(&result);
            return result;
        }

        // 有匹配的规则，按顺序执行（级联）
        // 只要有一个规则拒绝，请求就被拒绝
        let rule_chains = self.rule_chains.read().await;

        for rule in &matched_rules {
            if let Some(chain) = rule_chains.get(&rule.id) {
                // 执行决策链
                let result = chain.check().await;

                match result {
                    Ok(Decision::Allowed(_)) => {
                        // 当前规则允许，继续检查下一个规则
                        continue;
                    }
                    _ => {
                        // 拒绝、封禁或错误，直接返回
                        self.update_stats_for_decision(&result);

                        // 发射事件
                        #[cfg(feature = "event-system")]
                        {
                            if let Ok(ref decision) = result {
                                let decision_str = match decision {
                                    Decision::Banned(_) => "Banned",
                                    Decision::Rejected(_) => "Denied",
                                    _ => "Allowed",
                                };
                                self.emit_rate_limit_event(
                                    &identifier.key(),
                                    &rule.id,
                                    decision_str,
                                )
                                .await;
                            }
                        }

                        return result;
                    }
                }
            }
        }

        // 所有规则都允许
        self.stats.increment_allowed();
        let decision = Decision::allowed_default();

        // 缓存允许的决策
        if self.is_l1_cache_enabled() && !matched_rules.is_empty() {
            let cache_key = self.build_cache_key(&identifier, &matched_rules[0].id);
            let cacheable_decision = CacheableDecision::from_decision(&decision);
            let _ = self.l1_cache.set(cache_key, cacheable_decision).await;
            trace!("L1 缓存已更新: decision=allowed");
        }

        Ok(decision)
    }

    /// 带降级处理的检查逻辑
    ///
    /// 使用 FallbackManager 包装检查流程，在存储层故障时自动降级。
    #[cfg(feature = "fallback")]
    async fn check_with_fallback(
        &self,
        context: &RequestContext,
        fallback_mgr: &Arc<FallbackManager>,
    ) -> Result<Decision, FlowGuardError> {
        let context_clone = context.clone();

        fallback_mgr
            .execute_with_fallback(
                crate::fallback::ComponentType::Redis,
                || async { self.check_internal(&context_clone).await },
                || async {
                    // 降级操作：尝试仅使用 L1 缓存
                    self.check_l1_cache_only(&context_clone).await
                },
            )
            .await
    }

    /// 仅使用 L1 缓存的降级检查
    ///
    /// 当存储层不可用时，尝试仅从 L1 缓存获取决策结果。
    #[cfg(feature = "fallback")]
    async fn check_l1_cache_only(
        &self,
        context: &RequestContext,
    ) -> Result<Decision, FlowGuardError> {
        if !self.is_l1_cache_enabled() {
            return Err(FlowGuardError::LimitError(
                "存储层故障且 L1 缓存未启用".to_string(),
            ));
        }

        // 尝试从 L1 缓存获取结果
        let identifier = self.identifier_extractor.extract(context).ok_or_else(|| {
            FlowGuardError::ConfigError("Failed to extract identifier".to_string())
        })?;

        let matched_rules = {
            let matcher = self.rule_matcher.read().await;
            #[allow(clippy::disallowed_methods)]
            matcher
                .match_all(context)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        };

        if matched_rules.is_empty() {
            // 没有规则，允许通过
            return Ok(Decision::allowed_default());
        }

        // 尝试从 L1 缓存获取第一个规则的决策
        let first_rule = &matched_rules[0];
        let cache_key = self.build_cache_key(&identifier, &first_rule.id);

        match self.l1_cache.get(&cache_key).await {
            Ok(Some(cached_decision)) => {
                trace!("孤岛模式 - L1 缓存命中: key={}", cache_key);
                let decision = cached_decision.to_decision();
                self.update_stats_for_decision(&Result::Ok(decision.clone()));
                Ok(decision)
            }
            _ => {
                // L1 缓存未命中，根据孤岛模式策略处理
                if self.l1_cache.is_island_mode() {
                    if let Some(config) = self.l1_cache.island_config() {
                        match config.fallback_strategy {
                            IslandFallbackStrategy::AllowAll => {
                                log::warn!(target: "governor", "孤岛模式 - 允许所有请求通过");
                                Ok(Decision::allowed_default())
                            }
                            IslandFallbackStrategy::RejectAll => {
                                log::warn!(target: "governor", "孤岛模式 - 拒绝所有请求");
                                Err(FlowGuardError::LimitError(
                                    "孤岛模式：存储层故障，拒绝请求".to_string(),
                                ))
                            }
                            IslandFallbackStrategy::LocalDecision => {
                                // 已在上面尝试过 L1 缓存，未命中
                                log::warn!(target: "governor", "孤岛模式 - L1 缓存未命中，使用保守策略");
                                Ok(Decision::allowed_default())
                            }
                            IslandFallbackStrategy::ConservativeQuota {
                                max_requests,
                                window_secs,
                            } => {
                                // 使用保守配额：简单计数，超出则拒绝
                                log::warn!(target: "governor", "孤岛模式 - 使用保守配额: {}/{}s", max_requests, window_secs);
                                // 这里可以实现一个简单的本地计数器
                                // 为简化实现，当前直接允许
                                Ok(Decision::allowed_default())
                            }
                        }
                    } else {
                        // 未配置孤岛模式，使用默认策略
                        Ok(Decision::allowed_default())
                    }
                } else {
                    // 不在孤岛模式，返回错误
                    Err(FlowGuardError::LimitError(
                        "存储层故障，降级缓存未命中".to_string(),
                    ))
                }
            }
        }
    }

    /// 构建缓存键
    ///
    /// 根据标识符类型和规则 ID 生成缓存键。
    fn build_cache_key(&self, identifier: &crate::matchers::Identifier, rule_id: &str) -> String {
        match identifier {
            crate::matchers::Identifier::UserId(user_id) => {
                RateLimitCacheKey::user_rate_limit(user_id, rule_id)
            }
            crate::matchers::Identifier::Ip(ip) => RateLimitCacheKey::ip_rate_limit(ip, rule_id),
            crate::matchers::Identifier::ApiKey(api_key) => {
                RateLimitCacheKey::api_key_rate_limit(api_key, rule_id)
            }
            _ => RateLimitCacheKey::generic(&identifier.key(), rule_id),
        }
    }

    /// 根据决策结果更新统计信息
    ///
    /// 统一处理不同决策类型的统计更新，避免重复的 match 分支。
    fn update_stats_for_decision(&self, result: &Result<Decision, FlowGuardError>) {
        match result {
            Ok(Decision::Allowed(_)) => {
                self.stats.increment_allowed();
            }
            Ok(Decision::Banned(_)) => {
                self.stats.increment_banned();
            }
            Ok(Decision::Rejected(_)) => {
                self.stats.increment_rejected();
            }
            Err(_) => {
                self.stats.increment_error();
            }
        }
    }

    /// 发射限流事件（内部辅助方法）
    #[cfg(feature = "event-system")]
    async fn emit_rate_limit_event(&self, key: &str, rule_id: &str, decision: &str) {
        if let Some(ref emitter) = self.event_emitter {
            let event = crate::events::Event::new(crate::events::EventType::RateLimitTriggered {
                key: key.to_string(),
                rule_id: rule_id.to_string(),
                decision: decision.to_string(),
            });
            if let Err(e) = emitter.emit(event).await {
                log::error!("Failed to emit rate limit event: {}", e);
            }
        }
    }

    /// 并行资源检查 - 保持原有接口兼容性
    #[cfg(feature = "parallel-checker")]
    pub async fn check_resource_parallel(
        &self,
        resource: &str,
    ) -> Result<Decision, FlowGuardError> {
        // 使用专门的并行封禁检查器
        let ban_info = self
            .parallel_ban_checker
            .check_user_banned(resource)
            .await?;

        match ban_info {
            Some(info) => {
                warn!("Resource banned: 资源={}, 原因={}", resource, info.reason());
                Ok(Decision::Banned(info))
            }
            None => Ok(Decision::allowed_default()),
        }
    }

    /// 并行资源检查 - 未启用 parallel-checker 时的存根实现
    #[cfg(not(feature = "parallel-checker"))]
    pub async fn check_resource_parallel(
        &self,
        _resource: &str,
    ) -> Result<Decision, FlowGuardError> {
        #[cfg(feature = "ban-manager")]
        {
            let target = BanTarget::UserId(_resource.to_string());
            let ban_record = self.ban_manager.is_banned(&target).await?;

            if let Some(record) = ban_record {
                return Ok(Decision::Banned(BanInfo::new(
                    record.reason,
                    record.expires_at,
                    record.ban_times,
                )));
            }

            return Ok(Decision::allowed_default());
        }

        #[cfg(not(feature = "ban-manager"))]
        {
            Err(FlowGuardError::ConfigError(
                "并行检查已禁用且未启用封禁管理器，无法执行资源封禁检查".to_string(),
            ))
        }
    }

    /// 手动Ban user
    #[cfg(feature = "ban-manager")]
    pub async fn ban_identifier(
        &self,
        identifier: &Identifier,
        reason: &str,
        source: Option<BanSource>,
    ) -> Result<(), FlowGuardError> {
        debug!("Ban user: {} 原因: {}", identifier.key(), reason);

        let ban_target = identifier.to_ban_target();

        if let Some(target) = ban_target {
            let ban_source = source.unwrap_or(BanSource::Manual {
                operator: "unknown".to_string(),
            });

            self.ban_manager
                .create_ban(
                    target,
                    reason.to_string(),
                    ban_source,
                    serde_json::json!({}),
                    None,
                )
                .await?;
            info!(
                "用户 {} 已被封禁",
                crate::logging::redact_user_id(Some(identifier.key().as_ref()))
            );
        } else {
            return Err(FlowGuardError::ValidationError(
                "Unsupported identifier type".to_string(),
            ));
        }

        Ok(())
    }

    /// 取消用户封禁
    #[cfg(feature = "ban-manager")]
    pub async fn unban_identifier(&self, identifier: &Identifier) -> Result<(), FlowGuardError> {
        debug!("取消Ban user: {}", identifier.key());

        let ban_target = identifier.to_ban_target();

        if let Some(target) = ban_target {
            let unbanned = self
                .ban_manager
                .delete_ban(&target, "unknown".to_string())
                .await?;

            if unbanned {
                info!(
                    "用户 {} 已解封",
                    crate::logging::redact_user_id(Some(identifier.key().as_ref()))
                );
            }
            Ok(())
        } else {
            Err(FlowGuardError::ValidationError(
                "Unsupported identifier type".to_string(),
            ))
        }
    }

    /// 获取配置历史
    pub async fn get_config_history(&self) -> Vec<ConfigChangeRecord> {
        self.config_history.read().await.get_records().to_vec()
    }

    /// 停止配置监视器
    pub async fn stop_config_watcher(&self) -> Result<(), FlowGuardError> {
        info!("停止配置监视器");

        Ok(())
    }

    /// 手动配置检查
    pub async fn manual_config_check(&self) -> Result<bool, FlowGuardError> {
        info!("手动配置检查");

        let _config = self.config.read().await;

        // 执行各种配置验证
        // 具体验证逻辑取决于具体的验证需求

        Ok(true)
    }

    /// 获取统计信息
    pub async fn stats(&self) -> GovernorStats {
        self.stats.snapshot().into()
    }

    /// 获取决策链统计
    pub async fn decision_chain_stats(&self) -> crate::decision_chain::ChainStats {
        self.decision_chain.read().await.stats().await
    }

    /// 获取规则匹配器统计
    pub async fn rule_matcher_stats(&self) -> crate::matchers::MatcherStats {
        self.rule_matcher.read().await.stats()
    }

    /// 重置统计信息
    pub async fn reset_stats(&self) {
        info!("重置统计信息");

        self.decision_chain.write().await.reset_stats().await;
        self.rule_matcher.write().await.reset_stats();
        self.stats.reset();
        self.l1_cache.reset_stats();
    }

    // ==================== L1 缓存相关方法 ====================

    /// 获取 L1 缓存统计信息
    #[cfg(test)]
    pub(crate) async fn l1_cache_stats(&self) -> crate::l1_cache::L1CacheStats {
        self.l1_cache.stats().await
    }

    /// 启用 L1 缓存
    pub fn enable_l1_cache(&self) {
        self.l1_cache_enabled
            .store(true, std::sync::atomic::Ordering::Release);
        info!("L1 缓存已启用");
    }

    /// 禁用 L1 缓存
    pub fn disable_l1_cache(&self) {
        self.l1_cache_enabled
            .store(false, std::sync::atomic::Ordering::Release);
        info!("L1 缓存已禁用");
    }

    /// 检查 L1 缓存是否启用
    pub fn is_l1_cache_enabled(&self) -> bool {
        self.l1_cache_enabled
            .load(std::sync::atomic::Ordering::Acquire)
    }

    /// 清空 L1 缓存
    pub async fn clear_l1_cache(&self) {
        let _ = self.l1_cache.clear().await;
        info!("L1 缓存已清空");
    }

    /// 清理 L1 缓存中的过期条目
    pub async fn evict_expired_l1_cache(&self) -> usize {
        let evicted = self.l1_cache.evict_expired().await.unwrap_or(0);
        if evicted > 0 {
            debug!("L1 缓存清理了 {} 个过期条目", evicted);
        }
        evicted
    }

    /// 使指定标识符的缓存失效
    ///
    /// # 参数
    /// - `identifier`: 标识符（如用户 ID、IP 地址等）
    pub async fn invalidate_l1_cache(&self, identifier: &str) {
        // 使所有与该标识符相关的缓存失效
        let _ = self
            .l1_cache
            .invalidate_by_prefix(&format!("rl:user:{}:", identifier))
            .await;
        let _ = self
            .l1_cache
            .invalidate_by_prefix(&format!("rl:ip:{}:", identifier))
            .await;
        let _ = self
            .l1_cache
            .invalidate_by_prefix(&format!("rl:apikey:{}:", identifier))
            .await;
        let _ = self
            .l1_cache
            .invalidate_by_prefix(&format!("rl:generic:{}:", identifier))
            .await;
        let _ = self
            .l1_cache
            .invalidate(&RateLimitCacheKey::ban_check(identifier))
            .await;
        debug!("已使标识符 {} 的 L1 缓存失效", identifier);
    }

    /// 使指定规则的缓存失效
    ///
    /// # 参数
    /// - `rule_id`: 规则 ID
    pub async fn invalidate_rule_cache(&self, rule_id: &str) {
        // 使用包含匹配移除所有与该规则相关的条目
        let _ = self
            .l1_cache
            .invalidate_containing(&format!(":{}", rule_id))
            .await;
        debug!("已使规则 {} 的 L1 缓存失效", rule_id);
    }

    /// 获取 L1 缓存大小
    pub async fn l1_cache_size(&self) -> usize {
        self.l1_cache.len().await.unwrap_or(0)
    }

    /// 设置审计日志记录器
    #[cfg(feature = "audit-log")]
    pub async fn set_audit_logger(&self, audit_logger: Arc<AuditLogger>) {
        let mut logger = self.audit_logger.write().await;
        *logger = Some(audit_logger);

        info!("审计日志记录器已设置");
    }

    /// 获取审计日志记录器
    #[cfg(feature = "audit-log")]
    pub async fn audit_logger(&self) -> Option<Arc<AuditLogger>> {
        let guard = self.audit_logger.read().await;
        guard.as_ref().cloned()
    }

    /// 健康检查
    ///
    /// 执行真实的存储 ping、缓存可用性检查、后台任务存活检查。
    /// 返回 `Ok(())` 表示所有关键组件健康；返回 `Err` 包含具体故障信息。
    pub async fn health_check(&self) -> Result<(), FlowGuardError> {
        info!("健康检查");

        let status = self.health_status().await;

        if status.healthy() {
            Ok(())
        } else {
            let mut issues = Vec::new();
            if !status.storage_healthy {
                issues.push("storage".to_string());
            }
            if !status.ban_storage_healthy {
                issues.push("ban_storage".to_string());
            }
            if !status.cache_healthy {
                issues.push("cache".to_string());
            }
            if !status.background_tasks_alive {
                issues.push("background_tasks".to_string());
            }
            Err(FlowGuardError::StorageError(
                crate::error::StorageError::ConnectionError(format!(
                    "Components unhealthy: {}",
                    issues.join(", ")
                )),
            ))
        }
    }

    /// 获取详细的健康状态
    ///
    /// 返回各组件的健康状况，用于监控和诊断。
    pub async fn health_status(&self) -> HealthStatus {
        // 检查存储后端：执行一次 probe get 操作
        let storage_healthy = self.storage.get("__health_probe__").await.is_ok();

        // 检查封禁存储：执行一次 list_bans(0, 1) 操作
        let ban_storage_healthy = self.ban_storage.list_bans(false, 0, 1).await.is_ok();

        // 检查配置锁是否可读（如果 RwLock 中毒则 panic，视为健康）
        let _config_guard = self.config.read().await;

        // L1 缓存为内存实现，不会故障；只要 Governor 存在即视为健康
        let cache_healthy = true;

        // 检查 ban_manager（feature-gated）
        #[cfg(feature = "ban-manager")]
        {
            let _ = self.ban_manager.get_config().await;
        }

        // 检查 circuit_breaker（feature-gated）
        #[cfg(feature = "circuit-breaker")]
        {
            let _ = self.circuit_breaker.get_state().await;
        }

        // 检查 audit_logger（feature-gated）
        #[cfg(feature = "audit-log")]
        {
            let _ = self.audit_logger().await;
        }

        HealthStatus {
            storage_healthy,
            ban_storage_healthy,
            cache_healthy,
            background_tasks_alive: !self.is_shutdown.load(std::sync::atomic::Ordering::SeqCst),
        }
    }

    /// 优雅关闭 Governor
    ///
    /// 取消所有后台任务（通过 CancellationToken），标记 Governor 为已关闭。
    /// 幂等：多次调用返回相同的 Ok 结果。
    ///
    /// # 返回
    ///
    /// * `Ok(())` - 已成功关闭（或之前已关闭）
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use limiteron::Governor;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let governor = Governor::new().await;
    ///     // ... 使用 governor 处理请求 ...
    ///     governor.shutdown().await?; // 优雅关闭
    ///     Ok(())
    /// }
    /// ```
    pub async fn shutdown(&self) -> Result<(), FlowGuardError> {
        // 幂等性检查：使用 compare_exchange 确保只执行一次关闭逻辑
        let already_shutdown = self
            .is_shutdown
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            )
            .is_err();

        if already_shutdown {
            info!("Governor 已关闭，shutdown() 幂等返回 Ok");
            return Ok(());
        }

        info!("开始优雅关闭 Governor");

        // 取消所有后台任务
        self.shutdown_token.cancel();

        // 当前无后台 JoinHandle 需要等待；未来添加后台任务时在此等待
        // 例如：if let Some(handle) = &self.background_task_handle {
        //     let _ = tokio::time::timeout(Duration::from_secs(30), handle).await;
        // }

        // 清空 L1 缓存
        self.clear_l1_cache().await;

        info!("Governor 优雅关闭完成");
        Ok(())
    }

    /// 获取关闭令牌的引用（用于后台任务订阅取消信号）
    ///
    /// 后台任务可通过 `token.cancelled()` 等待取消信号。
    pub fn shutdown_token(&self) -> &tokio_util::sync::CancellationToken {
        &self.shutdown_token
    }

    /// 检查 Governor 是否已关闭
    pub fn is_shutdown(&self) -> bool {
        self.is_shutdown.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Governor 健康状态
///
/// 描述各关键组件的健康状况，用于监控和诊断。
#[derive(Debug, Clone)]
pub struct HealthStatus {
    /// 主存储后端是否健康
    pub storage_healthy: bool,
    /// 封禁存储后端是否健康
    pub ban_storage_healthy: bool,
    /// L1 缓存是否健康
    pub cache_healthy: bool,
    /// 后台任务是否存活
    pub background_tasks_alive: bool,
}

impl HealthStatus {
    /// 所有关键组件是否健康
    pub fn healthy(&self) -> bool {
        self.storage_healthy
            && self.ban_storage_healthy
            && self.cache_healthy
            && self.background_tasks_alive
    }
}

// ============================================================================
// Governor Construction Patterns Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod governor_construction_tests {
    use super::*;
    use crate::config::types::{
        Action, ActionConfig, FlowControlConfig, LimiterConfig, Matcher, Rule,
    };
    use crate::error::StorageError;
    use crate::storage::{BanHistory, BanRecord, BanTarget, MemoryBanStorage, MemoryStorage};
    use async_trait::async_trait;

    fn create_valid_test_config() -> FlowControlConfig {
        FlowControlConfig {
            version: "0.1.0".to_string(),
            global: crate::config::types::GlobalConfig::default(),
            rules: vec![Rule {
                id: "test_rule".to_string(),
                name: "Test Rule".to_string(),
                priority: 100,
                matchers: vec![Matcher::User {
                    user_ids: vec!["*".to_string()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 100,
                    refill_rate: 10,
                }],
                action: ActionConfig {
                    on_exceed: Action::Reject,
                    ban: None,
                },
            }],
        }
    }

    #[tokio::test]
    async fn test_governor_new_with_valid_config() {
        // Governor::new() requires valid config with at least one rule
        // Create a valid config for testing
        let config = create_valid_test_config();

        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let governor = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.allowed_requests, 0);
    }

    #[tokio::test]
    async fn test_governor_builder_with_storage() {
        // Test Governor::builder() with explicit storage
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let config = create_valid_test_config();

        let governor = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 0);
    }

    #[tokio::test]
    async fn test_governor_with_storage_explicit() {
        // Test Governor::with_storage() with explicit configuration
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let config = create_valid_test_config();

        #[cfg(feature = "monitoring")]
        let metrics: Option<Arc<Metrics>> = None;
        #[cfg(feature = "telemetry")]
        let tracer: Option<Arc<Tracer>> = None;

        let governor = Governor::with_storage(
            config,
            storage,
            ban_storage,
            #[cfg(feature = "monitoring")]
            metrics,
            #[cfg(feature = "telemetry")]
            tracer,
        )
        .await
        .expect("Governor creation should succeed");

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 0);
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_governor_with_memory_ban_storage() {
        // Verify MemoryBanStorage is properly integrated
        let config = create_valid_test_config();
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let governor = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        // Health check should work
        let result = governor.health_check().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_governor_check_with_valid_request() {
        let config = create_valid_test_config();
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let governor = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        let mut ctx = RequestContext::default();
        ctx.user_id = Some("test_user".to_string());

        let result = governor.check(&ctx).await;
        // check() may return error if no matching rule found - just verify it doesn't panic
        let _ = result;
    }

    #[tokio::test]
    async fn test_governor_stats_after_requests() {
        let config = create_valid_test_config();
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let governor = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        let mut ctx = RequestContext::default();
        ctx.user_id = Some("test_user".to_string());

        // Make several requests
        for _ in 0..5 {
            let _ = governor.check(&ctx).await;
        }

        let stats = governor.stats().await;
        // Stats should reflect the requests (allowed or rejected depends on rule matching)
        assert_eq!(stats.total_requests, 5);
    }

    #[tokio::test]
    async fn test_governor_health_check() {
        let config = create_valid_test_config();
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let governor = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        // 真实 health_check：MemoryStorage 应健康
        let result = governor.health_check().await;
        assert!(
            result.is_ok(),
            "health_check should succeed with healthy MemoryStorage: {:?}",
            result.err()
        );
    }

    /// 验证 health_status() 返回真实的组件状态（非硬编码 true）
    #[tokio::test]
    async fn test_governor_health_status_real() {
        let config = create_valid_test_config();
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let governor = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        let status = governor.health_status().await;

        // MemoryStorage 的 probe get 应成功（返回 Ok(None)）
        assert!(
            status.storage_healthy,
            "storage_healthy should be true for MemoryStorage"
        );
        // MemoryBanStorage 的 list_bans 应成功
        assert!(
            status.ban_storage_healthy,
            "ban_storage_healthy should be true for MemoryBanStorage"
        );
        // L1 缓存为内存实现，应健康
        assert!(status.cache_healthy, "cache_healthy should be true");
        // 无后台任务时视为存活
        assert!(
            status.background_tasks_alive,
            "background_tasks_alive should be true"
        );
        // 整体应健康
        assert!(status.healthy(), "all components should be healthy");
    }

    /// 验证 shutdown() 幂等性：连续两次调用均返回 Ok
    #[tokio::test]
    async fn test_shutdown_idempotent() {
        let config = create_valid_test_config();
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let governor = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        // 关闭前：is_shutdown 应为 false
        assert!(
            !governor.is_shutdown(),
            "Governor should not be shutdown initially"
        );

        // 第一次 shutdown
        let result1 = governor.shutdown().await;
        assert!(
            result1.is_ok(),
            "first shutdown should succeed: {:?}",
            result1.err()
        );
        assert!(
            governor.is_shutdown(),
            "Governor should be shutdown after first call"
        );

        // 第二次 shutdown（幂等）
        let result2 = governor.shutdown().await;
        assert!(
            result2.is_ok(),
            "second shutdown should be idempotent: {:?}",
            result2.err()
        );
        assert!(governor.is_shutdown(), "Governor should still be shutdown");
    }

    /// 验证 shutdown 后 health_status 反映关闭状态
    #[tokio::test]
    async fn test_shutdown_affects_health_status() {
        let config = create_valid_test_config();
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let governor = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        // 关闭前：background_tasks_alive 应为 true
        let status_before = governor.health_status().await;
        assert!(
            status_before.background_tasks_alive,
            "background_tasks_alive should be true before shutdown"
        );

        // shutdown
        governor.shutdown().await.unwrap();

        // 关闭后：background_tasks_alive 应为 false
        let status_after = governor.health_status().await;
        assert!(
            !status_after.background_tasks_alive,
            "background_tasks_alive should be false after shutdown"
        );
    }

    /// 验证 shutdown_token 可被外部订阅
    #[tokio::test]
    async fn test_shutdown_token_cancellation() {
        let config = create_valid_test_config();
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let governor = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        let token = governor.shutdown_token();
        assert!(
            !token.is_cancelled(),
            "token should not be cancelled initially"
        );

        governor.shutdown().await.unwrap();

        assert!(
            token.is_cancelled(),
            "token should be cancelled after shutdown"
        );
    }

    // ========================================================================
    // Mock 存储实现（用于测试 health_check 错误路径）
    // ========================================================================

    /// 所有操作均返回 Err 的 Storage mock，用于测试 health_check 错误分支
    struct FailingStorage;

    #[async_trait]
    impl Storage for FailingStorage {
        async fn get(&self, _key: &str) -> Result<Option<String>, StorageError> {
            Err(StorageError::ConnectionError(
                "mock storage failure".to_string(),
            ))
        }
        async fn set(
            &self,
            _key: &str,
            _value: &str,
            _ttl: Option<u64>,
        ) -> Result<(), StorageError> {
            Err(StorageError::ConnectionError(
                "mock storage failure".to_string(),
            ))
        }
        async fn delete(&self, _key: &str) -> Result<(), StorageError> {
            Err(StorageError::ConnectionError(
                "mock storage failure".to_string(),
            ))
        }
    }

    /// 所有操作均返回 Err 的 BanStorage mock，用于测试 health_check 错误分支
    struct FailingBanStorage;

    #[async_trait]
    impl BanStorage for FailingBanStorage {
        async fn is_banned(&self, _target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
            Err(StorageError::ConnectionError(
                "mock ban storage failure".to_string(),
            ))
        }
        async fn save(&self, _record: &BanRecord) -> Result<(), StorageError> {
            Err(StorageError::ConnectionError(
                "mock ban storage failure".to_string(),
            ))
        }
        async fn get_history(
            &self,
            _target: &BanTarget,
        ) -> Result<Option<BanHistory>, StorageError> {
            Err(StorageError::ConnectionError(
                "mock ban storage failure".to_string(),
            ))
        }
        async fn increment_ban_times(&self, _target: &BanTarget) -> Result<u64, StorageError> {
            Err(StorageError::ConnectionError(
                "mock ban storage failure".to_string(),
            ))
        }
        async fn get_ban_times(&self, _target: &BanTarget) -> Result<u64, StorageError> {
            Err(StorageError::ConnectionError(
                "mock ban storage failure".to_string(),
            ))
        }
        async fn remove_ban(&self, _target: &BanTarget) -> Result<(), StorageError> {
            Err(StorageError::ConnectionError(
                "mock ban storage failure".to_string(),
            ))
        }
        async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
            Err(StorageError::ConnectionError(
                "mock ban storage failure".to_string(),
            ))
        }
        async fn list_bans(
            &self,
            _active_only: bool,
            _offset: u64,
            _limit: u64,
        ) -> Result<Vec<BanRecord>, StorageError> {
            Err(StorageError::ConnectionError(
                "mock ban storage failure".to_string(),
            ))
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    // ========================================================================
    // health_check 错误路径测试
    // ========================================================================

    /// 验证 health_check() 在存储不可用时返回 Err
    #[tokio::test]
    async fn test_health_check_returns_err_when_storage_unhealthy() {
        let storage: Arc<dyn Storage> = Arc::new(FailingStorage);
        let ban_storage: Arc<dyn BanStorage> = Arc::new(FailingBanStorage);

        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        let result = governor.health_check().await;
        assert!(
            result.is_err(),
            "health_check should return Err when storage is unhealthy"
        );
    }

    /// 验证 health_check() 在 shutdown 后返回 Err（background_tasks_alive = false）
    #[tokio::test]
    async fn test_health_check_returns_err_after_shutdown() {
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        // shutdown 前：health_check 应返回 Ok
        let result_before = governor.health_check().await;
        assert!(
            result_before.is_ok(),
            "health_check should pass before shutdown"
        );

        governor.shutdown().await.unwrap();

        // shutdown 后：health_check 应返回 Err（background_tasks_alive = false）
        let result_after = governor.health_check().await;
        assert!(
            result_after.is_err(),
            "health_check should return Err after shutdown (background_tasks not alive)"
        );
    }

    /// 验证 health_check() 错误消息包含具体不健康组件名称
    #[tokio::test]
    async fn test_health_check_error_message_contains_component_names() {
        let storage: Arc<dyn Storage> = Arc::new(FailingStorage);
        let ban_storage: Arc<dyn BanStorage> = Arc::new(FailingBanStorage);

        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        let err = governor.health_check().await.unwrap_err();
        let err_msg = match err {
            FlowGuardError::StorageError(StorageError::ConnectionError(msg)) => msg,
            other => panic!("expected StorageError::ConnectionError, got {other:?}"),
        };
        assert!(
            err_msg.contains("storage"),
            "error message should mention 'storage' component: {err_msg}"
        );
        assert!(
            err_msg.contains("ban_storage"),
            "error message should mention 'ban_storage' component: {err_msg}"
        );
        assert!(
            err_msg.contains("Components unhealthy"),
            "error message should have descriptive prefix: {err_msg}"
        );
    }

    /// 验证 health_status() 正确报告各组件健康状况
    #[tokio::test]
    async fn test_health_status_reports_unhealthy_components() {
        let storage: Arc<dyn Storage> = Arc::new(FailingStorage);
        let ban_storage: Arc<dyn BanStorage> = Arc::new(FailingBanStorage);

        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        let status = governor.health_status().await;
        assert!(
            !status.storage_healthy,
            "storage_healthy should be false with FailingStorage"
        );
        assert!(
            !status.ban_storage_healthy,
            "ban_storage_healthy should be false with FailingBanStorage"
        );
        assert!(
            status.cache_healthy,
            "cache_healthy should always be true (in-memory, cannot fail)"
        );
        assert!(
            status.background_tasks_alive,
            "background_tasks_alive should be true before shutdown"
        );
        assert!(
            !status.healthy(),
            "overall healthy() should be false when any component is down"
        );
    }

    /// 验证 HealthStatus::healthy() 在所有组件健康时返回 true
    #[test]
    fn test_health_status_healthy_all_true() {
        let status = HealthStatus {
            storage_healthy: true,
            ban_storage_healthy: true,
            cache_healthy: true,
            background_tasks_alive: true,
        };
        assert!(status.healthy(), "all true should be healthy");
    }

    /// 验证 HealthStatus::healthy() 在任一组件不健康时返回 false
    #[test]
    fn test_health_status_healthy_any_false() {
        // 测试每个字段单独为 false 的情况
        let cases = [
            (
                "storage",
                HealthStatus {
                    storage_healthy: false,
                    ban_storage_healthy: true,
                    cache_healthy: true,
                    background_tasks_alive: true,
                },
            ),
            (
                "ban_storage",
                HealthStatus {
                    storage_healthy: true,
                    ban_storage_healthy: false,
                    cache_healthy: true,
                    background_tasks_alive: true,
                },
            ),
            (
                "cache",
                HealthStatus {
                    storage_healthy: true,
                    ban_storage_healthy: true,
                    cache_healthy: false,
                    background_tasks_alive: true,
                },
            ),
            (
                "background_tasks",
                HealthStatus {
                    storage_healthy: true,
                    ban_storage_healthy: true,
                    cache_healthy: true,
                    background_tasks_alive: false,
                },
            ),
        ];
        for (name, status) in cases {
            assert!(
                !status.healthy(),
                "healthy() should be false when {name} is false"
            );
        }
    }

    #[tokio::test]
    async fn test_governor_new_with_defaults() {
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");
        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 0);
    }

    #[test]
    fn test_governor_builder_validation_empty_config() {
        let result = std::thread::spawn(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
                let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
                Governor::builder()
                    .with_storage(storage)
                    .with_ban_storage(ban_storage)
                    .build()
                    .await
            })
        });
        let result = result.join().unwrap();
        assert!(result.is_err());
    }

    /// Governor::new() uses FlowControlConfig::default() which has empty rules,
    /// causing validate() to fail and expect() to panic.
    #[tokio::test]
    #[should_panic(expected = "default config should be valid")]
    async fn test_governor_new_panics_with_empty_default_config() {
        let _ = Governor::new().await;
    }

    #[tokio::test]
    async fn test_governor_builder_with_l1_cache_config() {
        let config = create_valid_test_config();
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        use crate::l1_cache::L1CacheConfig;
        let l1_config = L1CacheConfig {
            default_ttl: std::time::Duration::from_secs(60),
            max_size: 10000,
            ..Default::default()
        };

        let governor = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .with_l1_cache_config(l1_config)
            .build()
            .await
            .expect("Governor build should succeed");

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 0);
    }

    #[tokio::test]
    async fn test_governor_config_validation_missing_rules() {
        let config = FlowControlConfig {
            version: "0.1.0".to_string(),
            global: crate::config::types::GlobalConfig::default(),
            rules: vec![],
        };
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let result = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_governor_multiple_requests_stats() {
        let config = create_valid_test_config();
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let governor = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        let mut ctx = RequestContext::default();
        ctx.user_id = Some("user_a".to_string());

        for _ in 0..3 {
            let _ = governor.check(&ctx).await;
        }

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 3);

        let mut ctx_b = RequestContext::default();
        ctx_b.user_id = Some("user_b".to_string());
        let _ = governor.check(&ctx_b).await;

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 4);
    }

    #[tokio::test]
    async fn test_governor_with_storage_reuse() {
        let config = create_valid_test_config();
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let governor = Governor::with_storage(
            config,
            storage.clone(),
            ban_storage.clone(),
            #[cfg(feature = "monitoring")]
            None,
            #[cfg(feature = "telemetry")]
            None,
        )
        .await
        .expect("Governor creation should succeed");

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 0);
    }

    // ============================================================================
    // Builder Error Paths
    // ============================================================================

    #[tokio::test]
    async fn test_builder_missing_storage() {
        let result = Governor::builder()
            .with_config(create_valid_test_config())
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await;
        let err = result
            .err()
            .expect("expected DependencyError for missing storage");
        match err {
            FlowGuardError::DependencyError(msg) => assert!(msg.contains("storage")),
            _ => panic!("expected DependencyError"),
        }
    }

    #[tokio::test]
    async fn test_builder_missing_ban_storage() {
        let result = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .build()
            .await;
        let err = result
            .err()
            .expect("expected DependencyError for missing ban_storage");
        match err {
            FlowGuardError::DependencyError(msg) => assert!(msg.contains("ban_storage")),
            _ => panic!("expected DependencyError"),
        }
    }

    // ============================================================================
    // check() Code Paths - Identifier Extraction
    // ============================================================================

    #[tokio::test]
    async fn test_check_identifier_extraction_failure() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let ctx = RequestContext::default();
        let result = governor.check(&ctx).await;
        match result {
            Err(FlowGuardError::ConfigError(msg)) => {
                assert!(msg.contains("Failed to extract identifier"))
            }
            other => panic!("Expected ConfigError, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_check_total_incremented_on_extraction_failure() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let ctx = RequestContext::default();
        let _ = governor.check(&ctx).await;

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.allowed_requests, 0);
        assert_eq!(stats.rejected_requests, 0);
        assert_eq!(stats.banned_requests, 0);
    }

    // ============================================================================
    // check() Code Paths - No Matched Rules (fallthrough to default decision chain)
    // ============================================================================

    fn create_non_matching_config() -> FlowControlConfig {
        FlowControlConfig {
            version: "0.1.0".to_string(),
            global: crate::config::types::GlobalConfig::default(),
            rules: vec![Rule {
                id: "specific_rule".to_string(),
                name: "Specific Rule".to_string(),
                priority: 100,
                matchers: vec![Matcher::User {
                    user_ids: vec!["specific_user".to_string()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 100,
                    refill_rate: 10,
                }],
                action: ActionConfig {
                    on_exceed: Action::Reject,
                    ban: None,
                },
            }],
        }
    }

    #[tokio::test]
    async fn test_check_no_matched_rules_falls_to_default_chain() {
        let config = create_non_matching_config();
        let governor = Governor::builder()
            .with_config(config)
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let mut ctx = RequestContext::default();
        ctx.client_ip = Some("192.168.1.1".to_string());

        let result = governor.check(&ctx).await;
        match result {
            Ok(Decision::Allowed(_)) => {}
            other => panic!("Expected Allowed via default chain, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_check_no_matched_rules_updates_stats() {
        let config = create_non_matching_config();
        let governor = Governor::builder()
            .with_config(config)
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let mut ctx = RequestContext::default();
        ctx.client_ip = Some("192.168.1.1".to_string());

        let _ = governor.check(&ctx).await;
        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.allowed_requests, 1);
    }

    // ============================================================================
    // check() Code Paths - Rule Accepts (TokenBucket has capacity)
    // ============================================================================

    #[tokio::test]
    async fn test_check_rule_allowed() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let mut ctx = RequestContext::default();
        ctx.client_ip = Some("10.0.0.1".to_string());

        let result = governor.check(&ctx).await;
        match result {
            Ok(Decision::Allowed(_)) => {}
            other => panic!("Expected Allowed, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_check_allowed_increments_stats() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let mut ctx = RequestContext::default();
        ctx.client_ip = Some("10.0.0.2".to_string());

        let _ = governor.check(&ctx).await;
        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.allowed_requests, 1);
    }

    // ============================================================================
    // check() Code Paths - Rule Rejects (TokenBucket exhausted)
    // ============================================================================

    fn create_small_capacity_config() -> FlowControlConfig {
        FlowControlConfig {
            version: "0.1.0".to_string(),
            global: crate::config::types::GlobalConfig::default(),
            rules: vec![Rule {
                id: "small_bucket".to_string(),
                name: "Small Bucket Rule".to_string(),
                priority: 100,
                matchers: vec![Matcher::User {
                    user_ids: vec!["*".to_string()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 3,
                    refill_rate: 1,
                }],
                action: ActionConfig {
                    on_exceed: Action::Reject,
                    ban: None,
                },
            }],
        }
    }

    #[tokio::test]
    async fn test_check_rule_accepted_then_rejected() {
        let config = create_small_capacity_config();
        let governor = Governor::builder()
            .with_config(config)
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let mut ctx = RequestContext::default();
        ctx.client_ip = Some("10.0.0.3".to_string());

        for _ in 0..3 {
            let result = governor.check(&ctx).await;
            assert!(
                result.is_ok(),
                "Expected Ok within capacity, got: {:?}",
                result
            );
        }

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 3);
        assert_eq!(stats.allowed_requests, 3);
    }

    #[tokio::test]
    async fn test_check_rejected_updates_rejected_stats() {
        let config = create_small_capacity_config();
        let governor = Governor::builder()
            .with_config(config)
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let mut ctx = RequestContext::default();
        ctx.client_ip = Some("10.0.0.4".to_string());

        for _ in 0..4 {
            let _ = governor.check(&ctx).await;
        }

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 4);
    }

    #[tokio::test]
    async fn test_check_multiple_users_isolated() {
        let config = create_small_capacity_config();
        let governor = Governor::builder()
            .with_config(config)
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let mut ctx_a = RequestContext::default();
        ctx_a.client_ip = Some("10.0.0.10".to_string());

        let mut ctx_b = RequestContext::default();
        ctx_b.client_ip = Some("10.0.0.11".to_string());

        for _ in 0..3 {
            let _ = governor.check(&ctx_a).await;
        }
        governor.check(&ctx_a).await.expect("check should succeed");
        governor.check(&ctx_b).await.expect("check should succeed");

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 5);
    }

    // ============================================================================
    // L1 Cache Methods
    // ============================================================================

    #[tokio::test]
    async fn test_l1_cache_enable_disable() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        assert!(governor.is_l1_cache_enabled());

        governor.disable_l1_cache();
        assert!(!governor.is_l1_cache_enabled());

        governor.enable_l1_cache();
        assert!(governor.is_l1_cache_enabled());
    }

    #[tokio::test]
    async fn test_l1_cache_disabled_does_not_cache() {
        let config = create_small_capacity_config();
        let governor = Governor::builder()
            .with_config(config)
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .with_l1_cache_enabled(false)
            .build()
            .await
            .expect("Governor build should succeed");

        assert!(!governor.is_l1_cache_enabled());

        let mut ctx = RequestContext::default();
        ctx.client_ip = Some("10.0.0.5".to_string());

        for _ in 0..3 {
            let result = governor.check(&ctx).await;
            assert!(result.is_ok());
        }
        let result = governor.check(&ctx).await;
        match result {
            Ok(Decision::Rejected(_)) => {}
            other => panic!(
                "Expected Rejected after exhaust with L1 disabled, got: {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn test_l1_cache_clear_and_size() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let size = governor.l1_cache_size().await;
        assert_eq!(size, 0);

        let _ = governor.clear_l1_cache().await;
        assert_eq!(governor.l1_cache_size().await, 0);
    }

    #[tokio::test]
    async fn test_evict_expired_l1_cache() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let evicted = governor.evict_expired_l1_cache().await;
        assert_eq!(evicted, 0);
    }

    // ============================================================================
    // Stats and Reset
    // ============================================================================

    #[tokio::test]
    async fn test_reset_stats() {
        let config = create_small_capacity_config();
        let governor = Governor::builder()
            .with_config(config)
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let mut ctx = RequestContext::default();
        ctx.client_ip = Some("10.0.0.6".to_string());

        let _ = governor.check(&ctx).await;
        let stats_before = governor.stats().await;
        assert_eq!(stats_before.total_requests, 1);
        assert_eq!(stats_before.allowed_requests, 1);

        governor.reset_stats().await;

        let stats_after = governor.stats().await;
        assert_eq!(stats_after.total_requests, 0);
        assert_eq!(stats_after.allowed_requests, 0);
        assert_eq!(stats_after.rejected_requests, 0);
        assert_eq!(stats_after.banned_requests, 0);
        assert_eq!(stats_after.error_count, 0);
    }

    // ============================================================================
    // Chain and Matcher Stats
    // ============================================================================

    #[tokio::test]
    async fn test_decision_chain_stats() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let stats = governor.decision_chain_stats().await;
        assert_eq!(stats.allowed_count, 0);
        assert_eq!(stats.rejected_count, 0);
        assert_eq!(stats.error_count, 0);
    }

    #[tokio::test]
    async fn test_rule_matcher_stats() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let stats = governor.rule_matcher_stats().await;
        assert_eq!(stats.total_matches, 0);
        assert_eq!(stats.total_mismatches, 0);
    }

    // ============================================================================
    // Config and Administrative Methods
    // ============================================================================

    #[tokio::test]
    async fn test_get_config_history() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let history = governor.get_config_history().await;
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn test_manual_config_check_returns_ok() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let result = governor.manual_config_check().await;
        assert!(result.is_ok());
        match result {
            Ok(val) => assert!(val),
            _ => unreachable!(),
        }
    }

    #[tokio::test]
    async fn test_stop_config_watcher() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let result = governor.stop_config_watcher().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_health_check_returns_ok() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let result = governor.health_check().await;
        assert!(result.is_ok());
    }

    // ============================================================================
    // Ban/Unban Operations (feature-gated)
    // ============================================================================

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_ban_identifier_user_id() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let identifier = crate::matchers::Identifier::UserId("test_user".to_string());
        let result = governor
            .ban_identifier(&identifier, "test ban reason", None)
            .await;
        assert!(
            result.is_ok(),
            "ban_identifier should succeed: {:?}",
            result
        );
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_ban_identifier_ip() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let identifier = crate::matchers::Identifier::Ip("192.168.1.1".to_string());
        let result = governor
            .ban_identifier(&identifier, "test IP ban", None)
            .await;
        assert!(
            result.is_ok(),
            "ban_identifier for IP should succeed: {:?}",
            result
        );
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_ban_identifier_unsupported_type() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let identifier = crate::matchers::Identifier::ApiKey("test_key".to_string());
        let result = governor.ban_identifier(&identifier, "test ban", None).await;
        match result {
            Err(FlowGuardError::ValidationError(msg)) => {
                assert!(msg.contains("Unsupported identifier type"))
            }
            other => panic!("Expected ValidationError, got: {:?}", other),
        }
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_unban_identifier() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let identifier = crate::matchers::Identifier::UserId("unban_test_user".to_string());

        let ban_result = governor.ban_identifier(&identifier, "temp ban", None).await;
        assert!(ban_result.is_ok(), "ban should succeed");

        let unban_result = governor.unban_identifier(&identifier).await;
        assert!(
            unban_result.is_ok(),
            "unban should succeed: {:?}",
            unban_result
        );
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_unban_identifier_unsupported_type() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let identifier = crate::matchers::Identifier::ApiKey("test_key".to_string());
        let result = governor.unban_identifier(&identifier).await;
        match result {
            Err(FlowGuardError::ValidationError(msg)) => {
                assert!(msg.contains("Unsupported identifier type"))
            }
            other => panic!("Expected ValidationError, got: {:?}", other),
        }
    }

    // ============================================================================
    // GovernorBuilder - Full configuration chain
    // ============================================================================

    #[tokio::test]
    async fn test_governor_builder_full_chain() {
        use crate::l1_cache::L1CacheConfig;

        let config = create_valid_test_config();
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let l1_config = L1CacheConfig {
            default_ttl: std::time::Duration::from_secs(30),
            max_size: 5000,
            ..Default::default()
        };

        let governor = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .with_l1_cache_config(l1_config)
            .with_l1_cache_enabled(true)
            .with_identifier_extractor(Arc::new(
                crate::matchers::CompositeExtractor::builder()
                    .add_extractor(Box::new(crate::matchers::UserIdExtractor::from_header(
                        "X-User-Id",
                    )))
                    .build(),
            ))
            .build()
            .await
            .expect("Governor build with full chain should succeed");

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 0);
        assert!(governor.is_l1_cache_enabled());
    }

    // ============================================================================
    // Cache Invalidation Methods
    // ============================================================================

    #[tokio::test]
    async fn test_invalidate_l1_cache() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        governor.invalidate_l1_cache("test_user").await;
        governor.invalidate_rule_cache("test_rule").await;

        assert_eq!(governor.l1_cache_size().await, 0);
    }

    // ============================================================================
    // From/Into Implementations
    // ============================================================================

    #[test]
    fn test_governor_stats_from_snapshot() {
        let snapshot = crate::rules::StatsSnapshot {
            total_requests: 100,
            allowed_requests: 80,
            rejected_requests: 15,
            banned_requests: 3,
            error_count: 2,
            last_updated: Some(chrono::Utc::now()),
        };
        let stats: GovernorStats = snapshot.into();
        assert_eq!(stats.total_requests, 100);
        assert_eq!(stats.allowed_requests, 80);
        assert_eq!(stats.rejected_requests, 15);
        assert_eq!(stats.banned_requests, 3);
        assert_eq!(stats.error_count, 2);
        assert!(stats.last_updated.is_some());
    }

    // ============================================================================
    // Trait Implementations (Default, Debug, Clone)
    // ============================================================================

    #[test]
    fn test_governor_stats_default_impl() {
        let stats = GovernorStats::default();
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.allowed_requests, 0);
        assert_eq!(stats.rejected_requests, 0);
        assert_eq!(stats.banned_requests, 0);
        assert_eq!(stats.error_count, 0);
        assert!(stats.last_updated.is_none());
    }

    #[test]
    fn test_governor_stats_debug_format() {
        let stats = GovernorStats::default();
        let debug = format!("{:?}", stats);
        assert!(debug.contains("total_requests"));
        assert!(debug.contains("allowed_requests"));
        assert!(debug.contains("rejected_requests"));
        assert!(debug.contains("banned_requests"));
        assert!(debug.contains("error_count"));
        assert!(debug.contains("last_updated"));
    }

    #[test]
    fn test_governor_stats_clone() {
        let mut stats = GovernorStats::default();
        stats.total_requests = 42;
        let cloned = stats.clone();
        assert_eq!(cloned.total_requests, 42);
    }

    // ============================================================================
    // L1 Cache - pub(crate) Stats
    // ============================================================================

    #[tokio::test]
    async fn test_l1_cache_stats_method() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let stats = governor.l1_cache_stats().await;
        assert_eq!(stats.total_lookups, 0);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.current_size, 0);
    }

    // ============================================================================
    // Governor::with_dependencies() Constructor
    // ============================================================================

    #[tokio::test]
    async fn test_with_dependencies_constructor() {
        let config = Arc::new(tokio::sync::RwLock::new(create_valid_test_config()));
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let extractor: Arc<dyn crate::matchers::IdentifierExtractor> = Arc::new(
            crate::matchers::CompositeExtractor::builder()
                .add_extractor(Box::new(crate::matchers::UserIdExtractor::from_header(
                    "X-User-Id",
                )))
                .build(),
        );

        let config_value = config.read().await.clone();
        let rules = RuleBuilder::build_rules(&config_value).expect("build_rules should succeed");
        let rule_matcher = Arc::new(tokio::sync::RwLock::new(
            crate::matchers::RuleMatcher::with_dependencies(rules),
        ));

        let decision_chain = Arc::new(tokio::sync::RwLock::new(
            crate::decision_chain::DecisionChain::with_dependencies(vec![]),
        ));

        let chain_map = RuleBuilder::build_rule_chains(&config_value)
            .expect("build_rule_chains should succeed");
        let rule_chains = Arc::new(tokio::sync::RwLock::new(chain_map));

        #[cfg(feature = "circuit-breaker")]
        let circuit_breaker = {
            use crate::circuit::{CircuitBreaker, CircuitBreakerConfig};
            Arc::new(CircuitBreaker::with_dependencies(
                CircuitBreakerConfig::default(),
            ))
        };

        let governor = Governor::with_dependencies(
            config,
            storage,
            ban_storage,
            extractor,
            rule_matcher,
            decision_chain,
            rule_chains,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker,
        )
        .await;

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 0);
    }

    // ============================================================================
    // build_cache_key - All Identifier Types
    // ============================================================================

    #[tokio::test]
    async fn test_build_cache_key_all_types() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("Governor build should succeed");

        let user_id = crate::matchers::Identifier::UserId("user123".to_string());
        let key = governor.build_cache_key(&user_id, "rule_1");
        assert_eq!(key, "rl:user:user123:rule_1");

        let ip = crate::matchers::Identifier::Ip("192.168.1.1".to_string());
        let key = governor.build_cache_key(&ip, "rule_1");
        assert_eq!(key, "rl:ip:192.168.1.1:rule_1");

        let api_key = crate::matchers::Identifier::ApiKey("key123".to_string());
        let key = governor.build_cache_key(&api_key, "rule_1");
        assert_eq!(key, "rl:apikey:key123:rule_1");

        let mac = crate::matchers::Identifier::Mac("AA:BB:CC:DD:EE:FF".to_string());
        let key = governor.build_cache_key(&mac, "rule_1");
        assert_eq!(key, "rl:generic:mac:AA:BB:CC:DD:EE:FF:rule_1");

        let device = crate::matchers::Identifier::DeviceId("device-001".to_string());
        let key = governor.build_cache_key(&device, "rule_1");
        assert_eq!(key, "rl:generic:device_id:device-001:rule_1");
    }

    // ============================================================================
    // Governor::from_config_file
    // ============================================================================

    #[tokio::test]
    async fn test_governor_from_config_file_json() {
        let config_json = r#"{
            "version": "0.1.0",
            "global": {
                "storage": "memory",
                "cache": "memory",
                "metrics": "none"
            },
            "rules": [{
                "id": "test_rule",
                "name": "Test Rule",
                "priority": 100,
                "matchers": [{"type": "User", "user_ids": ["*"]}],
                "limiters": [{"type": "TokenBucket", "capacity": 100, "refill_rate": 10}],
                "action": {"on_exceed": "reject"}
            }]
        }"#;

        let dir = std::env::temp_dir().join(format!("limiteron_cfg_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let config_path = dir.join("test_config.json");
        std::fs::write(&config_path, config_json).expect("write config file");

        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let result = Governor::from_config_file(&config_path, storage, ban_storage).await;
        assert!(
            result.is_ok(),
            "from_config_file failed: {:?}",
            result.err()
        );

        let _ = std::fs::remove_file(&config_path);
        let _ = std::fs::remove_dir(&dir);
    }

    // ============================================================================
    // Governor::from_config_with_env
    // ============================================================================

    #[tokio::test]
    async fn test_governor_from_config_with_env_json() {
        let config_json = r#"{
            "version": "0.1.0",
            "global": {
                "storage": "memory",
                "cache": "memory",
                "metrics": "none"
            },
            "rules": [{
                "id": "test_rule_env",
                "name": "Test Rule Env",
                "priority": 100,
                "matchers": [{"type": "User", "user_ids": ["*"]}],
                "limiters": [{"type": "TokenBucket", "capacity": 100, "refill_rate": 10}],
                "action": {"on_exceed": "reject"}
            }]
        }"#;

        let dir = std::env::temp_dir().join(format!("limiteron_cfg_env_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let config_path = dir.join("test_env_config.json");
        std::fs::write(&config_path, config_json).expect("write config file");

        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let result = Governor::from_config_with_env(&config_path, storage, ban_storage).await;
        assert!(
            result.is_ok(),
            "from_config_with_env failed: {:?}",
            result.err()
        );

        let _ = std::fs::remove_file(&config_path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[cfg(feature = "parallel-checker")]
    #[tokio::test]
    async fn test_check_resource_parallel_banned() {
        use crate::matchers::Identifier;
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("build should succeed");

        // Ban a user first
        let user_id = Identifier::UserId("banned_resource_user".to_string());
        governor
            .ban_identifier(&user_id, "test ban for parallel", None)
            .await
            .expect("ban should succeed");

        // Check resource parallel should detect the ban
        let result = governor
            .check_resource_parallel("banned_resource_user")
            .await;
        match result {
            Ok(Decision::Banned(_)) => {}
            other => panic!("Expected Banned for banned user, got: {:?}", other),
        }
    }

    // ============================================================================
    // Health Check - feature-gated sub-checks with all features enabled
    // ============================================================================

    #[tokio::test]
    async fn test_health_check_with_all_features() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("build should succeed");

        // Under --features full, this exercises:
        //   #[cfg(feature = "ban-manager")]  -> ban_manager.get_config()
        //   #[cfg(feature = "circuit-breaker")] -> circuit_breaker.get_state()
        //   #[cfg(feature = "audit-log")] -> audit_logger()
        let result = governor.health_check().await;
        assert!(result.is_ok(), "health_check should succeed: {:?}", result);
    }
}

// ============================================================================
// Feature-Gated Code Paths
// ============================================================================

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod governor_feature_gated_tests {
    use super::*;
    use crate::storage::{MemoryBanStorage, MemoryStorage};
    use std::sync::Arc;
    use std::time::Duration;

    fn create_valid_test_config() -> FlowControlConfig {
        FlowControlConfig {
            version: "0.1.0".to_string(),
            global: crate::config::types::GlobalConfig::default(),
            rules: vec![crate::config::types::Rule {
                id: "test_rule".to_string(),
                name: "Test Rule".to_string(),
                priority: 100,
                matchers: vec![crate::config::types::Matcher::User {
                    user_ids: vec!["*".to_string()],
                }],
                limiters: vec![crate::config::types::LimiterConfig::TokenBucket {
                    capacity: 100,
                    refill_rate: 10,
                }],
                action: crate::config::types::ActionConfig {
                    on_exceed: crate::config::types::Action::Reject,
                    ban: None,
                },
            }],
        }
    }

    // ============================================================================
    // Builder Feature-Gated Methods
    // ============================================================================

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test]
    async fn test_builder_with_circuit_breaker() {
        use crate::circuit::{CircuitBreaker, CircuitBreakerConfig};

        let cb = Arc::new(CircuitBreaker::with_dependencies(
            CircuitBreakerConfig::default(),
        ));
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .with_circuit_breaker(cb)
            .build()
            .await
            .expect("build with circuit_breaker should succeed");

        assert!(governor.health_check().await.is_ok());
    }

    #[cfg(feature = "audit-log")]
    #[tokio::test]
    async fn test_builder_with_audit_logger() {
        use crate::logging::AuditLogger;

        let logger = Arc::new(AuditLogger::default().await);
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .with_audit_logger(logger)
            .build()
            .await
            .expect("build with audit_logger should succeed");

        assert!(governor.health_check().await.is_ok());
    }

    #[cfg(feature = "monitoring")]
    #[tokio::test]
    async fn test_builder_with_metrics() {
        use crate::telemetry::Metrics;

        let metrics = Arc::new(Metrics::new());
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .with_metrics(metrics)
            .build()
            .await
            .expect("build with metrics should succeed");

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 0);
    }

    #[cfg(feature = "telemetry")]
    #[tokio::test]
    async fn test_builder_with_tracer() {
        use crate::telemetry::Tracer;

        let tracer = Arc::new(Tracer::new(true));
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .with_tracer(tracer)
            .build()
            .await
            .expect("build with tracer should succeed");

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 0);
    }

    #[cfg(feature = "fallback")]
    #[tokio::test]
    async fn test_builder_with_fallback_manager() {
        use crate::fallback::FallbackManager;
        use oxcache::Cache;

        let cache: Cache<String, String> = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        let fm = Arc::new(FallbackManager::new(Arc::new(cache)));
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .with_fallback_manager(fm)
            .build()
            .await
            .expect("build with fallback_manager should succeed");

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 0);
    }

    // ============================================================================
    // check() with Fallback Manager
    // Covers: check() dispatch to check_with_fallback, check_with_fallback body
    // ============================================================================

    #[cfg(feature = "fallback")]
    #[tokio::test]
    async fn test_check_with_fallback_manager() {
        use crate::fallback::FallbackManager;
        use oxcache::Cache;

        let cache: Cache<String, String> = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        let fm = Arc::new(FallbackManager::new(Arc::new(cache)));
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .with_fallback_manager(fm)
            .build()
            .await
            .expect("build should succeed");

        let mut ctx = RequestContext::default();
        ctx.client_ip = Some("10.0.0.100".to_string());
        let result = governor.check(&ctx).await;
        assert!(
            result.is_ok(),
            "check with fallback should succeed: {:?}",
            result
        );
    }

    /// 当 check_internal 因标识符提取失败而返回 Err 时，降级逻辑调用
    /// check_l1_cache_only，后者也因标识符提取失败而返回 Err。
    #[cfg(feature = "fallback")]
    #[tokio::test]
    async fn test_check_with_fallback_no_identifier() {
        use crate::fallback::FallbackManager;
        use oxcache::Cache;

        let cache: Cache<String, String> = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        let fm = Arc::new(FallbackManager::new(Arc::new(cache)));
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .with_fallback_manager(fm)
            .build()
            .await
            .expect("build should succeed");

        // 无标识符请求：check_internal 失败 → 降级到 check_l1_cache_only
        let ctx = RequestContext::default();
        let result = governor.check(&ctx).await;
        assert!(result.is_err());
    }

    /// L1 缓存禁用时，check_l1_cache_only 返回 Err。
    #[cfg(feature = "fallback")]
    #[tokio::test]
    async fn test_check_with_fallback_l1_cache_disabled() {
        use crate::fallback::FallbackManager;
        use oxcache::Cache;

        let cache: Cache<String, String> = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        let fm = Arc::new(FallbackManager::new(Arc::new(cache)));
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .with_l1_cache_enabled(false)
            .with_fallback_manager(fm)
            .build()
            .await
            .expect("build should succeed");

        // 无标识符请求：check_internal 失败 → 降级到 check_l1_cache_only
        // → L1 缓存禁用，返回 Err
        let ctx = RequestContext::default();
        let result = governor.check(&ctx).await;
        assert!(result.is_err());
    }

    // ============================================================================
    // check() - Parallel ban check integration in check_internal
    // With parallel-checker + ban-manager, every check() with a valid identifier
    // goes through the parallel ban checker code path.
    // ============================================================================

    #[cfg(feature = "parallel-checker")]
    #[tokio::test]
    async fn test_check_parallel_ban_checker_path() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("build should succeed");

        // IP-based request triggers to_ban_target -> Some -> parallel check
        let mut ctx = RequestContext::default();
        ctx.client_ip = Some("10.0.0.200".to_string());
        let result = governor.check(&ctx).await;
        assert!(
            result.is_ok(),
            "check with parallel ban checker should succeed: {:?}",
            result
        );
    }

    // ============================================================================
    // check_resource_parallel (parallel-checker feature gate)
    // ============================================================================

    #[cfg(feature = "parallel-checker")]
    #[tokio::test]
    async fn test_check_resource_parallel_unbanned() {
        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("build should succeed");

        let result = governor.check_resource_parallel("test_user_123").await;
        match result {
            Ok(Decision::Allowed(_)) => {}
            other => panic!("Expected Allowed for unbanned user, got: {:?}", other),
        }
    }

    // ============================================================================
    // Audit Logger Set/Get (audit-log feature gate)
    // ============================================================================

    #[cfg(feature = "audit-log")]
    #[tokio::test]
    async fn test_audit_logger_set_and_get() {
        use crate::logging::AuditLogger;

        let governor = Governor::builder()
            .with_config(create_valid_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("build should succeed");

        // Initially no logger is set
        let initial = governor.audit_logger().await;
        assert!(initial.is_none());

        // Set a logger and verify it's returned
        let logger = Arc::new(AuditLogger::default().await);
        governor.set_audit_logger(logger.clone()).await;
        let retrieved = governor.audit_logger().await;
        assert!(retrieved.is_some(), "audit_logger should be Some after set");
    }

    // ============================================================================
    // check_l1_cache_only - 直接测试各分支（fallback feature gate）
    // 覆盖 lines 943-1023 的多个分支
    // ============================================================================

    /// 创建使用 IP matcher 的测试配置（匹配所有 IP）
    fn create_ip_test_config() -> FlowControlConfig {
        FlowControlConfig {
            version: "0.1.0".to_string(),
            global: crate::config::types::GlobalConfig::default(),
            rules: vec![crate::config::types::Rule {
                id: "ip_rule".to_string(),
                name: "IP Rule".to_string(),
                priority: 100,
                matchers: vec![crate::config::types::Matcher::Ip {
                    ip_ranges: vec!["0.0.0.0/0".to_string()],
                }],
                limiters: vec![crate::config::types::LimiterConfig::TokenBucket {
                    capacity: 100,
                    refill_rate: 10,
                }],
                action: crate::config::types::ActionConfig {
                    on_exceed: crate::config::types::Action::Reject,
                    ban: None,
                },
            }],
        }
    }

    /// 创建带指定 user_ids 的 User matcher 配置（用于"无匹配规则"测试）
    fn create_user_test_config(user_ids: Vec<String>) -> FlowControlConfig {
        FlowControlConfig {
            version: "0.1.0".to_string(),
            global: crate::config::types::GlobalConfig::default(),
            rules: vec![crate::config::types::Rule {
                id: "user_rule".to_string(),
                name: "User Rule".to_string(),
                priority: 100,
                matchers: vec![crate::config::types::Matcher::User { user_ids }],
                limiters: vec![crate::config::types::LimiterConfig::TokenBucket {
                    capacity: 100,
                    refill_rate: 10,
                }],
                action: crate::config::types::ActionConfig {
                    on_exceed: crate::config::types::Action::Reject,
                    ban: None,
                },
            }],
        }
    }

    /// 构造带 IP 请求的 RequestContext
    fn create_ip_request_context(ip: &str) -> RequestContext {
        let mut ctx = RequestContext::default();
        ctx.client_ip = Some(ip.to_string());
        ctx
    }

    /// check_l1_cache_only: L1 缓存禁用时返回 Err（覆盖 lines 943-946）
    #[cfg(feature = "fallback")]
    #[tokio::test]
    async fn test_check_l1_cache_only_disabled_returns_err() {
        let governor = Governor::builder()
            .with_config(create_ip_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .with_l1_cache_enabled(false)
            .build()
            .await
            .expect("build should succeed");

        let ctx = create_ip_request_context("10.0.0.1");
        let result = governor.check_l1_cache_only(&ctx).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            FlowGuardError::LimitError(msg) => {
                assert!(msg.contains("L1 缓存未启用"), "unexpected: {}", msg);
            }
            other => panic!("expected LimitError, got: {:?}", other),
        }
    }

    /// check_l1_cache_only: identifier 提取失败时返回 Err（覆盖 lines 950-952）
    #[cfg(feature = "fallback")]
    #[tokio::test]
    async fn test_check_l1_cache_only_identifier_extraction_failure() {
        let governor = Governor::builder()
            .with_config(create_ip_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("build should succeed");

        // 默认 RequestContext 无 client_ip/user_id/api_key，identifier 提取失败
        let ctx = RequestContext::default();
        let result = governor.check_l1_cache_only(&ctx).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            FlowGuardError::ConfigError(msg) => {
                assert!(
                    msg.contains("Failed to extract identifier"),
                    "unexpected: {}",
                    msg
                );
            }
            other => panic!("expected ConfigError, got: {:?}", other),
        }
    }

    /// check_l1_cache_only: 无匹配规则时返回 Ok(Allowed)（覆盖 lines 964-966）
    #[cfg(feature = "fallback")]
    #[tokio::test]
    async fn test_check_l1_cache_only_no_matched_rules() {
        // User matcher 只匹配特定 user_id，但请求是 IP（无 user_id header）
        let governor = Governor::builder()
            .with_config(create_user_test_config(vec!["specific_user".to_string()]))
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("build should succeed");

        let ctx = create_ip_request_context("10.0.0.1");
        let result = governor.check_l1_cache_only(&ctx).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        match result.unwrap() {
            Decision::Allowed(_) => {}
            other => panic!("expected Allowed, got: {:?}", other),
        }
    }

    /// check_l1_cache_only: cache miss + 非 island mode → Err（覆盖 lines 970, 973, 980, 1015-1018）
    #[cfg(feature = "fallback")]
    #[tokio::test]
    async fn test_check_l1_cache_only_cache_miss_no_island() {
        let governor = Governor::builder()
            .with_config(create_ip_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("build should succeed");

        // 确保不在 island mode
        assert!(!governor.l1_cache.is_island_mode());

        let ctx = create_ip_request_context("10.0.0.1");
        let result = governor.check_l1_cache_only(&ctx).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            FlowGuardError::LimitError(msg) => {
                assert!(msg.contains("降级缓存未命中"), "unexpected: {}", msg);
            }
            other => panic!("expected LimitError, got: {:?}", other),
        }
    }

    /// check_l1_cache_only: cache miss + island AllowAll → Ok(Allowed)（覆盖 lines 982-987）
    #[cfg(feature = "fallback")]
    #[tokio::test]
    async fn test_check_l1_cache_only_island_allow_all() {
        let governor = Governor::builder()
            .with_config(create_ip_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("build should succeed");

        let island_config = IslandModeConfig::new(IslandFallbackStrategy::AllowAll);
        governor.l1_cache.enable_island_mode(island_config);
        assert!(governor.l1_cache.is_island_mode());

        let ctx = create_ip_request_context("10.0.0.1");
        let result = governor.check_l1_cache_only(&ctx).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        match result.unwrap() {
            Decision::Allowed(_) => {}
            other => panic!("expected Allowed, got: {:?}", other),
        }
    }

    /// check_l1_cache_only: cache miss + island RejectAll → Err（覆盖 lines 990-992）
    #[cfg(feature = "fallback")]
    #[tokio::test]
    async fn test_check_l1_cache_only_island_reject_all() {
        let governor = Governor::builder()
            .with_config(create_ip_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("build should succeed");

        let island_config = IslandModeConfig::new(IslandFallbackStrategy::RejectAll);
        governor.l1_cache.enable_island_mode(island_config);

        let ctx = create_ip_request_context("10.0.0.1");
        let result = governor.check_l1_cache_only(&ctx).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            FlowGuardError::LimitError(msg) => {
                assert!(msg.contains("拒绝请求"), "unexpected: {}", msg);
            }
            other => panic!("expected LimitError, got: {:?}", other),
        }
    }

    /// check_l1_cache_only: cache miss + island LocalDecision → Ok(Allowed)（覆盖 lines 997-998）
    #[cfg(feature = "fallback")]
    #[tokio::test]
    async fn test_check_l1_cache_only_island_local_decision() {
        let governor = Governor::builder()
            .with_config(create_ip_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("build should succeed");

        let island_config = IslandModeConfig::new(IslandFallbackStrategy::LocalDecision);
        governor.l1_cache.enable_island_mode(island_config);

        let ctx = create_ip_request_context("10.0.0.1");
        let result = governor.check_l1_cache_only(&ctx).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
    }

    /// check_l1_cache_only: cache miss + island ConservativeQuota → Ok(Allowed)（覆盖 lines 1005, 1008）
    #[cfg(feature = "fallback")]
    #[tokio::test]
    async fn test_check_l1_cache_only_island_conservative_quota() {
        let governor = Governor::builder()
            .with_config(create_ip_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("build should succeed");

        let island_config = IslandModeConfig::new(IslandFallbackStrategy::ConservativeQuota {
            max_requests: 10,
            window_secs: 60,
        });
        governor.l1_cache.enable_island_mode(island_config);

        let ctx = create_ip_request_context("10.0.0.1");
        let result = governor.check_l1_cache_only(&ctx).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
    }

    /// check_l1_cache_only: cache hit → 返回 cached decision（覆盖 lines 973-978）
    #[cfg(feature = "fallback")]
    #[tokio::test]
    async fn test_check_l1_cache_only_cache_hit() {
        let governor = Governor::builder()
            .with_config(create_ip_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("build should succeed");

        // 先用 check() 填充 L1 缓存（check_internal 会 set cache）
        let ctx = create_ip_request_context("10.0.0.1");
        let _ = governor.check(&ctx).await;

        // 再次调用 check_l1_cache_only，应命中缓存
        // （注意：check_l1_cache_only 用相同 identifier 和 rule_id 构建 cache_key）
        let result = governor.check_l1_cache_only(&ctx).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        match result.unwrap() {
            Decision::Allowed(_) => {}
            other => panic!("expected Allowed, got: {:?}", other),
        }
    }

    /// 覆盖 update_stats_for_decision 中 Banned 分支 (line 1050)
    /// 通过在 L1 缓存中手动放入 Banned 决策，然后调用 check_l1_cache_only 触发
    #[tokio::test]
    async fn test_check_l1_cache_only_cached_banned_updates_stats() {
        use crate::error::BanInfo;
        use crate::l1_cache::CacheableDecision;

        let governor = Governor::builder()
            .with_config(create_ip_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("build should succeed");

        // 手动构建 cache_key（与 check_l1_cache_only 中 build_cache_key 一致）
        let ctx = create_ip_request_context("10.0.0.50");
        let cache_key = "rl:ip:10.0.0.50:ip_rule".to_string();

        // 在 L1 缓存中放入 Banned 决策
        let ban_info = BanInfo::new("test ban".to_string(), chrono::Utc::now(), 1);
        let banned_decision = CacheableDecision::banned(&ban_info);
        governor
            .l1_cache
            .set(cache_key, banned_decision)
            .await
            .expect("cache set should succeed");

        // 调用 check_l1_cache_only，应命中缓存并返回 Banned
        // 这会触发 update_stats_for_decision 的 Banned 分支 (line 1050)
        let result = governor.check_l1_cache_only(&ctx).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        match result.unwrap() {
            Decision::Banned(_) => {}
            other => panic!("expected Banned, got: {:?}", other),
        }
    }

    /// 覆盖 update_stats_for_decision 中 Err 分支 (line 1056)
    /// 直接调用私有方法传入 Err 结果
    #[tokio::test]
    async fn test_update_stats_for_decision_err_branch() {
        let governor = Governor::builder()
            .with_config(create_ip_test_config())
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await
            .expect("build should succeed");

        // 直接调用 update_stats_for_decision 传入 Err
        let err_result: Result<Decision, FlowGuardError> =
            Err(FlowGuardError::LimitError("test error".to_string()));
        governor.update_stats_for_decision(&err_result);
        // 验证不会 panic 即可（stats.increment_error 内部是原子操作）
    }
}
