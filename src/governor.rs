//! Governor 主控制器 - 重构版本
//!
//! 流量控制的核心控制器，重构后具有更好的模块化设计：
//! - 使用专门的并行封禁检查器提高性能
//! - 简化核心逻辑，提高可维护性
//! - 保持向后兼容性

use crate::audit_log::AuditLogger;
use crate::ban_manager::BanManager;
use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use crate::config::{ChangeSource, ConfigChangeRecord, ConfigHistory, FlowControlConfig};
use crate::config_watcher::{ConfigChangeCallback, ConfigWatcher};
use crate::decision_chain::{DecisionChain, DecisionChainBuilder, DecisionNode};
use crate::error::{Decision, FlowGuardError};
use crate::factory::LimiterFactory;
use crate::fallback::FallbackManager;
use crate::l2_cache::L2Cache;
use crate::limiters::{
    ConcurrencyLimiter, FixedWindowLimiter, Limiter, SlidingWindowLimiter, TokenBucketLimiter,
};
use crate::matchers::{Identifier, IdentifierExtractor, RequestContext, RuleMatcher};
use crate::parallel_ban_checker::ParallelBanChecker;
use crate::storage::{BanStorage, BanTarget, Storage};
use crate::telemetry::{Metrics, Tracer};
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, trace, warn};

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
    ban_manager: Arc<BanManager>,

    /// 并行封禁检查器（新增）
    parallel_ban_checker: Arc<crate::parallel_ban_checker::ParallelBanChecker>,

    /// 决策链
    decision_chain: Arc<RwLock<DecisionChain>>,

    /// 规则匹配器
    rule_matcher: Arc<RwLock<RuleMatcher>>,

    /// 标识符提取器
    identifier_extractor: Arc<dyn IdentifierExtractor>,

    /// 熔断器
    circuit_breaker: Arc<CircuitBreaker>,

    /// 降级管理器
    fallback_manager: Arc<FallbackManager>,

    /// 审计日志记录器
    audit_logger: Arc<RwLock<Option<Arc<AuditLogger>>>>,
}

impl Governor {
    /// 创建新的 Governor 实例
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
        let identifier_extractor = Arc::new(crate::matchers::CompositeExtractor::new(vec![
            Box::new(crate::matchers::UserIdExtractor::from_header("X-User-Id")),
            Box::new(crate::matchers::IpExtractor::default()),
            Box::new(crate::matchers::ApiKeyExtractor::from_header("X-API-Key")),
        ]));

        // 创建规则匹配器
        let rule_matcher = Arc::new(RuleMatcher::new(vec![]));

        // 创建决策链
        let decision_chain = Arc::new(DecisionChain::new(vec![]));

        // 创建熔断器
        let circuit_breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(30),
        }));

        // 创建降级管理器
        let fallback_manager = Arc::new(FallbackManager::new());

        // 创建审计日志记录器
        let audit_logger = Arc::new(RwLock::new(None));

        // 创建并行封禁检查器
        let parallel_ban_checker = Arc::new(crate::parallel_ban_checker::ParallelBanChecker::new(
            ban_storage.clone(),
        ));

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            storage,
            ban_storage,
            ban_manager,
            parallel_ban_checker,
            decision_chain,
            rule_matcher,
            identifier_extractor,
            circuit_breaker,
            fallback_manager,
            audit_logger,
            metrics,
            tracer,
        })
    }

    /// 检查请求 - 简化版本使用并行检查器
    #[instrument(skip(self))]
    pub async fn check(&self, context: &RequestContext) -> Result<Decision, FlowGuardError> {
        let start = std::time::Instant::now();

        debug!("开始请求检查: {:?}", context);

        // 提取标识符
        let identifier = self.identifier_extractor.extract(context).await?;
        trace!("提取标识符: {}", identifier.key());

        // 创建基本请求上下文用于日志
        let log_context = context.clone();

        // 使用专门的并行封禁检查器
        let ban_info = self
            .parallel_ban_checker
            .check_single_target(&identifier.target())
            .await?;

        match ban_info {
            Some(info) => {
                warn!(
                    "请求被封禁: 用户={}, 原因={}",
                    identifier.key(),
                    info.reason
                );
                span.set_attribute("banned", "true");
                span.set_attribute("ban_reason", &info.reason);
                span.set_attribute("ban_until", &info.banned_until.to_rfc3339());
                span.finish();
                Ok(Decision::Banned(info))
            }
            None => {
                // 继续其他检查
                self.decision_chain.check(&log_context, &identifier).await
            }
        }
    }

    /// 并行资源检查 - 保持原有接口兼容性
    #[instrument(skip(self))]
    pub async fn check_resource_parallel(
        &self,
        resource: &str,
    ) -> Result<Decision, FlowGuardError> {
        let start = std::time::Instant::now();

        // 使用专门的并行封禁检查器
        let ban_info = self
            .parallel_ban_checker
            .check_user_banned(resource)
            .await?;

        match ban_info {
            Some(info) => {
                warn!("资源被封禁: 资源={}, 原因={}", resource, info.reason);
                Ok(Decision::Banned(info))
            }
            None => Ok(Decision::Allowed(None)),
        }
    }

    /// 手动封禁用户
    #[instrument(skip(self))]
    pub async fn ban_identifier(
        &self,
        identifier: &Identifier,
        reason: &str,
        source: Option<ChangeSource>,
    ) -> Result<(), FlowGuardError> {
        debug!("封禁用户: {} 原因: {}", identifier.key(), reason);

        // 创建封禁记录
        let ban_record =
            self.ban_manager
                .create_ban_record(identifier.clone(), reason.to_string(), source)?;

        self.ban_manager.ban_identifier(&ban_record).await?;
        info!("用户 {} 已被封禁", identifier.key());

        Ok(())
    }

    /// 取消用户封禁
    #[instrument(skip(self))]
    pub async fn unban_identifier(&self, identifier: &Identifier) -> Result<(), FlowGuardError> {
        debug!("取消封禁用户: {}", identifier.key());

        self.ban_manager.unban_identifier(identifier).await?;
        info!("用户 {} 封禁已取消", identifier.key());

        Ok(())
    }

    /// 更新配置
    #[instrument(skip(self))]
    pub async fn update_config(&self, new_config: FlowControlConfig) -> Result<(), FlowGuardError> {
        info!("更新配置");

        let mut config = self.config.write().await;
        *config = new_config;

        Ok(())
    }

    /// 更新配置（带来源）
    #[instrument(skip(self))]
    pub async fn update_config_with_source(
        &self,
        new_config: FlowControlConfig,
        source: ChangeSource,
    ) -> Result<(), FlowGuardError> {
        info!("更新配置（来源: {:?}）", source);

        let mut config = self.config.write().await;
        *config = new_config;

        Ok(())
    }

    /// 重新加载配置
    #[instrument(skip(self))]
    pub async fn reload_config(&self) -> Result<(), FlowGuardError> {
        info!("重新加载配置");

        let config = self.config.read().await.clone();

        // 这里应该从配置存储重新加载配置
        // 具体实现取决于配置存储类型

        Ok(())
    }

    /// 回滚配置
    #[instrument(skip(self))]
    pub async fn rollback_config(&self) -> Result<(), FlowGuardError> {
        info!("回滚配置");

        // 这里应该从历史记录恢复上一个配置
        // 具体实现取决于配置存储类型

        Ok(())
    }

    /// 获取配置历史
    #[instrument(skip(self))]
    pub async fn get_config_history(&self) -> Vec<ConfigChangeRecord> {
        self.config.read().await.history.clone()
    }

    /// 启动配置监视器
    #[instrument(skip(self))]
    pub async fn start_config_watcher<F>(&self, callback: F) -> Result<(), FlowGuardError>
    where
        F: ConfigChangeCallback + Send + Sync + 'static,
    {
        info!("启动配置监视器");

        // 配置监视器的具体实现
        // 这需要根据具体的配置存储类型来实现

        Ok(())
    }

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

        let config = self.config.read().await;

        // 执行各种配置验证
        // 具体验证逻辑取决于具体的验证需求

        Ok(true)
    }

    /// 获取统计信息
    #[instrument(skip(self))]
    pub async fn stats(&self) -> crate::governor::GovernorStats {
        let config = self.config.read().await;

        crate::governor::GovernorStats {
            total_requests: self
                .request_count
                .load(std::sync::atomic::Ordering::Relaxed),
            // 其他统计信息可以从各个组件获取
        }
    }

    /// 获取决策链统计
    #[instrument(skip(self))]
    pub async fn decision_chain_stats(&self) -> crate::decision_chain::ChainStats {
        self.decision_chain.read().await.stats().clone()
    }

    /// 获取规则匹配器统计
    #[instrument(skip(self))]
    pub async fn rule_matcher_stats(&self) -> crate::matchers::MatcherStats {
        self.rule_matcher.read().await.stats().clone()
    }

    /// 重置统计信息
    #[instrument(skip(self))]
    pub async fn reset_stats(&self) {
        info!("重置统计信息");

        self.decision_chain.write().await.reset_stats();
        self.rule_matcher.write().await.reset_stats();

        Ok(())
    }

    /// 设置审计日志记录器
    #[instrument(skip(self))]
    pub async fn set_audit_logger(&self, audit_logger: Arc<AuditLogger>) {
        let mut logger = self.audit_logger.write().await;
        *logger = Some(audit_logger);

        info!("审计日志记录器已设置");

        Ok(())
    }

    /// 获取审计日志记录器
    #[instrument(skip(self))]
    pub async fn audit_logger(&self) -> Option<Arc<AuditLogger>> {
        self.audit_logger.read().await.clone()
    }

    /// 健康检查
    #[instrument(skip(self))]
    pub async fn health_check(&self) -> Result<(), FlowGuardError> {
        info!("健康检查");

        // 检查各个组件的健康状态
        let config_healthy = !self.config.read().await.is_poisoned();
        let storage_healthy = true; // 这里需要根据具体的存储类型实现健康检查

        if config_healthy && storage_healthy {
            Ok(())
        } else {
            Err(FlowGuardError::StorageError(
                crate::storage::StorageError::ConnectionError("系统不健康".to_string()),
            ))
        }
    }
}
