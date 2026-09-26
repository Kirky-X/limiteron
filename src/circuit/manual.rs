// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 手动记录式熔断器
//!
//! 与 [`CircuitBreaker`](super::types::CircuitBreaker) 的 execute 包装形态不同，
//! 本模块只提供状态机与记录接口，不包裹被保护操作的执行：调用方自行决定什么
//! 算成功/失败（业务语义——例如 HTTP 200 但响应体携带错误码），并可在 spawn 出
//! 的后台任务中上报结果（流式响应场景）。为此 [`ManualCircuitBreaker`] 内部全部
//! 走 `Arc` 共享，`Clone` 廉价且与原实例共享同一状态，并保证 `Send + Sync`。
//!
//! # 状态机
//!
//! - **Closed**：正常放行；累计请求达到 `min_requests` 且失败比例
//!   `>= failure_threshold` 时转 Open。
//! - **Open**：冷却 `half_open_interval` 后（在 `state()`/`admit()` 调用时惰性
//!   转换）进入 HalfOpen。
//! - **HalfOpen**：连续 `half_open_max_successes` 次成功回 Closed；任一次失败
//!   立即回 Open 并重新盖章冷却起点。
//!
//! # 示例
//!
//! ```rust
//! use limiteron::circuit::{ManualCircuitBreaker, ManualCircuitBreakerConfig};
//!
//! # async fn example() {
//! let breaker = ManualCircuitBreaker::new(ManualCircuitBreakerConfig::default());
//!
//! if breaker.admit().await {
//!     match call_downstream().await {
//!         Ok(v) => breaker.record_success().await,
//!         Err(_) => breaker.record_failure().await,
//!     }
//! }
//! # async fn call_downstream() -> Result<(), ()> { Ok(()) }
//! # }
//! ```

use crate::clock::{Clock, SystemClock};
use crate::constants::{
    DEFAULT_MANUAL_CIRCUIT_BREAKER_FAILURE_THRESHOLD,
    DEFAULT_MANUAL_CIRCUIT_BREAKER_HALF_OPEN_INTERVAL_SECS,
    DEFAULT_MANUAL_CIRCUIT_BREAKER_HALF_OPEN_MAX_SUCCESSES,
    DEFAULT_MANUAL_CIRCUIT_BREAKER_MIN_REQUESTS,
};
use crate::error::CircuitState;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// 手动记录式熔断器配置
///
/// 与计数触发式的 [`CircuitBreakerConfig`](super::types::CircuitBreakerConfig)
/// 语义不同：`failure_threshold` 是失败比例（0.0–1.0），且需同时满足
/// `min_requests` 样本量门槛才参与评估。
#[derive(Debug, Clone)]
pub struct ManualCircuitBreakerConfig {
    /// 触发 Open 的失败比例（0.0–1.0）
    pub failure_threshold: f64,
    /// 评估比例前所需的累计请求样本量
    pub min_requests: u64,
    /// Open 态冷却时长，到期后惰性转 HalfOpen
    pub half_open_interval: Duration,
    /// HalfOpen 态回 Closed 所需的连续成功次数
    pub half_open_max_successes: u64,
}

impl Default for ManualCircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: DEFAULT_MANUAL_CIRCUIT_BREAKER_FAILURE_THRESHOLD,
            min_requests: DEFAULT_MANUAL_CIRCUIT_BREAKER_MIN_REQUESTS,
            half_open_interval: Duration::from_secs(
                DEFAULT_MANUAL_CIRCUIT_BREAKER_HALF_OPEN_INTERVAL_SECS,
            ),
            half_open_max_successes: DEFAULT_MANUAL_CIRCUIT_BREAKER_HALF_OPEN_MAX_SUCCESSES,
        }
    }
}

/// 共享内核：全部可变状态集中于单个 `Arc`，克隆即共享同一熔断器
struct ManualCircuitBreakerInner {
    config: ManualCircuitBreakerConfig,
    state: Mutex<CircuitState>,
    /// 进入 Open 的时刻，HalfOpen 冷却判定基准
    opened_at: Mutex<Option<Instant>>,
    failures: AtomicU64,
    successes: AtomicU64,
    total: AtomicU64,
    half_open_successes: AtomicU64,
}

/// 手动记录式熔断器
///
/// 成败语义由调用方定义，通过 [`record_success`](Self::record_success) /
/// [`record_failure`](Self::record_failure) 手动上报；`Clone` 与原实例共享状态，
/// 可移入 spawn 的后台任务上报流式结果。
#[derive(Clone)]
pub struct ManualCircuitBreaker {
    inner: Arc<ManualCircuitBreakerInner>,
    clock: Arc<dyn Clock>,
}

impl ManualCircuitBreaker {
    /// 创建熔断器（默认真实系统时钟）
    pub fn new(config: ManualCircuitBreakerConfig) -> Self {
        Self::with_clock(config, Arc::new(SystemClock))
    }

    /// 创建熔断器并注入时钟（测试注入 `MockClock`）
    pub fn with_clock(config: ManualCircuitBreakerConfig, clock: Arc<dyn Clock>) -> Self {
        Self {
            inner: Arc::new(ManualCircuitBreakerInner {
                config,
                state: Mutex::new(CircuitState::Closed),
                opened_at: Mutex::new(None),
                failures: AtomicU64::new(0),
                successes: AtomicU64::new(0),
                total: AtomicU64::new(0),
                half_open_successes: AtomicU64::new(0),
            }),
            clock,
        }
    }

    /// 当前熔断状态（惰性执行 Open→HalfOpen 冷却转换）
    pub async fn state(&self) -> CircuitState {
        let mut state = self.inner.state.lock().await;
        self.maybe_transition(&mut state).await;
        *state
    }

    /// 通行判定：Open 态（冷却未到期）拒绝，其余状态放行
    pub async fn admit(&self) -> bool {
        self.state().await != CircuitState::Open
    }

    /// 记录一次成功调用
    pub async fn record_success(&self) {
        self.inner.successes.fetch_add(1, Ordering::Relaxed);
        self.inner.total.fetch_add(1, Ordering::Relaxed);

        let mut state = self.inner.state.lock().await;
        if *state == CircuitState::HalfOpen {
            let count = self
                .inner
                .half_open_successes
                .fetch_add(1, Ordering::Relaxed)
                + 1;
            if count >= self.inner.config.half_open_max_successes {
                self.reset_counters();
                *state = CircuitState::Closed;
            }
        }
    }

    /// 记录一次失败调用
    pub async fn record_failure(&self) {
        self.inner.failures.fetch_add(1, Ordering::Relaxed);
        self.inner.total.fetch_add(1, Ordering::Relaxed);

        let mut state = self.inner.state.lock().await;
        if *state == CircuitState::HalfOpen {
            self.open(&mut state).await;
        } else if *state == CircuitState::Closed {
            self.maybe_trip(&mut state).await;
        }
    }

    async fn maybe_transition(&self, state: &mut CircuitState) {
        if *state == CircuitState::Open {
            let opened_at = self.inner.opened_at.lock().await;
            if let Some(instant) = *opened_at
                && self.clock.now().duration_since(instant) >= self.inner.config.half_open_interval
            {
                self.inner.half_open_successes.store(0, Ordering::Relaxed);
                *state = CircuitState::HalfOpen;
            }
        }
    }

    async fn maybe_trip(&self, state: &mut CircuitState) {
        let total = self.inner.total.load(Ordering::Relaxed);
        if total < self.inner.config.min_requests {
            return;
        }
        let failures = self.inner.failures.load(Ordering::Relaxed);
        let ratio = failures as f64 / total as f64;
        if ratio >= self.inner.config.failure_threshold {
            self.open(state).await;
        }
    }

    // 每次进入 Open 都必须盖章 opened_at，否则惰性冷却检查读到 None，
    // 熔断器永远无法进入 HalfOpen。
    async fn open(&self, state: &mut CircuitState) {
        *state = CircuitState::Open;
        *self.inner.opened_at.lock().await = Some(self.clock.now());
    }

    fn reset_counters(&self) {
        self.inner.failures.store(0, Ordering::Relaxed);
        self.inner.successes.store(0, Ordering::Relaxed);
        self.inner.total.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;
    use std::time::Duration;

    fn mock_breaker(config: ManualCircuitBreakerConfig) -> (ManualCircuitBreaker, Arc<MockClock>) {
        let mock = Arc::new(MockClock::new());
        let breaker = ManualCircuitBreaker::with_clock(config, mock.clone() as Arc<dyn Clock>);
        (breaker, mock)
    }

    #[test]
    fn default_config() {
        let config = ManualCircuitBreakerConfig::default();
        assert!((config.failure_threshold - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.min_requests, 10);
        assert_eq!(config.half_open_interval, Duration::from_secs(30));
        assert_eq!(config.half_open_max_successes, 3);
    }

    #[tokio::test]
    async fn starts_closed() {
        let (cb, _clock) = mock_breaker(ManualCircuitBreakerConfig::default());
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn trips_on_threshold() {
        let config = ManualCircuitBreakerConfig {
            min_requests: 5,
            failure_threshold: 0.5,
            ..Default::default()
        };
        let (cb, _clock) = mock_breaker(config);

        for _ in 0..5 {
            cb.record_failure().await;
        }
        assert_eq!(cb.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn trips_exactly_at_ratio_boundary() {
        // 比例判定为 >=：恰好达到阈值即触发
        let config = ManualCircuitBreakerConfig {
            min_requests: 4,
            failure_threshold: 0.5,
            ..Default::default()
        };
        let (cb, _clock) = mock_breaker(config);

        for _ in 0..2 {
            cb.record_success().await;
        }
        for _ in 0..2 {
            cb.record_failure().await;
        }
        assert_eq!(cb.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn stays_closed_below_threshold() {
        let config = ManualCircuitBreakerConfig {
            min_requests: 10,
            failure_threshold: 0.5,
            ..Default::default()
        };
        let (cb, _clock) = mock_breaker(config);

        for _ in 0..6 {
            cb.record_success().await;
        }
        for _ in 0..3 {
            cb.record_failure().await;
        }
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn below_min_requests_never_trips() {
        // 样本量不足时即使 100% 失败也不评估比例
        let config = ManualCircuitBreakerConfig {
            min_requests: 5,
            failure_threshold: 0.5,
            ..Default::default()
        };
        let (cb, _clock) = mock_breaker(config);

        for _ in 0..4 {
            cb.record_failure().await;
        }
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn recovers_through_half_open_after_cooldown() {
        // 回归守卫：比例触发的 Open 必须盖章 opened_at，否则惰性冷却检查
        // 永不生效，熔断器将卡死在 Open。
        let config = ManualCircuitBreakerConfig {
            min_requests: 2,
            failure_threshold: 0.5,
            ..Default::default()
        };
        let interval = config.half_open_interval;
        let (cb, clock) = mock_breaker(config);

        for _ in 0..2 {
            cb.record_failure().await;
        }
        assert_eq!(cb.state().await, CircuitState::Open);

        clock.advance(interval + Duration::from_millis(50));
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        for _ in 0..3 {
            cb.record_success().await;
        }
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn half_open_probe_failure_reopens() {
        // 探针失败即回 Open，且重新盖章冷却起点（再次冷却到期可再进 HalfOpen）
        let config = ManualCircuitBreakerConfig {
            min_requests: 2,
            failure_threshold: 0.5,
            ..Default::default()
        };
        let interval = config.half_open_interval;
        let (cb, clock) = mock_breaker(config);

        for _ in 0..2 {
            cb.record_failure().await;
        }
        assert_eq!(cb.state().await, CircuitState::Open);

        clock.advance(interval + Duration::from_millis(50));
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);

        // 重新盖章的冷却到期后再次进入 HalfOpen
        clock.advance(interval + Duration::from_millis(50));
        assert_eq!(cb.state().await, CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn admit_gates_while_open() {
        let config = ManualCircuitBreakerConfig {
            min_requests: 2,
            failure_threshold: 0.5,
            ..Default::default()
        };
        let interval = config.half_open_interval;
        let (cb, clock) = mock_breaker(config);

        assert!(cb.admit().await, "Closed 态必须放行");

        for _ in 0..2 {
            cb.record_failure().await;
        }
        assert!(!cb.admit().await, "Open 冷却未到期必须拒绝");

        clock.advance(interval + Duration::from_millis(50));
        assert!(cb.admit().await, "冷却到期进 HalfOpen 后放行探针");
    }

    #[tokio::test]
    async fn clone_shares_state_and_reports_from_spawned_task() {
        // 克隆廉价且与原实例共享状态；后台任务（spawn 移入克隆）可上报成败
        let config = ManualCircuitBreakerConfig {
            min_requests: 3,
            failure_threshold: 0.5,
            half_open_interval: Duration::from_secs(3600),
            ..Default::default()
        };
        let (cb, _clock) = mock_breaker(config);
        let cloned = cb.clone();

        let handle = tokio::spawn(async move {
            for _ in 0..3 {
                cloned.record_failure().await;
            }
        });
        handle.await.unwrap();

        assert_eq!(cb.state().await, CircuitState::Open);
    }
}
