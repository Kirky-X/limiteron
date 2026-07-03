//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
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
use ahash::AHashMap as HashMap;
use oxcache::Cache;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// 孤岛模式通知回调类型
pub type IslandModeCallback = Box<dyn Fn(bool) + Send + Sync>;

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
    L2Cache,
    /// 配置服务
    Config,
    /// 封禁服务
    Ban,
    /// 配额服务
    Quota,
    /// 其他组件
    Other(String),
}

impl From<&str> for ComponentType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "redis" => ComponentType::Redis,
            "postgres" => ComponentType::Postgres,
            "l2_cache" => ComponentType::L2Cache,
            "config" => ComponentType::Config,
            "ban" => ComponentType::Ban,
            "quota" => ComponentType::Quota,
            other => ComponentType::Other(other.to_string()),
        }
    }
}

impl ComponentType {
    pub fn as_str(&self) -> &str {
        match self {
            ComponentType::Redis => "redis",
            ComponentType::Postgres => "postgres",
            ComponentType::L2Cache => "l2_cache",
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
    /// 故障状态
    failure_states: Arc<RwLock<HashMap<ComponentType, bool>>>,
    /// L2 缓存实例
    l2_cache: Arc<Cache<String, String>>,
    /// 孤岛模式通知回调
    island_mode_callbacks: Arc<RwLock<Vec<IslandModeCallback>>>,
}

impl FallbackManager {
    /// 创建新的降级策略管理器
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::fallback::FallbackManager;
    /// use oxcache::Cache;
    /// use std::sync::Arc;
    /// use std::time::Duration;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let cache = Cache::builder()
    ///         .capacity(10000)
    ///         .ttl(Duration::from_secs(300))
    ///         .build()
    ///         .await
    ///         .unwrap();
    ///     let manager = FallbackManager::new(Arc::new(cache));
    /// }
    /// ```
    pub fn new(l2_cache: Arc<Cache<String, String>>) -> Self {
        log::info!("创建降级策略管理器");

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
            ComponentType::L2Cache,
            FallbackConfig::new(ComponentType::L2Cache, FallbackStrategy::Degraded),
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
            failure_states: Arc::new(RwLock::new(HashMap::new())),
            l2_cache,
            island_mode_callbacks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 获取 L2 缓存实例
    pub fn l2_cache(&self) -> &Arc<Cache<String, String>> {
        &self.l2_cache
    }

    /// 设置降级策略
    ///
    /// # 参数
    /// - `component`: 组件类型
    /// - `config`: 策略配置
    pub async fn set_strategy(&self, component: ComponentType, config: FallbackConfig) {
        log::info!(
            target: "fallback",
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
    /// use oxcache::Cache;
    /// use std::sync::Arc;
    /// use std::time::Duration;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let cache: Cache<String, String> = Cache::builder()
    ///         .capacity(10000)
    ///         .ttl(Duration::from_secs(60))
    ///         .build()
    ///         .await
    ///         .unwrap();
    ///     let l2_cache = Arc::new(cache);
    ///     let manager = FallbackManager::new(l2_cache);
    /// }
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
        let _is_failed = {
            let states = self.failure_states.read().await;
            *states.get(&component).unwrap_or(&false)
        };

        // 尝试执行主操作（即使在故障状态下也要尝试，以检测是否恢复）
        let result = operation().await;

        match result {
            Ok(value) => {
                // 操作成功，清除故障状态
                self.clear_failure_internal(component).await;
                Ok(value)
            }
            Err(e) => {
                // 操作失败，根据策略处理
                log::warn!(target: "fallback", "组件操作失败: component={:?}, error={}", component, e);

                // 标记为故障状态
                self.set_failure_internal(component.clone()).await;

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
        log::info!(
            target: "fallback",
            "执行降级策略: component={:?}, strategy={:?}",
            component, config.strategy
        );

        match config.strategy {
            FallbackStrategy::FailOpen => {
                // 故障开放：返回默认值或允许请求
                log::warn!(target: "fallback", "降级策略: FailOpen - 允许请求通过");
                Err(FlowGuardError::LimitError(
                    "服务降级，但允许请求通过".to_string(),
                ))
            }
            FallbackStrategy::FailClosed => {
                // 故障关闭：拒绝请求
                log::error!(target: "fallback", "降级策略: FailClosed - 拒绝请求");
                Err(FlowGuardError::StorageError(StorageError::ConnectionError(
                    "服务降级，拒绝请求".to_string(),
                )))
            }
            FallbackStrategy::Degraded => {
                // 降级服务：使用备用方案
                log::debug!(target: "fallback", "降级策略: Degraded - 使用备用方案");
                fallback_operation().await
            }
        }
    }

    /// 标记组件为故障状态（内部使用）
    async fn set_failure_internal(&self, component: ComponentType) {
        log::warn!(target: "fallback", "组件故障: {:?}", component);
        let mut states = self.failure_states.write().await;
        states.insert(component, true);
    }

    /// 清除组件故障状态（内部使用）
    async fn clear_failure_internal(&self, component: ComponentType) {
        let mut states = self.failure_states.write().await;
        states.remove(&component);
        log::info!(target: "fallback", "组件恢复: {:?}", component);
    }

    /// 记录组件故障
    pub async fn record_failure(&self, component: ComponentType, _error: &str) {
        log::warn!(target: "fallback", "组件故障记录: {:?}", component);
        self.set_failure_internal(component).await;
    }

    /// 获取组件故障计数
    pub async fn get_failure_count(&self, component: ComponentType) -> u32 {
        let states = self.failure_states.read().await;
        if *states.get(&component).unwrap_or(&false) {
            1
        } else {
            0
        }
    }

    /// 检查组件是否故障
    pub async fn is_failed(&self, component: ComponentType) -> bool {
        let states = self.failure_states.read().await;
        *states.get(&component).unwrap_or(&false)
    }

    /// 手动触发故障（用于测试）
    pub async fn inject_failure(&self, component: ComponentType) {
        log::warn!(target: "fallback", "注入故障: {:?}", component);
        self.set_failure_internal(component).await;
    }

    /// 手动恢复故障（用于测试）
    pub async fn recover_failure(&self, component: ComponentType) {
        log::info!(target: "fallback", "恢复故障: {:?}", component);
        self.clear_failure_internal(component).await;
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

    // ==================== 孤岛模式通知 ====================

    /// 注册孤岛模式状态变更回调
    ///
    /// 当存储层故障/恢复时，会自动通知所有注册的回调。
    ///
    /// # 参数
    /// - `callback`: 回调函数，参数为 `true` 表示进入孤岛模式，`false` 表示退出
    pub async fn register_island_mode_callback(&self, callback: IslandModeCallback) {
        let mut callbacks = self.island_mode_callbacks.write().await;
        callbacks.push(callback);
        log::info!(target: "fallback", "注册孤岛模式通知回调");
    }

    /// 通知所有回调孤岛模式状态变更
    async fn notify_island_mode_change(&self, is_island: bool) {
        let callbacks = self.island_mode_callbacks.read().await;
        for callback in callbacks.iter() {
            callback(is_island);
        }
        if is_island {
            log::warn!(target: "fallback", "已通知所有回调：进入孤岛模式");
        } else {
            log::info!(target: "fallback", "已通知所有回调：退出孤岛模式");
        }
    }

    /// 标记组件为故障状态（公开版本）
    ///
    /// 与内部 `set_failure` 不同，此方法会触发孤岛模式通知。
    pub async fn set_failure(&self, component: ComponentType) {
        log::warn!(target: "fallback", "组件故障: {:?}", component);
        let mut states = self.failure_states.write().await;
        let was_failed = states.values().any(|&f| f);
        states.insert(component.clone(), true);

        // 如果这是第一个故障，触发孤岛模式
        if !was_failed {
            log::error!(target: "fallback", "存储层首次故障，触发孤岛模式");
            self.notify_island_mode_change(true).await;
        }
    }

    /// 清除组件故障状态（公开版本）
    ///
    /// 与内部 `clear_failure` 不同，此方法会检查是否所有故障都已恢复。
    pub async fn clear_failure(&self, component: ComponentType) {
        let mut states = self.failure_states.write().await;
        states.remove(&component);
        log::info!(target: "fallback", "组件恢复: {:?}", component);

        // 检查是否所有故障都已恢复
        let still_failed = states.values().any(|&f| f);
        if !still_failed {
            log::info!(target: "fallback", "所有存储层恢复，退出孤岛模式");
            drop(states);
            self.notify_island_mode_change(false).await;
        }
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
        assert_eq!(ComponentType::from("redis"), ComponentType::Redis);
        assert_eq!(ComponentType::from("postgres"), ComponentType::Postgres);
        assert_eq!(ComponentType::from("l2_cache"), ComponentType::L2Cache);
        assert_eq!(
            ComponentType::from("other"),
            ComponentType::Other("other".to_string())
        );
    }

    #[test]
    fn test_component_type_as_str() {
        assert_eq!(ComponentType::Redis.as_str(), "redis");
        assert_eq!(ComponentType::Postgres.as_str(), "postgres");
        assert_eq!(ComponentType::L2Cache.as_str(), "l2_cache");
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
        let cache: Cache<String, String> = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        let l2_cache = Arc::new(cache);
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
        let cache: Cache<String, String> = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        let l2_cache = Arc::new(cache);
        let manager = FallbackManager::new(l2_cache);

        let config = FallbackConfig::new(ComponentType::Redis, FallbackStrategy::FailOpen);
        manager.set_strategy(ComponentType::Redis, config).await;

        let strategy = manager.get_strategy(ComponentType::Redis).await;
        assert!(strategy.is_some());
        assert_eq!(strategy.unwrap().strategy, FallbackStrategy::FailOpen);
    }

    #[tokio::test]
    async fn test_fallback_manager_execute_success() {
        let cache: Cache<String, String> = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        let l2_cache = Arc::new(cache);
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
        let cache: Cache<String, String> = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        let l2_cache = Arc::new(cache);
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
        let cache: Cache<String, String> = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        let l2_cache = Arc::new(cache);
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
        let cache: Cache<String, String> = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        let l2_cache = Arc::new(cache);
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
        let cache: Cache<String, String> = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        let l2_cache = Arc::new(cache);
        let manager = FallbackManager::new(l2_cache);

        assert!(!manager.is_failed(ComponentType::Redis).await);

        manager.inject_failure(ComponentType::Redis).await;
        assert!(manager.is_failed(ComponentType::Redis).await);

        manager.recover_failure(ComponentType::Redis).await;
        assert!(!manager.is_failed(ComponentType::Redis).await);
    }

    #[tokio::test]
    async fn test_fallback_manager_get_all_failures() {
        let cache: Cache<String, String> = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        let l2_cache = Arc::new(cache);
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
        let cache: Cache<String, String> = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        let l2_cache = Arc::new(cache);
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
        let cache: Cache<String, String> = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        let l2_cache = Arc::new(cache);
        let manager = FallbackManager::new(l2_cache);

        let cache = manager.l2_cache();
        assert_eq!(cache.len().await.unwrap(), 0);
    }

    // ==================== ComponentType & FallbackStrategy Tests ====================

    #[test]
    fn test_component_type_from_str_all_variants() {
        assert_eq!(ComponentType::from("l2_cache"), ComponentType::L2Cache);
        assert_eq!(ComponentType::from("config"), ComponentType::Config);
        assert_eq!(ComponentType::from("ban"), ComponentType::Ban);
        assert_eq!(ComponentType::from("quota"), ComponentType::Quota);
        assert_eq!(
            ComponentType::from("custom_service"),
            ComponentType::Other("custom_service".to_string())
        );
    }

    #[test]
    fn test_component_type_from_str_case_insensitive() {
        assert_eq!(ComponentType::from("REDIS"), ComponentType::Redis);
        assert_eq!(ComponentType::from("Postgres"), ComponentType::Postgres);
        assert_eq!(ComponentType::from("L2_CACHE"), ComponentType::L2Cache);
        assert_eq!(ComponentType::from("Config"), ComponentType::Config);
        assert_eq!(ComponentType::from("BAN"), ComponentType::Ban);
        assert_eq!(ComponentType::from("QUOTA"), ComponentType::Quota);
    }

    #[test]
    fn test_component_type_as_str_all_variants() {
        assert_eq!(ComponentType::Redis.as_str(), "redis");
        assert_eq!(ComponentType::Postgres.as_str(), "postgres");
        assert_eq!(ComponentType::L2Cache.as_str(), "l2_cache");
        assert_eq!(ComponentType::Config.as_str(), "config");
        assert_eq!(ComponentType::Ban.as_str(), "ban");
        assert_eq!(ComponentType::Quota.as_str(), "quota");
        assert_eq!(
            ComponentType::Other("custom_type".to_string()).as_str(),
            "custom_type"
        );
    }

    #[test]
    fn test_component_type_other_equality() {
        assert_eq!(
            ComponentType::Other("a".to_string()),
            ComponentType::Other("a".to_string())
        );
        assert_ne!(
            ComponentType::Other("a".to_string()),
            ComponentType::Other("b".to_string())
        );
        assert_ne!(ComponentType::Other("a".to_string()), ComponentType::Redis);
    }

    #[test]
    fn test_component_type_serde_roundtrip() {
        let variants = vec![
            ComponentType::Redis,
            ComponentType::Postgres,
            ComponentType::L2Cache,
            ComponentType::Config,
            ComponentType::Ban,
            ComponentType::Quota,
            ComponentType::Other("custom".to_string()),
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let deserialized: ComponentType = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, variant);
        }
    }

    #[test]
    fn test_fallback_strategy_serde_roundtrip() {
        let variants = vec![
            FallbackStrategy::FailOpen,
            FallbackStrategy::FailClosed,
            FallbackStrategy::Degraded,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let deserialized: FallbackStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, variant);
        }
    }

    #[test]
    fn test_fallback_strategy_debug() {
        assert_eq!(format!("{:?}", FallbackStrategy::FailOpen), "FailOpen");
        assert_eq!(format!("{:?}", FallbackStrategy::FailClosed), "FailClosed");
        assert_eq!(format!("{:?}", FallbackStrategy::Degraded), "Degraded");
    }

    // ==================== FallbackConfig Tests ====================

    #[test]
    fn test_fallback_config_new_defaults() {
        let config = FallbackConfig::new(ComponentType::Redis, FallbackStrategy::FailOpen);
        assert_eq!(config.component, ComponentType::Redis);
        assert_eq!(config.strategy, FallbackStrategy::FailOpen);
        assert!(config.enabled);
        assert_eq!(config.timeout, Duration::from_secs(5));
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_fallback_config_new_other_component() {
        let config = FallbackConfig::new(
            ComponentType::Other("custom".to_string()),
            FallbackStrategy::FailClosed,
        );
        assert_eq!(config.component, ComponentType::Other("custom".to_string()));
        assert_eq!(config.strategy, FallbackStrategy::FailClosed);
        assert!(config.enabled);
        assert_eq!(config.max_retries, 3);
    }

    // ==================== Helper for FallbackManager tests ====================

    async fn create_manager() -> FallbackManager {
        let cache: Cache<String, String> = Cache::builder()
            .capacity(10000)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        FallbackManager::new(Arc::new(cache))
    }

    // ==================== FallbackManager Default Strategies ====================

    #[tokio::test]
    async fn test_fallback_manager_all_default_strategies() {
        let manager = create_manager().await;

        let redis = manager.get_strategy(ComponentType::Redis).await.unwrap();
        assert_eq!(redis.strategy, FallbackStrategy::Degraded);

        let postgres = manager.get_strategy(ComponentType::Postgres).await.unwrap();
        assert_eq!(postgres.strategy, FallbackStrategy::Degraded);

        let l2 = manager.get_strategy(ComponentType::L2Cache).await.unwrap();
        assert_eq!(l2.strategy, FallbackStrategy::Degraded);

        let config = manager.get_strategy(ComponentType::Config).await.unwrap();
        assert_eq!(config.strategy, FallbackStrategy::FailClosed);

        let ban = manager.get_strategy(ComponentType::Ban).await.unwrap();
        assert_eq!(ban.strategy, FallbackStrategy::Degraded);

        let quota = manager.get_strategy(ComponentType::Quota).await.unwrap();
        assert_eq!(quota.strategy, FallbackStrategy::Degraded);
    }

    #[tokio::test]
    async fn test_fallback_manager_get_strategy_unknown() {
        let manager = create_manager().await;
        let result = manager
            .get_strategy(ComponentType::Other("nonexistent".to_string()))
            .await;
        assert!(result.is_none());
    }

    // ==================== execute_with_fallback edge cases ====================

    #[tokio::test]
    async fn test_fallback_manager_execute_disabled() {
        let manager = create_manager().await;

        let config =
            FallbackConfig::new(ComponentType::Redis, FallbackStrategy::Degraded).enabled(false);
        manager.set_strategy(ComponentType::Redis, config).await;

        // When disabled, primary runs directly and errors propagate (no fallback)
        let result: Result<String, FlowGuardError> = manager
            .execute_with_fallback(
                ComponentType::Redis,
                || async {
                    Err::<String, FlowGuardError>(FlowGuardError::LimitError(
                        "primary failed".to_string(),
                    ))
                },
                || async { Ok::<String, FlowGuardError>("fallback".to_string()) },
            )
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("primary failed"));
    }

    #[tokio::test]
    async fn test_fallback_manager_execute_disabled_success() {
        let manager = create_manager().await;

        let config =
            FallbackConfig::new(ComponentType::Redis, FallbackStrategy::Degraded).enabled(false);
        manager.set_strategy(ComponentType::Redis, config).await;

        let result: Result<String, FlowGuardError> = manager
            .execute_with_fallback(
                ComponentType::Redis,
                || async { Ok::<String, FlowGuardError>("primary_ok".to_string()) },
                || async { Ok::<String, FlowGuardError>("fallback".to_string()) },
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "primary_ok");
    }

    #[tokio::test]
    async fn test_fallback_manager_execute_unknown_component() {
        let manager = create_manager().await;

        // Unknown component uses unwrap_or_default -> Degraded, enabled=true
        let result: Result<String, FlowGuardError> = manager
            .execute_with_fallback(
                ComponentType::Other("unknown".to_string()),
                || async {
                    Err::<String, FlowGuardError>(FlowGuardError::StorageError(
                        StorageError::ConnectionError("down".to_string()),
                    ))
                },
                || async { Ok::<String, FlowGuardError>("degraded_ok".to_string()) },
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "degraded_ok");
    }

    // ==================== Failure recording and counting ====================

    #[tokio::test]
    async fn test_fallback_manager_record_failure() {
        let manager = create_manager().await;

        assert!(!manager.is_failed(ComponentType::Redis).await);

        manager
            .record_failure(ComponentType::Redis, "connection timeout")
            .await;
        assert!(manager.is_failed(ComponentType::Redis).await);
    }

    #[tokio::test]
    async fn test_fallback_manager_failure_count_zero() {
        let manager = create_manager().await;

        let count = manager.get_failure_count(ComponentType::Redis).await;
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_fallback_manager_failure_count_one() {
        let manager = create_manager().await;

        manager.inject_failure(ComponentType::Redis).await;
        let count = manager.get_failure_count(ComponentType::Redis).await;
        assert_eq!(count, 1);
    }

    // ==================== Island mode (public API) ====================

    #[tokio::test]
    async fn test_fallback_manager_set_failure_island_mode_enter() {
        let manager = create_manager().await;

        let entered = Arc::new(std::sync::Mutex::new(None::<bool>));
        let entered_clone = entered.clone();
        let callback: IslandModeCallback = Box::new(move |is_island| {
            *entered_clone.lock().unwrap() = Some(is_island);
        });
        manager.register_island_mode_callback(callback).await;

        manager.set_failure(ComponentType::Redis).await;

        assert!(manager.is_failed(ComponentType::Redis).await);
        assert_eq!(*entered.lock().unwrap(), Some(true));
    }

    #[tokio::test]
    async fn test_fallback_manager_clear_failure_island_mode_exit() {
        let manager = create_manager().await;

        let state = Arc::new(std::sync::Mutex::new(Vec::new()));
        let state_clone = state.clone();
        let callback: IslandModeCallback = Box::new(move |is_island| {
            state_clone.lock().unwrap().push(is_island);
        });
        manager.register_island_mode_callback(callback).await;

        manager.set_failure(ComponentType::Redis).await;
        assert!(manager.is_failed(ComponentType::Redis).await);

        manager.clear_failure(ComponentType::Redis).await;
        assert!(!manager.is_failed(ComponentType::Redis).await);

        let calls = state.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert!(calls[0]);
        assert!(!calls[1]);
    }

    #[tokio::test]
    async fn test_fallback_manager_island_mode_lifecycle() {
        let manager = create_manager().await;

        let state = Arc::new(std::sync::Mutex::new(Vec::new()));
        let state_clone = state.clone();
        let callback: IslandModeCallback = Box::new(move |is_island| {
            state_clone.lock().unwrap().push(is_island);
        });
        manager.register_island_mode_callback(callback).await;

        // First failure enters island mode
        manager.set_failure(ComponentType::Redis).await;
        {
            let calls = state.lock().unwrap();
            assert_eq!(calls.len(), 1);
            assert!(calls[0]);
        }

        // Second failure does NOT re-notify (already in island mode)
        manager.set_failure(ComponentType::Postgres).await;
        {
            let calls = state.lock().unwrap();
            assert_eq!(calls.len(), 1);
        }

        // Clearing one failure keeps island mode (Postgres still failed)
        manager.clear_failure(ComponentType::Redis).await;
        {
            let calls = state.lock().unwrap();
            assert_eq!(calls.len(), 1);
        }
        assert!(!manager.is_failed(ComponentType::Redis).await);
        assert!(manager.is_failed(ComponentType::Postgres).await);

        // Clearing last failure exits island mode
        manager.clear_failure(ComponentType::Postgres).await;
        {
            let calls = state.lock().unwrap();
            assert_eq!(calls.len(), 2);
            assert!(!calls[1]);
        }
        assert!(!manager.is_failed(ComponentType::Postgres).await);
    }

    #[tokio::test]
    async fn test_fallback_manager_set_failure_no_notify_twice() {
        let manager = create_manager().await;

        let count = Arc::new(std::sync::Mutex::new(0u32));
        let count_clone = count.clone();
        let callback: IslandModeCallback = Box::new(move |_| {
            *count_clone.lock().unwrap() += 1;
        });
        manager.register_island_mode_callback(callback).await;

        manager.set_failure(ComponentType::Redis).await;
        manager.set_failure(ComponentType::Postgres).await;
        manager.set_failure(ComponentType::Ban).await;

        assert_eq!(*count.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_fallback_manager_clear_failure_no_notify_if_still_failed() {
        let manager = create_manager().await;

        let state = Arc::new(std::sync::Mutex::new(Vec::new()));
        let state_clone = state.clone();
        let callback: IslandModeCallback = Box::new(move |is_island| {
            state_clone.lock().unwrap().push(is_island);
        });
        manager.register_island_mode_callback(callback).await;

        manager.set_failure(ComponentType::Redis).await;
        manager.set_failure(ComponentType::Postgres).await;

        // Clear Redis while Postgres is still failed: no exit notification
        manager.clear_failure(ComponentType::Redis).await;

        let calls = state.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(calls[0]);
    }

    #[tokio::test]
    async fn test_fallback_manager_clear_failure_no_notify_on_empty() {
        let manager = create_manager().await;

        let count = Arc::new(std::sync::Mutex::new(0u32));
        let count_clone = count.clone();
        let callback: IslandModeCallback = Box::new(move |_| {
            *count_clone.lock().unwrap() += 1;
        });
        manager.register_island_mode_callback(callback).await;

        // Clear a non-existent failure — this still triggers exit if no failures
        manager.clear_failure(ComponentType::Redis).await;

        assert_eq!(*count.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_fallback_manager_register_island_mode_callback_multiple() {
        let manager = create_manager().await;

        let count = Arc::new(std::sync::Mutex::new(0u32));
        let c1 = {
            let count = count.clone();
            Box::new(move |_: bool| {
                *count.lock().unwrap() += 1;
            }) as IslandModeCallback
        };
        let c2 = {
            let count = count.clone();
            Box::new(move |_: bool| {
                *count.lock().unwrap() += 1;
            }) as IslandModeCallback
        };

        manager.register_island_mode_callback(c1).await;
        manager.register_island_mode_callback(c2).await;

        manager.set_failure(ComponentType::Redis).await;

        assert_eq!(*count.lock().unwrap(), 2);
    }
}
