// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 熔断器类型定义

use crate::clock::{Clock, SystemClock};
use crate::constants::{
    DEFAULT_CIRCUIT_BREAKER_FAILURE_THRESHOLD, DEFAULT_CIRCUIT_BREAKER_HALF_OPEN_MAX_CALLS,
    DEFAULT_CIRCUIT_BREAKER_SLOW_CALL_DURATION_MILLIS,
    DEFAULT_CIRCUIT_BREAKER_SLOW_CALL_RATE_THRESHOLD, DEFAULT_CIRCUIT_BREAKER_SUCCESS_THRESHOLD,
    DEFAULT_CIRCUIT_BREAKER_TIMEOUT_SECS,
};
use crate::error::{CircuitBreakerStats, CircuitState, FlowGuardError};
use log::{info, trace, warn};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// 错误分类器 trait
///
/// 用于判断错误是否应该计入失败计数。
/// 允许用户自定义哪些错误应该被视为失败。
///
/// # 示例
///
/// ```rust
/// use limiteron::circuit::ErrorClassifier;
/// use limiteron::error::FlowGuardError;
///
/// #[derive(Debug)]
/// struct CustomErrorClassifier;
/// impl ErrorClassifier for CustomErrorClassifier {
///     fn is_counted_as_failure(&self, error: &FlowGuardError) -> bool {
///         // 自定义逻辑：只有特定的错误才算失败
///         !matches!(error, FlowGuardError::ValidationError(_))
///     }
/// }
/// ```
pub trait ErrorClassifier: Send + Sync + std::fmt::Debug {
    /// 判断错误是否应该计入失败计数
    ///
    /// # 参数
    /// - `error`: 要判断的错误
    ///
    /// # 返回
    /// - `true`: 错误应计入失败计数
    /// - `false`: 错误不应计入失败计数
    fn is_counted_as_failure(&self, error: &FlowGuardError) -> bool;
}

/// 默认错误分类器
///
/// 默认行为：
/// - 5xx 错误（StorageError::ConnectionError, StorageError::TimeoutError）算失败
/// - 超时错误算失败
/// - 4xx 错误（ValidationError, NotFound）不算失败
#[derive(Debug)]
pub struct DefaultErrorClassifier;

impl ErrorClassifier for DefaultErrorClassifier {
    fn is_counted_as_failure(&self, error: &FlowGuardError) -> bool {
        match error {
            // 存储相关的临时错误算失败
            FlowGuardError::StorageError(storage_err) => storage_err.is_transient(),
            // 限流、熔断器错误不算失败（这些是预期的保护机制）
            FlowGuardError::LimitError(_) | FlowGuardError::CircuitBreakerError(_) => false,
            // 验证错误不算失败（客户端问题）
            FlowGuardError::ValidationError(_) => false,
            // 其他错误算失败
            _ => true,
        }
    }
}

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
    /// 慢调用时长阈值（超过此时长视为慢调用）
    pub slow_call_duration_threshold: Duration,
    /// 慢调用率阈值（慢调用占比超过此值时熔断）
    pub slow_call_rate_threshold: f64,
    /// 错误分类器
    pub error_classifier: Arc<dyn ErrorClassifier>,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: DEFAULT_CIRCUIT_BREAKER_FAILURE_THRESHOLD,
            success_threshold: DEFAULT_CIRCUIT_BREAKER_SUCCESS_THRESHOLD,
            timeout: Duration::from_secs(DEFAULT_CIRCUIT_BREAKER_TIMEOUT_SECS),
            half_open_max_calls: DEFAULT_CIRCUIT_BREAKER_HALF_OPEN_MAX_CALLS,
            slow_call_duration_threshold: Duration::from_millis(
                DEFAULT_CIRCUIT_BREAKER_SLOW_CALL_DURATION_MILLIS,
            ),
            slow_call_rate_threshold: DEFAULT_CIRCUIT_BREAKER_SLOW_CALL_RATE_THRESHOLD,
            error_classifier: Arc::new(DefaultErrorClassifier),
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
            slow_call_duration_threshold: Duration::from_millis(
                DEFAULT_CIRCUIT_BREAKER_SLOW_CALL_DURATION_MILLIS,
            ),
            slow_call_rate_threshold: DEFAULT_CIRCUIT_BREAKER_SLOW_CALL_RATE_THRESHOLD,
            error_classifier: Arc::new(DefaultErrorClassifier),
        }
    }

    /// 设置半开状态的最大调用次数
    pub fn half_open_max_calls(mut self, max_calls: u64) -> Self {
        self.half_open_max_calls = max_calls;
        self
    }

    /// 设置慢调用时长阈值
    pub fn slow_call_duration_threshold(mut self, threshold: Duration) -> Self {
        self.slow_call_duration_threshold = threshold;
        self
    }

    /// 设置慢调用率阈值
    pub fn slow_call_rate_threshold(mut self, threshold: f64) -> Self {
        self.slow_call_rate_threshold = threshold;
        self
    }

    /// 设置错误分类器
    pub fn error_classifier(mut self, classifier: Arc<dyn ErrorClassifier>) -> Self {
        self.error_classifier = classifier;
        self
    }
}

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
    /// 慢调用计数
    slow_call_count: Arc<AtomicU64>,
    /// 最后失败时间
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    /// 最后状态变更时间
    last_state_change: Arc<RwLock<Option<Instant>>>,
    /// 半开状态下的调用计数
    half_open_calls: Arc<AtomicU64>,
    /// 配置
    config: CircuitBreakerConfig,
    /// 时钟实例
    clock: Arc<dyn Clock>,
    /// 事件发射器（可选，feature-gated）
    #[cfg(feature = "event-system")]
    event_emitter: Option<Arc<crate::events::EventEmitter>>,
}

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

    /// 设置慢调用时长阈值
    pub fn slow_call_duration_threshold(mut self, threshold: Duration) -> Self {
        self.config.slow_call_duration_threshold = threshold;
        self
    }

    /// 设置慢调用率阈值
    pub fn slow_call_rate_threshold(mut self, threshold: f64) -> Self {
        self.config.slow_call_rate_threshold = threshold;
        self
    }

    /// 设置错误分类器
    pub fn error_classifier(mut self, classifier: Arc<dyn ErrorClassifier>) -> Self {
        self.config.error_classifier = classifier;
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
    /// use limiteron::circuit::{CircuitBreaker, CircuitBreakerConfig};
    /// use std::time::Duration;
    ///
    /// let config = CircuitBreakerConfig::new(5, 2, Duration::from_secs(60));
    /// let breaker = CircuitBreaker::with_dependencies(config);
    /// ```
    pub fn with_dependencies(config: CircuitBreakerConfig) -> Self {
        Self::with_clock(config, Arc::new(SystemClock))
    }

    /// 使用依赖注入模式和自定义时钟创建熔断器
    ///
    /// # 参数
    /// - `config`: 熔断器配置
    /// - `clock`: 时钟实现,用于时间注入(测试用)
    pub fn with_clock(config: CircuitBreakerConfig, clock: Arc<dyn Clock>) -> Self {
        info!(
            "创建熔断器: failure_threshold={}, success_threshold={}, timeout={:?}",
            config.failure_threshold, config.success_threshold, config.timeout
        );

        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            total_calls: Arc::new(AtomicU64::new(0)),
            slow_call_count: Arc::new(AtomicU64::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
            last_state_change: Arc::new(RwLock::new(Some(clock.now()))),
            half_open_calls: Arc::new(AtomicU64::new(0)),
            config,
            clock,
            #[cfg(feature = "event-system")]
            event_emitter: None,
        }
    }

    /// 创建熔断器构建器
    ///
    /// # 返回
    /// 新的构建器实例
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::circuit::CircuitBreaker;
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
    /// use limiteron::circuit::{CircuitBreaker, CircuitBreakerConfig};
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
    /// use limiteron::circuit::{CircuitBreaker, CircuitBreakerConfig};
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
                    if self.clock.now().duration_since(last_failure) >= self.config.timeout {
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
        let start_time = self.clock.now();
        let result = operation().await;
        let elapsed = start_time.elapsed();

        // 记录调用时长并检查是否为慢调用
        self.record_call_duration(elapsed).await;

        // 根据操作结果更新状态
        match result {
            Ok(value) => {
                self.on_success().await;
                Ok(value)
            }
            Err(e) => {
                self.on_failure(&e).await;
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
                        success_count, self.config.success_threshold
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
    async fn on_failure(&self, error: &FlowGuardError) {
        // 使用错误分类器判断是否应该计入失败计数
        if !self.config.error_classifier.is_counted_as_failure(error) {
            trace!("错误不计入失败计数: {:?}", error);
            return;
        }

        let state = self.state.read().await;

        match *state {
            CircuitState::Closed => {
                // 关闭状态下，增加失败计数
                let failure_count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;

                // 记录失败时间
                *self.last_failure_time.write().await = Some(self.clock.now());

                if failure_count >= self.config.failure_threshold {
                    // 达到失败阈值，切换到打开状态
                    drop(state);
                    self.transition_to_open().await;
                } else {
                    trace!(
                        "操作失败（关闭状态）: {}/{}",
                        failure_count, self.config.failure_threshold
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

    /// 记录调用时长并检查是否为慢调用
    ///
    /// 如果调用时长超过慢调用阈值，则增加慢调用计数。
    /// 如果慢调用率超过阈值，则触发熔断。
    async fn record_call_duration(&self, elapsed: Duration) {
        if elapsed >= self.config.slow_call_duration_threshold {
            let slow_calls = self.slow_call_count.fetch_add(1, Ordering::Relaxed) + 1;
            let total_calls = self.total_calls.load(Ordering::Relaxed);

            trace!(
                "慢调用检测: elapsed={:?}, threshold={:?}, slow_calls={}/{}",
                elapsed, self.config.slow_call_duration_threshold, slow_calls, total_calls
            );

            // 检查慢调用率是否超过阈值
            self.check_slow_call_rate(slow_calls, total_calls).await;
        }
    }

    /// 检查慢调用率是否超过阈值
    async fn check_slow_call_rate(&self, slow_calls: u64, total_calls: u64) {
        if total_calls == 0 {
            return;
        }

        let slow_call_rate = slow_calls as f64 / total_calls as f64;

        if slow_call_rate >= self.config.slow_call_rate_threshold {
            let state = self.state.read().await;
            if *state == CircuitState::Closed {
                drop(state);
                warn!(
                    "慢调用率超过阈值: {:.2}% >= {:.2}%，触发熔断",
                    slow_call_rate * 100.0,
                    self.config.slow_call_rate_threshold * 100.0
                );
                self.transition_to_open().await;
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
        *self.last_state_change.write().await = Some(self.clock.now());

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
                self.slow_call_count.store(0, Ordering::Relaxed);
                info!("熔断器状态变更: {:?} -> Closed", old_state);
            }
        }

        // 发射熔断器状态变更事件
        #[cfg(feature = "event-system")]
        {
            if let Some(ref emitter) = self.event_emitter {
                let old_state_str = format!("{:?}", old_state);
                let new_state_str = format!("{:?}", new_state);
                let event =
                    crate::events::Event::new(crate::events::EventType::CircuitStateChanged {
                        from: old_state_str,
                        to: new_state_str,
                    });
                if let Err(e) = emitter.emit(event).await {
                    log::error!("Failed to emit circuit state change event: {}", e);
                }
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
        self.slow_call_count.store(0, Ordering::Relaxed);
        *self.last_failure_time.write().await = None;
        *self.last_state_change.write().await = Some(self.clock.now());
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
                let elapsed = self.clock.now().duration_since(t);
                let duration = chrono::Duration::from_std(elapsed).ok()?;
                Some(chrono::Utc::now() - duration)
            }),
            last_state_change: last_state_change.and_then(|t| {
                let elapsed = self.clock.now().duration_since(t);
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
                Err::<(), FlowGuardError>(FlowGuardError::BanError("test error".to_string()))
            })
            .await;
        assert!(result.is_err());

        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 1);
        assert!(breaker.is_closed().await);

        // 第二次失败
        let result = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::BanError("test error".to_string()))
            })
            .await;
        assert!(result.is_err());

        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 2);
        assert!(breaker.is_closed().await);

        // 第三次失败，应该触发熔断
        let result = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::BanError("test error".to_string()))
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
                    Err::<(), FlowGuardError>(FlowGuardError::BanError("test error".to_string()))
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
                    Err::<(), FlowGuardError>(FlowGuardError::BanError("test error".to_string()))
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
                    Err::<(), FlowGuardError>(FlowGuardError::BanError("test error".to_string()))
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
                Err::<(), FlowGuardError>(FlowGuardError::BanError("test error".to_string()))
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
                    Err::<(), FlowGuardError>(FlowGuardError::BanError("test error".to_string()))
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
                    Err::<(), FlowGuardError>(FlowGuardError::BanError("test error".to_string()))
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
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("半开状态调用次数已达上限")
        );
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
                Err::<(), FlowGuardError>(FlowGuardError::BanError("error 1".to_string()))
            })
            .await;
        assert!(result.is_err());
        assert!(breaker.is_closed().await, "第一次失败后仍应为 Closed");

        // 第二次失败
        let result = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::BanError("error 2".to_string()))
            })
            .await;
        assert!(result.is_err());
        assert!(breaker.is_closed().await, "第二次失败后仍应为 Closed");

        // 第三次失败，应触发熔断
        let result = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::BanError("error 3".to_string()))
            })
            .await;
        assert!(result.is_err());
        assert!(breaker.is_open().await, "第三次失败后应转换为 Open");
        assert_eq!(breaker.get_state().await, CircuitState::Open);
    }

    /// 测试 Open → HalfOpen 转换
    #[tokio::test]
    async fn test_state_transition_open_to_half_open() {
        let config = CircuitBreakerConfig::new(2, 2, Duration::from_millis(100));
        let breaker = CircuitBreaker::new(config);

        // 触发熔断
        for i in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::BanError(format!("error {}", i)))
                })
                .await;
        }
        assert!(breaker.is_open().await, "应处于 Open 状态");

        // 未超时时请求应被拒绝
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_err());
        assert!(breaker.is_open().await, "未超时应保持 Open 状态");

        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 超时后第一次请求应进入 HalfOpen 状态
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(breaker.is_half_open().await, "超时后应转换为 HalfOpen 状态");
    }

    /// 测试 HalfOpen → Closed 转换
    #[tokio::test]
    async fn test_state_transition_half_open_to_closed() {
        let config = CircuitBreakerConfig::new(2, 2, Duration::from_millis(100));
        let breaker = CircuitBreaker::new(config);

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::BanError("error".to_string()))
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

        // 第二次成功，应恢复到 Closed 状态
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(
            breaker.is_closed().await,
            "成功次数达到阈值后应恢复到 Closed 状态"
        );
    }

    /// 测试完整的状态转换循环: Closed → Open → HalfOpen → Closed
    #[tokio::test]
    async fn test_state_transition_full_cycle() {
        let config = CircuitBreakerConfig::new(2, 2, Duration::from_millis(100));
        let breaker = CircuitBreaker::new(config);

        // 阶段1: Closed → Open
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::BanError("error".to_string()))
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

    // ==================== 慢调用检测测试 ====================

    /// 测试慢调用时长阈值配置
    #[tokio::test]
    async fn test_slow_call_duration_threshold_config() {
        let config = CircuitBreakerConfig::default()
            .slow_call_duration_threshold(Duration::from_millis(100));
        assert_eq!(
            config.slow_call_duration_threshold,
            Duration::from_millis(100)
        );
    }

    /// 测试慢调用率阈值配置
    #[tokio::test]
    async fn test_slow_call_rate_threshold_config() {
        let config = CircuitBreakerConfig::default().slow_call_rate_threshold(0.8);
        assert_eq!(config.slow_call_rate_threshold, 0.8);
    }

    /// 测试默认错误分类器 - StorageError 超时算失败
    #[test]
    fn test_default_error_classifier_storage_timeout() {
        let classifier = DefaultErrorClassifier;
        let error = FlowGuardError::StorageError(crate::error::StorageError::TimeoutError(
            "timeout".into(),
        ));
        assert!(classifier.is_counted_as_failure(&error));
    }

    /// 测试默认错误分类器 - StorageError 连接错误算失败
    #[test]
    fn test_default_error_classifier_connection_error() {
        let classifier = DefaultErrorClassifier;
        let error = FlowGuardError::StorageError(crate::error::StorageError::ConnectionError(
            "connection".into(),
        ));
        assert!(classifier.is_counted_as_failure(&error));
    }

    /// 测试默认错误分类器 - LimitError 不算失败
    #[test]
    fn test_default_error_classifier_limit_error() {
        let classifier = DefaultErrorClassifier;
        let error = FlowGuardError::LimitError("rate limited".into());
        assert!(!classifier.is_counted_as_failure(&error));
    }

    /// 测试默认错误分类器 - ValidationError 不算失败
    #[test]
    fn test_default_error_classifier_validation_error() {
        let classifier = DefaultErrorClassifier;
        let error = FlowGuardError::ValidationError("invalid input".into());
        assert!(!classifier.is_counted_as_failure(&error));
    }

    /// 测试默认错误分类器 - CircuitBreakerError 不算失败
    #[test]
    fn test_default_error_classifier_circuit_breaker_error() {
        let classifier = DefaultErrorClassifier;
        let error = FlowGuardError::CircuitBreakerError("circuit open".into());
        assert!(!classifier.is_counted_as_failure(&error));
    }

    /// 测试默认错误分类器 - 其他错误算失败
    #[test]
    fn test_default_error_classifier_other_errors() {
        let classifier = DefaultErrorClassifier;
        let error = FlowGuardError::Other("unknown error".into());
        assert!(classifier.is_counted_as_failure(&error));
    }

    /// 测试自定义错误分类器
    #[tokio::test]
    async fn test_custom_error_classifier() {
        #[derive(Debug)]
        struct CustomClassifier;
        impl ErrorClassifier for CustomClassifier {
            fn is_counted_as_failure(&self, error: &FlowGuardError) -> bool {
                // 只有 StorageError 算失败
                matches!(error, FlowGuardError::StorageError(_))
            }
        }

        let config = CircuitBreakerConfig {
            error_classifier: Arc::new(CustomClassifier),
            failure_threshold: 2,
            ..Default::default()
        };

        let breaker = CircuitBreaker::new(config);

        // ValidationError 不应触发失败计数
        let _ = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::ValidationError("test".into()))
            })
            .await;

        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 0);

        // StorageError 应该触发失败计数
        let _ = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::StorageError(
                    crate::error::StorageError::TimeoutError("timeout".into()),
                ))
            })
            .await;

        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 1);
    }

    /// 测试 Builder 模式设置慢调用配置
    #[tokio::test]
    async fn test_builder_with_slow_call_config() {
        let breaker = CircuitBreaker::builder()
            .slow_call_duration_threshold(Duration::from_millis(200))
            .slow_call_rate_threshold(0.6)
            .build();

        let config = breaker.config();
        assert_eq!(
            config.slow_call_duration_threshold,
            Duration::from_millis(200)
        );
        assert_eq!(config.slow_call_rate_threshold, 0.6);
    }

    #[test]
    fn test_circuit_breaker_builder_default() {
        let builder = CircuitBreakerBuilder::default();
        assert_eq!(builder.config.failure_threshold, 5);
        assert_eq!(builder.config.success_threshold, 3);
    }

    #[test]
    fn test_config_error_classifier_builder() {
        let classifier: Arc<dyn ErrorClassifier> = Arc::new(DefaultErrorClassifier);
        let config = CircuitBreakerConfig::default().error_classifier(classifier);
        // Just verify it doesn't panic and config is accessible
        assert_eq!(config.failure_threshold, 5);
    }

    #[test]
    fn test_config_all_builder_methods() {
        let classifier: Arc<dyn ErrorClassifier> = Arc::new(DefaultErrorClassifier);
        let config = CircuitBreakerConfig::new(10, 5, Duration::from_secs(30))
            .half_open_max_calls(4)
            .slow_call_duration_threshold(Duration::from_millis(100))
            .slow_call_rate_threshold(0.7)
            .error_classifier(classifier);
        assert_eq!(config.failure_threshold, 10);
        assert_eq!(config.success_threshold, 5);
        assert_eq!(config.half_open_max_calls, 4);
        assert_eq!(
            config.slow_call_duration_threshold,
            Duration::from_millis(100)
        );
        assert!((config.slow_call_rate_threshold - 0.7).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_slow_call_rate_triggers_open() {
        // Every call is "slow" (threshold=0) and rate threshold is 0.5
        // After 1 call: slow=1/total=1 = 1.0 >= 0.5 -> should open
        let config = CircuitBreakerConfig {
            slow_call_duration_threshold: Duration::ZERO,
            slow_call_rate_threshold: 0.5,
            failure_threshold: 100,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new(config);

        let _ = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;

        assert!(
            breaker.is_open().await,
            "Slow call rate should trigger Open state"
        );
    }

    #[tokio::test]
    async fn test_slow_call_rate_below_threshold_stays_closed() {
        // threshold is very high so no calls are "slow"
        let config = CircuitBreakerConfig {
            slow_call_duration_threshold: Duration::from_secs(60),
            slow_call_rate_threshold: 0.5,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new(config);

        for _ in 0..5 {
            let _ = breaker
                .execute(|| async { Ok::<(), FlowGuardError>(()) })
                .await;
        }

        assert!(breaker.is_closed().await);
    }

    #[tokio::test]
    async fn test_on_success_in_open_state_logs_warning() {
        // Force breaker into Open state, then call on_success path
        let config = CircuitBreakerConfig::new(1, 1, Duration::from_secs(60));
        let breaker = CircuitBreaker::new(config);

        // Trigger open
        let _ = breaker
            .execute(|| async { Err::<(), FlowGuardError>(FlowGuardError::BanError("e".into())) })
            .await;
        assert!(breaker.is_open().await);

        // Next call should be rejected (still open, not timed out)
        let result = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_circuit_breaker_with_clock() {
        use crate::clock::MockClock;
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let config = CircuitBreakerConfig::new(2, 2, Duration::from_secs(60));
        let breaker = CircuitBreaker::with_clock(config, clock);

        assert!(breaker.is_closed().await);
        let stats = breaker.get_stats().await;
        assert_eq!(stats.state, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_builder_error_classifier() {
        let classifier: Arc<dyn ErrorClassifier> = Arc::new(DefaultErrorClassifier);
        let breaker = CircuitBreaker::builder()
            .failure_threshold(3)
            .error_classifier(classifier)
            .build();
        assert_eq!(breaker.config().failure_threshold, 3);
    }

    #[tokio::test]
    async fn test_get_stats_after_failure() {
        let config = CircuitBreakerConfig::new(5, 2, Duration::from_secs(60));
        let breaker = CircuitBreaker::new(config);

        let _ = breaker
            .execute(|| async { Err::<(), FlowGuardError>(FlowGuardError::BanError("e".into())) })
            .await;

        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 1);
        assert_eq!(stats.total_calls, 1);
        assert!(stats.last_failure_time.is_some());
    }

    #[test]
    fn test_default_error_classifier_storage_not_transient() {
        let classifier = DefaultErrorClassifier;
        // NotFound is NOT transient, so it should NOT be counted as failure
        let error = FlowGuardError::StorageError(crate::error::StorageError::NotFound("nf".into()));
        assert!(!classifier.is_counted_as_failure(&error));
    }

    /// 测试在 Open 状态下调用 on_failure
    /// 覆盖 on_failure 内 CircuitState::Open 分支（line 500, 502）
    #[tokio::test]
    async fn test_on_failure_when_open() {
        let config = CircuitBreakerConfig::new(2, 2, Duration::from_secs(60));
        let breaker = CircuitBreaker::new(config);

        // 触发熔断，进入 Open 状态
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), FlowGuardError>(FlowGuardError::BanError("e".to_string()))
                })
                .await;
        }
        assert!(breaker.is_open().await);

        // 直接调用 on_failure，覆盖 Open 分支
        // 此时状态为 Open，on_failure 内的 Open 分支会打印 warn 但不做状态转换
        let error = FlowGuardError::BanError("open-state failure".to_string());
        breaker.on_failure(&error).await;

        // 状态应仍为 Open
        assert!(breaker.is_open().await);
    }
}
