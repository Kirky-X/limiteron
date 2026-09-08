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
use crate::error::{CircuitBreakerStats, CircuitState, LimiteronError};
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
/// use limiteron::error::LimiteronError;
///
/// #[derive(Debug)]
/// struct CustomErrorClassifier;
/// impl ErrorClassifier for CustomErrorClassifier {
///     fn is_counted_as_failure(&self, error: &LimiteronError) -> bool {
///         // 自定义逻辑：只有特定的错误才算失败
///         !matches!(error, LimiteronError::ValidationError(_))
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
    fn is_counted_as_failure(&self, error: &LimiteronError) -> bool;
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
    fn is_counted_as_failure(&self, error: &LimiteronError) -> bool {
        match error {
            // 存储相关的临时错误算失败
            LimiteronError::StorageError(storage_err) => storage_err.is_transient(),
            // 限流、熔断器错误不算失败（这些是预期的保护机制）
            LimiteronError::LimitError(_) | LimiteronError::CircuitBreakerError(_) => false,
            // 验证错误不算失败（客户端问题）
            LimiteronError::ValidationError(_) => false,
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
    /// 最后失败时间（墙钟）
    ///
    /// 统计展示用：`last_failure_time` 位于自定义时钟域（MockClock 下为
    /// 虚拟时间），无法换算回真实墙钟；在事件发生点直接记录墙钟时间戳，
    /// 避免 get_stats 用「虚拟时长」倒推 `Utc::now() - duration` 产生错误时间。
    last_failure_time_utc: Arc<RwLock<Option<chrono::DateTime<chrono::Utc>>>>,
    /// 最后状态变更时间（墙钟）
    last_state_change_utc: Arc<RwLock<Option<chrono::DateTime<chrono::Utc>>>>,
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
            last_failure_time_utc: Arc::new(RwLock::new(None)),
            last_state_change_utc: Arc::new(RwLock::new(Some(chrono::Utc::now()))),
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
    /// - `Err(LimiteronError)`: 操作失败或熔断器打开
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
    ///     Ok::<(), limiteron::error::LimiteronError>(())
    /// }).await;
    /// # }
    /// ```
    pub async fn execute<F, Fut, T>(&self, operation: F) -> Result<T, LimiteronError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, LimiteronError>>,
    {
        // 注意：total_calls 只统计实际执行的操作（慢调用率的分母）。
        // 被熔断/限流拒绝的调用不进入分母——否则它们会稀释慢调用率，
        // 持续推迟慢调用熔断的触发。

        // 检查熔断器状态
        let state = self.state.read().await;

        // 标记本次调用是否作为"探针"（半开态准入受 half_open_max_calls 限制）
        let mut half_open_probe = false;

        match *state {
            CircuitState::Open => {
                // 检查是否可以尝试恢复
                let last_failure = self.last_failure_time.read().await;
                if let Some(last_failure) = *last_failure {
                    if self.clock.now().duration_since(last_failure) >= self.config.timeout {
                        // 超时到期：尝试切换到半开状态。
                        // 判定与写入在同一写锁内（diting Medium：旧实现
                        // 「读锁检查 + 无守卫写入」的 TOCTOU 会让并发的
                        // 第二个转换重复执行 finalize，把 half_open_calls
                        // 清零而部分击穿 B1 的精确准入）。
                        drop(state);
                        if self.transition_to_half_open_if_open().await {
                            half_open_probe = true;
                        } else if *self.state.read().await == CircuitState::HalfOpen {
                            // 他人已完成切换：本调用者同为恢复流量，按探针准入
                            half_open_probe = true;
                        }
                        // 否则状态已漂移（如 Closed），按非探针正常路径继续
                    } else {
                        // 仍在熔断状态，拒绝请求
                        drop(state);
                        warn!("熔断器打开，拒绝请求");
                        return Err(LimiteronError::LimitError(
                            "熔断器打开，请求被拒绝".to_string(),
                        ));
                    }
                } else {
                    // 无失败时间戳（不应出现在 Open 态），保守拒绝
                    drop(state);
                    warn!("熔断器打开，拒绝请求");
                    return Err(LimiteronError::LimitError(
                        "熔断器打开，请求被拒绝".to_string(),
                    ));
                }
            }
            CircuitState::HalfOpen => {
                half_open_probe = true;
                drop(state);
            }
            CircuitState::Closed => {
                // 正常状态，继续执行
                drop(state);
            }
        }

        // 半开准入检查：对 Open 超时转来的调用者与 HalfOpen 调用者一视同仁，
        // 只放行 half_open_max_calls 个并发探针，其余拒绝。
        // CAS 原子准入：load + fetch_add 分离会让并发探针超额进入（B1）。
        if half_open_probe {
            loop {
                let calls = self.half_open_calls.load(Ordering::Relaxed);
                if calls >= self.config.half_open_max_calls {
                    warn!("半开状态调用次数已达上限，拒绝请求");
                    return Err(LimiteronError::LimitError(
                        "半开状态调用次数已达上限".to_string(),
                    ));
                }
                match self.half_open_calls.compare_exchange(
                    calls,
                    calls + 1,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(_) => std::hint::spin_loop(), // 被并发抢占，重读后重试
                }
            }
        }

        // 执行操作（仅已获准的调用计入 total_calls 分母）
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        let start_time = self.clock.now();
        let result = operation().await;
        let elapsed = start_time.elapsed();

        // 记录调用时长并检查是否为慢调用
        self.record_call_duration(elapsed).await;

        // 根据操作结果更新状态（携带探针标记：探针的失败/成功归属
        // 不应受准入后状态漂移影响）
        match result {
            Ok(value) => {
                self.on_success().await;
                Ok(value)
            }
            Err(e) => {
                self.on_failure_probe_aware(&e, half_open_probe).await;
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
                    // 达到成功阈值，切换到关闭状态。
                    // 仅在仍为 HalfOpen 时关闭（B4）：并发探针失败可能已把
                    // 状态重新切到 Open，陈旧的成功不得把 Open 强行转为 Closed
                    // 造成"故障刚触发熔断即被误关闭"。
                    drop(state);
                    self.transition_to_closed_if_half_open().await;
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

    /// 操作失败时的处理（探针感知）
    ///
    /// `was_probe` 表示本次失败来自半开准入的探针调用。探针失败是
    /// 后端仍处于故障状态的确证：即使准入后状态被并发的其他探针成功
    /// 漂移回 Closed，也必须重新熔断，而非按 Closed 计数等待阈值（B3）。
    async fn on_failure_probe_aware(&self, error: &LimiteronError, was_probe: bool) {
        // 使用错误分类器判断是否应该计入失败计数
        if !self.config.error_classifier.is_counted_as_failure(error) {
            trace!("错误不计入失败计数: {:?}", error);
            return;
        }

        let state = self.state.read().await;

        match *state {
            CircuitState::Closed => {
                if was_probe {
                    drop(state);
                    warn!("半开探针失败（期间状态已漂移至 Closed），重新熔断");
                    self.transition_to_open().await;
                    return;
                }
                // 关闭状态下，增加失败计数
                let failure_count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;

                // 记录失败时间
                *self.last_failure_time.write().await = Some(self.clock.now());
                *self.last_failure_time_utc.write().await = Some(chrono::Utc::now());

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
        self.finalize_transition(old_state, new_state).await;
    }

    /// 仅当当前仍为 `CircuitState::HalfOpen` 时原子地切换到 Closed
    ///
    /// 状态判定与写入在同一写锁内完成，关闭半开判定与切换之间的
    /// TOCTOU 窗口。
    async fn transition_to_closed_if_half_open(&self) -> bool {
        let old_state = {
            let mut state = self.state.write().await;
            if *state != CircuitState::HalfOpen {
                return false;
            }
            let old = *state;
            *state = CircuitState::Closed;
            old
        };
        self.finalize_transition(old_state, CircuitState::Closed)
            .await;
        true
    }

    /// 状态写入后的收尾：时间戳、计数器重置与事件发射
    async fn finalize_transition(&self, old_state: CircuitState, new_state: CircuitState) {
        *self.last_state_change.write().await = Some(self.clock.now());
        *self.last_state_change_utc.write().await = Some(chrono::Utc::now());

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
                // 重置半开状态调用计数为 0：所有探针（含本次触发的过渡请求）
                // 统一经过 execute() 的半开准入检查来计数，受 half_open_max_calls 限制。
                self.half_open_calls.store(0, Ordering::Relaxed);
                info!("熔断器状态变更: {:?} -> HalfOpen", old_state);
            }
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::Relaxed);
                self.success_count.store(0, Ordering::Relaxed);
                self.half_open_calls.store(0, Ordering::Relaxed);
                self.slow_call_count.store(0, Ordering::Relaxed);
                // 同步重置 total_calls（B2）：它作为慢调用率分母，跨熔断周期
                // 单调增长会持续稀释慢调用率，延迟/阻碍慢调用熔断触发
                self.total_calls.store(0, Ordering::Relaxed);
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

    /// 仅当当前仍为 `CircuitState::Open` 时原子地切换到 HalfOpen
    ///
    /// 与 [`Self::transition_to_closed_if_half_open`] 同治（diting Medium）：
    /// 判定与写入在同一写锁内，防止并发调用者的第二个 Open→HalfOpen
    /// 转换重复执行 finalize 而把 `half_open_calls` 清零、部分击穿
    /// half-open 的精确准入。
    async fn transition_to_half_open_if_open(&self) -> bool {
        let old_state = {
            let mut state = self.state.write().await;
            if *state != CircuitState::Open {
                return false;
            }
            let old = *state;
            *state = CircuitState::HalfOpen;
            old
        };
        self.finalize_transition(old_state, CircuitState::HalfOpen)
            .await;
        true
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
        *self.last_failure_time_utc.write().await = None;
        *self.last_state_change.write().await = Some(self.clock.now());
        *self.last_state_change_utc.write().await = Some(chrono::Utc::now());
        self.half_open_calls.store(0, Ordering::Relaxed);
    }

    /// 获取统计信息
    pub async fn get_stats(&self) -> CircuitBreakerStats {
        let state = *self.state.read().await;
        let last_failure = self.last_failure_time_utc.read().await;
        let last_state_change = self.last_state_change_utc.read().await;

        CircuitBreakerStats {
            state,
            failure_count: self.failure_count.load(Ordering::Relaxed),
            success_count: self.success_count.load(Ordering::Relaxed),
            total_calls: self.total_calls.load(Ordering::Relaxed),
            // 直接读取事件发生点的墙钟时间戳（B5）：旧的「时钟域时长
            // 倒推墙钟」在 MockClock（虚拟时间）下产生错误时间
            last_failure_time: *last_failure,
            last_state_change: *last_state_change,
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
            .execute(|| async { Ok::<(), LimiteronError>(()) })
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
                Err::<(), LimiteronError>(LimiteronError::BanError("test error".to_string()))
            })
            .await;
        assert!(result.is_err());

        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 1);
        assert!(breaker.is_closed().await);

        // 第二次失败
        let result = breaker
            .execute(|| async {
                Err::<(), LimiteronError>(LimiteronError::BanError("test error".to_string()))
            })
            .await;
        assert!(result.is_err());

        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 2);
        assert!(breaker.is_closed().await);

        // 第三次失败，应该触发熔断
        let result = breaker
            .execute(|| async {
                Err::<(), LimiteronError>(LimiteronError::BanError("test error".to_string()))
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
                    Err::<(), LimiteronError>(LimiteronError::BanError("test error".to_string()))
                })
                .await;
        }

        assert!(breaker.is_open().await);

        // 熔断器打开，请求应该被拒绝
        let result = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
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
                    Err::<(), LimiteronError>(LimiteronError::BanError("test error".to_string()))
                })
                .await;
        }

        assert!(breaker.is_open().await);

        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 第一次成功，进入半开状态
        let result = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(breaker.is_half_open().await);

        // 第二次成功，应该恢复到关闭状态
        let result = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
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
                    Err::<(), LimiteronError>(LimiteronError::BanError("test error".to_string()))
                })
                .await;
        }

        assert!(breaker.is_open().await);

        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 第一次成功，进入半开状态
        let result = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(breaker.is_half_open().await);

        // 再次失败，应该回到打开状态
        let result = breaker
            .execute(|| async {
                Err::<(), LimiteronError>(LimiteronError::BanError("test error".to_string()))
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
                    Err::<(), LimiteronError>(LimiteronError::BanError("test error".to_string()))
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
            .execute(|| async { Ok::<(), LimiteronError>(()) })
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
                    Err::<(), LimiteronError>(LimiteronError::BanError("test error".to_string()))
                })
                .await;
        }

        assert!(breaker.is_open().await);

        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 第一次调用，进入半开状态
        let result = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
            .await;
        assert!(result.is_ok());

        // 第二次调用，达到上限
        let result = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
            .await;
        assert!(result.is_ok());

        // 第三次调用，应该被拒绝
        let result = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
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
                Err::<(), LimiteronError>(LimiteronError::BanError("error 1".to_string()))
            })
            .await;
        assert!(result.is_err());
        assert!(breaker.is_closed().await, "第一次失败后仍应为 Closed");

        // 第二次失败
        let result = breaker
            .execute(|| async {
                Err::<(), LimiteronError>(LimiteronError::BanError("error 2".to_string()))
            })
            .await;
        assert!(result.is_err());
        assert!(breaker.is_closed().await, "第二次失败后仍应为 Closed");

        // 第三次失败，应触发熔断
        let result = breaker
            .execute(|| async {
                Err::<(), LimiteronError>(LimiteronError::BanError("error 3".to_string()))
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
                    Err::<(), LimiteronError>(LimiteronError::BanError(format!("error {}", i)))
                })
                .await;
        }
        assert!(breaker.is_open().await, "应处于 Open 状态");

        // 未超时时请求应被拒绝
        let result = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
            .await;
        assert!(result.is_err());
        assert!(breaker.is_open().await, "未超时应保持 Open 状态");

        // 等待超时
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 超时后第一次请求应进入 HalfOpen 状态
        let result = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
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
                    Err::<(), LimiteronError>(LimiteronError::BanError("error".to_string()))
                })
                .await;
        }
        assert!(breaker.is_open().await);

        // 等待超时进入半开状态
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 第一次成功，进入半开状态
        let result = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(breaker.is_half_open().await);

        // 第二次成功，应恢复到 Closed 状态
        let result = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
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
                    Err::<(), LimiteronError>(LimiteronError::BanError("error".to_string()))
                })
                .await;
        }
        assert!(breaker.is_open().await, "阶段1: 应处于 Open 状态");

        // 阶段2: Open → HalfOpen
        tokio::time::sleep(Duration::from_millis(150)).await;
        let result = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
            .await;
        assert!(result.is_ok());
        assert!(breaker.is_half_open().await, "阶段2: 应处于 HalfOpen 状态");

        // 阶段3: HalfOpen → Closed
        let result = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
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
        let error = LimiteronError::StorageError(crate::error::StorageError::TimeoutError(
            "timeout".into(),
        ));
        assert!(classifier.is_counted_as_failure(&error));
    }

    /// 测试默认错误分类器 - StorageError 连接错误算失败
    #[test]
    fn test_default_error_classifier_connection_error() {
        let classifier = DefaultErrorClassifier;
        let error = LimiteronError::StorageError(crate::error::StorageError::ConnectionError(
            "connection".into(),
        ));
        assert!(classifier.is_counted_as_failure(&error));
    }

    /// 测试默认错误分类器 - LimitError 不算失败
    #[test]
    fn test_default_error_classifier_limit_error() {
        let classifier = DefaultErrorClassifier;
        let error = LimiteronError::LimitError("rate limited".into());
        assert!(!classifier.is_counted_as_failure(&error));
    }

    /// 测试默认错误分类器 - ValidationError 不算失败
    #[test]
    fn test_default_error_classifier_validation_error() {
        let classifier = DefaultErrorClassifier;
        let error = LimiteronError::ValidationError("invalid input".into());
        assert!(!classifier.is_counted_as_failure(&error));
    }

    /// 测试默认错误分类器 - CircuitBreakerError 不算失败
    #[test]
    fn test_default_error_classifier_circuit_breaker_error() {
        let classifier = DefaultErrorClassifier;
        let error = LimiteronError::CircuitBreakerError("circuit open".into());
        assert!(!classifier.is_counted_as_failure(&error));
    }

    /// 测试默认错误分类器 - 其他错误算失败
    #[test]
    fn test_default_error_classifier_other_errors() {
        let classifier = DefaultErrorClassifier;
        let error = LimiteronError::Other("unknown error".into());
        assert!(classifier.is_counted_as_failure(&error));
    }

    /// 测试自定义错误分类器
    #[tokio::test]
    async fn test_custom_error_classifier() {
        #[derive(Debug)]
        struct CustomClassifier;
        impl ErrorClassifier for CustomClassifier {
            fn is_counted_as_failure(&self, error: &LimiteronError) -> bool {
                // 只有 StorageError 算失败
                matches!(error, LimiteronError::StorageError(_))
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
                Err::<(), LimiteronError>(LimiteronError::ValidationError("test".into()))
            })
            .await;

        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 0);

        // StorageError 应该触发失败计数
        let _ = breaker
            .execute(|| async {
                Err::<(), LimiteronError>(LimiteronError::StorageError(
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
            .execute(|| async { Ok::<(), LimiteronError>(()) })
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
                .execute(|| async { Ok::<(), LimiteronError>(()) })
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
            .execute(|| async { Err::<(), LimiteronError>(LimiteronError::BanError("e".into())) })
            .await;
        assert!(breaker.is_open().await);

        // Next call should be rejected (still open, not timed out)
        let result = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
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
            .execute(|| async { Err::<(), LimiteronError>(LimiteronError::BanError("e".into())) })
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
        let error = LimiteronError::StorageError(crate::error::StorageError::NotFound("nf".into()));
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
                    Err::<(), LimiteronError>(LimiteronError::BanError("e".to_string()))
                })
                .await;
        }
        assert!(breaker.is_open().await);

        // 直接调用 on_failure_probe_aware（非探针），覆盖 Open 分支
        // 此时状态为 Open，on_failure 内的 Open 分支会打印 warn 但不做状态转换
        let error = LimiteronError::BanError("open-state failure".to_string());
        breaker.on_failure_probe_aware(&error, false).await;

        // 状态应仍为 Open
        assert!(breaker.is_open().await);
    }

    #[tokio::test]
    async fn test_half_open_admission_cas_no_overshoot() {
        // B1 回归：半开准入的 load + fetch_add 分离会让并发探针超额进入。
        // CAS 原子准入后，放行数必须精确等于 half_open_max_calls。
        let config =
            CircuitBreakerConfig::new(1, 100, Duration::from_millis(50)).half_open_max_calls(3);
        let breaker = Arc::new(CircuitBreaker::new(config));

        // 触发熔断（failure_threshold=1）
        let _ = breaker
            .execute(|| async {
                Err::<(), LimiteronError>(LimiteronError::BanError("boom".to_string()))
            })
            .await;
        assert!(breaker.is_open().await);

        // 等待超时到期，状态可转为 HalfOpen
        tokio::time::sleep(Duration::from_millis(80)).await;

        // 16 个并发请求同时冲击半开准入
        let tasks = 16usize;
        let barrier = Arc::new(tokio::sync::Barrier::new(tasks));
        let allowed = Arc::new(std::sync::atomic::AtomicU64::new(0));

        let mut handles = Vec::with_capacity(tasks);
        for _ in 0..tasks {
            let breaker = Arc::clone(&breaker);
            let barrier = Arc::clone(&barrier);
            let allowed = Arc::clone(&allowed);
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                let result = breaker
                    .execute(|| async {
                        // 稍作停留，制造准入窗口
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        Ok::<(), LimiteronError>(())
                    })
                    .await;
                if result.is_ok() {
                    allowed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(
            allowed.load(std::sync::atomic::Ordering::SeqCst),
            3,
            "半开准入必须精确限制在 half_open_max_calls"
        );
    }

    #[tokio::test]
    async fn test_total_calls_excludes_rejected_requests() {
        // B2 回归：被熔断拒绝的调用不得计入 total_calls（慢调用率分母），
        // 否则会持续稀释慢调用率、阻碍慢调用熔断。
        let config = CircuitBreakerConfig::new(1, 1, Duration::from_secs(3600));
        let breaker = CircuitBreaker::new(config);

        // 一次成功调用 → total_calls = 1
        let _ = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
            .await;
        assert_eq!(breaker.get_stats().await.total_calls, 1);

        // 触发熔断（长 timeout，期间不会转半开）
        let _ = breaker
            .execute(|| async {
                Err::<(), LimiteronError>(LimiteronError::BanError("boom".to_string()))
            })
            .await;
        assert!(breaker.is_open().await);
        let total_after_open = breaker.get_stats().await.total_calls;

        // 熔断期间的拒绝调用不增加分母
        let rejected = breaker
            .execute(|| async { Ok::<(), LimiteronError>(()) })
            .await;
        assert!(rejected.is_err());
        assert_eq!(
            breaker.get_stats().await.total_calls,
            total_after_open,
            "被熔断拒绝的调用不得计入 total_calls"
        );
    }

    #[tokio::test]
    async fn test_transition_to_closed_only_from_half_open() {
        // B4 回归：陈旧的成功不得把 Open 强行转为 Closed。
        // transition_to_closed_if_half_open 仅在 HalfOpen 态生效。
        let breaker = CircuitBreaker::new(CircuitBreakerConfig::new(2, 1, Duration::from_secs(60)));

        // 强制进入 Open（模拟并发探针失败刚触发的熔断）
        *breaker.state.write().await = CircuitState::Open;
        assert!(breaker.is_open().await);

        let transitioned = breaker.transition_to_closed_if_half_open().await;
        assert!(!transitioned, "Open 态不得经条件关闭转为 Closed");
        assert!(breaker.is_open().await, "状态必须保持 Open");
    }
}
