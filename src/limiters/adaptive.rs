// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 自适应并发限流器（T610，feature `adaptive-limiting`）
//!
//! AIMD（Additive Increase / Multiplicative Decrease）并发窗口：
//! 以「调整窗口」内的成功/错误/慢调用率为信号动态升降并发上限，
//! 真正落地 `adaptive-limiting` feature（此前为 no-op 兼容占位）。
//!
//! - **加性增**：窗口内零错误零慢调用 → `current_max += increase_step`（封顶
//!   `max_limit`）
//! - **乘性减**：窗口内错误率+慢调用率达到 `degrade_rate` →
//!   `current_max = max(current_max × decrease_factor, min_max)`
//! - **熔断联动**（`circuit-breaker` feature）：注入 [`CircuitBreaker`] 后，
//!   熔断打开期间直接拒绝（计错误信号），半开/恢复后窗口自然回升
//!
//! 与 [`ConcurrencyLimiter`](super::ConcurrencyLimiter) 不同，本限流器
//! **没有固定许可数**：`current_max` 是可调窗口，允许「租约」式占用
//! （`acquire` 显式获取 / [`AdaptivePermit`] Drop 自动释放）。
//!
//! # Example
//!
//! ```rust
//! use limiteron::limiters::adaptive::{AdaptiveConcurrencyConfig, AdaptiveConcurrencyLimiter};
//! use std::time::Duration;
//!
//! let limiter = AdaptiveConcurrencyLimiter::new(AdaptiveConcurrencyConfig {
//!     initial_max: 8,
//!     min_max: 2,
//!     max_limit: 64,
//!     window_size: 10,
//!     increase_step: 1,
//!     decrease_factor: 0.5,
//!     degrade_rate: 0.5,
//!     slow_threshold: Duration::from_millis(100),
//! });
//!
//! // 反馈驱动窗口：错误 → 收紧；无错成功 → 放宽
//! limiter.record_success(Duration::from_millis(1));
//! limiter.record_error();
//! ```

use crate::error::LimiteronError;
use crate::limiters::traits::Limiter;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Duration;

/// 自适应并发窗口配置
#[derive(Debug, Clone)]
pub struct AdaptiveConcurrencyConfig {
    /// 初始并发上限
    pub initial_max: u64,
    /// 并发下限（乘性减不再低于此值）
    pub min_max: u64,
    /// 并发上限（加性增不再高于此值）
    pub max_limit: u64,
    /// 调整窗口样本数（每 N 个反馈信号评估一次升降）
    pub window_size: u64,
    /// 加性增步长（无错窗口内增加的并发数）
    pub increase_step: u64,
    /// 乘性减系数（降级窗口内 `current_max × factor`，0 < f < 1）
    pub decrease_factor: f64,
    /// 降级阈值：窗口内 (错误 + 慢调用) / 样本数 ≥ 此值时乘性减
    pub degrade_rate: f64,
    /// 慢调用阈值（超过记为 slow 信号）
    pub slow_threshold: Duration,
}

impl Default for AdaptiveConcurrencyConfig {
    fn default() -> Self {
        Self {
            initial_max: 16,
            min_max: 4,
            max_limit: 256,
            window_size: 32,
            increase_step: 1,
            decrease_factor: 0.5,
            degrade_rate: 0.5,
            slow_threshold: Duration::from_millis(200),
        }
    }
}

#[derive(Debug, Default, Clone)]
struct WindowSignals {
    samples: u64,
    errors: u64,
    slow: u64,
    successes: u64,
}

/// AIMD 自适应并发限流器
pub struct AdaptiveConcurrencyLimiter {
    config: AdaptiveConcurrencyConfig,
    /// 当前并发窗口（可调）
    current_max: AtomicU64,
    /// 在途占用计数（Arc：链式租约任务克隆后延迟归还）
    in_flight: Arc<AtomicI64>,
    /// 调整窗口信号（Mutex 保护，临界区极短）
    window: parking_lot::Mutex<WindowSignals>,
    /// 熔断器（可选，联动：打开即拒绝）
    #[cfg(feature = "circuit-breaker")]
    circuit_breaker: Option<Arc<crate::circuit::CircuitBreaker>>,
}

/// 自适应并发占用许可（Drop 自动释放）
pub struct AdaptivePermit {
    limiter: Arc<AdaptiveConcurrencyLimiter>,
    units: u64,
}

impl Drop for AdaptivePermit {
    fn drop(&mut self) {
        self.limiter.release(self.units);
    }
}

impl AdaptiveConcurrencyLimiter {
    /// 创建自适应并发限流器
    pub fn new(config: AdaptiveConcurrencyConfig) -> Self {
        assert!(
            config.decrease_factor > 0.0 && config.decrease_factor < 1.0,
            "decrease_factor must be in (0, 1)"
        );
        assert!(
            config.min_max <= config.initial_max && config.initial_max <= config.max_limit,
            "require min_max <= initial_max <= max_limit"
        );
        let initial = config.initial_max;
        Self {
            config,
            current_max: AtomicU64::new(initial),
            in_flight: Arc::new(AtomicI64::new(0)),
            window: parking_lot::Mutex::new(WindowSignals::default()),
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
        }
    }

    /// 注入熔断器（联动：打开即拒绝并计错误信号）
    #[cfg(feature = "circuit-breaker")]
    pub fn with_circuit_breaker(mut self, cb: Arc<crate::circuit::CircuitBreaker>) -> Self {
        self.circuit_breaker = Some(cb);
        self
    }

    /// 当前并发窗口
    pub fn current_max(&self) -> u64 {
        self.current_max.load(Ordering::Relaxed)
    }

    /// 当前在途占用
    pub fn in_flight(&self) -> i64 {
        self.in_flight.load(Ordering::Relaxed)
    }

    /// 记录成功（含延迟；超慢调用计为 slow 信号）
    pub fn record_success(&self, latency: Duration) {
        let mut window = self.window.lock();
        window.samples += 1;
        window.successes += 1;
        if latency > self.config.slow_threshold {
            window.slow += 1;
        }
        self.maybe_adjust(&mut window);
    }

    /// 记录错误
    pub fn record_error(&self) {
        let mut window = self.window.lock();
        window.samples += 1;
        window.errors += 1;
        self.maybe_adjust(&mut window);
    }

    /// 窗口评估：达到窗口样本数时按 AIMD 升降（并重置窗口）
    fn maybe_adjust(&self, window: &mut WindowSignals) {
        if window.samples < self.config.window_size {
            return;
        }
        let degrade_rate = window.errors + window.slow;
        let bad_rate = degrade_rate as f64 / window.samples as f64;
        let current = self.current_max.load(Ordering::Relaxed);
        let next = if bad_rate >= self.config.degrade_rate {
            // 乘性减：下限保护
            let decreased = (current as f64 * self.config.decrease_factor) as u64;
            decreased.max(self.config.min_max)
        } else if window.errors == 0 {
            // 加性增：完全无错才放宽，上限封顶
            (current + self.config.increase_step).min(self.config.max_limit)
        } else {
            current
        };
        if next != current {
            self.current_max.store(next, Ordering::Relaxed);
        }
        *window = WindowSignals::default();
    }

    /// 尝试占用 `units` 个并发额度（成功返回许可，Drop 自动释放）
    ///
    /// 注：熔断联动在 [`Limiter::allow`]（异步路径）生效；本同步方法不做
    /// 熔断判定（熔断状态读取是异步的）。
    pub fn try_acquire(self: &Arc<Self>, units: u64) -> Option<AdaptivePermit> {
        // CAS 占用：in_flight + units <= current_max
        loop {
            let in_flight = self.in_flight.load(Ordering::Relaxed);
            let max = self.current_max.load(Ordering::Relaxed) as i64;
            if in_flight + units as i64 > max {
                return None;
            }
            match self.in_flight.compare_exchange(
                in_flight,
                in_flight + units as i64,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    return Some(AdaptivePermit {
                        limiter: Arc::clone(self),
                        units,
                    });
                }
                Err(_) => continue,
            }
        }
    }

    /// 释放 `units` 个占用
    pub fn release(&self, units: u64) {
        self.in_flight.fetch_sub(units as i64, Ordering::AcqRel);
    }
}

#[async_trait]
impl Limiter for AdaptiveConcurrencyLimiter {
    /// 并发额度消费：成功占用 `cost` 单位并立即以短租约释放
    /// （与 `ConcurrencyLimiter` 的链式租约语义一致：`allow` 无显式
    /// release 通道，占用在租约到期后自动归还）
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError> {
        if cost == 0 {
            return Err(LimiteronError::ConfigError(
                "Cost cannot be zero".to_string(),
            ));
        }

        // 熔断联动（T610）：打开即拒绝并计错误信号收紧窗口
        #[cfg(feature = "circuit-breaker")]
        if let Some(cb) = &self.circuit_breaker {
            if cb.is_open().await {
                self.record_error();
                return Ok(false);
            }
        }

        loop {
            let in_flight = self.in_flight.load(Ordering::Relaxed);
            let max = self.current_max.load(Ordering::Relaxed) as i64;
            if in_flight + cost as i64 > max {
                self.record_error();
                return Ok(false);
            }
            match self.in_flight.compare_exchange(
                in_flight,
                in_flight + cost as i64,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.record_success(Duration::ZERO);
                    // 链式租约语义（与 ConcurrencyLimiter 一致）：
                    // 有 tokio 运行时 → 短租约后自动归还；否则立即归还
                    if let Ok(handle) = tokio::runtime::Handle::try_current() {
                        let counter = Arc::clone(&self.in_flight);
                        handle.spawn(async move {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            counter.fetch_sub(cost as i64, Ordering::AcqRel);
                        });
                    } else {
                        self.release(cost);
                    }
                    return Ok(true);
                }
                Err(_) => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AdaptiveConcurrencyConfig {
        AdaptiveConcurrencyConfig {
            initial_max: 8,
            min_max: 2,
            max_limit: 16,
            window_size: 4,
            increase_step: 1,
            decrease_factor: 0.5,
            degrade_rate: 0.5,
            slow_threshold: Duration::from_millis(100),
        }
    }

    #[test]
    fn test_t610_initial_window() {
        let limiter = AdaptiveConcurrencyLimiter::new(config());
        assert_eq!(limiter.current_max(), 8);
        assert_eq!(limiter.in_flight(), 0);
    }

    #[test]
    fn test_t610_additive_increase_on_clean_window() {
        let limiter = AdaptiveConcurrencyLimiter::new(config());
        for _ in 0..4 {
            limiter.record_success(Duration::from_millis(1));
        }
        assert_eq!(limiter.current_max(), 9, "无错窗口应加性增 1（8 → 9）");
    }

    #[test]
    fn test_t610_multiplicative_decrease_on_errors() {
        let limiter = AdaptiveConcurrencyLimiter::new(config());
        // 窗口内 2/4 错误 → degrade_rate=0.5 → 乘性减 8 → 4
        limiter.record_success(Duration::from_millis(1));
        limiter.record_success(Duration::from_millis(1));
        limiter.record_error();
        limiter.record_error();
        assert_eq!(
            limiter.current_max(),
            4,
            "错误率达阈值的窗口应乘性减半（8 → 4）"
        );
    }

    #[test]
    fn test_t610_slow_calls_degrade_window() {
        let limiter = AdaptiveConcurrencyLimiter::new(config());
        // 慢调用计入降级信号：3 成功（1 慢）+ 1 慢 = 2/4 慢率
        limiter.record_success(Duration::from_millis(1));
        limiter.record_success(Duration::from_millis(1));
        limiter.record_success(Duration::from_millis(500));
        limiter.record_success(Duration::from_millis(500));
        assert!(limiter.current_max() < 8, "慢调用率过半应收紧窗口（8 → 4）");
    }

    #[test]
    fn test_t610_min_max_floor_respected() {
        let limiter = AdaptiveConcurrencyLimiter::new(config());
        // 连续全错窗口：8 → 4 → 2（触底）→ 2 …
        for _ in 0..12 {
            limiter.record_error();
        }
        assert_eq!(limiter.current_max(), 2, "乘性减不得低于 min_max 下限");
    }

    #[test]
    fn test_t610_max_limit_cap_respected() {
        let limiter = AdaptiveConcurrencyLimiter::new(config());
        for _ in 0..400 {
            limiter.record_success(Duration::from_millis(1));
        }
        assert_eq!(limiter.current_max(), 16, "加性增不得超过 max_limit 上限");
    }

    #[tokio::test]
    async fn test_t610_reject_when_window_exhausted() {
        let limiter = Arc::new(AdaptiveConcurrencyLimiter::new(config()));
        let permit = limiter.try_acquire(8);
        assert!(permit.is_some(), "空载应可占满窗口");
        let second = limiter.try_acquire(1);
        assert!(second.is_none(), "窗口占满后应拒绝");
        drop(permit);
        assert_eq!(limiter.in_flight(), 0, "许可 Drop 后应自动释放");
        assert!(limiter.try_acquire(1).is_some(), "释放后应可再占用");
    }

    #[test]
    fn test_t610_partial_units_reject_keeps_state() {
        let limiter = Arc::new(AdaptiveConcurrencyLimiter::new(config()));
        let permit = limiter.try_acquire(6);
        assert!(permit.is_some());
        // 剩余 2，请求 3 应拒绝且不改变 in_flight
        assert!(limiter.try_acquire(3).is_none());
        assert_eq!(limiter.in_flight(), 6);
        assert!(limiter.try_acquire(2).is_some());
    }

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test]
    async fn test_t610_circuit_open_rejects() {
        use crate::CircuitState;
        use crate::circuit::{CircuitBreaker, CircuitBreakerConfig};

        let cb_config = CircuitBreakerConfig {
            failure_threshold: 1,
            ..CircuitBreakerConfig::default()
        };
        let cb = Arc::new(CircuitBreaker::with_dependencies(cb_config));
        let limiter =
            Arc::new(AdaptiveConcurrencyLimiter::new(config()).with_circuit_breaker(cb.clone()));

        // 通过 execute 注入失败，触发阈值打开熔断
        let _ = cb
            .execute(|| async { Err::<(), _>(LimiteronError::Other("boom".to_string())) })
            .await;
        assert_eq!(cb.get_state().await, CircuitState::Open, "熔断应已打开");

        let allowed = <AdaptiveConcurrencyLimiter as Limiter>::allow(&*limiter, 1)
            .await
            .unwrap();
        assert!(!allowed, "熔断打开期间自适应限流器应拒绝");
    }
}
