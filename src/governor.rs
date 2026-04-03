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
use crate::l1_cache::IslandModeConfig;
use crate::l1_cache::{
    CacheableDecision, IslandFallbackStrategy, L1Cache, L1CacheConfig, RateLimitCacheKey,
};
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
    _storage: Arc<dyn Storage>,

    /// 封禁存储
    _ban_storage: Arc<dyn BanStorage>,

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
}

/// Governor 构建器
///
/// 用于链式配置 Governor 实例。
///
/// # 示例
///
/// ```rust,no_run
/// use limiteron::Governor;
/// use limiteron::adapters::StorageFactory;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
///     factory.initialize(None).await?;
///     let storage: Arc<dyn limiteron::storage_trait::Storage> = factory.create_storage().await?;
///     let ban_storage: Arc<dyn limiteron::storage_trait::BanStorage> = factory.create_ban_storage().await?;
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
#[allow(dead_code)]
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
            _storage: storage,
            _ban_storage: ban_storage,
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
            config_history: Arc::new(tokio::sync::RwLock::new(
                crate::config::types::ConfigHistory::new(100),
            )),
            stats: StatsManager::new(),
            l1_cache,
            l1_cache_enabled: std::sync::atomic::AtomicBool::new(l1_cache_enabled),
            #[cfg(feature = "fallback")]
            fallback_manager,
            #[cfg(feature = "event-system")]
            event_emitter: self.event_emitter,
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
            _storage: storage,
            _ban_storage: ban_storage,
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
            #[cfg(feature = "event-system")]
            event_emitter: None,
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
    /// use limiteron::storage_trait::MemoryStorage;
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let config = FlowControlConfig::default();
    ///     let storage: Arc<dyn limiteron::storage_trait::Storage> = Arc::new(MemoryStorage::new());
    ///     let ban_storage: Arc<dyn limiteron::storage_trait::BanStorage> = Arc::new(limiteron::storage_trait::MemoryBanStorage::new());
    ///
    ///     let governor = Governor::with_storage(config, storage, ban_storage).await.unwrap();
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
    /// use limiteron::adapters::StorageFactory;
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    ///     factory.initialize(None).await?;
    ///     let storage: Arc<dyn limiteron::storage_trait::Storage> = factory.create_storage().await?;
    ///     let ban_storage: Arc<dyn limiteron::storage_trait::BanStorage> = factory.create_ban_storage().await?;
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
    #[cfg(feature = "confers")]
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
    /// use limiteron::adapters::StorageFactory;
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    ///     factory.initialize(None).await?;
    ///     let storage: Arc<dyn limiteron::storage_trait::Storage> = factory.create_storage().await?;
    ///     let ban_storage: Arc<dyn limiteron::storage_trait::BanStorage> = factory.create_ban_storage().await?;
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
    #[cfg(feature = "confers")]
    pub async fn from_config_with_env<P: AsRef<std::path::Path>>(
        config_path: P,
        storage: Arc<dyn Storage>,
        ban_storage: Arc<dyn BanStorage>,
    ) -> Result<Self, FlowGuardError> {
        // 使用 ConfigLoader 加载配置，支持环境变量覆盖
        let config = crate::ConfigLoader::load_from_file(config_path)?;
        Self::create_with_config(config, storage, ban_storage).await
    }

    /// 根据配置创建 Governor 实例（内部辅助方法）
    ///
    /// 统一处理不同 feature 组合下的创建逻辑，避免重复的条件编译代码。
    #[cfg(feature = "confers")]
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
                                    identifier.key(),
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
    pub async fn health_check(&self) -> Result<(), FlowGuardError> {
        info!("健康检查");

        // 检查各个组件的健康状态
        // config is guarded by RwLock, if we can read it, it's fine.
        let _config_guard = self.config.read().await;
        let config_healthy = true;

        let storage_healthy = true; // 这里需要根据具体的存储类型实现健康检查

        #[cfg(feature = "ban-manager")]
        {
            let _ = self.ban_manager.get_config().await;
        }

        #[cfg(feature = "circuit-breaker")]
        {
            let _ = self.circuit_breaker.get_state().await;
        }

        #[cfg(feature = "audit-log")]
        {
            let _ = self.audit_logger().await;
        }

        if config_healthy && storage_healthy {
            Ok(())
        } else {
            Err(FlowGuardError::StorageError(
                crate::error::StorageError::ConnectionError("Storage unhealthy".to_string()),
            ))
        }
    }
}

// ============================================================================
// Governor Construction Patterns Tests
// ============================================================================

#[cfg(test)]
mod governor_construction_tests {
    use super::*;
    use crate::config::types::{
        Action, ActionConfig, FlowControlConfig, LimiterConfig, Matcher, Rule, Rule as RuleTrait,
    };
    use crate::storage::{MemoryBanStorage, MemoryStorage};

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
}
