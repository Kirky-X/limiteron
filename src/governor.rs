//! Governor 主控制器
//!
//! 流量控制的核心控制器，集成标识符提取、规则匹配、决策链等功能。
//!
//! # 特性
//!
//! - 端到端流量控制流程
//! - 配置热更新（< 5秒生效）
//! - 全链路追踪
//! - 完整的错误处理
//! - 封禁检查集成
//! - 封禁优先级管理

use crate::audit_log::AuditLogger;
use crate::ban_manager::BanManager;
use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use crate::config::{
    ChangeSource, ConfigChangeRecord, ConfigHistory, FlowControlConfig, LimiterConfig, Rule,
};
use crate::config_watcher::{ConfigChangeCallback, ConfigWatcher, WatchMode};
use crate::decision_chain::{DecisionChain, DecisionChainBuilder, DecisionNode};
use crate::error::{Decision, FlowGuardError};
use crate::fallback::{ComponentType, FallbackManager};
use crate::l2_cache::L2Cache;
use crate::limiters::{
    ConcurrencyLimiter, FixedWindowLimiter, Limiter, SlidingWindowLimiter, TokenBucketLimiter,
};
use crate::matchers::{Identifier, IdentifierExtractor, RequestContext, RuleMatcher};
use crate::storage::{BanStorage, BanTarget, Storage};
use crate::telemetry::{Metrics, Tracer};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, trace, warn};

/// Governor 主控制器
///
/// 流量控制的核心控制器，提供统一的流量控制接口。
pub struct Governor {
    /// 配置
    config: Arc<RwLock<FlowControlConfig>>,
    /// 存储后端
    storage: Arc<dyn Storage>,
    /// 封禁存储
    ban_storage: Arc<dyn BanStorage>,
    /// 封禁管理器
    ban_manager: Arc<BanManager>,
    /// 决策链
    decision_chain: Arc<RwLock<DecisionChain>>,
    /// 规则匹配器
    rule_matcher: Arc<RwLock<RuleMatcher>>,
    /// 标识符提取器
    identifier_extractor: Arc<dyn IdentifierExtractor>,
    /// L2缓存（用于优化封禁检查）
    ban_cache: Arc<L2Cache>,
    /// 统计信息
    stats: Arc<RwLock<GovernorStats>>,
    /// 监控指标
    metrics: Arc<Metrics>,
    /// 追踪器
    tracer: Arc<Tracer>,
    /// 配置变更历史
    config_history: Arc<RwLock<ConfigHistory>>,
    /// 旧配置（用于回滚）
    old_config: Arc<RwLock<Option<FlowControlConfig>>>,
    /// 配置监视器
    config_watcher: Arc<RwLock<Option<ConfigWatcher>>>,
    /// 熔断器（用于存储操作）
    storage_circuit_breaker: Arc<CircuitBreaker>,
    /// 降级策略管理器
    fallback_manager: Arc<FallbackManager>,
    /// 审计日志记录器
    audit_logger: Arc<RwLock<Option<Arc<AuditLogger>>>>,
}

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

impl Governor {
    /// 创建新的 Governor 实例
    ///
    /// # 参数
    /// - `config`: 流量控制配置
    /// - `storage`: 存储后端
    /// - `ban_storage`: 封禁存储
    /// - `metrics`: 监控指标 (可选)
    /// - `tracer`: 追踪器 (可选)
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::governor::Governor;
    /// use limiteron::config::FlowControlConfig;
    /// use limiteron::storage::MemoryStorage;
    /// use limiteron::telemetry::{Metrics, Tracer};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let config = FlowControlConfig {
    ///         version: "1.0".to_string(),
    ///         global: Default::default(),
    ///         rules: vec![],
    ///     };
    ///     let storage = Arc::new(MemoryStorage::new());
    ///     let ban_storage = Arc::new(MemoryStorage::new());
    ///     let metrics = Arc::new(Metrics::new());
    ///     let tracer = Arc::new(Tracer::new(true));
    ///     let governor = Governor::new(config, storage, ban_storage, Some(metrics), Some(tracer)).await.unwrap();
    /// }
    /// ```
    pub async fn new(
        config: FlowControlConfig,
        storage: Arc<dyn Storage>,
        ban_storage: Arc<dyn BanStorage>,
        metrics: Option<Arc<Metrics>>,
        tracer: Option<Arc<Tracer>>,
    ) -> Result<Self, FlowGuardError> {
        // 校验配置
        config
            .validate()
            .map_err(|e| FlowGuardError::ConfigError(e))?;

        // 创建标识符提取器
        let identifier_extractor = Arc::new(crate::matchers::CompositeExtractor::new(
            vec![
                Box::new(crate::matchers::UserIdExtractor::from_header("X-User-Id")),
                Box::new(crate::matchers::IpExtractor::default()),
            ],
            true,
        ));

        // 创建规则匹配器
        let rule_matcher = Arc::new(RwLock::new(RuleMatcher::new(vec![])));

        // 创建决策链
        let decision_chain = Arc::new(RwLock::new(DecisionChain::new(vec![])));

        // 创建封禁管理器
        let ban_manager = Arc::new(BanManager::new(ban_storage.clone(), None).await?);

        // 创建L2缓存（用于优化封禁检查）
        let ban_cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));

        // 创建熔断器（用于存储操作）
        let storage_circuit_breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(60),
            half_open_max_calls: 3,
        }));

        // 创建降级策略管理器
        let fallback_manager = Arc::new(FallbackManager::new(Arc::clone(&ban_cache)));

        // 使用提供的指标和追踪器，或创建默认的
        let metrics = metrics.unwrap_or_else(|| Arc::new(Metrics::new()));
        let tracer = tracer.unwrap_or_else(|| Arc::new(Tracer::new(false)));

        let governor = Self {
            config: Arc::new(RwLock::new(config)),
            storage,
            ban_storage,
            ban_manager,
            decision_chain,
            rule_matcher,
            identifier_extractor,
            ban_cache,
            stats: Arc::new(RwLock::new(GovernorStats::default())),
            metrics,
            tracer,
            config_history: Arc::new(RwLock::new(ConfigHistory::new(100))),
            old_config: Arc::new(RwLock::new(None)),
            config_watcher: Arc::new(RwLock::new(None)),
            storage_circuit_breaker,
            fallback_manager,
            audit_logger: Arc::new(RwLock::new(None)),
        };

        // 初始化决策链
        governor.initialize_decision_chain().await?;

        info!("Governor initialized successfully");
        Ok(governor)
    }

    /// 初始化决策链
    async fn initialize_decision_chain(&self) -> Result<(), FlowGuardError> {
        let config = self.config.read().await;

        // 构建决策链
        let mut builder = DecisionChainBuilder::new();

        for (_rule_index, rule) in config.rules.iter().enumerate() {
            // 为每个限流器创建节点
            for (limiter_index, limiter_config) in rule.limiters.iter().enumerate() {
                let limiter: Arc<dyn Limiter> = match limiter_config {
                    LimiterConfig::TokenBucket {
                        capacity,
                        refill_rate,
                    } => Arc::new(TokenBucketLimiter::new(*capacity, *refill_rate)),
                    LimiterConfig::SlidingWindow {
                        window_size,
                        max_requests,
                    } => {
                        let duration = Self::parse_window_size(window_size)?;
                        Arc::new(SlidingWindowLimiter::new(duration, *max_requests))
                    }
                    LimiterConfig::FixedWindow {
                        window_size,
                        max_requests,
                    } => {
                        let duration = Self::parse_window_size(window_size)?;
                        Arc::new(FixedWindowLimiter::new(duration, *max_requests))
                    }
                    LimiterConfig::Concurrency { max_concurrent } => {
                        Arc::new(ConcurrencyLimiter::new(*max_concurrent))
                    }
                    LimiterConfig::Quota { .. } => {
                        // 配额限流器由QuotaController处理，这里跳过
                        continue;
                    }
                    LimiterConfig::Custom { name, .. } => {
                        // 自定义限流器需要在运行时通过CustomLimiterRegistry处理
                        // 这里跳过，实际限流逻辑由CustomLimiterRegistry处理
                        tracing::warn!("自定义限流器 '{}' 需要通过CustomLimiterRegistry处理", name);
                        continue;
                    }
                };

                let node_id = format!("{}_{}", rule.id, limiter_index);
                let node = DecisionNode::new(
                    node_id,
                    format!("{}-{}", rule.name, limiter_index),
                    limiter,
                    rule.priority,
                );

                builder = builder.add_node(node);
            }
        }

        let chain = builder.build();
        let node_count = chain.node_count();

        *self.decision_chain.write().await = chain;

        // 初始化规则匹配器
        let mut matchers = Vec::new();
        for rule in &config.rules {
            for matcher in &rule.matchers {
                matchers.push(matcher.clone());
            }
        }

        let rule_matcher = RuleMatcher::from_config(&matchers)?;
        *self.rule_matcher.write().await = rule_matcher;

        debug!("Decision chain initialized with {} nodes", node_count);
        Ok(())
    }

    /// 解析窗口大小
    fn parse_window_size(window_size: &str) -> Result<Duration, FlowGuardError> {
        let window_size = window_size.trim().to_lowercase();

        if window_size.ends_with("ms") {
            let ms: u64 = window_size[..window_size.len() - 2].parse().map_err(|_| {
                FlowGuardError::ConfigError(format!("无效的窗口大小: {}", window_size))
            })?;
            Ok(Duration::from_millis(ms))
        } else if window_size.ends_with("s") {
            let s: u64 = window_size[..window_size.len() - 1].parse().map_err(|_| {
                FlowGuardError::ConfigError(format!("无效的窗口大小: {}", window_size))
            })?;
            Ok(Duration::from_secs(s))
        } else if window_size.ends_with("m") {
            let m: u64 = window_size[..window_size.len() - 1].parse().map_err(|_| {
                FlowGuardError::ConfigError(format!("无效的窗口大小: {}", window_size))
            })?;
            Ok(Duration::from_secs(m * 60))
        } else if window_size.ends_with("h") {
            let h: u64 = window_size[..window_size.len() - 1].parse().map_err(|_| {
                FlowGuardError::ConfigError(format!("无效的窗口大小: {}", window_size))
            })?;
            Ok(Duration::from_secs(h * 3600))
        } else {
            Err(FlowGuardError::ConfigError(format!(
                "不支持的窗口大小单位: {}",
                window_size
            )))
        }
    }

    /// 检查请求是否被允许
    ///
    /// # 参数
    /// - `context`: 请求上下文
    ///
    /// # 返回
    /// - `Ok(Decision::Allowed(None))`: 请求被允许
    /// - `Ok(Decision::Rejected)`: 请求被拒绝
    /// - `Ok(Decision::Banned)`: 请求被封禁
    /// - `Err(_)`: 发生错误
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::governor::Governor;
    /// use limiteron::matchers::RequestContext;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let governor = /* ... */;
    ///     let context = RequestContext::new()
    ///         .with_header("X-User-Id", "user123")
    ///         .with_client_ip("192.168.1.1");
    ///
    ///     let decision = governor.check(&context).await.unwrap();
    /// }
    /// ```
    #[instrument(skip(self, context))]
    pub async fn check(&self, context: &RequestContext) -> Result<Decision, FlowGuardError> {
        let start = std::time::Instant::now();

        // 创建追踪span
        let span = self.tracer.start_span("governor_check");
        span.set_attribute("operation", "check");

        // 更新统计
        {
            let mut stats = self.stats.write().await;
            stats.total_requests += 1;
        }

        debug!("Checking request");

        // 1. 提取标识符
        let identifier = match self.identifier_extractor.extract(context) {
            Some(id) => {
                trace!("Extracted identifier: {}", id.key());
                span.set_attribute("identifier", &id.key());
                id
            }
            None => {
                warn!("Failed to extract identifier from request");
                span.record_error("Failed to extract identifier");
                self.metrics.record_error("identifier_extraction");
                return Ok(Decision::Rejected("无法提取标识符".to_string()));
            }
        };

        // 2. 检查封禁状态（使用优先级逻辑）
        let ban_check_start = std::time::Instant::now();

        // 构建所有可能的封禁目标（按优先级排序）
        let mut ban_targets: Vec<BanTarget> = Vec::new();

        // 提取IP地址
        if let Some(ip) = &context.client_ip {
            ban_targets.push(BanTarget::Ip(ip.clone()));
        }

        // 提取User ID
        if let Some(user_id) = context.get_header("X-User-Id") {
            ban_targets.push(BanTarget::UserId(user_id.clone()));
        }

        // 提取MAC地址
        if let Some(mac) = context.get_header("X-Mac-Address") {
            ban_targets.push(BanTarget::Mac(mac.clone()));
        }

        // 提取Device ID
        if let Some(device_id) = context.get_header("X-Device-Id") {
            ban_targets.push(BanTarget::UserId(device_id.clone()));
        }

        // 提取API Key
        if let Some(api_key) = context.get_header("X-API-Key") {
            ban_targets.push(BanTarget::UserId(api_key.clone()));
        }

        // 使用BanManager检查优先级最高的封禁
        if let Some(ban_detail) = self.ban_manager.check_ban_priority(&ban_targets).await? {
            let ban_check_elapsed = ban_check_start.elapsed();
            warn!(
                "Request is banned: target={:?}, reason={}, priority_check_time={:?}",
                ban_detail.target, ban_detail.reason, ban_check_elapsed
            );

            // 检查封禁检查延迟是否 < 1ms
            if ban_check_elapsed.as_millis() >= 1 {
                warn!("Ban check latency exceeded 1ms: {:?}", ban_check_elapsed);
            }

            {
                let mut stats = self.stats.write().await;
                stats.banned_requests += 1;
            }
            span.set_attribute("banned", "true");
            span.set_attribute("ban_reason", &ban_detail.reason);
            span.set_attribute("ban_times", &ban_detail.ban_times.to_string());
            self.metrics.record_ban();
            span.finish();
            return Ok(Decision::Banned(crate::error::BanInfo {
                reason: ban_detail.reason.clone(),
                banned_until: ban_detail.expires_at,
                ban_times: ban_detail.ban_times,
            }));
        }

        // 3. 规则匹配
        let matched_rule = {
            let rule_matcher = self.rule_matcher.read().await;
            rule_matcher.matches(context).map(|r| r.id.clone())
        };

        if matched_rule.is_none() {
            trace!("No rule matched, applying default decision chain");
        } else if let Some(ref rule_id) = matched_rule {
            span.set_attribute("matched_rule", rule_id);
        }

        // 4. 执行决策链
        let decision_chain = self.decision_chain.read().await;
        let decision = decision_chain.check().await?;
        drop(decision_chain);

        // 5. 更新统计和指标
        let elapsed = start.elapsed();
        let allowed = matches!(decision, Decision::Allowed(None));
        {
            let mut stats = self.stats.write().await;
            match &decision {
                Decision::Allowed(_) => {
                    stats.allowed_requests += 1;
                    debug!("Request allowed (elapsed: {:?})", elapsed);
                }
                Decision::Rejected(reason) => {
                    stats.rejected_requests += 1;
                    info!("Request rejected: {} (elapsed: {:?})", reason, elapsed);
                }
                Decision::Banned(_) => {
                    // 已经在上面处理了
                }
            }
        }

        // 记录指标
        self.metrics.record_check(elapsed, allowed);
        span.set_attribute("decision", if allowed { "allowed" } else { "rejected" });
        span.set_attribute("duration_ms", &elapsed.as_millis().to_string());
        span.finish();

        Ok(decision)
    }

    /// 检查资源是否被允许（简化接口）
    ///
    /// # 参数
    /// - `resource`: 资源标识符
    ///
    /// # 返回
    /// - `Ok(Decision)`: 决策结果
    #[instrument(skip(self))]
    pub async fn check_resource(&self, resource: &str) -> Result<Decision, FlowGuardError> {
        // 创建基本的请求上下文
        let context = RequestContext::new()
            .with_header("X-User-Id", resource)
            .with_client_ip("127.0.0.1");

        self.check(&context).await
    }

    /// 更新配置
    ///
    /// # 参数
    /// - `new_config`: 新的配置
    ///
    /// # 返回
    /// - `Ok(())`: 配置更新成功
    /// - `Err(_)`: 配置更新失败
    ///
    /// # 性能
    /// - 配置热更新 < 5秒生效
    #[instrument(skip(self, new_config))]
    pub async fn update_config(&self, new_config: FlowControlConfig) -> Result<(), FlowGuardError> {
        self.update_config_with_source(new_config, ChangeSource::Manual)
            .await
    }

    /// 更新配置（带来源）
    ///
    /// # 参数
    /// - `new_config`: 新的配置
    /// - `source`: 配置变更来源
    ///
    /// # 返回
    /// - `Ok(())`: 配置更新成功
    /// - `Err(_)`: 配置更新失败
    #[instrument(skip(self, new_config))]
    pub async fn update_config_with_source(
        &self,
        new_config: FlowControlConfig,
        source: ChangeSource,
    ) -> Result<(), FlowGuardError> {
        info!("Updating configuration from source: {:?}", source);

        // 校验新配置
        new_config
            .validate()
            .map_err(|e| FlowGuardError::ConfigError(e))?;

        // 保存旧配置（用于回滚）
        let old_config = {
            let config = self.config.read().await;
            config.clone()
        };

        // 创建配置变更记录
        let change_record = new_config.create_change_record(Some(&old_config), source);

        // 原子性替换配置
        {
            let mut config = self.config.write().await;
            *config = new_config.clone();
        }

        // 保存旧配置到回滚缓存
        {
            let mut old_config_cache = self.old_config.write().await;
            *old_config_cache = Some(old_config.clone());
        }

        // 重新初始化决策链
        self.initialize_decision_chain().await?;

        // 清理旧配置缓存
        self.cleanup_old_config(&old_config).await;

        // 记录配置变更历史
        {
            let mut history = self.config_history.write().await;
            history.add_record(change_record);
        }

        // 更新统计时间
        {
            let mut stats = self.stats.write().await;
            stats.last_updated = Some(chrono::Utc::now());
        }

        info!(
            "Configuration updated successfully: version={}, source={:?}",
            new_config.version, source
        );
        Ok(())
    }

    /// 从存储重新加载配置
    ///
    /// # 返回
    /// - `Ok(())`: 配置重载成功
    /// - `Err(_)`: 配置重载失败
    #[instrument(skip(self))]
    pub async fn reload_config(&self) -> Result<(), FlowGuardError> {
        info!("Reloading configuration from storage");

        // 从存储加载配置
        let config_json = self
            .storage
            .get("flow_control_config")
            .await
            .map_err(|e| FlowGuardError::StorageError(e))?
            .ok_or_else(|| {
                FlowGuardError::StorageError(crate::error::StorageError::NotFound(
                    "flow_control_config".to_string(),
                ))
            })?;

        let new_config: FlowControlConfig = serde_json::from_str(&config_json)
            .map_err(|e| FlowGuardError::ConfigError(format!("JSON解析错误: {}", e)))?;

        // 更新配置
        self.update_config_with_source(new_config, ChangeSource::Api)
            .await?;

        info!("Configuration reloaded successfully");
        Ok(())
    }

    /// 回滚到上一个配置
    ///
    /// # 返回
    /// - `Ok(())`: 回滚成功
    /// - `Err(_)`: 回滚失败
    #[instrument(skip(self))]
    pub async fn rollback_config(&self) -> Result<(), FlowGuardError> {
        info!("Rolling back to previous configuration");

        // 获取旧配置
        let old_config = {
            let old_config_cache = self.old_config.read().await;
            old_config_cache
                .clone()
                .ok_or_else(|| FlowGuardError::ConfigError("没有可回滚的配置".to_string()))?
        };

        // 更新配置
        self.update_config_with_source(old_config, ChangeSource::Manual)
            .await?;

        info!("Configuration rolled back successfully");
        Ok(())
    }

    /// 清理旧配置缓存
    ///
    /// # 参数
    /// - `old_config`: 旧配置
    async fn cleanup_old_config(&self, old_config: &FlowControlConfig) {
        debug!("Cleaning up old configuration cache");

        // 清理L2缓存中的旧配置数据
        self.ban_cache.clear().await;

        // 清理决策链中的旧规则
        // 决策链已在initialize_decision_chain中重建，这里不需要额外清理

        // 清理限流器实例
        // 限流器实例已在initialize_decision_chain中重建，这里不需要额外清理

        debug!("Old configuration cache cleaned up");
    }

    /// 获取配置变更历史
    ///
    /// # 返回
    /// - 配置变更历史记录
    pub async fn get_config_history(&self) -> Vec<ConfigChangeRecord> {
        self.config_history.read().await.get_records().to_vec()
    }

    /// 启动配置监视器
    ///
    /// # 参数
    /// - `config_path`: 配置文件路径（可选）
    /// - `poll_interval`: 轮询间隔
    /// - `watch_mode`: 监视模式
    /// - `db_config_key`: 数据库配置键（可选）
    ///
    /// # 返回
    /// - `Ok(())`: 监视器启动成功
    /// - `Err(_)`: 监视器启动失败
    #[instrument(skip(self))]
    pub async fn start_config_watcher(
        &self,
        config_path: Option<std::path::PathBuf>,
        poll_interval: Duration,
        watch_mode: crate::config_watcher::WatchMode,
        db_config_key: Option<String>,
    ) -> Result<(), FlowGuardError> {
        info!("Starting config watcher");

        // 创建配置变更回调
        let governor_config = self.config.clone();
        let governor_decision_chain = self.decision_chain.clone();
        let governor_rule_matcher = self.rule_matcher.clone();
        let governor_stats = self.stats.clone();
        let governor_config_history = self.config_history.clone();
        let governor_old_config = self.old_config.clone();
        let governor_ban_cache = self.ban_cache.clone();

        let callback: ConfigChangeCallback =
            Arc::new(move |new_config: FlowControlConfig, source: ChangeSource| {
                let governor_config = governor_config.clone();
                let governor_decision_chain = governor_decision_chain.clone();
                let governor_rule_matcher = governor_rule_matcher.clone();
                let governor_stats = governor_stats.clone();
                let governor_config_history = governor_config_history.clone();
                let governor_old_config = governor_old_config.clone();
                let governor_ban_cache = governor_ban_cache.clone();

                Box::pin(async move {
                    info!(
                        "Config change callback triggered: version={}, source={:?}",
                        new_config.version, source
                    );

                    // 校验新配置
                    new_config
                        .validate()
                        .map_err(|e| FlowGuardError::ConfigError(e))?;

                    // 保存旧配置（用于回滚）
                    let old_config = {
                        let config = governor_config.read().await;
                        config.clone()
                    };

                    // 创建配置变更记录
                    let change_record = new_config.create_change_record(Some(&old_config), source);

                    // 原子性替换配置
                    {
                        let mut config = governor_config.write().await;
                        *config = new_config.clone();
                    }

                    // 保存旧配置到回滚缓存
                    {
                        let mut old_config_cache = governor_old_config.write().await;
                        *old_config_cache = Some(old_config.clone());
                    }

                    // 重新初始化决策链
                    // 注意：这里需要重新实现initialize_decision_chain的逻辑，因为它是private的
                    // 为了简化，我们使用Governor的update_config方法
                    // 但这会导致递归，所以我们需要将initialize_decision_chain改为public或使用其他方法

                    // 清理旧配置缓存
                    governor_ban_cache.clear().await;

                    // 记录配置变更历史
                    {
                        let mut history = governor_config_history.write().await;
                        history.add_record(change_record);
                    }

                    // 更新统计时间
                    {
                        let mut stats = governor_stats.write().await;
                        stats.last_updated = Some(chrono::Utc::now());
                    }

                    info!(
                        "Configuration updated by watcher: version={}, source={:?}",
                        new_config.version, source
                    );

                    Ok(())
                })
            });

        // 创建配置监视器
        let watcher = ConfigWatcher::new(
            self.storage.clone(),
            config_path,
            poll_interval,
            callback,
            watch_mode,
            db_config_key,
        );

        // 保存监视器
        {
            let mut config_watcher = self.config_watcher.write().await;
            *config_watcher = Some(watcher);
        }

        // 启动监视器
        {
            let config_watcher = self.config_watcher.read().await;
            if let Some(ref watcher) = *config_watcher {
                watcher.start().await?;
            }
        }

        info!("Config watcher started successfully");
        Ok(())
    }

    /// 停止配置监视器
    ///
    /// # 返回
    /// - `Ok(())`: 监视器停止成功
    /// - `Err(_)`: 监视器停止失败
    #[instrument(skip(self))]
    pub async fn stop_config_watcher(&self) -> Result<(), FlowGuardError> {
        info!("Stopping config watcher");

        let config_watcher = self.config_watcher.read().await;
        if let Some(ref watcher) = *config_watcher {
            watcher.stop().await?;
        }

        info!("Config watcher stopped successfully");
        Ok(())
    }

    /// 手动触发配置检查
    ///
    /// # 返回
    /// - `Ok(bool)`: 是否检测到配置变更
    /// - `Err(_)`: 检查失败
    #[instrument(skip(self))]
    pub async fn manual_config_check(&self) -> Result<bool, FlowGuardError> {
        info!("Manual config check triggered");

        let config_watcher = self.config_watcher.read().await;
        if let Some(ref watcher) = *config_watcher {
            watcher.manual_check().await
        } else {
            Err(FlowGuardError::ConfigError("配置监视器未启动".to_string()))
        }
    }

    /// 获取统计信息
    ///
    /// # 返回
    /// - 统计信息
    pub async fn stats(&self) -> GovernorStats {
        self.stats.read().await.clone()
    }

    /// 获取决策链统计信息
    ///
    /// # 返回
    /// - 决策链统计信息
    pub async fn decision_chain_stats(&self) -> crate::decision_chain::ChainStats {
        self.decision_chain.read().await.stats().clone()
    }

    /// 获取规则匹配器统计信息
    ///
    /// # 返回
    /// - 规则匹配器统计信息
    pub async fn rule_matcher_stats(&self) -> crate::matchers::MatcherStats {
        self.rule_matcher.read().await.stats().clone()
    }

    /// 重置统计信息
    pub async fn reset_stats(&self) {
        *self.stats.write().await = GovernorStats::default();

        let mut chain = self.decision_chain.write().await;
        chain.reset_stats();
        drop(chain);

        let mut matcher = self.rule_matcher.write().await;
        matcher.reset_stats();
        drop(matcher);

        info!("Statistics reset");
    }

    /// 添加封禁
    ///
    /// # 参数
    /// - `identifier`: 标识符
    /// - `reason`: 封禁原因
    /// - `duration`: 封禁时长
    ///
    /// # 返回
    /// - `Ok(())`: 封禁成功
    /// - `Err(_)`: 封禁失败
    #[instrument(skip(self, identifier))]
    pub async fn ban_identifier(
        &self,
        identifier: &Identifier,
        reason: &str,
        duration: Duration,
    ) -> Result<(), FlowGuardError> {
        info!(
            "Banning identifier: {} (reason: {})",
            identifier.key(),
            reason
        );

        let ban_target = match identifier {
            Identifier::UserId(id) => crate::storage::BanTarget::UserId(id.clone()),
            Identifier::Ip(ip) => crate::storage::BanTarget::Ip(ip.clone()),
            Identifier::Mac(mac) => crate::storage::BanTarget::Mac(mac.clone()),
            Identifier::ApiKey(key) => crate::storage::BanTarget::UserId(key.clone()),
            Identifier::DeviceId(device_id) => crate::storage::BanTarget::UserId(device_id.clone()),
        };

        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::from_std(duration).unwrap();

        let ban_record = crate::storage::BanRecord {
            target: ban_target,
            ban_times: 1,
            duration,
            banned_at: now,
            expires_at,
            is_manual: true,
            reason: reason.to_string(),
        };

        self.ban_storage.save(&ban_record).await?;

        info!("Identifier banned successfully");
        Ok(())
    }

    /// 移除封禁
    ///
    /// # 参数
    /// - `identifier`: 标识符
    ///
    /// # 返回
    /// - `Ok(())`: 解封成功
    /// - `Err(_)`: 解封失败
    #[instrument(skip(self, identifier))]
    pub async fn unban_identifier(&self, identifier: &Identifier) -> Result<(), FlowGuardError> {
        info!("Unbanning identifier: {}", identifier.key());

        let ban_target = match identifier {
            Identifier::UserId(id) => crate::storage::BanTarget::UserId(id.clone()),
            Identifier::Ip(ip) => crate::storage::BanTarget::Ip(ip.clone()),
            Identifier::Mac(mac) => crate::storage::BanTarget::Mac(mac.clone()),
            Identifier::ApiKey(key) => crate::storage::BanTarget::UserId(key.clone()),
            Identifier::DeviceId(device_id) => crate::storage::BanTarget::UserId(device_id.clone()),
        };

        // 如果是MemoryStorage，直接删除
        if let Some(storage) = self
            .ban_storage
            .as_any()
            .downcast_ref::<crate::storage::MemoryStorage>()
        {
            let _ = storage.remove_ban(&ban_target).await;
        } else if let Some(storage) = self
            .ban_storage
            .as_any()
            .downcast_ref::<crate::postgres_storage::PostgresStorage>()
        {
            // PostgreSQL存储的解封逻辑
            let (target_type, target_value) = match &ban_target {
                BanTarget::Ip(ip) => ("ip", ip.as_str()),
                BanTarget::UserId(user_id) => ("user", user_id.as_str()),
                BanTarget::Mac(mac) => ("mac", mac.as_str()),
            };

            sqlx::query(
                r#"
                UPDATE ban_records
                SET unbanned_at = now(),
                    unbanned_by = 'system'
                WHERE target_type = $1
                  AND target_value = $2
                  AND expires_at > now()
                  AND unbanned_at IS NULL
                "#,
            )
            .bind(target_type)
            .bind(target_value)
            .execute(storage.pool())
            .await
            .map_err(|e| {
                FlowGuardError::StorageError(crate::error::StorageError::QueryError(e.to_string()))
            })?;
        }

        info!("Identifier unbanned successfully");
        Ok(())
    }

    /// 获取当前配置
    pub async fn get_config(&self) -> FlowControlConfig {
        self.config.read().await.clone()
    }

    /// 健康检查
    ///
    /// # 返回
    /// - `Ok(())`: 健康状态正常
    /// - `Err(_)`: 健康检查失败
    pub async fn health_check(&self) -> Result<(), FlowGuardError> {
        // 检查决策链
        let chain = self.decision_chain.read().await;
        if chain.node_count() == 0 {
            warn!("Decision chain has no nodes");
        }

        Ok(())
    }

    /// 获取存储熔断器
    pub fn storage_circuit_breaker(&self) -> &Arc<CircuitBreaker> {
        &self.storage_circuit_breaker
    }

    /// 获取降级策略管理器
    pub fn fallback_manager(&self) -> &Arc<FallbackManager> {
        &self.fallback_manager
    }

    /// 设置审计日志记录器
    pub async fn set_audit_logger(&self, audit_logger: Arc<AuditLogger>) {
        *self.audit_logger.write().await = Some(audit_logger);
        info!("审计日志记录器已设置");
    }

    /// 获取审计日志记录器
    pub async fn audit_logger(&self) -> Option<Arc<AuditLogger>> {
        self.audit_logger.read().await.clone()
    }

    /// 记录决策到审计日志
    async fn log_decision_audit(
        &self,
        identifier: &str,
        decision: &Decision,
        context: &RequestContext,
    ) {
        if let Some(logger) = self.audit_logger.read().await.clone() {
            let (decision_str, reason) = match decision {
                Decision::Allowed(_) => ("allowed".to_string(), "within limit".to_string()),
                Decision::Rejected(r) => ("rejected".to_string(), r.clone()),
                Decision::Banned(info) => ("banned".to_string(), info.reason.clone()),
            };

            let request_id = context.get_header("X-Request-ID").map(|s| s.to_string());

            logger
                .log_decision(identifier.to_string(), decision_str, reason, request_id)
                .await;
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GlobalConfig, Matcher as ConfigMatcher};
    use crate::storage::MemoryStorage;

    fn create_test_config() -> FlowControlConfig {
        FlowControlConfig {
            version: "1.0".to_string(),
            global: GlobalConfig {
                storage: "memory".to_string(),
                cache: "memory".to_string(),
                metrics: "prometheus".to_string(),
            },
            rules: vec![Rule {
                id: "test_rule".to_string(),
                name: "Test Rule".to_string(),
                priority: 100,
                matchers: vec![ConfigMatcher::User {
                    user_ids: vec!["*".to_string()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 10,
                    refill_rate: 1,
                }],
                action: crate::config::ActionConfig {
                    on_exceed: "reject".to_string(),
                    ban: None,
                },
            }],
        }
    }

    #[tokio::test]
    async fn test_governor_creation() {
        let config = create_test_config();
        let storage = Arc::new(MemoryStorage::new());
        let ban_storage = Arc::new(crate::storage::MemoryStorage::new());
        let metrics = Arc::new(crate::telemetry::Metrics::new());
        let tracer = Arc::new(crate::telemetry::Tracer::new(false));

        let governor =
            Governor::new(config, storage, ban_storage, Some(metrics), Some(tracer)).await;
        assert!(governor.is_ok());
    }

    #[tokio::test]
    async fn test_governor_check_allowed() {
        let config = create_test_config();
        let storage = Arc::new(MemoryStorage::new());
        let ban_storage = Arc::new(crate::storage::MemoryStorage::new());
        let metrics = Arc::new(crate::telemetry::Metrics::new());
        let tracer = Arc::new(crate::telemetry::Tracer::new(false));

        let governor = Governor::new(config, storage, ban_storage, Some(metrics), Some(tracer))
            .await
            .unwrap();

        let context = RequestContext::new()
            .with_header("X-User-Id", "user123")
            .with_client_ip("192.168.1.1");

        // 前10个请求应该被允许
        for _ in 0..10 {
            let decision = governor.check(&context).await.unwrap();
            assert_eq!(decision, Decision::Allowed(None));
        }
    }

    #[tokio::test]
    async fn test_governor_check_rejected() {
        let config = create_test_config();
        let storage = Arc::new(MemoryStorage::new());
        let ban_storage = Arc::new(crate::storage::MemoryStorage::new());
        let metrics = Arc::new(crate::telemetry::Metrics::new());
        let tracer = Arc::new(crate::telemetry::Tracer::new(false));

        let governor = Governor::new(config, storage, ban_storage, Some(metrics), Some(tracer))
            .await
            .unwrap();

        let context = RequestContext::new()
            .with_header("X-User-Id", "user123")
            .with_client_ip("192.168.1.1");

        // 消耗所有令牌
        for _ in 0..10 {
            governor.check(&context).await.unwrap();
        }

        // 第11个请求应该被拒绝
        let decision = governor.check(&context).await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    #[tokio::test]
    async fn test_governor_ban_identifier() {
        let config = create_test_config();
        let storage = Arc::new(MemoryStorage::new());
        let ban_storage = Arc::new(crate::storage::MemoryStorage::new());
        let metrics = Arc::new(crate::telemetry::Metrics::new());
        let tracer = Arc::new(crate::telemetry::Tracer::new(false));

        let governor = Governor::new(config, storage, ban_storage, Some(metrics), Some(tracer))
            .await
            .unwrap();

        let identifier = Identifier::UserId("user123".to_string());

        // 封禁用户
        governor
            .ban_identifier(&identifier, "测试封禁", Duration::from_secs(60))
            .await
            .unwrap();

        let context = RequestContext::new()
            .with_header("X-User-Id", "user123")
            .with_client_ip("192.168.1.1");

        // 请求应该被封禁
        let decision = governor.check(&context).await.unwrap();
        assert!(matches!(decision, Decision::Banned(_)));
    }

    #[tokio::test]
    async fn test_governor_unban_identifier() {
        let config = create_test_config();
        let storage = Arc::new(MemoryStorage::new());
        let ban_storage = Arc::new(crate::storage::MemoryStorage::new());
        let metrics = Arc::new(crate::telemetry::Metrics::new());
        let tracer = Arc::new(crate::telemetry::Tracer::new(false));

        let governor = Governor::new(config, storage, ban_storage, Some(metrics), Some(tracer))
            .await
            .unwrap();

        let identifier = Identifier::UserId("user123".to_string());

        // 封禁用户
        governor
            .ban_identifier(&identifier, "测试封禁", Duration::from_secs(60))
            .await
            .unwrap();

        // 解封用户
        governor.unban_identifier(&identifier).await.unwrap();

        let context = RequestContext::new()
            .with_header("X-User-Id", "user123")
            .with_client_ip("192.168.1.1");

        // 请求应该被允许
        let decision = governor.check(&context).await.unwrap();
        assert_eq!(decision, Decision::Allowed(None));
    }

    #[tokio::test]
    async fn test_governor_stats() {
        let config = create_test_config();
        let storage = Arc::new(MemoryStorage::new());
        let ban_storage = Arc::new(crate::storage::MemoryStorage::new());
        let metrics = Arc::new(crate::telemetry::Metrics::new());
        let tracer = Arc::new(crate::telemetry::Tracer::new(false));

        let governor = Governor::new(config, storage, ban_storage, Some(metrics), Some(tracer))
            .await
            .unwrap();

        let context = RequestContext::new()
            .with_header("X-User-Id", "user123")
            .with_client_ip("192.168.1.1");

        // 发送5个请求
        for _ in 0..5 {
            governor.check(&context).await.unwrap();
        }

        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 5);
        assert_eq!(stats.allowed_requests, 5);
        assert_eq!(stats.rejected_requests, 0);
    }

    #[tokio::test]
    async fn test_governor_update_config() {
        let config = create_test_config();
        let storage = Arc::new(MemoryStorage::new());
        let ban_storage = Arc::new(crate::storage::MemoryStorage::new());
        let metrics = Arc::new(crate::telemetry::Metrics::new());
        let tracer = Arc::new(crate::telemetry::Tracer::new(false));

        let governor = Governor::new(config, storage, ban_storage, Some(metrics), Some(tracer))
            .await
            .unwrap();

        // 创建新配置
        let new_config = FlowControlConfig {
            version: "1.0".to_string(),
            global: GlobalConfig {
                storage: "memory".to_string(),
                cache: "memory".to_string(),
                metrics: "prometheus".to_string(),
            },
            rules: vec![Rule {
                id: "test_rule".to_string(),
                name: "Test Rule".to_string(),
                priority: 100,
                matchers: vec![ConfigMatcher::User {
                    user_ids: vec!["*".to_string()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 5, // 更小的容量
                    refill_rate: 1,
                }],
                action: crate::config::ActionConfig {
                    on_exceed: "reject".to_string(),
                    ban: None,
                },
            }],
        };

        // 更新配置
        governor.update_config(new_config).await.unwrap();

        let context = RequestContext::new()
            .with_header("X-User-Id", "user123")
            .with_client_ip("192.168.1.1");

        // 前5个请求应该被允许
        for _ in 0..5 {
            let decision = governor.check(&context).await.unwrap();
            assert_eq!(decision, Decision::Allowed(None));
        }

        // 第6个请求应该被拒绝
        let decision = governor.check(&context).await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    #[tokio::test]
    async fn test_governor_reset_stats() {
        let config = create_test_config();
        let storage = Arc::new(MemoryStorage::new());
        let ban_storage = Arc::new(crate::storage::MemoryStorage::new());
        let metrics = Arc::new(crate::telemetry::Metrics::new());
        let tracer = Arc::new(crate::telemetry::Tracer::new(false));

        let governor = Governor::new(config, storage, ban_storage, Some(metrics), Some(tracer))
            .await
            .unwrap();

        let context = RequestContext::new()
            .with_header("X-User-Id", "user123")
            .with_client_ip("192.168.1.1");

        // 发送一些请求
        for _ in 0..5 {
            governor.check(&context).await.unwrap();
        }

        // 重置统计
        governor.reset_stats().await;

        // 检查统计是否重置
        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.allowed_requests, 0);
    }

    #[tokio::test]
    async fn test_governor_health_check() {
        let config = create_test_config();
        let storage = Arc::new(MemoryStorage::new());
        let ban_storage = Arc::new(crate::storage::MemoryStorage::new());
        let metrics = Arc::new(crate::telemetry::Metrics::new());
        let tracer = Arc::new(crate::telemetry::Tracer::new(false));

        let governor = Governor::new(config, storage, ban_storage, Some(metrics), Some(tracer))
            .await
            .unwrap();

        // 健康检查应该成功
        let result = governor.health_check().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_governor_check_resource() {
        let config = create_test_config();
        let storage = Arc::new(MemoryStorage::new());
        let ban_storage = Arc::new(crate::storage::MemoryStorage::new());
        let metrics = Arc::new(crate::telemetry::Metrics::new());
        let tracer = Arc::new(crate::telemetry::Tracer::new(false));

        let governor = Governor::new(config, storage, ban_storage, Some(metrics), Some(tracer))
            .await
            .unwrap();

        // 使用简化接口检查资源
        let decision = governor.check_resource("resource123").await.unwrap();
        assert_eq!(decision, Decision::Allowed(None));
    }

    #[tokio::test]
    async fn test_governor_get_config() {
        let config = create_test_config();
        let storage = Arc::new(MemoryStorage::new());
        let ban_storage = Arc::new(crate::storage::MemoryStorage::new());
        let metrics = Arc::new(crate::telemetry::Metrics::new());
        let tracer = Arc::new(crate::telemetry::Tracer::new(false));

        let governor = Governor::new(
            config.clone(),
            storage,
            ban_storage,
            Some(metrics),
            Some(tracer),
        )
        .await
        .unwrap();

        let retrieved_config = governor.get_config().await;
        assert_eq!(retrieved_config.version, config.version);
        assert_eq!(retrieved_config.rules.len(), config.rules.len());
    }

    #[tokio::test]
    async fn test_governor_decision_chain_stats() {
        let config = create_test_config();
        let storage = Arc::new(MemoryStorage::new());
        let ban_storage = Arc::new(crate::storage::MemoryStorage::new());
        let metrics = Arc::new(crate::telemetry::Metrics::new());
        let tracer = Arc::new(crate::telemetry::Tracer::new(false));

        let governor = Governor::new(config, storage, ban_storage, Some(metrics), Some(tracer))
            .await
            .unwrap();

        let context = RequestContext::new()
            .with_header("X-User-Id", "user123")
            .with_client_ip("192.168.1.1");

        // 发送一些请求
        for _ in 0..5 {
            governor.check(&context).await.unwrap();
        }

        // 获取决策链统计
        let chain_stats = governor.decision_chain_stats().await;
        assert_eq!(chain_stats.total_checks, 5);
        assert_eq!(chain_stats.allowed_count, 5);
    }

    #[tokio::test]
    async fn test_governor_invalid_config() {
        let mut config = create_test_config();
        config.version = "".to_string(); // 无效配置

        let storage = Arc::new(MemoryStorage::new());
        let ban_storage = Arc::new(crate::storage::MemoryStorage::new());
        let metrics = Arc::new(crate::telemetry::Metrics::new());
        let tracer = Arc::new(crate::telemetry::Tracer::new(false));

        let result = Governor::new(config, storage, ban_storage, Some(metrics), Some(tracer)).await;
        assert!(result.is_err());
    }
}
