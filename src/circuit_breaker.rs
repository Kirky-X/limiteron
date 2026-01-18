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

/// 默认失败阈值
pub const DEFAULT_FAILURE_THRESHOLD: u64 = 5;

/// 默认成功阈值
pub const DEFAULT_SUCCESS_THRESHOLD: u64 = 2;

/// 默认超时时间（1分钟）
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// 默认半开状态最大调用数
pub const DEFAULT_HALF_OPEN_MAX_CALLS: u64 = 3;

use crate::error::{CircuitBreakerStats, CircuitState, FlowGuardError};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, trace, warn};

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
            failure_threshold: DEFAULT_FAILURE_THRESHOLD,
            success_threshold: DEFAULT_SUCCESS_THRESHOLD,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            half_open_max_calls: DEFAULT_HALF_OPEN_MAX_CALLS,
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
            half_open_max_calls: 3,
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

impl CircuitBreaker {
    /// 创建新的熔断器
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
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(CircuitBreakerConfig::default())
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

    /// 切换到打开状态
    async fn transition_to_open(&self) {
        let old_state = *self.state.read().await;
        if old_state != CircuitState::Open {
            *self.state.write().await = CircuitState::Open;
            *self.last_state_change.write().await = Some(Instant::now());
            self.success_count.store(0, Ordering::Relaxed);
            self.half_open_calls.store(0, Ordering::Relaxed);
            warn!(
                "熔断器状态变更: {:?} -> Open (failure_count={})",
                old_state,
                self.failure_count.load(Ordering::Relaxed)
            );
        }
    }

    /// 切换到半开状态
    async fn transition_to_half_open(&self) {
        let old_state = *self.state.read().await;
        if old_state != CircuitState::HalfOpen {
            *self.state.write().await = CircuitState::HalfOpen;
            *self.last_state_change.write().await = Some(Instant::now());
            self.success_count.store(0, Ordering::Relaxed);
            // 重置半开状态调用计数
            // 注意：将计数设置为1，因为当前请求（探针请求）将被允许通过
            self.half_open_calls.store(1, Ordering::Relaxed);
            info!("熔断器状态变更: {:?} -> HalfOpen", old_state);
        }
    }

    /// 切换到关闭状态
    async fn transition_to_closed(&self) {
        let old_state = *self.state.read().await;
        if old_state != CircuitState::Closed {
            *self.state.write().await = CircuitState::Closed;
            *self.last_state_change.write().await = Some(Instant::now());
            self.failure_count.store(0, Ordering::Relaxed);
            self.success_count.store(0, Ordering::Relaxed);
            self.half_open_calls.store(0, Ordering::Relaxed);
            info!("熔断器状态变更: {:?} -> Closed", old_state);
        }
    }

    /// 检查熔断器是否打开
    pub async fn is_open(&self) -> bool {
        let state = self.state.read().await;
        *state == CircuitState::Open
    }

    /// 检查熔断器是否半开
    pub async fn is_half_open(&self) -> bool {
        let state = self.state.read().await;
        *state == CircuitState::HalfOpen
    }

    /// 检查熔断器是否关闭
    pub async fn is_closed(&self) -> bool {
        let state = self.state.read().await;
        *state == CircuitState::Closed
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
        assert_eq!(config.success_threshold, 2);
        assert_eq!(config.timeout, Duration::from_secs(60));
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
}
