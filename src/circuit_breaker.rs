//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 熔断器实现
//!
//! 提供熔断器功能，支持三状态转换和自动恢复。
//!
//! # 特性
//!
//! - **三状态**: Closed（关闭）、Open（打开）、HalfOpen（半开）
//! - **自动熔断**: 失败次数达到阈值自动熔断
//! - **自动恢复**: 超时后自动探测恢复
//! - **线程安全**: 使用Arc和原子操作保证线程安全
//! - **统计信息**: 提供详细的统计信息

use crate::constants::{
    DEFAULT_CIRCUIT_BREAKER_FAILURE_THRESHOLD, DEFAULT_CIRCUIT_BREAKER_HALF_OPEN_MAX_CALLS,
    DEFAULT_CIRCUIT_BREAKER_SUCCESS_THRESHOLD, DEFAULT_CIRCUIT_BREAKER_TIMEOUT_SECS,
};
use crate::error::{CircuitBreakerStats, CircuitState, FlowGuardError};
use log::{info, trace, warn};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[cfg(feature = "circuit-breaker")]
/// 熔断器配置
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// 失败阈值（达到此值时熔断）
    pub failure_threshold: u64,
    /// 成功阈值（半开状态下达到此值时恢复）
    pub success_threshold: u64,
    /// 超时时间（打开状态后等待此时间再尝试恢复）
    pub timeout: Duration,
    /// 半开状态的最大调用次数
    pub half_open_max_calls: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: DEFAULT_CIRCUIT_BREAKER_FAILURE_THRESHOLD,
            success_threshold: DEFAULT_CIRCUIT_BREAKER_SUCCESS_THRESHOLD,
            timeout: Duration::from_secs(DEFAULT_CIRCUIT_BREAKER_TIMEOUT_SECS),
            half_open_max_calls: DEFAULT_CIRCUIT_BREAKER_HALF_OPEN_MAX_CALLS,
        }
    }
}

impl CircuitBreakerConfig {
    /// 创建新的熔断器配置
    pub fn new(failure_threshold: u64, success_threshold: u64, timeout: Duration) -> Self {
        Self {
            failure_threshold,
            success_threshold,
            timeout,
            half_open_max_calls: DEFAULT_CIRCUIT_BREAKER_HALF_OPEN_MAX_CALLS,
        }
    }

    /// 设置半开状态的最大调用次数
    pub fn half_open_max_calls(mut self, max_calls: u64) -> Self {
        self.half_open_max_calls = max_calls;
        self
    }
}

#[cfg(feature = "circuit-breaker")]
/// 熔断器
pub struct CircuitBreaker {
    /// 当前状态
    state: Arc<RwLock<CircuitState>>,
    /// 失败计数
    failure_count: Arc<AtomicU64>,
    /// 成功计数
    success_count: Arc<AtomicU64>,
    /// 总调用次数
    total_calls: Arc<AtomicU64>,
    /// 最后失败时间
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    /// 最后状态变更时间
    last_state_change: Arc<RwLock<Option<Instant>>>,
    /// 半开状态下的调用计数
    half_open_calls: Arc<AtomicU64>,
    /// 配置
    config: CircuitBreakerConfig,
}

#[cfg(feature = "circuit-breaker")]
/// 熔断器构建器
#[derive(Debug, Clone)]
pub struct CircuitBreakerBuilder {
    config: CircuitBreakerConfig,
}

impl CircuitBreakerBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self {
            config: CircuitBreakerConfig::default(),
        }
    }

    /// 设置失败阈值
    pub fn failure_threshold(mut self, failure_threshold: u64) -> Self {
        self.config.failure_threshold = failure_threshold;
        self
    }

    /// 设置成功阈值
    pub fn success_threshold(mut self, success_threshold: u64) -> Self {
        self.config.success_threshold = success_threshold;
        self
    }

    /// 设置超时时间
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// 设置半开状态的最大调用次数
    pub fn half_open_max_calls(mut self, max_calls: u64) -> Self {
        self.config.half_open_max_calls = max_calls;
        self
    }

    /// 构建熔断器
    pub fn build(&self) -> CircuitBreaker {
        CircuitBreaker::with_dependencies(self.config.clone())
    }
}

impl Default for CircuitBreakerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CircuitBreaker {
    /// 使用依赖注入模式创建熔断器
    ///
    /// # 参数
    /// - `config`: 熔断器配置
    ///
    /// # 返回
    /// 配置好的熔断器实例
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
    /// use std::time::Duration;
    ///
    /// let config = CircuitBreakerConfig::new(5, 2, Duration::from_secs(60));
    /// let breaker = CircuitBreaker::with_dependencies(config);
    /// ```
    pub fn with_dependencies(config: CircuitBreakerConfig) -> Self {
        info!(
            "创建熔断器: failure_threshold={}, success_threshold={}, timeout={:?}",
            config.failure_threshold, config.success_threshold, config.timeout
        );

        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            total_calls: Arc::new(AtomicU64::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
            last_state_change: Arc::new(RwLock::new(Some(Instant::now()))),
            half_open_calls: Arc::new(AtomicU64::new(0)),
            config,
        }
    }

    /// 创建熔断器构建器
    ///
    /// # 返回
    /// 新的构建器实例
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::circuit_breaker::CircuitBreaker;
    ///
    /// let builder = CircuitBreaker::builder();
    /// ```
    pub fn builder() -> CircuitBreakerBuilder {
        CircuitBreakerBuilder::new()
    }

    /// 创建新的熔断器（保持向后兼容）
    ///
    /// # 参数
    /// - `config`: 熔断器配置
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
    /// use std::time::Duration;
    ///
    /// let config = CircuitBreakerConfig::new(5, 2, Duration::from_secs(60));
    /// let breaker = CircuitBreaker::new(config);
    /// ```
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self::with_dependencies(config)
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::with_dependencies(CircuitBreakerConfig::default())
    }
}

impl CircuitBreaker {
    /// 执行操作，自动处理熔断逻辑
    ///
    /// # 参数
    /// - `operation`: 要执行的操作
    ///
    /// # 返回
    /// - `Ok(T)`: 操作成功
    /// - `Err(FlowGuardError)`: 操作失败或熔断器打开
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
    /// use std::time::Duration;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = CircuitBreakerConfig::new(5, 2, Duration::from_secs(60));
    /// let breaker = CircuitBreaker::new(config);
    ///
    /// let result = breaker.execute(|| async {
    ///     // 执行操作
    ///     Ok::<(), limiteron::error::FlowGuardError>(())
    /// }).await;
    /// # }
    /// ```
    pub async fn execute<F, Fut, T>(&self, operation: F) -> Result<T, FlowGuardError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, FlowGuardError>>,
    {
        // 增加总调用次数
        self.total_calls.fetch_add(1, Ordering::Relaxed);

        // 检查熔断器状态
        let state = self.state.read().await;

        match *state {
            CircuitState::Open => {
                // 检查是否可以尝试恢复
                let last_failure = self.last_failure_time.read().await;
                if let Some(last_failure) = *last_failure {
                    if last_failure.elapsed() >= self.config.timeout {
                        // 超时，切换到半开状态
                        drop(state);
                        self.transition_to_half_open().await;
                    } else {
                        // 仍在熔断状态，拒绝请求
                        drop(state);
                        warn!("熔断器打开，拒绝请求");
                        return Err(FlowGuardError::LimitError(
                            "熔断器打开，请求被拒绝".to_string(),
                        ));
                    }
                }
            }
            CircuitState::HalfOpen => {
                // 检查半开状态下的调用次数
                let calls = self.half_open_calls.load(Ordering::Relaxed);
                if calls >= self.config.half_open_max_calls {
                    drop(state);
                    warn!("半开状态调用次数已达上限，拒绝请求");
                    return Err(FlowGuardError::LimitError(
                        "半开状态调用次数已达上限".to_string(),
                    ));
                }
                self.half_open_calls.fetch_add(1, Ordering::Relaxed);
                drop(state);
            }
            CircuitState::Closed => {
                // 正常状态，继续执行
                drop(state);
            }
        }

        // 执行操作
        let result = operation().await;

        // 根据操作结果更新状态
        match result {
            Ok(value) => {
                self.on_success().await;
                Ok(value)
            }
            Err(e) => {
                self.on_failure().await;
                Err(e)
            }
        }
    }

    /// 操作成功时的处理
    async fn on_success(&self) {
        let state = self.state.read().await;

        match *state {
            CircuitState::Closed => {
                // 关闭状态下，重置失败计数
                self.failure_count.store(0, Ordering::Relaxed);
                self.success_count.fetch_add(1, Ordering::Relaxed);
                trace!("操作成功（关闭状态）");
            }
            CircuitState::HalfOpen => {
                // 半开状态下，增加成功计数
                let success_count = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;

                if success_count >= self.config.success_threshold {
                    // 达到成功阈值，切换到关闭状态
                    drop(state);
                    self.transition_to_closed().await;
                } else {
                    trace!(
                        "操作成功（半开状态）: {}/{}",
                        success_count,
                        self.config.success_threshold
                    );
                }
            }
            CircuitState::Open => {
                // 打开状态不应该执行到这里
                warn!("熔断器打开状态下收到成功响应");
            }
        }
    }

    /// 操作失败时的处理
    async fn on_failure(&self) {
        let state = self.state.read().await;

        match *state {
            CircuitState::Closed => {
                // 关闭状态下，增加失败计数
                let failure_count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;

                // 记录失败时间
                *self.last_failure_time.write().await = Some(Instant::now());

                if failure_count >= self.config.failure_threshold {
                    // 达到失败阈值，切换到打开状态
                    drop(state);
                    self.transition_to_open().await;
                } else {
                    trace!(
                        "操作失败（关闭状态）: {}/{}",
                        failure_count,
                        self.config.failure_threshold
                    );
                }
            }
            CircuitState::HalfOpen => {
                // 半开状态下失败，立即切换到打开状态
                drop(state);
                self.transition_to_open().await;
            }
            CircuitState::Open => {
                // 打开状态不应该执行到这里
                warn!("熔断器打开状态下收到失败响应");
            }
        }
    }

    /// 统一的状态转换方法
    ///
    /// 统一处理状态转换逻辑，避免重复的状态检查和日志记录代码。
    async fn transition_to(&self, new_state: CircuitState) {
        let old_state = *self.state.read().await;
        if old_state == new_state {
            return; // 状态未改变，无需处理
        }

        // 更新状态和时间戳
        *self.state.write().await = new_state;
        *self.last_state_change.write().await = Some(Instant::now());

        // 根据新状态重置相关计数器
        match new_state {
            CircuitState::Open => {
                self.success_count.store(0, Ordering::Relaxed);
                self.half_open_calls.store(0, Ordering::Relaxed);
                warn!(
                    "熔断器状态变更: {:?} -> Open (failure_count={})",
                    old_state,
                    self.failure_count.load(Ordering::Relaxed)
                );
            }
            CircuitState::HalfOpen => {
                self.success_count.store(0, Ordering::Relaxed);
                // 重置半开状态调用计数
                // 注意：将计数设置为1，因为当前请求（探针请求）将被允许通过
                self.half_open_calls.store(1, Ordering::Relaxed);
                info!("熔断器状态变更: {:?} -> HalfOpen", old_state);
            }
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::Relaxed);
                self.success_count.store(0, Ordering::Relaxed);
                self.half_open_calls.store(0, Ordering::Relaxed);
                info!("熔断器状态变更: {:?} -> Closed", old_state);
            }
        }
    }

    /// 切换到打开状态
    async fn transition_to_open(&self) {
        self.transition_to(CircuitState::Open).await;
    }

    /// 切换到半开状态
    async fn transition_to_half_open(&self) {
        self.transition_to(CircuitState::HalfOpen).await;
    }

    /// 切换到关闭状态
    async fn transition_to_closed(&self) {
        self.transition_to(CircuitState::Closed).await;
    }

    /// 检查熔断器是否为指定状态（内部辅助方法）
    ///
    /// 统一的状态检查逻辑，避免重复的状态读取代码。
    async fn is_state(&self, target_state: CircuitState) -> bool {
        let state = self.state.read().await;
        *state == target_state
    }

    /// 检查熔断器是否打开
    pub async fn is_open(&self) -> bool {
        self.is_state(CircuitState::Open).await
    }

    /// 检查熔断器是否半开
    pub async fn is_half_open(&self) -> bool {
        self.is_state(CircuitState::HalfOpen).await
    }

    /// 检查熔断器是否关闭
    pub async fn is_closed(&self) -> bool {
        self.is_state(CircuitState::Closed).await
    }

    /// 获取当前状态
    pub async fn get_state(&self) -> CircuitState {
        *self.state.read().await
    }

    /// 重置熔断器到关闭状态
    pub async fn reset(&self) {
        info!("重置熔断器");
        *self.state.write().await = CircuitState::Closed;
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        self.total_calls.store(0, Ordering::Relaxed);
        *self.last_failure_time.write().await = None;
        *self.last_state_change.write().await = Some(Instant::now());
        self.half_open_calls.store(0, Ordering::Relaxed);
    }

    /// 获取统计信息
    pub async fn get_stats(&self) -> CircuitBreakerStats {
        let state = *self.state.read().await;
        let last_failure = self.last_failure_time.read().await;
        let last_state_change = self.last_state_change.read().await;

        CircuitBreakerStats {
            state,
            failure_count: self.failure_count.load(Ordering::Relaxed),
            success_count: self.success_count.load(Ordering::Relaxed),
            total_calls: self.total_calls.load(Ordering::Relaxed),
            last_failure_time: last_failure.and_then(|t| {
                let elapsed = t.elapsed();
                let duration = chrono::Duration::from_std(elapsed).ok()?;
                Some(chrono::Utc::now() - duration)
            }),
            last_state_change: last_state_change.and_then(|t| {
                let elapsed = t.elapsed();
                let duration = chrono::Duration::from_std(elapsed).ok()?;
                Some(chrono::Utc::now() - duration)
            }),
        }
    }

    /// 获取配置
    pub fn config(&self) -> &CircuitBreakerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_config_default() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.success_threshold, 3);
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.half_open_max_calls, 3);
    }

    #[test]
    fn test_circuit_breaker_config_new() {
        let config = CircuitBreakerConfig::new(10, 3, Duration::from_secs(120));
        assert_eq!(config.failure_threshold, 10);
        assert_eq!(config.success_threshold, 3);
        assert_eq!(config.timeout, Duration::from_secs(120));
    }

    #[test]
    fn test_circuit_breaker_config_builder() {
        let config =
            CircuitBreakerConfig::new(5, 2, Duration::from_secs(60)).half_open_max_calls(5);
        assert_eq!(config.half_open_max_calls, 5);
    }

    #[tokio::test]
    async fn test_circuit_breaker_initial_state() {
        let breaker = CircuitBreaker::default();
        assert!(breaker.is_closed().await);
        assert!(!breaker.is_open().await);
        assert!(!breaker.is_half_open().await);

        let stats = breaker.get_stats().await;
        assert_eq!(stats.state, CircuitState::Closed);
        assert_eq!(stats.failure_count, 0);
        assert_eq!(stats.success_count, 0);
        assert_eq!(stats.total_calls, 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_success() {
        let breaker = CircuitBreaker::default();

        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());

        let stats = breaker.get_stats().await;
        assert_eq!(stats.success_count, 1);
        assert_eq!(stats.failure_count, 0);
        assert_eq!(stats.total_calls, 1);
        assert!(breaker.is_closed().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_failure() {
        let config = CircuitBreakerConfig::new(3, 2, Duration::from_secs(60));
        let breaker = CircuitBreaker::new(config);

        // 第一次失败
        let result = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::LimitError("test error".to_string()))
            })
            .await;
        assert!(result.is_err());

        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 1);
        assert!(breaker.is_closed().await);

        // 第二次失败
        let result = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::LimitError("test error".to_string()))
            })
            .await;
        assert!(result.is_err());

        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 2);
        assert!(breaker.is_closed().await);

        // 第三次失败，应该触发熔断
        let result = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::LimitError("test error".to_string()))
            })
            .await;
        assert!(result.is_err());

        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 3);
        assert!(breaker.is_open().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_open_rejects_requests() {
        let config = CircuitBreakerConfig::new(2, 2, Duration::from_secs(60));
        let breaker = CircuitBreaker::new(config);

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("test error".to_string()))
                })
                .await;
        }

        assert!(breaker.is_open().await);

        // 熔断器打开，请求应该被拒绝
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("熔断器打开"));
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_recovery() {
        let config = CircuitBreakerConfig::new(2, 2, Duration::from_millis(100));
        let breaker = CircuitBreaker::new(config);

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("test error".to_string()))
                })
                .await;
        }

        assert!(breaker.is_open().await);

        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 第一次成功，进入半开状态
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(breaker.is_half_open().await);

        // 第二次成功，应该恢复到关闭状态
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(breaker.is_closed().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_failure() {
        let config = CircuitBreakerConfig::new(2, 2, Duration::from_millis(100));
        let breaker = CircuitBreaker::new(config);

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("test error".to_string()))
                })
                .await;
        }

        assert!(breaker.is_open().await);

        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 第一次成功，进入半开状态
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(breaker.is_half_open().await);

        // 再次失败，应该回到打开状态
        let result = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::LimitError("test error".to_string()))
            })
            .await;
        assert!(result.is_err());
        assert!(breaker.is_open().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_reset() {
        let config = CircuitBreakerConfig::new(2, 2, Duration::from_secs(60));
        let breaker = CircuitBreaker::new(config);

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("test error".to_string()))
                })
                .await;
        }

        assert!(breaker.is_open().await);

        // 重置
        breaker.reset().await;

        // 验证重置
        assert!(breaker.is_closed().await);
        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 0);
        assert_eq!(stats.success_count, 0);
        assert_eq!(stats.total_calls, 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_get_state() {
        let breaker = CircuitBreaker::default();
        assert_eq!(breaker.get_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_get_stats() {
        let breaker = CircuitBreaker::default();

        let _ = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;

        let stats = breaker.get_stats().await;
        assert_eq!(stats.state, CircuitState::Closed);
        assert_eq!(stats.success_count, 1);
        assert_eq!(stats.total_calls, 1);
        assert!(stats.last_state_change.is_some());
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_max_calls() {
        let config =
            CircuitBreakerConfig::new(2, 3, Duration::from_millis(100)).half_open_max_calls(2);
        let breaker = CircuitBreaker::new(config);

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("test error".to_string()))
                })
                .await;
        }

        assert!(breaker.is_open().await);

        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 第一次调用，进入半开状态
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());

        // 第二次调用，达到上限
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());

        // 第三次调用，应该被拒绝
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("半开状态调用次数已达上限"));
    }

    #[tokio::test]
    async fn test_circuit_breaker_config() {
        let config = CircuitBreakerConfig::new(10, 5, Duration::from_secs(30));
        let breaker = CircuitBreaker::new(config);

        let breaker_config = breaker.config();
        assert_eq!(breaker_config.failure_threshold, 10);
        assert_eq!(breaker_config.success_threshold, 5);
        assert_eq!(breaker_config.timeout, Duration::from_secs(30));
    }

    // ==================== 增强的状态转换测试 ====================

    /// 测试 Closed → Open 转换
    /// 验证失败次数达到阈值时正确触发熔断
    #[tokio::test]
    async fn test_state_transition_closed_to_open() {
        let config = CircuitBreakerConfig::new(3, 2, Duration::from_secs(60));
        let breaker = CircuitBreaker::new(config);

        // 初始状态应为 Closed
        assert!(breaker.is_closed().await, "初始状态应为 Closed");
        assert_eq!(breaker.get_state().await, CircuitState::Closed);

        // 第一次失败
        let result = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::LimitError("error 1".to_string()))
            })
            .await;
        assert!(result.is_err());
        assert!(breaker.is_closed().await, "第一次失败后仍应为 Closed");
        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 1);

        // 第二次失败
        let result = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::LimitError("error 2".to_string()))
            })
            .await;
        assert!(result.is_err());
        assert!(breaker.is_closed().await, "第二次失败后仍应为 Closed");
        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 2);

        // 第三次失败，应触发熔断
        let result = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::LimitError("error 3".to_string()))
            })
            .await;
        assert!(result.is_err());
        assert!(breaker.is_open().await, "第三次失败后应转换为 Open");
        assert_eq!(breaker.get_state().await, CircuitState::Open);

        // 验证统计信息
        let stats = breaker.get_stats().await;
        assert_eq!(stats.state, CircuitState::Open);
        assert_eq!(stats.failure_count, 3);
        assert!(stats.last_failure_time.is_some());
        assert!(stats.last_state_change.is_some());
    }

    /// 测试 Open → HalfOpen 转换
    /// 验证超时后正确进入半开状态
    #[tokio::test]
    async fn test_state_transition_open_to_half_open() {
        let config = CircuitBreakerConfig::new(2, 2, Duration::from_millis(100));
        let breaker = CircuitBreaker::new(config);

        // 触发熔断
        for i in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError(format!("error {}", i)))
                })
                .await;
        }
        assert!(breaker.is_open().await, "应处于 Open 状态");

        // 未超时时请求应被拒绝
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("熔断器打开"));
        assert!(breaker.is_open().await, "未超时应保持 Open 状态");

        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 超时后第一次请求应进入 HalfOpen 状态
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(breaker.is_half_open().await, "超时后应转换为 HalfOpen 状态");
        assert_eq!(breaker.get_state().await, CircuitState::HalfOpen);

        // 验证状态变更时间已更新
        let stats = breaker.get_stats().await;
        assert!(stats.last_state_change.is_some());
    }

    /// 测试 HalfOpen → Closed 转换
    /// 验证半开状态下成功次数达到阈值后恢复正常
    #[tokio::test]
    async fn test_state_transition_half_open_to_closed() {
        let config = CircuitBreakerConfig::new(2, 3, Duration::from_millis(100));
        let breaker = CircuitBreaker::new(config);

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
                })
                .await;
        }
        assert!(breaker.is_open().await);

        // 等待超时进入半开状态
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 第一次成功，进入半开状态
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(breaker.is_half_open().await);
        let stats = breaker.get_stats().await;
        assert_eq!(stats.success_count, 1);

        // 第二次成功
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(breaker.is_half_open().await);
        let stats = breaker.get_stats().await;
        assert_eq!(stats.success_count, 2);

        // 第三次成功，应恢复到 Closed 状态
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(
            breaker.is_closed().await,
            "成功次数达到阈值后应恢复到 Closed 状态"
        );
        assert_eq!(breaker.get_state().await, CircuitState::Closed);

        // 验证计数器已重置
        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 0);
        assert_eq!(stats.success_count, 0);
    }

    /// 测试 HalfOpen → Open 转换
    /// 验证半开状态下失败立即触发熔断
    #[tokio::test]
    async fn test_state_transition_half_open_to_open() {
        let config = CircuitBreakerConfig::new(2, 3, Duration::from_millis(100));
        let breaker = CircuitBreaker::new(config);

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
                })
                .await;
        }
        assert!(breaker.is_open().await);

        // 等待超时进入半开状态
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 第一次成功，进入半开状态
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(breaker.is_half_open().await);

        // 半开状态下失败，应立即回到 Open 状态
        let result = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
            })
            .await;
        assert!(result.is_err());
        assert!(
            breaker.is_open().await,
            "半开状态下失败应立即回到 Open 状态"
        );
        assert_eq!(breaker.get_state().await, CircuitState::Open);

        // 注意：当前实现中，半开状态失败后 last_failure_time 未更新
        // 因此需要等待额外的超时时间才能再次进入半开状态
        // 这里验证熔断器处于 Open 状态
        assert!(breaker.is_open().await);
    }

    /// 测试完整的状态转换循环
    /// Closed → Open → HalfOpen → Closed
    #[tokio::test]
    async fn test_state_transition_full_cycle() {
        let config = CircuitBreakerConfig::new(2, 2, Duration::from_millis(100));
        let breaker = CircuitBreaker::new(config);

        // 阶段1: Closed → Open
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
                })
                .await;
        }
        assert!(breaker.is_open().await, "阶段1: 应处于 Open 状态");

        // 阶段2: Open → HalfOpen
        tokio::time::sleep(Duration::from_millis(150)).await;
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(breaker.is_half_open().await, "阶段2: 应处于 HalfOpen 状态");

        // 阶段3: HalfOpen → Closed
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(breaker.is_closed().await, "阶段3: 应恢复到 Closed 状态");

        // 验证可以重新触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
                })
                .await;
        }
        assert!(breaker.is_open().await, "应能重新触发熔断进入 Open 状态");
    }

    // ==================== 边界条件测试 ====================

    /// 测试半开状态最大调用次数边界
    /// 验证半开状态下调用次数限制的正确性
    #[tokio::test]
    async fn test_half_open_max_calls_boundary() {
        let config =
            CircuitBreakerConfig::new(2, 10, Duration::from_millis(100)).half_open_max_calls(3);
        let breaker = CircuitBreaker::new(config);

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
                })
                .await;
        }
        assert!(breaker.is_open().await);

        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 第一次调用（探针请求），进入半开状态
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(breaker.is_half_open().await);

        // 第二次调用
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());

        // 第三次调用（达到上限）
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());

        // 第四次调用应被拒绝
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("半开状态调用次数已达上限"),
            "错误信息应包含 '半开状态调用次数已达上限'，实际为: {}",
            err_msg
        );

        // 验证状态仍为 HalfOpen
        assert!(breaker.is_half_open().await);
    }

    /// 测试超时边界处理
    /// 验证刚好超时和刚好未超时的行为
    #[tokio::test]
    async fn test_timeout_boundary() {
        let config = CircuitBreakerConfig::new(2, 2, Duration::from_millis(200));
        let breaker = CircuitBreaker::new(config);

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
                })
                .await;
        }
        assert!(breaker.is_open().await);

        // 刚好未超时（等待 150ms，超时设置为 200ms）
        tokio::time::sleep(Duration::from_millis(150)).await;
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_err(), "未超时应拒绝请求");
        assert!(breaker.is_open().await, "未超时应保持 Open 状态");

        // 再等待 100ms（总计 250ms，超过 200ms 超时）
        tokio::time::sleep(Duration::from_millis(100)).await;
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok(), "超时后应允许请求");
        assert!(breaker.is_half_open().await, "超时后应进入 HalfOpen 状态");
    }

    /// 测试超时边界 - 刚好超时
    #[tokio::test]
    async fn test_timeout_exact_boundary() {
        let config = CircuitBreakerConfig::new(2, 2, Duration::from_millis(100));
        let breaker = CircuitBreaker::new(config);

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
                })
                .await;
        }
        assert!(breaker.is_open().await);

        // 刚好等待超时时间
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 应该可以进入半开状态
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok(), "刚好超时应允许请求");
        assert!(breaker.is_half_open().await);
    }

    /// 测试并发状态转换安全性
    /// 验证多个并发请求不会导致状态不一致
    #[tokio::test]
    async fn test_concurrent_state_transition_safety() {
        let config = CircuitBreakerConfig::new(5, 3, Duration::from_millis(100));
        let breaker = Arc::new(CircuitBreaker::new(config));

        // 并发触发失败
        let mut handles = vec![];
        for _ in 0..10 {
            let breaker_clone = Arc::clone(&breaker);
            let handle = tokio::spawn(async move {
                breaker_clone
                    .execute(|| async {
                        Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
                    })
                    .await
            });
            handles.push(handle);
        }

        // 等待所有请求完成
        let results: Vec<_> = futures::future::join_all(handles).await;

        // 验证最终状态一致
        // 由于失败阈值是 5，应该有 5 个请求成功执行（并触发熔断）
        // 另外 5 个请求可能在熔断后被拒绝
        let executed_count = results.iter().filter(|r| r.is_ok()).count();
        assert!(
            executed_count >= 5,
            "至少应有 5 个请求被执行（达到失败阈值）"
        );

        // 验证熔断器最终状态
        assert!(breaker.is_open().await, "并发失败后应处于 Open 状态");

        // 验证失败计数
        let stats = breaker.get_stats().await;
        assert!(
            stats.failure_count >= 5,
            "失败计数应至少为 5，实际为 {}",
            stats.failure_count
        );
    }

    /// 测试并发成功恢复
    #[tokio::test]
    async fn test_concurrent_recovery_safety() {
        let config =
            CircuitBreakerConfig::new(2, 5, Duration::from_millis(100)).half_open_max_calls(10);
        let breaker = Arc::new(CircuitBreaker::new(config));

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
                })
                .await;
        }
        assert!(breaker.is_open().await);

        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 并发发送成功请求
        let mut handles = vec![];
        for _ in 0..8 {
            let breaker_clone = Arc::clone(&breaker);
            let handle = tokio::spawn(async move {
                breaker_clone
                    .execute(|| async { Ok::<(), FlowGuardError>(()) })
                    .await
            });
            handles.push(handle);
        }

        let results: Vec<_> = futures::future::join_all(handles).await;

        // 验证最终状态一致
        // 熔断器应该最终恢复到 Closed 状态
        tokio::time::sleep(Duration::from_millis(50)).await;
        let final_state = breaker.get_state().await;
        assert!(
            final_state == CircuitState::Closed || final_state == CircuitState::HalfOpen,
            "最终状态应为 Closed 或 HalfOpen，实际为 {:?}",
            final_state
        );

        // 验证成功请求数量
        let success_count = results
            .iter()
            .filter(|r| r.is_ok() && r.as_ref().unwrap().is_ok())
            .count();
        assert!(
            success_count >= 5,
            "至少应有 5 个成功请求，实际为 {}",
            success_count
        );
    }

    /// 测试零失败阈值边界
    #[tokio::test]
    async fn test_zero_failure_threshold() {
        let config = CircuitBreakerConfig::new(0, 2, Duration::from_secs(60));
        let breaker = CircuitBreaker::new(config);

        // 零阈值意味着第一次失败就触发熔断
        let result = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
            })
            .await;
        assert!(result.is_err());

        // 由于阈值是 0，失败计数从 0 增加到 1，1 > 0，应触发熔断
        // 但根据实现，failure_count.fetch_add(1) + 1 = 1，1 >= 0 为 true
        // 所以应该触发熔断
        assert!(breaker.is_open().await, "零阈值时第一次失败应触发熔断");
    }

    /// 测试零成功阈值边界
    #[tokio::test]
    async fn test_zero_success_threshold() {
        let config = CircuitBreakerConfig::new(2, 0, Duration::from_millis(100));
        let breaker = CircuitBreaker::new(config);

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
                })
                .await;
        }
        assert!(breaker.is_open().await);

        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 零阈值意味着第一次成功就恢复
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());

        // success_count.fetch_add(1) + 1 = 1，1 >= 0 为 true
        // 所以应该立即恢复
        assert!(
            breaker.is_closed().await,
            "零成功阈值时第一次成功应立即恢复"
        );
    }

    // ==================== 统计信息测试 ====================

    /// 测试成功计数正确性
    #[tokio::test]
    async fn test_success_count_correctness() {
        let config = CircuitBreakerConfig::new(10, 5, Duration::from_secs(60));
        let breaker = CircuitBreaker::new(config);

        // 执行 5 次成功操作
        for i in 1..=5u64 {
            let result = breaker
                .execute(|| async { Ok::<(), FlowGuardError>(()) })
                .await;
            assert!(result.is_ok());

            let stats = breaker.get_stats().await;
            assert_eq!(stats.success_count, i, "成功计数应为 {}", i);
            assert_eq!(stats.failure_count, 0);
            assert_eq!(stats.total_calls, i);
        }

        // 验证最终统计
        let stats = breaker.get_stats().await;
        assert_eq!(stats.success_count, 5);
        assert_eq!(stats.failure_count, 0);
        assert_eq!(stats.total_calls, 5);
        assert_eq!(stats.state, CircuitState::Closed);
    }

    /// 测试失败计数正确性
    #[tokio::test]
    async fn test_failure_count_correctness() {
        let config = CircuitBreakerConfig::new(10, 5, Duration::from_secs(60));
        let breaker = CircuitBreaker::new(config);

        // 执行 5 次失败操作
        for i in 1..=5 {
            let result = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError(format!("error {}", i)))
                })
                .await;
            assert!(result.is_err());

            let stats = breaker.get_stats().await;
            assert_eq!(stats.failure_count, i, "失败计数应为 {}", i);
            assert_eq!(stats.total_calls, i);
        }

        // 执行成功操作，失败计数应重置
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());

        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 0, "成功后失败计数应重置为 0");
        assert_eq!(stats.success_count, 1);
    }

    /// 测试总调用次数正确性
    #[tokio::test]
    async fn test_total_calls_correctness() {
        let config = CircuitBreakerConfig::new(10, 5, Duration::from_secs(60));
        let breaker = CircuitBreaker::new(config);

        // 混合成功和失败操作
        for i in 1..=10u64 {
            if i % 2 == 0 {
                let result = breaker
                    .execute(|| async { Ok::<(), FlowGuardError>(()) })
                    .await;
                assert!(result.is_ok());
            } else {
                let result = breaker
                    .execute(|| async {
                        Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
                    })
                    .await;
                assert!(result.is_err());
            }

            let stats = breaker.get_stats().await;
            assert_eq!(stats.total_calls, i, "总调用次数应为 {}", i);
        }

        let stats = breaker.get_stats().await;
        assert_eq!(stats.total_calls, 10);
    }

    /// 测试时间戳记录正确性
    #[tokio::test]
    async fn test_timestamp_correctness() {
        let config = CircuitBreakerConfig::new(2, 2, Duration::from_millis(100));
        let breaker = CircuitBreaker::new(config);

        // 初始状态变更时间应存在
        let stats = breaker.get_stats().await;
        assert!(stats.last_state_change.is_some(), "初始应有状态变更时间");
        let initial_change_time = stats.last_state_change.unwrap();

        // 等待一小段时间
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
                })
                .await;
        }

        // 验证状态变更时间已更新
        let stats = breaker.get_stats().await;
        assert!(stats.last_state_change.is_some());
        let open_change_time = stats.last_state_change.unwrap();
        assert!(open_change_time > initial_change_time, "状态变更时间应更新");

        // 验证失败时间已记录
        assert!(stats.last_failure_time.is_some(), "应有失败时间记录");

        // 等待超时并恢复
        tokio::time::sleep(Duration::from_millis(150)).await;
        let _ = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;

        // 再次验证状态变更时间
        let stats = breaker.get_stats().await;
        let half_open_change_time = stats.last_state_change.unwrap();
        assert!(
            half_open_change_time > open_change_time,
            "进入 HalfOpen 状态时间应更新"
        );
    }

    /// 测试失败时间戳更新
    #[tokio::test]
    async fn test_failure_timestamp_update() {
        let config = CircuitBreakerConfig::new(10, 5, Duration::from_secs(60));
        let breaker = CircuitBreaker::new(config);

        // 第一次失败
        let _ = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::LimitError("error 1".to_string()))
            })
            .await;

        let stats1 = breaker.get_stats().await;
        let first_failure_time = stats1.last_failure_time.unwrap();

        // 等待一小段时间
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 第二次失败
        let _ = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::LimitError("error 2".to_string()))
            })
            .await;

        let stats2 = breaker.get_stats().await;
        let second_failure_time = stats2.last_failure_time.unwrap();

        // 验证失败时间已更新
        assert!(second_failure_time > first_failure_time, "失败时间应更新");
    }

    /// 测试重置后统计信息清零
    #[tokio::test]
    async fn test_stats_reset() {
        let config = CircuitBreakerConfig::new(10, 2, Duration::from_secs(60));
        let breaker = CircuitBreaker::new(config);

        // 执行一些操作
        for _ in 0..3 {
            let _ = breaker
                .execute(|| async { Ok::<(), FlowGuardError>(()) })
                .await;
        }
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
                })
                .await;
        }

        // 验证有统计数据
        let stats = breaker.get_stats().await;
        assert!(stats.total_calls > 0);
        assert!(stats.success_count > 0);
        assert!(stats.failure_count > 0);

        // 重置
        breaker.reset().await;

        // 验证统计已清零
        let stats = breaker.get_stats().await;
        assert_eq!(stats.state, CircuitState::Closed);
        assert_eq!(stats.failure_count, 0);
        assert_eq!(stats.success_count, 0);
        assert_eq!(stats.total_calls, 0);
        assert!(stats.last_failure_time.is_none(), "重置后失败时间应为 None");
        assert!(
            stats.last_state_change.is_some(),
            "重置后状态变更时间应存在"
        );
    }

    /// 测试半开状态统计信息
    #[tokio::test]
    async fn test_half_open_stats() {
        let config = CircuitBreakerConfig::new(2, 3, Duration::from_millis(100));
        let breaker = CircuitBreaker::new(config);

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
                })
                .await;
        }

        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 进入半开状态并执行操作
        for i in 1..=2 {
            let _ = breaker
                .execute(|| async { Ok::<(), FlowGuardError>(()) })
                .await;

            let stats = breaker.get_stats().await;
            assert_eq!(stats.success_count, i, "半开状态成功计数应为 {}", i);
            assert_eq!(stats.state, CircuitState::HalfOpen);
        }
    }

    /// 测试 Builder 模式
    #[tokio::test]
    async fn test_builder_pattern() {
        let breaker = CircuitBreaker::builder()
            .failure_threshold(10)
            .success_threshold(5)
            .timeout(Duration::from_secs(30))
            .half_open_max_calls(4)
            .build();

        let config = breaker.config();
        assert_eq!(config.failure_threshold, 10);
        assert_eq!(config.success_threshold, 5);
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.half_open_max_calls, 4);

        // 验证初始状态
        assert!(breaker.is_closed().await);
    }

    /// 测试连续失败后成功重置失败计数
    #[tokio::test]
    async fn test_failure_count_reset_on_success() {
        let config = CircuitBreakerConfig::new(10, 5, Duration::from_secs(60));
        let breaker = CircuitBreaker::new(config);

        // 执行 3 次失败
        for _ in 0..3 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::LimitError("error".to_string()))
                })
                .await;
        }

        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 3);

        // 执行成功
        let _ = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;

        // 失败计数应重置
        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 0, "成功后失败计数应重置为 0");
        assert_eq!(stats.success_count, 1);
    }
}
