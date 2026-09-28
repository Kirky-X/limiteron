// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 准入控制器（feature `admission-control`）
//!
//! 并发 + 速率双门准入：请求须同时通过并发门（在途占用上限）与
//! 速率门（每秒准入上限）方可进入；任一门拒绝即拒绝并按原因计数。
//! 拒绝计数经 [`AdmissionController::stats`] 暴露，供过载观测与告警。
//!
//! - **并发门**：[`AdmissionPermit`] 租约式占用（Drop 自动释放）
//! - **速率门**：固定 1 秒窗口计数（确定性判定）
//! - **判定顺序**：先并发门后速率门（并发不足时计数到并发门，
//!   不重复计入速率门）
//!
//! # Example
//!
//! ```rust
//! use limiteron::limiters::admission_control::{AdmissionControlConfig, AdmissionController};
//!
//! let controller = Arc::new(AdmissionController::new(AdmissionControlConfig {
//!     max_concurrent: 10,
//!     max_per_second: 100,
//! }));
//! # tokio::runtime::Runtime::new().unwrap().block_on(async {
//! if let Some(permit) = controller.try_acquire(1).await.unwrap() {
//!     // 双门通过：持租约处理请求，Drop 自动释放在途占用
//!     drop(permit);
//! }
//! # });
//! ```

use crate::clock::{Clock, SystemClock};
use crate::error::LimiteronError;
use crate::limiters::traits::{Limiter, RateLimitSnapshot, validate_cost};
use async_trait::async_trait;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// 准入控制配置
#[derive(Debug, Clone)]
pub struct AdmissionControlConfig {
    /// 并发门：最大在途占用
    pub max_concurrent: u64,
    /// 速率门：每秒最大准入数（固定 1 秒窗口）
    pub max_per_second: u64,
}

impl AdmissionControlConfig {
    /// 校验配置合法性
    pub fn validate(&self) -> Result<(), LimiteronError> {
        if self.max_concurrent == 0 {
            return Err(LimiteronError::ConfigError(
                "admission-control max_concurrent must be non-zero".to_string(),
            ));
        }
        if self.max_per_second == 0 {
            return Err(LimiteronError::ConfigError(
                "admission-control max_per_second must be non-zero".to_string(),
            ));
        }
        Ok(())
    }
}

/// 准入统计快照（含按原因分类的拒绝计数）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AdmissionStats {
    /// 双门通过的准入总数
    pub admitted: u64,
    /// 并发门拒绝数（在途占满）
    pub rejected_concurrency: u64,
    /// 速率门拒绝数（本秒配额耗尽）
    pub rejected_rate: u64,
    /// 当前在途占用
    pub in_flight: i64,
}

#[derive(Debug, Default)]
struct StatsCounters {
    admitted: AtomicU64,
    rejected_concurrency: AtomicU64,
    rejected_rate: AtomicU64,
}

#[derive(Debug)]
struct RateWindowState {
    window_start: u64,
    count: u64,
}

/// 并发 + 速率双门准入控制器
pub struct AdmissionController {
    max_concurrent: u64,
    max_per_second: u64,
    clock: Arc<dyn Clock>,
    in_flight: AtomicI64,
    rate_window: Mutex<RateWindowState>,
    stats: StatsCounters,
}

/// 准入租约（Drop 自动释放在途占用）
#[must_use = "permit 丢失即立即释放在途占用，应持有至请求处理完成"]
pub struct AdmissionPermit {
    controller: Arc<AdmissionController>,
    units: u64,
}

impl Drop for AdmissionPermit {
    fn drop(&mut self) {
        self.controller
            .in_flight
            .fetch_sub(self.units as i64, Ordering::AcqRel);
    }
}

impl AdmissionController {
    /// 创建准入控制器（系统时钟）
    pub fn new(config: AdmissionControlConfig) -> Self {
        Self::with_clock(config, Arc::new(SystemClock))
    }

    /// 以自定义时钟创建（测试注入用）
    pub fn with_clock(config: AdmissionControlConfig, clock: Arc<dyn Clock>) -> Self {
        // 配置非法属编程错误（构造期即暴露），panic 与同步原子量初始化一致
        config.validate().expect("valid admission control config");
        let window_start = clock.unix_timestamp();
        Self {
            max_concurrent: config.max_concurrent,
            max_per_second: config.max_per_second,
            clock,
            in_flight: AtomicI64::new(0),
            rate_window: Mutex::new(RateWindowState {
                window_start,
                count: 0,
            }),
            stats: StatsCounters::default(),
        }
    }

    /// 双门准入申请：通过返回租约（Drop 释放在途占用），拒绝返回 `None` 并计数
    pub async fn try_acquire(
        self: &Arc<Self>,
        cost: u64,
    ) -> Result<Option<AdmissionPermit>, LimiteronError> {
        let _cost = validate_cost(cost)?;
        // 并发门：CAS 占用，占满即拒（不触及速率门）
        let mut in_flight = self.in_flight.load(Ordering::Acquire);
        let occupied = loop {
            if in_flight + cost as i64 > self.max_concurrent as i64 {
                self.stats
                    .rejected_concurrency
                    .fetch_add(1, Ordering::Relaxed);
                return Ok(None);
            }
            match self.in_flight.compare_exchange(
                in_flight,
                in_flight + cost as i64,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break true,
                Err(actual) => in_flight = actual,
            }
        };
        debug_assert!(occupied);
        // 速率门：固定 1 秒窗口；拒绝需回滚并发占用
        let granted = {
            let mut state = self.rate_window.lock();
            self.advance_rate_window(&mut state);
            if state.count + cost <= self.max_per_second {
                state.count += cost;
                true
            } else {
                false
            }
        };
        if !granted {
            self.in_flight.fetch_sub(cost as i64, Ordering::AcqRel);
            self.stats.rejected_rate.fetch_add(1, Ordering::Relaxed);
            return Ok(None);
        }
        self.stats.admitted.fetch_add(1, Ordering::Relaxed);
        Ok(Some(AdmissionPermit {
            controller: Arc::clone(self),
            units: cost,
        }))
    }

    /// 准入统计快照（含按原因分类的拒绝计数）
    pub async fn stats(&self) -> AdmissionStats {
        AdmissionStats {
            admitted: self.stats.admitted.load(Ordering::Relaxed),
            rejected_concurrency: self.stats.rejected_concurrency.load(Ordering::Relaxed),
            rejected_rate: self.stats.rejected_rate.load(Ordering::Relaxed),
            in_flight: self.in_flight.load(Ordering::Relaxed),
        }
    }

    fn advance_rate_window(&self, state: &mut RateWindowState) {
        let now = self.clock.unix_timestamp();
        if now >= state.window_start.saturating_add(1) {
            state.window_start = now;
            state.count = 0;
        }
    }
}

#[async_trait]
impl Limiter for AdmissionController {
    /// 双门判定（短租约语义）：占用并发额度后立即释放，仅作准入判定；
    /// 需要真实租约请用 [`AdmissionController::try_acquire`]
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError> {
        let _cost = validate_cost(cost)?;
        // 并发门（短租约：判定即占用即释放）
        let mut in_flight = self.in_flight.load(Ordering::Acquire);
        let occupied = loop {
            if in_flight + cost as i64 > self.max_concurrent as i64 {
                break false;
            }
            match self.in_flight.compare_exchange(
                in_flight,
                in_flight + cost as i64,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break true,
                Err(actual) => in_flight = actual,
            }
        };
        if !occupied {
            self.stats
                .rejected_concurrency
                .fetch_add(1, Ordering::Relaxed);
            return Ok(false);
        }
        // 速率门
        let granted = {
            let mut state = self.rate_window.lock();
            self.advance_rate_window(&mut state);
            if state.count + cost <= self.max_per_second {
                state.count += cost;
                true
            } else {
                false
            }
        };
        self.in_flight.fetch_sub(cost as i64, Ordering::AcqRel);
        if !granted {
            self.stats.rejected_rate.fetch_add(1, Ordering::Relaxed);
            return Ok(false);
        }
        self.stats.admitted.fetch_add(1, Ordering::Relaxed);
        Ok(true)
    }

    /// 非消费预检：返回速率门视角的标准限流头
    async fn peek(&self, cost: u64) -> Result<RateLimitSnapshot, LimiteronError> {
        let _cost = validate_cost(cost)?;
        let mut state = self.rate_window.lock();
        self.advance_rate_window(&mut state);
        Ok(RateLimitSnapshot {
            limit: self.max_per_second,
            remaining: self.max_per_second.saturating_sub(state.count),
            reset_secs: state
                .window_start
                .saturating_add(1)
                .saturating_sub(self.clock.unix_timestamp()),
        })
    }

    async fn remaining(&self) -> Result<RateLimitSnapshot, LimiteronError> {
        self.peek(1).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;

    fn controller(max_concurrent: u64, max_per_second: u64) -> Arc<AdmissionController> {
        Arc::new(AdmissionController::new(AdmissionControlConfig {
            max_concurrent,
            max_per_second,
        }))
    }

    #[tokio::test]
    async fn test_acquire_within_limits_grants_permit() {
        let controller = controller(10, 100);
        let permit = controller.try_acquire(2).await.unwrap();
        assert!(permit.is_some(), "within limits should be admitted");
        let stats = controller.stats().await;
        assert_eq!(stats.admitted, 1);
        assert_eq!(stats.in_flight, 2);
    }

    #[tokio::test]
    async fn test_permit_drop_releases_concurrency() {
        let controller = controller(4, 100);
        drop(controller.try_acquire(3).await.unwrap().unwrap());
        let stats = controller.stats().await;
        assert_eq!(stats.in_flight, 0, "dropping permit should release units");
        assert!(controller.try_acquire(4).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_concurrency_gate_rejects_and_counts() {
        let controller = controller(4, 100);
        // 持有租约（临时丢弃的 permit 语句结束即释放在途占用）
        let _permit = controller.try_acquire(3).await.unwrap().unwrap();
        assert!(controller.try_acquire(2).await.unwrap().is_none());
        let stats = controller.stats().await;
        assert_eq!(stats.rejected_concurrency, 1);
        assert_eq!(stats.rejected_rate, 0);
        assert_eq!(stats.in_flight, 3, "rejected request must not occupy units");
    }

    #[tokio::test]
    async fn test_rate_gate_rejects_and_counts() {
        let controller = controller(100, 3);
        let mut permits = Vec::new();
        for _ in 0..3 {
            permits.push(
                controller
                    .try_acquire(1)
                    .await
                    .unwrap()
                    .expect("within rate limit should be admitted"),
            );
        }
        assert!(controller.try_acquire(1).await.unwrap().is_none());
        let stats = controller.stats().await;
        assert_eq!(stats.admitted, 3);
        assert_eq!(stats.rejected_rate, 1);
        assert_eq!(
            stats.in_flight, 3,
            "rate-rejected request must release concurrency reservation; held permits remain"
        );
        drop(permits);
    }

    #[tokio::test]
    async fn test_rate_window_advances() {
        let clock = Arc::new(MockClock::new());
        let controller = Arc::new(AdmissionController::with_clock(
            AdmissionControlConfig {
                max_concurrent: 100,
                max_per_second: 2,
            },
            clock.clone(),
        ));
        let _first = controller.try_acquire(2).await.unwrap().unwrap();
        assert!(controller.try_acquire(1).await.unwrap().is_none());
        clock.advance(std::time::Duration::from_secs(2));
        let _second = controller.try_acquire(2).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_limiter_allow_is_non_leasing_verdict() {
        let controller = controller(2, 100);
        assert!(controller.allow(2).await.unwrap());
        let stats = controller.stats().await;
        assert_eq!(
            stats.in_flight, 0,
            "Limiter::allow must not leak in-flight units"
        );
        assert_eq!(stats.admitted, 1);
    }

    #[tokio::test]
    async fn test_peek_does_not_consume() {
        let controller = controller(10, 50);
        let _permit = controller.try_acquire(5).await.unwrap().unwrap();
        let snapshot = controller.peek(1).await.unwrap();
        assert_eq!(snapshot.limit, 50);
        assert_eq!(snapshot.remaining, 45);
    }

    #[tokio::test]
    async fn test_zero_cost_rejected() {
        let controller = controller(10, 10);
        assert!(controller.try_acquire(0).await.is_err());
        assert!(controller.allow(0).await.is_err());
    }

    #[test]
    #[should_panic(expected = "valid admission control config")]
    fn test_invalid_config_panics_at_construction() {
        let _ = AdmissionController::new(AdmissionControlConfig {
            max_concurrent: 0,
            max_per_second: 10,
        });
    }
}
