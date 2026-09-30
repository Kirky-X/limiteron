// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 自适应阈值限流器（feature `adaptive-threshold`，默认关）
//!
//! **统计启发式**（显式决策，非机器学习）：滑动观测窗口内的错误率与
//! 延迟分位驱动动态配额升降——下游错误率升高或延迟劣化时乘性收紧配额
//! （减压下游），指标恢复后加性放宽（钳制在显式配置的 `[min, max]`）。
//! 所有阈值、步长、冷却期均为显式配置项，无隐藏启发参数；判定逻辑是
//! 确定性条件语句（规则 5：确定性决策不交给模型）。
//!
//! 与 [`AdaptiveConcurrencyLimiter`](super::AdaptiveConcurrencyLimiter)
//! （AIMD 并发窗口、租约式占用）的分工：本限流器是**配额型**——窗口内
//! 消费计数对动态配额判定 `allow`，适配「调用方对下游的出口配额随下游
//! 健康度伸缩」与「服务端按错误率自降载」两类场景；反馈经
//! [`report`](AdaptiveThresholdLimiter::report) 显式上报（限流拒绝本身
//! 不是错误信号，错误/延迟语义由调用方定义）。
//!
//! 统计口径（与 AIMD 同为样本驱动，不依赖墙钟，无时钟回拨面）：
//! - **观测窗口**：最近 `window_samples` 个反馈样本的错误率与 p95 延迟；
//!   每 `min_samples_for_adjust` 个样本构成一个**判定点**（校验要求
//!   `1 <= min_samples_for_adjust <= window_samples`——冷启动期不抖动）
//! - **下调**：错误率 ≥ `error_rate_increase_threshold` **或** p95 ≥
//!   `latency_increase_ms` → `limit = max(min, limit × decrease_ratio)`
//! - **上调**：错误率 ≤ `error_rate_decrease_threshold` **且** p95 ≤
//!   `latency_decrease_ms` → `limit = min(max, limit + increase_step)`
//! - **冷却期**：每次调整后 `cooldown_samples` 个样本内禁止再次调整
//!   （防阈值附近振荡）；下调置入冷却，上调同样置入
//! - **配额周期**：消费计数（`consumed`）随观测窗口同步滚动——每
//!   `window_samples` 个反馈样本重置一次，计满拒绝的配额在窗口滚动后
//!   恢复放行（否则消费计数只增不减，累计达配额后永久全拒）
//!
//! # Example
//!
//! ```rust
//! use limiteron::limiters::adaptive_threshold::{
//!     AdaptiveThresholdConfig, AdaptiveThresholdLimiter, Feedback,
//! };
//!
//! let limiter = AdaptiveThresholdLimiter::with_validation(AdaptiveThresholdConfig {
//!     base_limit: 100,
//!     min_limit: 20,
//!     max_limit: 200,
//!     window_samples: 10,
//!     min_samples_for_adjust: 5,
//!     error_rate_increase_threshold: 0.1,
//!     error_rate_decrease_threshold: 0.01,
//!     latency_increase_ms: 500,
//!     latency_decrease_ms: 100,
//!     decrease_ratio: 0.5,
//!     increase_step: 10,
//!     cooldown_samples: 10,
//! })
//! .unwrap();
//!
//! // 出口配额随下游健康度伸缩：错误反馈 → 收紧
//! limiter.report(Feedback {
//!     ok: false,
//!     latency_ms: 1_000,
//! });
//! assert!(limiter.current_limit() <= 100);
//! ```

use crate::error::LimiteronError;
use crate::limiters::traits::{Limiter, RateLimitSnapshot};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};

/// 自适应阈值配置（全部显式，无隐藏默认行为分歧）
#[derive(Debug, Clone)]
pub struct AdaptiveThresholdConfig {
    /// 名义配额（动态配额初始值）
    pub base_limit: u64,
    /// 配额下界（乘性减不再低于此值）
    pub min_limit: u64,
    /// 配额上界（加性增不再高于此值）
    pub max_limit: u64,
    /// 观测滑动窗口样本数（错误率/延迟分位的统计基数）
    pub window_samples: usize,
    /// 触发调整所需的最小样本数（冷启动期不抖动）
    pub min_samples_for_adjust: usize,
    /// 下调信号：错误率 ≥ 此值（0.0-1.0）
    pub error_rate_increase_threshold: f64,
    /// 上调信号：错误率 ≤ 此值（0.0-1.0）
    pub error_rate_decrease_threshold: f64,
    /// 下调信号：p95 延迟 ≥ 此值（毫秒）
    pub latency_increase_ms: u64,
    /// 上调信号：p95 延迟 ≤ 此值（毫秒）
    pub latency_decrease_ms: u64,
    /// 下调幅度：`limit × decrease_ratio`（0.0 < ratio < 1.0）
    pub decrease_ratio: f64,
    /// 上调步长：`limit + increase_step`（加性，与乘性减对称防振荡）
    pub increase_step: u64,
    /// 冷却期样本数：调整后 N 个样本内禁止再调整（防振荡）
    pub cooldown_samples: usize,
}

impl AdaptiveThresholdConfig {
    /// 显式校验（构造期 fail-loud，防带病运行）
    pub fn validate(&self) -> Result<(), LimiteronError> {
        if self.min_limit == 0 {
            return Err(LimiteronError::ConfigError(
                "adaptive-threshold: min_limit must be > 0".to_string(),
            ));
        }
        if self.min_limit > self.base_limit || self.base_limit > self.max_limit {
            return Err(LimiteronError::ConfigError(
                "adaptive-threshold: require min_limit <= base_limit <= max_limit".to_string(),
            ));
        }
        if !(0.0 < self.decrease_ratio && self.decrease_ratio < 1.0) {
            return Err(LimiteronError::ConfigError(
                "adaptive-threshold: decrease_ratio must be in (0, 1)".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.error_rate_increase_threshold)
            || !(0.0..=1.0).contains(&self.error_rate_decrease_threshold)
        {
            return Err(LimiteronError::ConfigError(
                "adaptive-threshold: error_rate_increase_threshold and \
                 error_rate_decrease_threshold must be in [0, 1]"
                    .to_string(),
            ));
        }
        if self.error_rate_decrease_threshold > self.error_rate_increase_threshold {
            return Err(LimiteronError::ConfigError(
                "adaptive-threshold: error_rate_decrease_threshold must be <= increase threshold"
                    .to_string(),
            ));
        }
        if self.latency_decrease_ms > self.latency_increase_ms {
            return Err(LimiteronError::ConfigError(
                "adaptive-threshold: latency_decrease_ms must be <= latency_increase_ms"
                    .to_string(),
            ));
        }
        if self.window_samples == 0 || self.cooldown_samples == 0 {
            return Err(LimiteronError::ConfigError(
                "adaptive-threshold: window_samples and cooldown_samples must be > 0".to_string(),
            ));
        }
        // 0 使冷启动判定永不满足；超过 window_samples 时窗口 sample_count
        // 封顶在 window_samples、判定点永不触发——两者均为静默失效配置
        if self.min_samples_for_adjust == 0 || self.min_samples_for_adjust > self.window_samples {
            return Err(LimiteronError::ConfigError(
                "adaptive-threshold: min_samples_for_adjust must be in 1..=window_samples"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

/// 观测滑动窗口（样本数驱动，无墙钟依赖）
///
/// 延迟环与错误标记环同游标；`errors` 是环上错误样本的同步计数
/// （覆盖旧样本时扣减、写入新样本时累加），单一事实来源。冷却剩余
/// 样本数与累计样本数同驻本结构：反馈路径只需这一把锁。
#[derive(Debug)]
struct ObservationWindow {
    /// 延迟样本环形缓冲（毫秒）
    latencies: Vec<u64>,
    /// 错误标记环（与 latencies 同游标）
    error_ring: Vec<bool>,
    /// 写入游标（环形）
    cursor: usize,
    /// 窗口内有效样本数（≤ 容量）
    filled: usize,
    /// 环上错误样本数
    errors: usize,
    /// 累计样本数（判定点与窗口滚动的节拍基准，不随环形覆盖清零）
    total_pushed: usize,
    /// 冷却剩余样本数（每样本递减，调整落地时重置）
    cooldown_left: usize,
}

impl ObservationWindow {
    fn new(capacity: usize) -> Self {
        Self {
            latencies: vec![0; capacity],
            error_ring: vec![false; capacity],
            cursor: 0,
            filled: 0,
            errors: 0,
            total_pushed: 0,
            cooldown_left: 0,
        }
    }

    fn capacity(&self) -> usize {
        self.latencies.len()
    }

    /// 记录一个反馈样本（满容量时环形覆盖最旧样本并同步错误计数）。
    ///
    /// 返回本次写入是否使窗口滚动满一周（每 `capacity` 个样本一次）——
    /// 这是配额周期（消费计数）的重置节拍。
    fn push(&mut self, ok: bool, latency_ms: u64) -> bool {
        if self.filled == self.latencies.len() {
            if self.error_ring[self.cursor] {
                self.errors = self.errors.saturating_sub(1);
            }
        } else {
            self.filled += 1;
        }
        self.latencies[self.cursor] = latency_ms;
        self.error_ring[self.cursor] = !ok;
        if !ok {
            self.errors += 1;
        }
        self.cursor = (self.cursor + 1) % self.latencies.len();
        self.total_pushed += 1;
        self.total_pushed % self.latencies.len() == 0
    }

    fn sample_count(&self) -> usize {
        self.filled
    }

    /// 窗口错误率
    fn error_rate(&self) -> f64 {
        if self.filled == 0 {
            return 0.0;
        }
        self.errors as f64 / self.filled as f64
    }

    /// 窗口 p95 延迟（排序取 95 分位）
    fn latency_p95_ms(&self) -> u64 {
        if self.filled == 0 {
            return 0;
        }
        let mut latencies = self.latencies[..self.filled].to_vec();
        latencies.sort_unstable();
        let index = ((self.filled as f64 * 0.95).ceil() as usize)
            .saturating_sub(1)
            .min(self.filled - 1);
        latencies[index]
    }
}

/// 每请求结果反馈（错误/延迟语义由调用方定义——下游失败、超时、
/// 业务错误均可；限流拒绝不应上报为本算法的错误信号）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feedback {
    /// 请求是否成功
    pub ok: bool,
    /// 请求耗时（毫秒）
    pub latency_ms: u64,
}

/// 调整动作（`last_adjustment` 自省面）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adjustment {
    /// 尚未调整
    None,
    /// 冷却期中（剩余样本数）
    Cooling(usize),
    /// 乘性下调至该值
    Decreased(u64),
    /// 加性上调至该值
    Increased(u64),
}

/// 自适应阈值限流器（统计启发式动态配额）
pub struct AdaptiveThresholdLimiter {
    config: AdaptiveThresholdConfig,
    /// 动态配额（可调）
    dynamic_limit: AtomicU64,
    /// 当前配额周期已消费计数（随观测窗口滚动由 [`Self::report`] 重置）
    consumed: AtomicU64,
    /// 观测滑动窗口（含冷却剩余样本数与累计样本计数）
    observations: parking_lot::Mutex<ObservationWindow>,
    /// 最近一次调整动作（自省）
    last_adjustment: parking_lot::Mutex<Adjustment>,
}

impl AdaptiveThresholdLimiter {
    /// 构造（crate 内部入口，不校验——外部路径一律经
    /// [`with_validation`](Self::with_validation)，工厂/配置侧须先调
    /// [`AdaptiveThresholdConfig::validate`]）
    #[must_use]
    pub(crate) fn new(config: AdaptiveThresholdConfig) -> Self {
        debug_assert!(
            config.validate().is_ok(),
            "adaptive-threshold: invalid config passed to new()"
        );
        let base = config.base_limit;
        Self {
            dynamic_limit: AtomicU64::new(base),
            consumed: AtomicU64::new(0),
            observations: parking_lot::Mutex::new(ObservationWindow::new(config.window_samples)),
            last_adjustment: parking_lot::Mutex::new(Adjustment::None),
            config,
        }
    }

    /// 经配置校验的构造（校验失败返回 Err）
    pub fn with_validation(config: AdaptiveThresholdConfig) -> Result<Self, LimiteronError> {
        config.validate()?;
        Ok(Self::new(config))
    }

    /// 当前动态配额
    #[must_use]
    pub fn current_limit(&self) -> u64 {
        self.dynamic_limit.load(Ordering::Relaxed)
    }

    /// 最近一次调整动作
    #[must_use]
    pub fn last_adjustment(&self) -> Adjustment {
        *self.last_adjustment.lock()
    }

    /// 当前窗口统计快照（自省/测试面）
    #[must_use]
    pub fn window_stats(&self) -> (usize, f64, u64) {
        let obs = self.observations.lock();
        (obs.sample_count(), obs.error_rate(), obs.latency_p95_ms())
    }

    /// 上报请求结果并在样本边界按需调整动态配额
    ///
    /// 每个样本：冷却递减一格、计入观测窗口；每 `min_samples_for_adjust`
    /// 个样本构成一个**判定点**（冷却期已过且样本充足时执行一次调整判定，
    /// 下调优先——保守原则：模糊信号先收紧）。每 `window_samples` 个样本
    /// 观测窗口滚动满一周，配额周期（消费计数）随之重置。
    pub fn report(&self, feedback: Feedback) {
        // 单锁完成冷却递减、样本入环与判定点检测：反馈路径从「每样本
        // 4 次锁 + 一次 p95 全量排序」收敛为 1 次锁；p95 排序仅在判定点
        // 执行（每 min_samples_for_adjust 个样本一次）
        let (window_rolled, decision) = {
            let mut obs = self.observations.lock();
            obs.cooldown_left = obs.cooldown_left.saturating_sub(1);
            obs.push(feedback.ok, feedback.latency_ms);
            let evaluate = obs.total_pushed % self.config.min_samples_for_adjust == 0
                && obs.sample_count() >= self.config.min_samples_for_adjust
                && obs.cooldown_left == 0;
            if evaluate {
                (
                    obs.total_pushed % obs.capacity() == 0,
                    Some((obs.error_rate(), obs.latency_p95_ms())),
                )
            } else {
                (obs.total_pushed % obs.capacity() == 0, None)
            }
        };

        if window_rolled {
            // 配额周期重置：consumed 只增不减会令计满的配额永久全拒。
            // 并发在途 allow 的 CAS 可能紧随重置写回旧值（误差上界为在途
            // 请求成本），下一窗口边界再次重置，语义自收敛。
            self.consumed.store(0, Ordering::Relaxed);
        }

        if let Some((error_rate, p95)) = decision {
            self.apply_adjustment(error_rate, p95);
        }
    }

    /// 执行一次调整判定（下调优先；判定通过即置入冷却——冷却语义与
    /// 判定节奏解耦，无论判定点是否落在冷却外，冷却都按样本推进）
    fn apply_adjustment(&self, error_rate: f64, p95: u64) {
        let should_decrease = error_rate >= self.config.error_rate_increase_threshold
            || p95 >= self.config.latency_increase_ms;
        let should_increase = error_rate <= self.config.error_rate_decrease_threshold
            && p95 <= self.config.latency_decrease_ms;

        if should_decrease {
            let current = self.current_limit();
            let next = ((current as f64 * self.config.decrease_ratio) as u64)
                .max(self.config.min_limit)
                .min(current);
            if next < current {
                self.dynamic_limit.store(next, Ordering::Relaxed);
                self.observations.lock().cooldown_left = self.config.cooldown_samples;
                *self.last_adjustment.lock() = Adjustment::Decreased(next);
            }
        } else if should_increase {
            let current = self.current_limit();
            let next = current
                .saturating_add(self.config.increase_step)
                .min(self.config.max_limit);
            if next > current {
                self.dynamic_limit.store(next, Ordering::Relaxed);
                self.observations.lock().cooldown_left = self.config.cooldown_samples;
                *self.last_adjustment.lock() = Adjustment::Increased(next);
            }
        }
    }

    /// 标准限流头快照（动态配额即 limit；`cost` 不参与快照——
    /// peek 的可行性由调用方按 `remaining >= cost` 判定）
    #[must_use]
    pub fn snapshot(&self) -> RateLimitSnapshot {
        let limit = self.current_limit();
        let consumed = self.consumed.load(Ordering::Relaxed);
        RateLimitSnapshot {
            limit,
            remaining: limit.saturating_sub(consumed),
            reset_secs: 0,
        }
    }
}

#[async_trait]
impl Limiter for AdaptiveThresholdLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError> {
        // 热路径纯 CAS：consumed 只增不判窗口，重置由 report() 在观测
        // 窗口滚动边界执行（配额周期语义见 report）；配额调整即刻生效
        let limit = self.current_limit();
        let mut consumed = self.consumed.load(Ordering::Relaxed);
        loop {
            if consumed.saturating_add(cost) > limit {
                return Ok(false);
            }
            match self.consumed.compare_exchange_weak(
                consumed,
                consumed + cost,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(true),
                Err(actual) => consumed = actual,
            }
        }
    }

    async fn peek(&self, _cost: u64) -> Result<RateLimitSnapshot, LimiteronError> {
        Ok(self.snapshot())
    }

    async fn remaining(&self) -> Result<RateLimitSnapshot, LimiteronError> {
        Ok(self.snapshot())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AdaptiveThresholdConfig {
        AdaptiveThresholdConfig {
            base_limit: 100,
            min_limit: 20,
            max_limit: 200,
            window_samples: 10,
            min_samples_for_adjust: 5,
            error_rate_increase_threshold: 0.2,
            error_rate_decrease_threshold: 0.01,
            latency_increase_ms: 500,
            latency_decrease_ms: 100,
            decrease_ratio: 0.5,
            increase_step: 10,
            cooldown_samples: 10,
        }
    }

    #[test]
    fn config_validation_rejects_invariants() {
        let mut c = config();
        c.validate().unwrap();

        c.min_limit = 0;
        assert!(c.validate().is_err());

        let mut c = config();
        c.base_limit = 10;
        c.min_limit = 20;
        assert!(c.validate().is_err());

        let mut c = config();
        c.decrease_ratio = 1.0;
        assert!(c.validate().is_err());

        let mut c = config();
        c.error_rate_decrease_threshold = 0.5;
        assert!(c.validate().is_err());
    }

    #[test]
    fn config_validation_bounds_min_samples_for_adjust() {
        // 0 使冷启动判定永不满足；超过 window_samples 使判定永不触发
        // （窗口 sample_count 封顶在 window_samples）——两者均为静默配置病
        let mut c = config();
        c.min_samples_for_adjust = 0;
        let err = c.validate().unwrap_err();
        assert!(
            matches!(err, LimiteronError::ConfigError(ref m) if m.contains("min_samples_for_adjust")),
            "错误信息应含字段名: {err}"
        );

        let mut c = config();
        c.min_samples_for_adjust = c.window_samples + 1;
        assert!(c.validate().is_err());

        // 边界合法：1 与 window_samples 本身
        let mut c = config();
        c.min_samples_for_adjust = 1;
        c.validate().unwrap();

        let mut c = config();
        c.min_samples_for_adjust = c.window_samples;
        c.validate().unwrap();
    }

    #[test]
    fn config_validation_clamps_error_rate_thresholds_to_unit_interval() {
        // > 1.0 永不下调、负值恒下调——均属静默失效配置
        let mut c = config();
        c.error_rate_increase_threshold = 1.5;
        let err = c.validate().unwrap_err();
        assert!(
            matches!(err, LimiteronError::ConfigError(ref m) if m.contains("error_rate_increase_threshold")),
            "错误信息应含字段名: {err}"
        );

        let mut c = config();
        c.error_rate_decrease_threshold = -0.1;
        assert!(c.validate().is_err());

        // 边界 [0, 1] 合法
        let mut c = config();
        c.error_rate_increase_threshold = 1.0;
        c.error_rate_decrease_threshold = 0.0;
        c.validate().unwrap();
    }

    /// 紧配置：window/min_samples/cooldown 小步进，便于钉住滚动与判定节奏
    fn small_config() -> AdaptiveThresholdConfig {
        AdaptiveThresholdConfig {
            base_limit: 3,
            min_limit: 1,
            max_limit: 3,
            window_samples: 4,
            min_samples_for_adjust: 2,
            error_rate_increase_threshold: 0.5,
            error_rate_decrease_threshold: 0.01,
            latency_increase_ms: 500,
            latency_decrease_ms: 100,
            decrease_ratio: 0.5,
            increase_step: 10,
            cooldown_samples: 2,
        }
    }

    #[test]
    fn quota_period_resets_on_window_rollover() {
        let limiter = AdaptiveThresholdLimiter::with_validation(small_config()).unwrap();

        // 计满拒绝：consumed 达动态配额后全拒
        for _ in 0..3 {
            assert!(tokio_test_block_on(limiter.allow(1)).unwrap());
        }
        assert!(
            !tokio_test_block_on(limiter.allow(1)).unwrap(),
            "计满后应拒绝"
        );

        // 窗口未滚动前维持拒绝（重置只发生在 window_samples 边界）
        for _ in 0..2 {
            limiter.report(Feedback {
                ok: true,
                latency_ms: 1,
            });
        }
        assert!(
            !tokio_test_block_on(limiter.allow(1)).unwrap(),
            "窗口未滚动前不得恢复放行"
        );

        // 窗口滚动（第 4 个样本）→ 配额周期重置 → 恢复放行
        for _ in 0..2 {
            limiter.report(Feedback {
                ok: true,
                latency_ms: 1,
            });
        }
        assert!(
            tokio_test_block_on(limiter.allow(1)).unwrap(),
            "窗口滚动后应恢复放行"
        );

        // 周期性：再次计满 → 下一次窗口滚动再次恢复（非一次性修补）
        for _ in 0..2 {
            assert!(tokio_test_block_on(limiter.allow(1)).unwrap());
        }
        assert!(!tokio_test_block_on(limiter.allow(1)).unwrap());
        for _ in 0..4 {
            limiter.report(Feedback {
                ok: true,
                latency_ms: 1,
            });
        }
        assert!(
            tokio_test_block_on(limiter.allow(1)).unwrap(),
            "第二次窗口滚动应再次恢复放行"
        );
    }

    #[test]
    fn adjustment_decisions_fire_on_min_sample_boundaries() {
        let limiter = AdaptiveThresholdLimiter::with_validation(small_config()).unwrap();

        // 第 1 个样本不足判定点（min_samples_for_adjust=2）：不调整
        limiter.report(Feedback {
            ok: false,
            latency_ms: 1,
        });
        assert_eq!(limiter.current_limit(), 3, "判定点之间不得调整");

        // 第 2 个样本构成判定点：错误率 100% ≥ 50% → 下调 3→1（钳在 min_limit）
        limiter.report(Feedback {
            ok: false,
            latency_ms: 1,
        });
        assert_eq!(limiter.current_limit(), 1);
        assert_eq!(limiter.last_adjustment(), Adjustment::Decreased(1));
    }

    #[test]
    fn allow_consumes_against_dynamic_limit() {
        let limiter = AdaptiveThresholdLimiter::with_validation(config()).unwrap();
        assert_eq!(limiter.current_limit(), 100);

        // 100 个 1-cost 全放行，第 101 个拒绝
        for _ in 0..100 {
            assert!(tokio_test_block_on(limiter.allow(1)).unwrap());
        }
        assert!(!tokio_test_block_on(limiter.allow(1)).unwrap());

        // 大 cost 直接拒绝（不回绕）
        assert!(!tokio_test_block_on(limiter.allow(u64::MAX)).unwrap());
    }

    // 极简同步执行器：本模块测试全部为无 IO 原子逻辑，block_on 即可
    fn tokio_test_block_on<F: std::future::Future>(fut: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(fut)
    }

    #[test]
    fn high_error_rate_triggers_decrease_to_floor() {
        let limiter = AdaptiveThresholdLimiter::with_validation(config()).unwrap();

        // min_samples=5：5 个错误样本（100% 错误率）→ 下调 100→50
        for _ in 0..5 {
            limiter.report(Feedback {
                ok: false,
                latency_ms: 1,
            });
        }
        assert_eq!(limiter.current_limit(), 50);
        assert_eq!(limiter.last_adjustment(), Adjustment::Decreased(50));

        // 冷却期（10 样本）：继续报错不再下调（抑制振荡）
        for _ in 0..9 {
            limiter.report(Feedback {
                ok: false,
                latency_ms: 1,
            });
        }
        assert_eq!(limiter.current_limit(), 50, "冷却期内不得继续下调");

        // 冷却期结束后第 10 个样本触发第二次下调 50→25
        limiter.report(Feedback {
            ok: false,
            latency_ms: 1,
        });
        assert_eq!(limiter.current_limit(), 25);

        // 窗口被错误填满后持续下调直到钳制在 min_limit=20
        for _ in 0..40 {
            limiter.report(Feedback {
                ok: false,
                latency_ms: 1,
            });
        }
        assert_eq!(limiter.current_limit(), 20, "不得低于 min_limit");
    }

    #[test]
    fn healthy_feedback_triggers_increase_to_ceiling() {
        let limiter = AdaptiveThresholdLimiter::with_validation(config()).unwrap();

        // 低错误 + 低延迟：每轮 5 样本触发判定；冷却 10 样本 → 每 10 个
        // 样本获得一次 +10 机会。20 轮 = 100 样本 → 约 10 次调整到达上限
        for _ in 0..20 {
            for _ in 0..5 {
                limiter.report(Feedback {
                    ok: true,
                    latency_ms: 10,
                });
            }
        }
        assert_eq!(limiter.current_limit(), 200, "加性增应钳制在 max_limit");
        assert_eq!(limiter.last_adjustment(), Adjustment::Increased(200));
    }

    #[test]
    fn cooldown_suppresses_immediate_reincrease() {
        let limiter = AdaptiveThresholdLimiter::with_validation(config()).unwrap();

        // 先下调（1 错误 + 4 快成功 → 错误率 20% ≥ 0.2 → 100→50）
        limiter.report(Feedback {
            ok: false,
            latency_ms: 1,
        });
        for _ in 0..4 {
            limiter.report(Feedback {
                ok: true,
                latency_ms: 10,
            });
        }
        assert_eq!(limiter.current_limit(), 50);

        // 冷却期内喂全好样本：不得立即上调（防振荡核心断言）
        for _ in 0..5 {
            limiter.report(Feedback {
                ok: true,
                latency_ms: 10,
            });
        }
        assert_eq!(limiter.current_limit(), 50, "冷却期内不得上调");
        assert!(
            matches!(
                limiter.last_adjustment(),
                Adjustment::Decreased(50) | Adjustment::Cooling(_)
            ),
            "冷却期调整动作应保持/显示冷却: {:?}",
            limiter.last_adjustment()
        );

        // 冷却耗尽后（10 样本后）恢复上调路径：50→60
        for _ in 0..5 {
            limiter.report(Feedback {
                ok: true,
                latency_ms: 10,
            });
        }
        assert_eq!(limiter.current_limit(), 60, "冷却期结束后应恢复加性增");
    }

    #[test]
    fn latency_p95_alone_triggers_decrease() {
        let limiter = AdaptiveThresholdLimiter::with_validation(config()).unwrap();

        // 零错误但 p95 ≥ 500ms：延迟信号独立触发下调
        for _ in 0..5 {
            limiter.report(Feedback {
                ok: true,
                latency_ms: 800,
            });
        }
        assert_eq!(limiter.current_limit(), 50, "延迟劣化应独立触发收紧");
    }

    #[test]
    fn cold_start_below_min_samples_skips_adjustment() {
        let limiter = AdaptiveThresholdLimiter::with_validation(config()).unwrap();

        // 样本数 < min_samples_for_adjust：不调整
        for _ in 0..4 {
            limiter.report(Feedback {
                ok: false,
                latency_ms: 900,
            });
        }
        assert_eq!(limiter.current_limit(), 100, "冷启动期不得抖动");
        assert_eq!(limiter.last_adjustment(), Adjustment::None);
    }

    #[test]
    fn window_stats_and_snapshot_reflect_state() {
        let limiter = AdaptiveThresholdLimiter::with_validation(config()).unwrap();
        limiter.report(Feedback {
            ok: true,
            latency_ms: 50,
        });
        limiter.report(Feedback {
            ok: false,
            latency_ms: 150,
        });

        let (count, error_rate, p95) = limiter.window_stats();
        assert_eq!(count, 2);
        assert!((error_rate - 0.5).abs() < f64::EPSILON);
        assert_eq!(p95, 150);

        let snap = limiter.snapshot();
        assert_eq!(snap.limit, 100);
        assert_eq!(snap.remaining, 100);
    }

    #[tokio::test]
    async fn limiter_trait_check_maps_rejection_to_error() {
        use crate::limiters::traits::Limiter;
        let limiter = AdaptiveThresholdLimiter::with_validation(config()).unwrap();
        limiter.allow(100).await.unwrap();
        // 超出剩余 → check() 映射为 LimitError（非静默）
        let result = limiter.check("any-key").await;
        assert!(result.is_err());
    }
}
