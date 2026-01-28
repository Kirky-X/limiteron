//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! Governor 主控制器 - 重构版本
//!
//! 流量控制的核心控制器，重构后具有更好的模块化设计：
//! - 使用专门的并行封禁检查器提高性能
//! - 简化核心逻辑，提高可维护性
//! - 保持向后兼容性

use crate::config::{
    ConfigChangeRecord, ConfigHistory, FlowControlConfig, LimiterConfig, Matcher as ConfigMatcher,
};
use crate::constants::{SECONDS_PER_HOUR, SECONDS_PER_MINUTE};
use crate::decision_chain::{DecisionChain, DecisionNode};
#[cfg(feature = "ban-manager")]
use crate::error::BanInfo;
use crate::error::Decision;
use crate::error::FlowGuardError;
#[cfg(feature = "fallback")]
use crate::fallback::FallbackManager;
use crate::limiters::{
    ConcurrencyLimiter, FixedWindowLimiter, Limiter, SlidingWindowLimiter, TokenBucketLimiter,
};
use crate::log_redaction::{redact_ip, redact_user_id};
use crate::matchers::{
    CompositeCondition, ConditionEvaluator, IdentifierExtractor, IpRange, LogicalOperator,
    MatchCondition, RequestContext, Rule as MatcherRule, RuleMatcher,
};
use crate::storage::{BanStorage, Storage};
use dashmap::DashMap;
#[cfg(feature = "fallback")]
use oxcache::Cache;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, trace, warn};

// Conditional imports for optional features
#[cfg(feature = "audit-log")]
use crate::audit_log::AuditLogger;
#[cfg(feature = "ban-manager")]
use crate::ban_manager::BanManager;
#[cfg(feature = "circuit-breaker")]
use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
#[cfg(feature = "parallel-checker")]
use crate::matchers::Identifier;
#[cfg(feature = "parallel-checker")]
use crate::storage::BanTarget;
#[cfg(feature = "monitoring")]
use crate::telemetry::Metrics;
#[cfg(feature = "telemetry")]
use crate::telemetry::Tracer;
#[cfg(feature = "ban-manager")]
use crate::BanSource;

/// Governor 统计信息
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

/// Governor 主控制器
///
/// 重构后的 Governor，具有更清晰的职责分离和更好的性能。
pub struct Governor {
    /// 配置
    config: Arc<RwLock<FlowControlConfig>>,

    /// 存储后端
    storage: Arc<dyn Storage>,

    /// 封禁存储
    _ban_storage: Arc<dyn BanStorage>,

    /// 封禁管理器
    #[cfg(feature = "ban-manager")]
    ban_manager: Arc<BanManager>,

    /// 并行封禁检查器（新增）
    #[cfg(feature = "parallel-checker")]
    parallel_ban_checker: Arc<crate::parallel_ban_checker::ParallelBanChecker>,

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

    // 统计计数器
    total_requests: AtomicU64,
    allowed_requests: AtomicU64,
    rejected_requests: AtomicU64,
    banned_requests: AtomicU64,
    error_count: AtomicU64,
}

impl Governor {
    fn parse_duration(s: &str) -> Result<Duration, FlowGuardError> {
        let s = s.trim();
        let (num, unit) = if s.ends_with("ms") {
            (s.trim_end_matches("ms"), "ms")
        } else if s.ends_with('s') {
            (s.trim_end_matches('s'), "s")
        } else if s.ends_with('m') {
            (s.trim_end_matches('m'), "m")
        } else if s.ends_with('h') {
            (s.trim_end_matches('h'), "h")
        } else {
            return Err(FlowGuardError::ConfigError(format!(
                "Invalid duration format: {}",
                s
            )));
        };

        let val: u64 = num.parse().map_err(|_| {
            FlowGuardError::ConfigError(format!("Invalid duration number: {}", num))
        })?;

        match unit {
            "ms" => Ok(Duration::from_millis(val)),
            "s" => Ok(Duration::from_secs(val)),
            "m" => Ok(Duration::from_secs(val * SECONDS_PER_MINUTE)),
            "h" => Ok(Duration::from_secs(val * SECONDS_PER_HOUR)),
            _ => Err(FlowGuardError::ConfigError(format!(
                "Invalid duration unit '{}'. Valid units: ms, s, m, h",
                unit
            ))),
        }
    }

    fn build_rule_chains(
        config: &FlowControlConfig,
    ) -> Result<DashMap<String, DecisionChain>, FlowGuardError> {
        let chains = DashMap::new();

        for rule in &config.rules {
            let mut nodes: Vec<DecisionNode> = Vec::new();

            for (index, limiter_config) in rule.limiters.iter().enumerate() {
                let (limiter, type_name): (Arc<dyn Limiter>, &str) = match limiter_config {
                    LimiterConfig::TokenBucket {
                        capacity,
                        refill_rate,
                    } => (
                        Arc::new(TokenBucketLimiter::new(*capacity, *refill_rate)),
                        "TokenBucket",
                    ),
                    LimiterConfig::SlidingWindow {
                        window_size,
                        max_requests,
                    } => {
                        let duration = Self::parse_duration(window_size)?;
                        (
                            Arc::new(SlidingWindowLimiter::new(duration, *max_requests)),
                            "SlidingWindow",
                        )
                    }
                    LimiterConfig::FixedWindow {
                        window_size,
                        max_requests,
                    } => {
                        let duration = Self::parse_duration(window_size)?;
                        (
                            Arc::new(FixedWindowLimiter::new(duration, *max_requests)),
                            "FixedWindow",
                        )
                    }
                    LimiterConfig::Quota {
                        quota_type: _,
                        limit: _,
                        window: _,
                        alert_threshold: _,
                        overdraft: _,
                    } => {
                        // Quota limiter requires quota-control feature
                        warn!(
                            "QuotaLimiter requires 'quota-control' feature to be enabled, \
                             skipping Quota configuration"
                        );
                        continue;
                    }
                    LimiterConfig::Concurrency { max_concurrent } => (
                        Arc::new(ConcurrencyLimiter::new(*max_concurrent)),
                        "Concurrency",
                    ),
                    LimiterConfig::Custom { name, config: _ } => {
                        // CustomLimiter integration requires custom-limiter feature and
                        // manual registration via CustomLimiterRegistry
                        #[cfg(feature = "custom-limiter")]
                        warn!(
                            "CustomLimiter '{}' requires registration via CustomLimiterRegistry, skipping",
                            name
                        );
                        #[cfg(not(feature = "custom-limiter"))]
                        warn!(
                            "CustomLimiter '{}' skipped - custom-limiter feature not enabled",
                            name
                        );
                        continue;
                    }
                };

                let node = DecisionNode::new(
                    format!("{}_limiter_{}", rule.id, index),
                    format!("{} - {}", rule.name, type_name),
                    limiter,
                    100u16.saturating_sub(index as u16), // Priority: earlier limiters have higher priority
                );
                nodes.push(node);
            }

            chains.insert(rule.id.clone(), DecisionChain::new(nodes));
        }

        Ok(chains)
    }

    /// 从配置构建规则列表
    fn build_rules(config: &FlowControlConfig) -> Result<Vec<MatcherRule>, FlowGuardError> {
        let mut rules = Vec::new();

        for rule_config in &config.rules {
            let mut conditions: Vec<Box<dyn ConditionEvaluator>> = Vec::new();

            for matcher in &rule_config.matchers {
                let condition: Box<dyn ConditionEvaluator> = match matcher {
                    ConfigMatcher::User { user_ids } => {
                        Box::new(MatchCondition::User(user_ids.clone()))
                    }
                    ConfigMatcher::Ip { ip_ranges } => {
                        let ranges: Result<Vec<IpRange>, _> =
                            ip_ranges.iter().map(|s| s.parse()).collect();
                        Box::new(MatchCondition::Ip(ranges?))
                    }
                    ConfigMatcher::Geo { countries } => {
                        Box::new(MatchCondition::Geo(countries.clone()))
                    }
                    ConfigMatcher::ApiVersion { versions } => {
                        Box::new(MatchCondition::ApiVersion(versions.clone()))
                    }
                    ConfigMatcher::Device { device_types } => {
                        Box::new(MatchCondition::Device(device_types.clone()))
                    }
                    ConfigMatcher::Custom { name, config: _ } => {
                        let name = name.clone();
                        Box::new(MatchCondition::Custom(Arc::new(move |_context| {
                            tracing::warn!(
                                "自定义匹配器 '{}' 需要通过CustomMatcherRegistry处理",
                                name
                            );
                            false
                        })))
                    }
                };
                conditions.push(condition);
            }

            let final_condition: Box<dyn ConditionEvaluator> = if conditions.len() == 1 {
                conditions.pop().unwrap()
            } else if conditions.is_empty() {
                continue;
            } else {
                Box::new(CompositeCondition {
                    conditions,
                    operator: LogicalOperator::And,
                })
            };

            rules.push(MatcherRule {
                id: rule_config.id.clone(),
                name: rule_config.name.clone(),
                priority: rule_config.priority,
                condition: final_condition,
                enabled: true,
            });
        }

        Ok(rules)
    }

    /// 创建新的 Governor 实例
    #[allow(unused_variables)]
    pub async fn new(
        config: FlowControlConfig,
        storage: Arc<dyn Storage>,
        ban_storage: Arc<dyn BanStorage>,
        #[cfg(feature = "monitoring")] metrics: Option<Arc<Metrics>>,
        #[cfg(feature = "telemetry")] tracer: Option<Arc<Tracer>>,
    ) -> Result<Self, FlowGuardError> {
        // 校验配置
        config.validate().map_err(FlowGuardError::ConfigError)?;

        // 创建标识符提取器
        let identifier_extractor = Arc::new(crate::matchers::CompositeExtractor::new(
            vec![
                Box::new(crate::matchers::UserIdExtractor::from_header("X-User-Id")),
                Box::new(crate::matchers::IpExtractor::new_default()),
                Box::new(crate::matchers::ApiKeyExtractor::from_header("X-API-Key")),
            ],
            true,
        ));

        // 创建规则匹配器
        let rules = Self::build_rules(&config)?;
        let rule_matcher = Arc::new(RwLock::new(RuleMatcher::new(rules)));

        // 创建决策链
        let decision_chain = Arc::new(RwLock::new(DecisionChain::new(vec![])));

        // 创建熔断器 (仅当 circuit-breaker 特性启用时)
        #[cfg(feature = "circuit-breaker")]
        let circuit_breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(30),
            half_open_max_calls: 3,
        }));

        // 创建内存缓存用于 FallbackManager（使用 oxcache 直接）
        #[cfg(feature = "fallback")]
        let fallback_l2_cache: Arc<Cache<String, String>> = Arc::new(
            Cache::builder()
                .capacity(crate::constants::DEFAULT_L2_CACHE_CAPACITY as u64)
                .ttl(Duration::from_secs(
                    crate::constants::DEFAULT_L2_CACHE_TTL_SECS,
                ))
                .build()
                .await
                .expect("Failed to create oxcache for FallbackManager"),
        );
        // 创建降级管理器
        #[cfg(feature = "fallback")]
        let fallback_manager = Arc::new(FallbackManager::new(fallback_l2_cache.clone()));

        // 创建审计日志记录器 (仅当 audit-log 特性启用时)
        #[cfg(feature = "audit-log")]
        let audit_logger = Arc::new(RwLock::new(None));

        // 创建封禁管理器 (仅当 ban-manager 特性启用时)
        #[cfg(feature = "ban-manager")]
        let ban_manager = Arc::new(BanManager::new(ban_storage.clone(), None).await?);

        // 创建并行封禁检查器 (仅当 parallel-checker 特性启用时)
        #[cfg(feature = "parallel-checker")]
        let parallel_ban_checker = Arc::new(crate::parallel_ban_checker::ParallelBanChecker::new(
            ban_manager.clone(),
        ));

        // 创建规则对应的决策链
        let rule_chains_map = Self::build_rule_chains(&config)?;
        let rule_chains = Arc::new(RwLock::new(rule_chains_map));

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            storage,
            _ban_storage: ban_storage,
            #[cfg(feature = "ban-manager")]
            ban_manager,
            #[cfg(feature = "parallel-checker")]
            parallel_ban_checker,
            decision_chain,
            rule_matcher,
            rule_chains,
            identifier_extractor,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker,
            #[cfg(feature = "audit-log")]
            audit_logger,
            config_history: Arc::new(RwLock::new(ConfigHistory::new(100))),
            total_requests: AtomicU64::new(0),
            allowed_requests: AtomicU64::new(0),
            rejected_requests: AtomicU64::new(0),
            banned_requests: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
        })
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
    /// use limiteron::db_storage::DbStorageConfig;
    /// use limiteron::storage::{Storage, BanStorage};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let config = DbStorageConfig::new("postgres://localhost/limiteron");
    ///     let storage: std::sync::Arc<dyn Storage> = std::sync::Arc::new(
    ///         DbStorage::new(config).await?
    ///     );
    ///     let ban_storage: std::sync::Arc<dyn BanStorage> = storage.clone();
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
        let config = crate::config_loader::ConfigLoader::load_from_file(config_path)?;

        #[cfg(all(feature = "monitoring", feature = "telemetry"))]
        {
            Self::new(config, storage, ban_storage, None, None).await
        }
        #[cfg(all(feature = "monitoring", not(feature = "telemetry")))]
        {
            Self::new(config, storage, ban_storage, None).await
        }
        #[cfg(all(not(feature = "monitoring"), feature = "telemetry"))]
        {
            Self::new(config, storage, ban_storage, None, None).await
        }
        #[cfg(all(not(feature = "monitoring"), not(feature = "telemetry")))]
        {
            Self::new(config, storage, ban_storage).await
        }
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
    /// use limiteron::storage::MemoryStorage;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let storage = std::sync::Arc::new(MemoryStorage::new());
    ///     let ban_storage = std::sync::Arc::new(MemoryStorage::new());
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
        let config = crate::config_loader::ConfigLoader::load_from_file(config_path)?;

        #[cfg(all(feature = "monitoring", feature = "telemetry"))]
        {
            Self::new(config, storage, ban_storage, None, None).await
        }
        #[cfg(all(feature = "monitoring", not(feature = "telemetry")))]
        {
            Self::new(config, storage, ban_storage, None).await
        }
        #[cfg(all(not(feature = "monitoring"), feature = "telemetry"))]
        {
            Self::new(config, storage, ban_storage, None, None).await
        }
        #[cfg(all(not(feature = "monitoring"), not(feature = "telemetry")))]
        {
            Self::new(config, storage, ban_storage).await
        }
    }

    /// 检查请求 - 简化版本使用并行检查器
    #[instrument(skip(self), fields(
        user_id = %redact_user_id(context.user_id.as_deref()),
        ip = %redact_ip(context.ip.as_deref()),
        path = %context.path,
        method = %context.method
    ))]
    pub async fn check(&self, context: &RequestContext) -> Result<Decision, FlowGuardError> {
        self.total_requests.fetch_add(1, Ordering::Relaxed);

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
                        crate::log_redaction::redact_user_id(identifier.key()),
                        &info.reason
                    );
                    self.banned_requests.fetch_add(1, Ordering::Relaxed);
                    return Ok(Decision::Banned(info));
                }
            }
        }

        // 继续其他检查
        // 规则匹配
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
            // 如果没有匹配的规则，检查默认决策链
            // 目前默认决策链为空，相当于直接允许
            let result = self.decision_chain.read().await.check().await;
            match &result {
                Ok(Decision::Allowed(_)) => {
                    self.allowed_requests.fetch_add(1, Ordering::Relaxed);
                }
                Ok(Decision::Banned(_)) => {
                    self.banned_requests.fetch_add(1, Ordering::Relaxed);
                }
                Ok(Decision::Rejected(_)) => {
                    self.rejected_requests.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    self.error_count.fetch_add(1, Ordering::Relaxed);
                }
            }
            return result;
        }

        // 有匹配的规则，按顺序执行（级联）
        // 只要有一个规则拒绝，请求就被拒绝
        let rule_chains = self.rule_chains.read().await;

        for rule in matched_rules {
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
                        match &result {
                            Ok(Decision::Rejected(_)) => {
                                self.rejected_requests.fetch_add(1, Ordering::Relaxed);
                            }
                            Ok(Decision::Banned(_)) => {
                                self.banned_requests.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(_) => {
                                self.error_count.fetch_add(1, Ordering::Relaxed);
                            }
                            _ => {}
                        }
                        return result;
                    }
                }
            }
        }

        // 所有规则都允许
        self.allowed_requests.fetch_add(1, Ordering::Relaxed);
        Ok(Decision::Allowed(None))
    }

    /// 并行资源检查 - 保持原有接口兼容性
    #[cfg(feature = "parallel-checker")]
    #[instrument(skip(self))]
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
                warn!("Resource banned: 资源={}, 原因={}", resource, info.reason);
                Ok(Decision::Banned(info))
            }
            None => Ok(Decision::Allowed(None)),
        }
    }

    /// 并行资源检查 - 未启用 parallel-checker 时的存根实现
    #[cfg(not(feature = "parallel-checker"))]
    #[instrument(skip(self))]
    pub async fn check_resource_parallel(
        &self,
        resource: &str,
    ) -> Result<Decision, FlowGuardError> {
        #[cfg(feature = "ban-manager")]
        {
            let target = BanTarget::UserId(resource.to_string());
            let ban_record = self.ban_manager.is_banned(&target).await?;

            if let Some(record) = ban_record {
                return Ok(Decision::Banned(BanInfo {
                    reason: record.reason,
                    banned_until: record.expires_at,
                    ban_times: record.ban_times,
                }));
            }

            return Ok(Decision::Allowed(None));
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
    #[instrument(skip(self))]
    pub async fn ban_identifier(
        &self,
        identifier: &Identifier,
        reason: &str,
        source: Option<ChangeSource>,
    ) -> Result<(), FlowGuardError> {
        debug!("Ban user: {} 原因: {}", identifier.key(), reason);

        let ban_target = identifier.to_ban_target();

        if let Some(target) = ban_target {
            let ban_source = match source {
                Some(ChangeSource::Manual { operator }) => BanSource::Manual { operator },
                _ => BanSource::Manual {
                    operator: "unknown".to_string(),
                },
            };

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
                crate::log_redaction::redact_user_id(identifier.key().as_ref())
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
    #[instrument(skip(self))]
    pub async fn unban_identifier(&self, identifier: &Identifier) -> Result<(), FlowGuardError> {
        debug!("取消Ban user: {}", identifier.key());

        let ban_target = identifier.to_ban_target();

        // 检查是否有可回滚的旧版本
        let Some(old_version) = last_record.old_version.clone() else {
            return Err(FlowGuardError::ConfigError(
                "无法获取上一个配置版本".to_string(),
            ));
        };

        drop(history);

        // 配置回滚由 confers 库管理
        return Err(FlowGuardError::ConfigError(
            "配置回滚需要使用 confers 库".to_string(),
        ));
    }

    /// 获取配置历史
    pub async fn get_config_history(&self) -> Vec<ConfigChangeRecord> {
        self.config_history.read().await.get_records().to_vec()
    }

    /*
    /// 启动配置监视器
    #[instrument(skip(self))]
    #[cfg(feature = "config-watcher")]
    pub async fn start_config_watcher<F>(&self, callback: F) -> Result<(), FlowGuardError>
    where
        F: crate::config_watcher::ConfigChangeCallback + Send + Sync + 'static,
    {
        info!("启动配置监视器");

        // 配置监视器的具体实现
        // 这需要根据具体的配置存储类型来实现

        Ok(())
    }
    */

    /// 停止配置监视器
    #[instrument(skip(self))]
    pub async fn stop_config_watcher(&self) -> Result<(), FlowGuardError> {
        info!("停止配置监视器");

        Ok(())
    }

    /// 手动配置检查
    #[instrument(skip(self))]
    pub async fn manual_config_check(&self) -> Result<bool, FlowGuardError> {
        info!("手动配置检查");

        let _config = self.config.read().await;

        // 执行各种配置验证
        // 具体验证逻辑取决于具体的验证需求

        Ok(true)
    }

    /// 获取统计信息
    #[instrument(skip(self))]
    pub async fn stats(&self) -> crate::governor::GovernorStats {
        let total = self.total_requests.load(Ordering::Relaxed);
        let allowed = self.allowed_requests.load(Ordering::Relaxed);
        let rejected = self.rejected_requests.load(Ordering::Relaxed);
        let banned = self.banned_requests.load(Ordering::Relaxed);
        let error = self.error_count.load(Ordering::Relaxed);
        let last_updated = Some(chrono::Utc::now());

        crate::governor::GovernorStats {
            total_requests: total,
            allowed_requests: allowed,
            rejected_requests: rejected,
            banned_requests: banned,
            error_count: error,
            last_updated,
        }
    }

    /// 获取决策链统计
    #[instrument(skip(self))]
    pub async fn decision_chain_stats(&self) -> crate::decision_chain::ChainStats {
        self.decision_chain.read().await.stats()
    }

    /// 获取规则匹配器统计
    #[instrument(skip(self))]
    pub async fn rule_matcher_stats(&self) -> crate::matchers::MatcherStats {
        self.rule_matcher.read().await.stats()
    }

    /// 重置统计信息
    #[instrument(skip(self))]
    pub async fn reset_stats(&self) {
        info!("重置统计信息");

        self.decision_chain.write().await.reset_stats();
        self.rule_matcher.write().await.reset_stats();
        self.total_requests.store(0, Ordering::Relaxed);
        self.allowed_requests.store(0, Ordering::Relaxed);
        self.rejected_requests.store(0, Ordering::Relaxed);
        self.banned_requests.store(0, Ordering::Relaxed);
        self.error_count.store(0, Ordering::Relaxed);
    }

    /// 设置审计日志记录器
    #[cfg(feature = "audit-log")]
    #[instrument(skip(self))]
    pub async fn set_audit_logger(&self, audit_logger: Arc<AuditLogger>) {
        let mut logger = self.audit_logger.write().await;
        *logger = Some(audit_logger);

        info!("审计日志记录器已设置");
    }

    /// 获取审计日志记录器
    #[cfg(feature = "audit-log")]
    #[instrument(skip(self))]
    pub async fn audit_logger(&self) -> Option<Arc<AuditLogger>> {
        self.audit_logger.read().await.as_deref().copied()
    }

    /// 健康检查
    #[instrument(skip(self))]
    pub async fn health_check(&self) -> Result<(), FlowGuardError> {
        info!("健康检查");

        // 检查各个组件的健康状态
        // config is guarded by RwLock, if we can read it, it's fine.
        let _config_guard = self.config.read().await;
        let config_healthy = true;

        let storage_healthy = true; // 这里需要根据具体的存储类型实现健康检查

        if config_healthy && storage_healthy {
            Ok(())
        } else {
            Err(FlowGuardError::StorageError(
                crate::error::StorageError::ConnectionError("Storage unhealthy".to_string()),
            ))
        }
    }
}
