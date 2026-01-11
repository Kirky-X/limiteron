//! Governor 主控制器 - 重构版本
//!
//! 流量控制的核心控制器，重构后具有更好的模块化设计：
//! - 使用专门的并行封禁检查器提高性能
//! - 简化核心逻辑，提高可维护性
//! - 保持向后兼容性

use crate::config::{ChangeSource, ConfigChangeRecord, FlowControlConfig};
use crate::decision_chain::DecisionChain;
use crate::error::{Decision, FlowGuardError};
use crate::fallback::FallbackManager;
use crate::l2_cache::L2Cache;
use crate::matchers::{Identifier, IdentifierExtractor, RequestContext, RuleMatcher};
use crate::storage::{BanStorage, BanTarget, Storage};
use chrono::Utc;
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
    _storage: Arc<dyn Storage>,

    /// 封禁存储
    _ban_storage: Arc<dyn BanStorage>,

    /// 封禁管理器
    #[cfg(feature = "ban-manager")]
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
    #[cfg(feature = "circuit-breaker")]
    circuit_breaker: Arc<CircuitBreaker>,

    /// 降级管理器
    _fallback_manager: Arc<FallbackManager>,

    /// 审计日志记录器
    #[cfg(feature = "audit-log")]
    audit_logger: Arc<RwLock<Option<Arc<AuditLogger>>>>,

    // 统计计数器
    total_requests: AtomicU64,
    allowed_requests: AtomicU64,
    rejected_requests: AtomicU64,
    banned_requests: AtomicU64,
    error_count: AtomicU64,
}

fn redact_for_log(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "unknown".to_string();
    };

    let value = value.trim();
    if value.is_empty() {
        return "unknown".to_string();
    }

    if value.len() <= 4 {
        return "***".to_string();
    }

    let prefix = &value[..2];
    let suffix = &value[value.len() - 2..];
    format!("{}***{}", prefix, suffix)
}

impl Governor {
    /// 将 Identifier 转换为 BanTarget
    ///
    /// # 参数
    /// - `identifier`: 标识符
    ///
    /// # 返回
    /// - `Some(BanTarget)`: 转换成功
    /// - `None`: 无法转换（如 ApiKey, DeviceId）
    fn identifier_to_ban_target(identifier: &Identifier) -> Option<BanTarget> {
        match identifier {
            Identifier::UserId(id) => Some(BanTarget::UserId(id.clone())),
            Identifier::Ip(ip) => Some(BanTarget::Ip(ip.clone())),
            Identifier::Mac(mac) => Some(BanTarget::Mac(mac.clone())),
            _ => None,
        }
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
        let rule_matcher = Arc::new(RwLock::new(RuleMatcher::new(vec![])));

        // 创建决策链
        let decision_chain = Arc::new(RwLock::new(DecisionChain::new(vec![])));

        // 创建熔断器 (仅当 circuit-breaker 特性启用时)
        #[cfg(feature = "circuit-breaker")]
        let circuit_breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(30),
            half_open_max_calls: 10,
        }));

        // 创建 L2Cache 用于 FallbackManager
        let fallback_l2_cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));
        // 创建降级管理器
        let fallback_manager = Arc::new(FallbackManager::new(fallback_l2_cache));

        // 创建审计日志记录器 (仅当 audit-log 特性启用时)
        #[cfg(feature = "audit-log")]
        let audit_logger = Arc::new(RwLock::new(None));

        // 创建封禁管理器 (仅当 ban-manager 特性启用时)
        #[cfg(feature = "ban-manager")]
        let ban_manager = Arc::new(BanManager::new(ban_storage.clone(), None).await?);

        // 创建并行封禁检查器
        #[cfg(feature = "ban-manager")]
        let parallel_ban_checker = Arc::new(crate::parallel_ban_checker::ParallelBanChecker::new(
            ban_manager.clone(),
        ));

        #[cfg(not(feature = "ban-manager"))]
        let parallel_ban_checker = Arc::new(crate::parallel_ban_checker::ParallelBanChecker::new(
            ban_storage.clone(),
        ));

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            _storage: storage,
            _ban_storage: ban_storage,
            #[cfg(feature = "ban-manager")]
            ban_manager,
            parallel_ban_checker,
            decision_chain,
            rule_matcher,
            identifier_extractor,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker,
            _fallback_manager: fallback_manager,
            #[cfg(feature = "audit-log")]
            audit_logger,
            total_requests: AtomicU64::new(0),
            allowed_requests: AtomicU64::new(0),
            rejected_requests: AtomicU64::new(0),
            banned_requests: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
        })
    }

    /// 检查请求 - 简化版本使用并行检查器
    #[instrument(skip(self), fields(
        user_id = %redact_for_log(context.user_id.as_deref()),
        ip = %redact_for_log(context.ip.as_deref()),
        path = %context.path,
        method = %context.method
    ))]
    pub async fn check(&self, context: &RequestContext) -> Result<Decision, FlowGuardError> {
        self.total_requests.fetch_add(1, Ordering::Relaxed);

        debug!(
            "开始请求检查: user_id={}, ip={}, path={}, method={}",
            redact_for_log(context.user_id.as_deref()),
            redact_for_log(context.ip.as_deref()),
            context.path,
            context.method
        );

        // 提取标识符
        let identifier = self
            .identifier_extractor
            .extract(context)
            .ok_or_else(|| FlowGuardError::ConfigError("无法提取标识符".to_string()))?;
        trace!("提取标识符: {}", identifier.key());

        // 尝试转换为 BanTarget 进行检查
        let ban_target = Self::identifier_to_ban_target(&identifier);

        if let Some(target) = ban_target {
            // 使用专门的并行封禁检查器
            let ban_info = self
                .parallel_ban_checker
                .check_single_target(&target)
                .await?;

            if let Some(info) = ban_info {
                warn!(
                    "请求被封禁: 用户={}, 原因={}",
                    identifier.key(),
                    info.reason
                );
                self.banned_requests.fetch_add(1, Ordering::Relaxed);
                return Ok(Decision::Banned(info));
            }
        }

        // 继续其他检查
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

        result
    }

    /// 并行资源检查 - 保持原有接口兼容性
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
                warn!("资源被封禁: 资源={}, 原因={}", resource, info.reason);
                Ok(Decision::Banned(info))
            }
            None => Ok(Decision::Allowed(None)),
        }
    }

    /// 手动封禁用户
    #[cfg(feature = "ban-manager")]
    #[instrument(skip(self))]
    pub async fn ban_identifier(
        &self,
        identifier: &Identifier,
        reason: &str,
        source: Option<ChangeSource>,
    ) -> Result<(), FlowGuardError> {
        debug!("封禁用户: {} 原因: {}", identifier.key(), reason);

        let ban_target = Self::identifier_to_ban_target(identifier);

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
            info!("用户 {} 已被封禁", identifier.key());
        } else {
            return Err(FlowGuardError::ValidationError(
                "不支持的标识符类型".to_string(),
            ));
        }

        Ok(())
    }

    /// 取消用户封禁
    #[cfg(feature = "ban-manager")]
    #[instrument(skip(self))]
    pub async fn unban_identifier(&self, identifier: &Identifier) -> Result<(), FlowGuardError> {
        debug!("取消封禁用户: {}", identifier.key());

        let ban_target = Self::identifier_to_ban_target(identifier);

        if let Some(target) = ban_target {
            self.ban_manager
                .delete_ban(&target, "admin".to_string())
                .await?;
            info!("用户 {} 封禁已取消", identifier.key());
        } else {
            return Err(FlowGuardError::ValidationError(
                "不支持的标识符类型".to_string(),
            ));
        }

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

        let _config = self.config.read().await.clone();

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
    pub async fn get_config_history(&self) -> Vec<ConfigChangeRecord> {
        // self.config.read().await.history.clone()
        vec![] // TODO: Implement configuration history
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
        let _config = self.config.read().await;

        crate::governor::GovernorStats {
            total_requests: self.total_requests.load(Ordering::Relaxed),
            allowed_requests: self.allowed_requests.load(Ordering::Relaxed),
            rejected_requests: self.rejected_requests.load(Ordering::Relaxed),
            banned_requests: self.banned_requests.load(Ordering::Relaxed),
            error_count: self.error_count.load(Ordering::Relaxed),
            last_updated: Some(Utc::now()),
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
        self.audit_logger.read().await.clone()
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
                crate::error::StorageError::ConnectionError("系统不健康".to_string()),
            ))
        }
    }
}
