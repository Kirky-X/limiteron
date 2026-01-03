//! 降级策略实现
//!
//! 提供降级策略管理，支持故障时自动降级到备用方案。
//!
//! # 特性
//!
//! - **多种策略**: FailOpen、FailClosed、Degraded
//! - **组件级配置**: 为不同组件配置不同策略
//! - **热更新**: 支持动态更新策略
//! - **故障注入**: 支持模拟故障进行测试

use crate::error::{FlowGuardError, StorageError};
use crate::l2_cache::L2Cache;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// 降级策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FallbackStrategy {
    /// 故障时允许所有请求（降级为全开放）
    FailOpen,
    /// 故障时拒绝所有请求（降级为全关闭）
    FailClosed,
    /// 故障时使用降级服务（如L2缓存、缓存配置）
    Degraded,
}

/// 组件类型
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ComponentType {
    /// Redis存储
    Redis,
    /// PostgreSQL存储
    Postgres,
    /// L3缓存
    L3Cache,
    /// 配置服务
    Config,
    /// 封禁服务
    Ban,
    /// 配额服务
    Quota,
    /// 其他组件
    Other(String),
}

impl ComponentType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "redis" => ComponentType::Redis,
            "postgres" => ComponentType::Postgres,
            "l3_cache" => ComponentType::L3Cache,
            "config" => ComponentType::Config,
            "ban" => ComponentType::Ban,
            "quota" => ComponentType::Quota,
            other => ComponentType::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            ComponentType::Redis => "redis",
            ComponentType::Postgres => "postgres",
            ComponentType::L3Cache => "l3_cache",
            ComponentType::Config => "config",
            ComponentType::Ban => "ban",
            ComponentType::Quota => "quota",
            ComponentType::Other(s) => s,
        }
    }
}

/// 降级策略配置
#[derive(Debug, Clone)]
pub struct FallbackConfig {
    /// 组件类型
    pub component: ComponentType,
    /// 降级策略
    pub strategy: FallbackStrategy,
    /// 是否启用
    pub enabled: bool,
    /// 降级超时时间
    pub timeout: Duration,
    /// 最大重试次数
    pub max_retries: u32,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            component: ComponentType::Other("default".to_string()),
            strategy: FallbackStrategy::Degraded,
            enabled: true,
            timeout: Duration::from_secs(5),
            max_retries: 3,
        }
    }
}

impl FallbackConfig {
    pub fn new(component: ComponentType, strategy: FallbackStrategy) -> Self {
        Self {
            component,
            strategy,
            ..Default::default()
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }
}

/// 降级策略管理器
pub struct FallbackManager {
    /// 策略配置
    strategies: Arc<RwLock<HashMap<ComponentType, FallbackConfig>>>,
    /// L2缓存（用于降级）
    l2_cache: Arc<L2Cache>,
    /// 故障状态
    failure_states: Arc<RwLock<HashMap<ComponentType, bool>>>,
}

impl FallbackManager {
    /// 创建新的降级策略管理器
    ///
    /// # 参数
    /// - `l2_cache`: L2缓存实例
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::fallback::{FallbackManager, FallbackStrategy, ComponentType};
    /// use limiteron::l2_cache::L2Cache;
    /// use std::time::Duration;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let l2_cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));
    /// let manager = FallbackManager::new(l2_cache);
    /// # }
    /// ```
    pub fn new(l2_cache: Arc<L2Cache>) -> Self {
        info!("创建降级策略管理器");

        // 默认策略
        let mut strategies = HashMap::new();
        strategies.insert(
            ComponentType::Redis,
            FallbackConfig::new(ComponentType::Redis, FallbackStrategy::Degraded),
        );
        strategies.insert(
            ComponentType::Postgres,
            FallbackConfig::new(ComponentType::Postgres, FallbackStrategy::Degraded),
        );
        strategies.insert(
            ComponentType::L3Cache,
            FallbackConfig::new(ComponentType::L3Cache, FallbackStrategy::Degraded),
        );
        strategies.insert(
            ComponentType::Config,
            FallbackConfig::new(ComponentType::Config, FallbackStrategy::FailClosed),
        );
        strategies.insert(
            ComponentType::Ban,
            FallbackConfig::new(ComponentType::Ban, FallbackStrategy::Degraded),
        );
        strategies.insert(
            ComponentType::Quota,
            FallbackConfig::new(ComponentType::Quota, FallbackStrategy::Degraded),
        );

        Self {
            strategies: Arc::new(RwLock::new(strategies)),
            l2_cache,
            failure_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 设置降级策略
    ///
    /// # 参数
    /// - `component`: 组件类型
    /// - `config`: 策略配置
    pub async fn set_strategy(&self, component: ComponentType, config: FallbackConfig) {
        info!(
            "设置降级策略: component={:?}, strategy={:?}",
            component, config.strategy
        );

        let mut strategies = self.strategies.write().await;
        strategies.insert(component, config);
    }

    /// 获取降级策略
    ///
    /// # 参数
    /// - `component`: 组件类型
    ///
    /// # 返回
    /// - 策略配置
    pub async fn get_strategy(&self, component: ComponentType) -> Option<FallbackConfig> {
        let strategies = self.strategies.read().await;
        strategies.get(&component).cloned()
    }

    /// 执行带降级策略的操作
    ///
    /// # 参数
    /// - `component`: 组件类型
    /// - `operation`: 要执行的操作
    /// - `fallback_operation`: 降级操作
    ///
    /// # 返回
    /// - `Ok(T)`: 操作成功
    /// - `Err(FlowGuardError)`: 操作失败且降级也失败
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::fallback::{FallbackManager, FallbackStrategy, ComponentType};
    /// use limiteron::error::FlowGuardError;
    /// use limiteron::l2_cache::L2Cache;
    /// use std::time::Duration;
    /// use std::sync::Arc;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let l2_cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));
    /// let manager = FallbackManager::new(l2_cache);
    ///
    /// let result = manager.execute_with_fallback(
    ///     ComponentType::Redis,
    ///     || async { Ok::<String, FlowGuardError>("primary".to_string()) },
    ///     || async { Ok::<String, FlowGuardError>("fallback".to_string()) }
    /// ).await;
    /// # }
    /// ```
    pub async fn execute_with_fallback<F, Fut, FB, FBFut, T>(
        &self,
        component: ComponentType,
        operation: F,
        fallback_operation: FB,
    ) -> Result<T, FlowGuardError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, FlowGuardError>>,
        FB: FnOnce() -> FBFut,
        FBFut: std::future::Future<Output = Result<T, FlowGuardError>>,
    {
        let config = self
            .get_strategy(component.clone())
            .await
            .unwrap_or_default();

        if !config.enabled {
            // 策略未启用，直接执行操作
            return operation().await;
        }

        // 检查是否处于故障状态
        let is_failed = {
            let states = self.failure_states.read().await;
            *states.get(&component).unwrap_or(&false)
        };

        // 尝试执行主操作（即使在故障状态下也要尝试，以检测是否恢复）
        let result = operation().await;

        match result {
            Ok(value) => {
                // 操作成功，清除故障状态
                self.clear_failure(component).await;
                Ok(value)
            }
            Err(e) => {
                // 操作失败，根据策略处理
                warn!("组件操作失败: component={:?}, error={}", component, e);

                // 标记为故障状态
                self.set_failure(component.clone()).await;

                // 执行降级策略
                self.execute_fallback(component, config, fallback_operation)
                    .await
            }
        }
    }

    /// 执行降级策略
    async fn execute_fallback<F, Fut, T>(
        &self,
        component: ComponentType,
        config: FallbackConfig,
        fallback_operation: F,
    ) -> Result<T, FlowGuardError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, FlowGuardError>>,
    {
        info!(
            "执行降级策略: component={:?}, strategy={:?}",
            component, config.strategy
        );

        match config.strategy {
            FallbackStrategy::FailOpen => {
                // 故障开放：返回默认值或允许请求
                warn!("降级策略: FailOpen - 允许请求通过");
                Err(FlowGuardError::LimitError(
                    "服务降级，但允许请求通过".to_string(),
                ))
            }
            FallbackStrategy::FailClosed => {
                // 故障关闭：拒绝请求
                error!("降级策略: FailClosed - 拒绝请求");
                Err(FlowGuardError::StorageError(StorageError::ConnectionError(
                    "服务降级，拒绝请求".to_string(),
                )))
            }
            FallbackStrategy::Degraded => {
                // 降级服务：使用备用方案
                debug!("降级策略: Degraded - 使用备用方案");
                fallback_operation().await
            }
        }
    }

    /// 标记组件为故障状态
    async fn set_failure(&self, component: ComponentType) {
        warn!("组件故障: {:?}", component);
        let mut states = self.failure_states.write().await;
        states.insert(component, true);
    }

    /// 清除组件故障状态
    async fn clear_failure(&self, component: ComponentType) {
        let mut states = self.failure_states.write().await;
        states.remove(&component);
        info!("组件恢复: {:?}", component);
    }

    /// 检查组件是否故障
    pub async fn is_failed(&self, component: ComponentType) -> bool {
        let states = self.failure_states.read().await;
        *states.get(&component).unwrap_or(&false)
    }

    /// 手动触发故障（用于测试）
    pub async fn inject_failure(&self, component: ComponentType) {
        warn!("注入故障: {:?}", component);
        self.set_failure(component).await;
    }

    /// 手动恢复故障（用于测试）
    pub async fn recover_failure(&self, component: ComponentType) {
        info!("恢复故障: {:?}", component);
        self.clear_failure(component).await;
    }

    /// 获取所有故障状态
    pub async fn get_all_failures(&self) -> Vec<ComponentType> {
        let states = self.failure_states.read().await;
        states
            .iter()
            .filter(|(_, &failed)| failed)
            .map(|(component, _)| component.clone())
            .collect()
    }

    /// 获取L2缓存
    pub fn l2_cache(&self) -> &Arc<L2Cache> {
        &self.l2_cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_strategy_display() {
        let strategy = FallbackStrategy::FailOpen;
        assert_eq!(format!("{:?}", strategy), "FailOpen");

        let strategy = FallbackStrategy::FailClosed;
        assert_eq!(format!("{:?}", strategy), "FailClosed");

        let strategy = FallbackStrategy::Degraded;
        assert_eq!(format!("{:?}", strategy), "Degraded");
    }

    #[test]
    fn test_component_type_from_str() {
        assert_eq!(ComponentType::from_str("redis"), ComponentType::Redis);
        assert_eq!(ComponentType::from_str("postgres"), ComponentType::Postgres);
        assert_eq!(ComponentType::from_str("l3_cache"), ComponentType::L3Cache);
        assert_eq!(
            ComponentType::from_str("other"),
            ComponentType::Other("other".to_string())
        );
    }

    #[test]
    fn test_component_type_as_str() {
        assert_eq!(ComponentType::Redis.as_str(), "redis");
        assert_eq!(ComponentType::Postgres.as_str(), "postgres");
        assert_eq!(ComponentType::L3Cache.as_str(), "l3_cache");
    }

    #[test]
    fn test_fallback_config_default() {
        let config = FallbackConfig::default();
        assert_eq!(config.strategy, FallbackStrategy::Degraded);
        assert!(config.enabled);
        assert_eq!(config.timeout, Duration::from_secs(5));
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_fallback_config_builder() {
        let config = FallbackConfig::new(ComponentType::Redis, FallbackStrategy::FailOpen)
            .enabled(false)
            .timeout(Duration::from_secs(10))
            .max_retries(5);

        assert_eq!(config.component, ComponentType::Redis);
        assert_eq!(config.strategy, FallbackStrategy::FailOpen);
        assert!(!config.enabled);
        assert_eq!(config.timeout, Duration::from_secs(10));
        assert_eq!(config.max_retries, 5);
    }

    #[tokio::test]
    async fn test_fallback_manager_new() {
        let l2_cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));
        let manager = FallbackManager::new(l2_cache);

        // 验证默认策略
        let redis_strategy = manager.get_strategy(ComponentType::Redis).await;
        assert!(redis_strategy.is_some());
        assert_eq!(redis_strategy.unwrap().strategy, FallbackStrategy::Degraded);

        let postgres_strategy = manager.get_strategy(ComponentType::Postgres).await;
        assert!(postgres_strategy.is_some());
        assert_eq!(
            postgres_strategy.unwrap().strategy,
            FallbackStrategy::Degraded
        );
    }

    #[tokio::test]
    async fn test_fallback_manager_set_strategy() {
        let l2_cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));
        let manager = FallbackManager::new(l2_cache);

        let config = FallbackConfig::new(ComponentType::Redis, FallbackStrategy::FailOpen);
        manager.set_strategy(ComponentType::Redis, config).await;

        let strategy = manager.get_strategy(ComponentType::Redis).await;
        assert!(strategy.is_some());
        assert_eq!(strategy.unwrap().strategy, FallbackStrategy::FailOpen);
    }

    #[tokio::test]
    async fn test_fallback_manager_execute_success() {
        let l2_cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));
        let manager = FallbackManager::new(l2_cache);

        let result = manager
            .execute_with_fallback(
                ComponentType::Redis,
                || async { Ok::<String, FlowGuardError>("primary".to_string()) },
                || async { Ok::<String, FlowGuardError>("fallback".to_string()) },
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "primary");
        assert!(!manager.is_failed(ComponentType::Redis).await);
    }

    #[tokio::test]
    async fn test_fallback_manager_execute_fail_degraded() {
        let l2_cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));
        let manager = FallbackManager::new(l2_cache);

        let config = FallbackConfig::new(ComponentType::Redis, FallbackStrategy::Degraded);
        manager.set_strategy(ComponentType::Redis, config).await;

        let result = manager
            .execute_with_fallback(
                ComponentType::Redis,
                || async {
                    Err::<String, FlowGuardError>(FlowGuardError::StorageError(
                        StorageError::ConnectionError("connection failed".to_string()),
                    ))
                },
                || async { Ok::<String, FlowGuardError>("fallback".to_string()) },
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "fallback");
        assert!(manager.is_failed(ComponentType::Redis).await);
    }

    #[tokio::test]
    async fn test_fallback_manager_execute_fail_fail_open() {
        let l2_cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));
        let manager = FallbackManager::new(l2_cache);

        let config = FallbackConfig::new(ComponentType::Redis, FallbackStrategy::FailOpen);
        manager.set_strategy(ComponentType::Redis, config).await;

        let result = manager
            .execute_with_fallback(
                ComponentType::Redis,
                || async {
                    Err::<String, FlowGuardError>(FlowGuardError::StorageError(
                        StorageError::ConnectionError("connection failed".to_string()),
                    ))
                },
                || async { Ok::<String, FlowGuardError>("fallback".to_string()) },
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("服务降级，但允许请求通过"));
    }

    #[tokio::test]
    async fn test_fallback_manager_execute_fail_fail_closed() {
        let l2_cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));
        let manager = FallbackManager::new(l2_cache);

        let config = FallbackConfig::new(ComponentType::Redis, FallbackStrategy::FailClosed);
        manager.set_strategy(ComponentType::Redis, config).await;

        let result = manager
            .execute_with_fallback(
                ComponentType::Redis,
                || async {
                    Err::<String, FlowGuardError>(FlowGuardError::StorageError(
                        StorageError::ConnectionError("connection failed".to_string()),
                    ))
                },
                || async { Ok::<String, FlowGuardError>("fallback".to_string()) },
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("服务降级，拒绝请求"));
    }

    #[tokio::test]
    async fn test_fallback_manager_inject_failure() {
        let l2_cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));
        let manager = FallbackManager::new(l2_cache);

        assert!(!manager.is_failed(ComponentType::Redis).await);

        manager.inject_failure(ComponentType::Redis).await;
        assert!(manager.is_failed(ComponentType::Redis).await);

        manager.recover_failure(ComponentType::Redis).await;
        assert!(!manager.is_failed(ComponentType::Redis).await);
    }

    #[tokio::test]
    async fn test_fallback_manager_get_all_failures() {
        let l2_cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));
        let manager = FallbackManager::new(l2_cache);

        manager.inject_failure(ComponentType::Redis).await;
        manager.inject_failure(ComponentType::Postgres).await;

        let failures = manager.get_all_failures().await;
        assert_eq!(failures.len(), 2);
        assert!(failures.contains(&ComponentType::Redis));
        assert!(failures.contains(&ComponentType::Postgres));
    }

    #[tokio::test]
    async fn test_fallback_manager_recovery() {
        let l2_cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));
        let manager = FallbackManager::new(l2_cache);

        // 第一次失败
        let result = manager
            .execute_with_fallback(
                ComponentType::Redis,
                || async {
                    Err::<String, FlowGuardError>(FlowGuardError::StorageError(
                        StorageError::ConnectionError("connection failed".to_string()),
                    ))
                },
                || async { Ok::<String, FlowGuardError>("fallback".to_string()) },
            )
            .await;

        assert!(result.is_ok());
        assert!(manager.is_failed(ComponentType::Redis).await);

        // 第二次成功，应该清除故障状态
        let result = manager
            .execute_with_fallback(
                ComponentType::Redis,
                || async { Ok::<String, FlowGuardError>("recovered".to_string()) },
                || async { Ok::<String, FlowGuardError>("fallback".to_string()) },
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "recovered");
        assert!(!manager.is_failed(ComponentType::Redis).await);
    }

    #[tokio::test]
    async fn test_fallback_manager_l2_cache() {
        let l2_cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));
        let manager = FallbackManager::new(l2_cache);

        let cache = manager.l2_cache();
        assert_eq!(cache.len().await, 0);
    }
}
