// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT

//! 六档容量调光（capacity dial）。
//!
//! 能力下沉自 mnemis mnemis-core 的 graceful degradation 模块（六档渐进
//! 容量收缩），类型按本库命名惯例对齐：[`DialLevel`]（原 DegradationLevel）、
//! [`DialThresholds`]（原 DegradationThresholds）、[`CapacityDial`]
//! （原 GracefulDegradation）。
//!
//! 与原实现的能力差异：状态由 `&mut self` 字段改为 `AtomicU8`（档位）+
//! `AtomicU32`（连续健康计数）内部可变性，供全局共享实例无锁评估。
//! 并发方向取保守侧：升档经 `fetch_max` 单调不回退；降档 CAS 失败即
//! 让位于并发的升档。真正的调光执行点（系统指标采集、按 capacity_ratio
//! 限流放行）由消费方装配，本模块只维护档位机。
//!
//! | Level | Capacity | Trigger example |
//! |-------|----------|-----------------|
//! | L0 | 100% | Healthy |
//! | L1 | 80% | Minor error rate |
//! | L2 | 60% | Elevated latency |
//! | L3 | 40% | High error rate |
//! | L4 | 20% | Resource exhaustion |
//! | L5 | 0% | Total failure (≈ circuit open) |
//!
//! 升档即时生效（单调），恢复须连续 `recovery_consecutive_healthy` 次
//! 健康评估才降一档（防振荡）。

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

/// 调光档位（L0 = 健康，L5 = 完全降级）。
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum DialLevel {
    /// 100% capacity — healthy.
    #[default]
    L0,
    /// 80% capacity — minor degradation.
    L1,
    /// 60% capacity — moderate degradation.
    L2,
    /// 40% capacity — significant degradation.
    L3,
    /// 20% capacity — severe degradation.
    L4,
    /// 0% capacity — fully degraded (equivalent to circuit open).
    L5,
}

impl DialLevel {
    /// Capacity ratio for this level: `L0 → 1.0`, `L1 → 0.8`, ..., `L5 → 0.0`.
    pub fn capacity_ratio(self) -> f64 {
        match self {
            Self::L0 => 1.0,
            Self::L1 => 0.8,
            Self::L2 => 0.6,
            Self::L3 => 0.4,
            Self::L4 => 0.2,
            Self::L5 => 0.0,
        }
    }

    /// Escalate to the next higher (worse) level. Returns `None` if already at L5.
    pub fn escalate(self) -> Option<Self> {
        match self {
            Self::L0 => Some(Self::L1),
            Self::L1 => Some(Self::L2),
            Self::L2 => Some(Self::L3),
            Self::L3 => Some(Self::L4),
            Self::L4 => Some(Self::L5),
            Self::L5 => None,
        }
    }

    /// De-escalate to the next lower (better) level. Returns `None` if already at L0.
    pub fn deescalate(self) -> Option<Self> {
        match self {
            Self::L0 => None,
            Self::L1 => Some(Self::L0),
            Self::L2 => Some(Self::L1),
            Self::L3 => Some(Self::L2),
            Self::L4 => Some(Self::L3),
            Self::L5 => Some(Self::L4),
        }
    }

    /// Numeric level (0 for L0, 5 for L5).
    pub fn as_usize(self) -> usize {
        match self {
            Self::L0 => 0,
            Self::L1 => 1,
            Self::L2 => 2,
            Self::L3 => 3,
            Self::L4 => 4,
            Self::L5 => 5,
        }
    }

    /// 档位的原子存储表示（与 [`Self::as_usize`] 一致，供 `AtomicU8` 使用）。
    fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::L0,
            1 => Self::L1,
            2 => Self::L2,
            3 => Self::L3,
            4 => Self::L4,
            _ => Self::L5,
        }
    }
}

impl std::fmt::Display for DialLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::L0 => write!(f, "L0"),
            Self::L1 => write!(f, "L1"),
            Self::L2 => write!(f, "L2"),
            Self::L3 => write!(f, "L3"),
            Self::L4 => write!(f, "L4"),
            Self::L5 => write!(f, "L5"),
        }
    }
}

/// 各升档边界的触发阈值。
///
/// 每组数组 5 项对应 5 个升档边界（L0→L1 ... L4→L5）。指标达到
/// （≥）阈值即触发升至该档。
///
/// # 契约
///
/// 每组数组应按档位**非递减**排列（`t[i] <= t[i+1]`）。字段为公开
/// 形状（含 serde 反序列化），构造时不强制校验——乱序不产生错误
/// 行为（evaluate 取各指标触发的最高档），仅使档位-阈值对应关系
/// 失去直觉，调用方自行保证。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialThresholds {
    /// Error rate thresholds [0.05, 0.10, 0.20, 0.30, 0.50].
    pub error_rate: [f64; 5],
    /// Latency thresholds in milliseconds [100, 200, 500, 1000, 2000].
    pub latency_ms: [f64; 5],
    /// Resource usage thresholds (0.0–1.0) [0.70, 0.80, 0.90, 0.95, 0.99].
    pub resource_usage: [f64; 5],
}

impl Default for DialThresholds {
    fn default() -> Self {
        Self {
            error_rate: [0.05, 0.10, 0.20, 0.30, 0.50],
            latency_ms: [100.0, 200.0, 500.0, 1000.0, 2000.0],
            resource_usage: [0.70, 0.80, 0.90, 0.95, 0.99],
        }
    }
}

/// 用于评估档位的当前系统指标。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct SystemMetrics {
    /// Current error rate (0.0–1.0).
    pub error_rate: f64,
    /// Current latency in milliseconds.
    pub latency_ms: f64,
    /// Current resource usage (0.0–1.0).
    pub resource_usage: f64,
}

impl SystemMetrics {
    /// Returns `true` if all metrics are within healthy (L0) range.
    pub fn is_healthy(&self, thresholds: &DialThresholds) -> bool {
        self.error_rate < thresholds.error_rate[0]
            && self.latency_ms < thresholds.latency_ms[0]
            && self.resource_usage < thresholds.resource_usage[0]
    }
}

/// 容量调光控制器。
///
/// 维护当前 [`DialLevel`] 并按 [`SystemMetrics`] 升/降档。升档即时；
/// 恢复需 `recovery_consecutive_healthy` 次连续健康评估降一档。
/// 内部可变性：所有方法 `&self`，实例可经 `Arc`/`static` 共享。
pub struct CapacityDial {
    thresholds: DialThresholds,
    recovery_consecutive_healthy: u32,
    /// 当前档位（0=L0 ... 5=L5），`fetch_max` 保证升档单调不回退
    current_level: AtomicU8,
    /// 连续健康评估计数（达到恢复阈值后降一档并清零）
    consecutive_healthy: AtomicU32,
}

// 档位与计数的读写均为独立告警信号，无跨变量顺序不变量；
// 并发竞争的方向性由 fetch_max / CAS 保证（升档优先），Relaxed 足够。
impl CapacityDial {
    /// Create a new controller starting at L0 (healthy).
    ///
    /// # Panic
    ///
    /// `recovery_consecutive_healthy == 0` 时 panic：计数在健康评估时
    /// 先自增再比较（恒 ≥ 1），0 值会使每次健康评估立即降一档，
    /// 「连续 N 次健康才降档」的防振荡承诺失效——这是配置 bug，
    /// 应在构造阶段显性失败。
    pub fn new(thresholds: DialThresholds, recovery_consecutive_healthy: u32) -> Self {
        assert!(
            recovery_consecutive_healthy >= 1,
            "CapacityDial: recovery_consecutive_healthy must be >= 1; \
             0 would de-escalate on every healthy evaluation (anti-oscillation broken)"
        );
        Self {
            thresholds,
            recovery_consecutive_healthy,
            current_level: AtomicU8::new(0),
            consecutive_healthy: AtomicU32::new(0),
        }
    }

    /// Current degradation level.
    pub fn level(&self) -> DialLevel {
        DialLevel::from_u8(self.current_level.load(Ordering::Relaxed))
    }

    /// Current capacity ratio (0.0–1.0).
    pub fn capacity_ratio(&self) -> f64 {
        self.level().capacity_ratio()
    }

    /// Compute the target level for given metrics (the highest level any
    /// single metric triggers).
    fn target_level(&self, metrics: &SystemMetrics) -> DialLevel {
        let t = &self.thresholds;
        let mut max_level = DialLevel::L0;

        for (i, &threshold) in t.error_rate.iter().enumerate() {
            if metrics.error_rate >= threshold {
                max_level = max_level.max(level_from_index(i));
            }
        }
        for (i, &threshold) in t.latency_ms.iter().enumerate() {
            if metrics.latency_ms >= threshold {
                max_level = max_level.max(level_from_index(i));
            }
        }
        for (i, &threshold) in t.resource_usage.iter().enumerate() {
            if metrics.resource_usage >= threshold {
                max_level = max_level.max(level_from_index(i));
            }
        }

        max_level
    }

    /// Evaluate metrics and update the degradation level.
    ///
    /// - If target level > current: escalate immediately to target.
    /// - If metrics are healthy: increment consecutive_healthy counter;
    ///   when it reaches `recovery_consecutive_healthy`, de-escalate one level.
    /// - Otherwise: reset consecutive_healthy counter (no change).
    ///
    /// Returns the level after evaluation.
    pub fn evaluate(&self, metrics: &SystemMetrics) -> DialLevel {
        let target = self.target_level(metrics);
        let current = self.level();

        if target > current {
            // Escalate immediately（fetch_max：并发评估下档位只升不降）
            self.current_level
                .fetch_max(target as u8, Ordering::Relaxed);
            self.consecutive_healthy.store(0, Ordering::Relaxed);
            return self.level();
        }

        if metrics.is_healthy(&self.thresholds) {
            let healthy = self.consecutive_healthy.fetch_add(1, Ordering::Relaxed) + 1;
            if healthy >= self.recovery_consecutive_healthy {
                // CAS 降一档：失败说明并发已升档，让位（方向保守）
                let cur = self.level();
                if let Some(lower) = cur.deescalate() {
                    let _ = self.current_level.compare_exchange(
                        cur as u8,
                        lower as u8,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    );
                }
                self.consecutive_healthy.store(0, Ordering::Relaxed);
            }
        } else {
            // Metrics not healthy but target <= current: hold level, reset counter
            self.consecutive_healthy.store(0, Ordering::Relaxed);
        }

        self.level()
    }

    /// Force a recovery step (de-escalate one level) regardless of metrics.
    ///
    /// Useful for manual intervention or testing. Returns the new level.
    pub fn force_recover(&self) -> DialLevel {
        let cur = self.level();
        if let Some(lower) = cur.deescalate() {
            let _ = self.current_level.compare_exchange(
                cur as u8,
                lower as u8,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }
        self.consecutive_healthy.store(0, Ordering::Relaxed);
        self.level()
    }

    /// Reset to L0 (fully healthy).
    pub fn reset(&self) {
        self.current_level.store(0, Ordering::Relaxed);
        self.consecutive_healthy.store(0, Ordering::Relaxed);
    }
}

impl Default for CapacityDial {
    fn default() -> Self {
        Self::new(DialThresholds::default(), 3)
    }
}

/// Map a threshold index (0-4) to the corresponding dial level.
fn level_from_index(i: usize) -> DialLevel {
    match i {
        0 => DialLevel::L1,
        1 => DialLevel::L2,
        2 => DialLevel::L3,
        3 => DialLevel::L4,
        _ => DialLevel::L5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- DialLevel tests ---

    #[test]
    fn level_capacity_ratios() {
        assert_eq!(DialLevel::L0.capacity_ratio(), 1.0);
        assert_eq!(DialLevel::L1.capacity_ratio(), 0.8);
        assert_eq!(DialLevel::L2.capacity_ratio(), 0.6);
        assert_eq!(DialLevel::L3.capacity_ratio(), 0.4);
        assert_eq!(DialLevel::L4.capacity_ratio(), 0.2);
        assert_eq!(DialLevel::L5.capacity_ratio(), 0.0);
    }

    #[test]
    fn level_escalate() {
        assert_eq!(DialLevel::L0.escalate(), Some(DialLevel::L1));
        assert_eq!(DialLevel::L1.escalate(), Some(DialLevel::L2));
        assert_eq!(DialLevel::L4.escalate(), Some(DialLevel::L5));
        assert_eq!(DialLevel::L5.escalate(), None);
    }

    #[test]
    fn level_deescalate() {
        assert_eq!(DialLevel::L5.deescalate(), Some(DialLevel::L4));
        assert_eq!(DialLevel::L1.deescalate(), Some(DialLevel::L0));
        assert_eq!(DialLevel::L0.deescalate(), None);
    }

    #[test]
    fn level_as_usize() {
        assert_eq!(DialLevel::L0.as_usize(), 0);
        assert_eq!(DialLevel::L5.as_usize(), 5);
    }

    #[test]
    fn level_display() {
        assert_eq!(DialLevel::L0.to_string(), "L0");
        assert_eq!(DialLevel::L5.to_string(), "L5");
    }

    #[test]
    fn level_default_is_l0() {
        assert_eq!(DialLevel::default(), DialLevel::L0);
    }

    #[test]
    fn level_serde_roundtrip() {
        let json = serde_json::to_string(&DialLevel::L3).unwrap();
        let back: DialLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, DialLevel::L3);
    }

    #[test]
    fn level_ordering() {
        assert!(DialLevel::L0 < DialLevel::L1);
        assert!(DialLevel::L4 < DialLevel::L5);
        assert!(DialLevel::L2 < DialLevel::L4);
    }

    // --- SystemMetrics tests ---

    #[test]
    fn metrics_healthy_when_below_all_thresholds() {
        let t = DialThresholds::default();
        let m = SystemMetrics {
            error_rate: 0.01,
            latency_ms: 50.0,
            resource_usage: 0.5,
        };
        assert!(m.is_healthy(&t));
    }

    #[test]
    fn metrics_unhealthy_when_error_rate_at_threshold() {
        let t = DialThresholds::default();
        let m = SystemMetrics {
            error_rate: 0.05,
            latency_ms: 50.0,
            resource_usage: 0.5,
        };
        assert!(
            !m.is_healthy(&t),
            "error_rate >= threshold[0] is not healthy"
        );
    }

    #[test]
    fn metrics_unhealthy_when_latency_high() {
        let t = DialThresholds::default();
        let m = SystemMetrics {
            error_rate: 0.01,
            latency_ms: 150.0,
            resource_usage: 0.5,
        };
        assert!(!m.is_healthy(&t));
    }

    // --- CapacityDial evaluate tests ---

    #[test]
    #[should_panic(expected = "recovery_consecutive_healthy must be >= 1")]
    fn dial_zero_recovery_panics() {
        // recovery=0 会使每次健康评估立即降档(防振荡失效),构造期显性失败
        let _ = CapacityDial::new(DialThresholds::default(), 0);
    }

    #[test]
    fn dial_starts_at_l0() {
        let dial = CapacityDial::default();
        assert_eq!(dial.level(), DialLevel::L0);
        assert_eq!(dial.capacity_ratio(), 1.0);
    }

    #[test]
    fn evaluate_escalates_on_high_error_rate() {
        let dial = CapacityDial::default();
        // error_rate = 0.25 → triggers L3 (>= 0.20)
        let metrics = SystemMetrics {
            error_rate: 0.25,
            latency_ms: 50.0,
            resource_usage: 0.5,
        };
        let level = dial.evaluate(&metrics);
        assert_eq!(level, DialLevel::L3);
        assert!((dial.capacity_ratio() - 0.4).abs() < 1e-9);
    }

    #[test]
    fn evaluate_escalates_on_high_latency() {
        let dial = CapacityDial::default();
        // latency = 600ms → triggers L3 (>= 500)
        let metrics = SystemMetrics {
            error_rate: 0.01,
            latency_ms: 600.0,
            resource_usage: 0.5,
        };
        let level = dial.evaluate(&metrics);
        assert_eq!(level, DialLevel::L3);
    }

    #[test]
    fn evaluate_escalates_on_high_resource_usage() {
        let dial = CapacityDial::default();
        // resource_usage = 0.92 → triggers L3 (>= 0.90)
        let metrics = SystemMetrics {
            error_rate: 0.01,
            latency_ms: 50.0,
            resource_usage: 0.92,
        };
        let level = dial.evaluate(&metrics);
        assert_eq!(level, DialLevel::L3);
    }

    #[test]
    fn evaluate_takes_max_level_across_metrics() {
        let dial = CapacityDial::default();
        // error_rate → L2 (0.15 >= 0.10), latency → L4 (1200 >= 1000)
        // max(L2, L4) = L4
        let metrics = SystemMetrics {
            error_rate: 0.15,
            latency_ms: 1200.0,
            resource_usage: 0.5,
        };
        let level = dial.evaluate(&metrics);
        assert_eq!(level, DialLevel::L4);
    }

    #[test]
    fn evaluate_escalates_to_l5_on_total_failure() {
        let dial = CapacityDial::default();
        // error_rate = 0.6 → triggers L5 (>= 0.50)
        let metrics = SystemMetrics {
            error_rate: 0.6,
            latency_ms: 3000.0,
            resource_usage: 0.99,
        };
        let level = dial.evaluate(&metrics);
        assert_eq!(level, DialLevel::L5);
        assert_eq!(dial.capacity_ratio(), 0.0);
    }

    #[test]
    fn evaluate_no_change_when_metrics_moderate_and_already_at_target() {
        let dial = CapacityDial::default();
        // First escalate to L2
        let m1 = SystemMetrics {
            error_rate: 0.15,
            latency_ms: 50.0,
            resource_usage: 0.5,
        };
        dial.evaluate(&m1);
        assert_eq!(dial.level(), DialLevel::L2);
        // Metrics still at L2 level but not healthy → hold
        let m2 = SystemMetrics {
            error_rate: 0.12,
            latency_ms: 50.0,
            resource_usage: 0.5,
        };
        let level = dial.evaluate(&m2);
        assert_eq!(level, DialLevel::L2);
    }

    #[test]
    fn evaluate_recovers_after_consecutive_healthy() {
        let dial = CapacityDial::default();
        // Escalate to L3
        let bad = SystemMetrics {
            error_rate: 0.25,
            latency_ms: 600.0,
            resource_usage: 0.92,
        };
        dial.evaluate(&bad);
        assert_eq!(dial.level(), DialLevel::L3);

        // Need 3 consecutive healthy evaluations to recover one level
        let healthy = SystemMetrics {
            error_rate: 0.01,
            latency_ms: 50.0,
            resource_usage: 0.5,
        };
        dial.evaluate(&healthy);
        assert_eq!(dial.level(), DialLevel::L3, "1st healthy: no recovery yet");
        dial.evaluate(&healthy);
        assert_eq!(dial.level(), DialLevel::L3, "2nd healthy: no recovery yet");
        dial.evaluate(&healthy);
        assert_eq!(dial.level(), DialLevel::L2, "3rd healthy: recover to L2");
    }

    #[test]
    fn evaluate_recovery_resets_on_unhealthy() {
        let dial = CapacityDial::default();
        // Escalate to L2
        dial.evaluate(&SystemMetrics {
            error_rate: 0.15,
            latency_ms: 50.0,
            resource_usage: 0.5,
        });
        assert_eq!(dial.level(), DialLevel::L2);

        let healthy = SystemMetrics {
            error_rate: 0.01,
            latency_ms: 50.0,
            resource_usage: 0.5,
        };
        dial.evaluate(&healthy);
        dial.evaluate(&healthy);
        // 2 healthy, need 1 more

        // Unhealthy metric (but still at L2 level, not escalating)
        let moderate = SystemMetrics {
            error_rate: 0.12,
            latency_ms: 50.0,
            resource_usage: 0.5,
        };
        dial.evaluate(&moderate);
        assert_eq!(dial.level(), DialLevel::L2, "no recovery");

        // Need 3 more consecutive healthy
        dial.evaluate(&healthy);
        dial.evaluate(&healthy);
        assert_eq!(dial.level(), DialLevel::L2, "only 2 healthy after reset");
        dial.evaluate(&healthy);
        assert_eq!(
            dial.level(),
            DialLevel::L1,
            "3rd healthy after reset: recover"
        );
    }

    #[test]
    fn evaluate_escalation_resets_healthy_counter() {
        let dial = CapacityDial::default();
        // Escalate to L2
        dial.evaluate(&SystemMetrics {
            error_rate: 0.15,
            latency_ms: 50.0,
            resource_usage: 0.5,
        });

        let healthy = SystemMetrics {
            error_rate: 0.01,
            latency_ms: 50.0,
            resource_usage: 0.5,
        };
        dial.evaluate(&healthy);
        dial.evaluate(&healthy);
        // 2 healthy

        // Escalate to L4
        dial.evaluate(&SystemMetrics {
            error_rate: 0.35,
            latency_ms: 1200.0,
            resource_usage: 0.5,
        });
        assert_eq!(dial.level(), DialLevel::L4);

        // Need 3 healthy again
        dial.evaluate(&healthy);
        dial.evaluate(&healthy);
        assert_eq!(dial.level(), DialLevel::L4, "escalation reset counter");
        dial.evaluate(&healthy);
        assert_eq!(dial.level(), DialLevel::L3, "3rd healthy: recover to L3");
    }

    // --- force_recover & reset tests ---

    #[test]
    fn force_recover_deescalates_one_level() {
        let dial = CapacityDial::default();
        dial.evaluate(&SystemMetrics {
            error_rate: 0.6,
            latency_ms: 3000.0,
            resource_usage: 0.99,
        });
        assert_eq!(dial.level(), DialLevel::L5);

        dial.force_recover();
        assert_eq!(dial.level(), DialLevel::L4);

        dial.force_recover();
        assert_eq!(dial.level(), DialLevel::L3);
    }

    #[test]
    fn force_recover_at_l0_no_change() {
        let dial = CapacityDial::default();
        dial.force_recover();
        assert_eq!(dial.level(), DialLevel::L0);
    }

    #[test]
    fn reset_returns_to_l0() {
        let dial = CapacityDial::default();
        dial.evaluate(&SystemMetrics {
            error_rate: 0.6,
            latency_ms: 3000.0,
            resource_usage: 0.99,
        });
        assert_eq!(dial.level(), DialLevel::L5);

        dial.reset();
        assert_eq!(dial.level(), DialLevel::L0);
        assert_eq!(dial.capacity_ratio(), 1.0);
    }

    // --- thresholds & serde tests ---

    #[test]
    fn thresholds_default_values() {
        let t = DialThresholds::default();
        assert_eq!(t.error_rate, [0.05, 0.10, 0.20, 0.30, 0.50]);
        assert_eq!(t.latency_ms, [100.0, 200.0, 500.0, 1000.0, 2000.0]);
        assert_eq!(t.resource_usage, [0.70, 0.80, 0.90, 0.95, 0.99]);
    }

    #[test]
    fn thresholds_serde_roundtrip() {
        let t = DialThresholds::default();
        let json = serde_json::to_string(&t).unwrap();
        let back: DialThresholds = serde_json::from_str(&json).unwrap();
        assert_eq!(back.error_rate, t.error_rate);
        assert_eq!(back.latency_ms, t.latency_ms);
        assert_eq!(back.resource_usage, t.resource_usage);
    }

    #[test]
    fn metrics_serde_roundtrip() {
        let m = SystemMetrics {
            error_rate: 0.15,
            latency_ms: 250.0,
            resource_usage: 0.85,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: SystemMetrics = serde_json::from_str(&json).unwrap();
        assert!((back.error_rate - 0.15).abs() < 1e-9);
        assert!((back.latency_ms - 250.0).abs() < 1e-9);
        assert!((back.resource_usage - 0.85).abs() < 1e-9);
    }

    #[test]
    fn full_degradation_and_recovery_cycle() {
        let dial = CapacityDial::default();

        // Degrade to L5
        for _ in 0..5 {
            dial.evaluate(&SystemMetrics {
                error_rate: 0.6,
                latency_ms: 3000.0,
                resource_usage: 0.99,
            });
        }
        assert_eq!(dial.level(), DialLevel::L5);

        // Recover step by step: L5 → L4 → L3 → L2 → L1 → L0
        let healthy = SystemMetrics {
            error_rate: 0.01,
            latency_ms: 50.0,
            resource_usage: 0.5,
        };
        let expected_sequence = [
            DialLevel::L4,
            DialLevel::L3,
            DialLevel::L2,
            DialLevel::L1,
            DialLevel::L0,
        ];

        for (i, &expected) in expected_sequence.iter().enumerate() {
            // Need 3 consecutive healthy evaluations per recovery step
            for _ in 0..3 {
                dial.evaluate(&healthy);
            }
            assert_eq!(
                dial.level(),
                expected,
                "recovery step {}: expected {:?}",
                i,
                expected
            );
        }
    }

    // --- 原子内部可变性带来的并发语义（原实现无此面） ---

    #[test]
    fn concurrent_evaluation_state_stays_coherent() {
        // 混跑升/降档后状态不得损坏：并发阶段无死锁、无非法档位；
        // 排水阶段（单线程纯健康输入）锁定恢复语义——target=L0 不升档，
        // 每恰 3 次评估降一档直至 L0,证明并发后 level/counter 仍一致。
        let dial = std::sync::Arc::new(CapacityDial::default());
        let bad = SystemMetrics {
            error_rate: 0.6,
            latency_ms: 3000.0,
            resource_usage: 0.99,
        };
        let healthy = SystemMetrics {
            error_rate: 0.01,
            latency_ms: 50.0,
            resource_usage: 0.5,
        };

        std::thread::scope(|s| {
            for _ in 0..4 {
                let dial = dial.clone();
                s.spawn(move || {
                    for _ in 0..200 {
                        dial.evaluate(&bad);
                    }
                });
            }
            for _ in 0..4 {
                let dial = dial.clone();
                s.spawn(move || {
                    for _ in 0..200 {
                        dial.evaluate(&healthy);
                    }
                });
            }
        });

        // 排水：健康输入下档位只降不升，每 3 次评估恰好降一档
        let start = dial.level().as_usize();
        let healthy_owned = healthy;
        let mut evaluations = 0usize;
        let mut prev = dial.level().as_usize();
        loop {
            let level = dial.evaluate(&healthy_owned);
            evaluations += 1;
            let now = level.as_usize();
            assert!(now <= prev, "健康输入下档位不得回升: {prev} → {now}");
            prev = now;
            if level == DialLevel::L0 {
                break;
            }
            // 上界护栏：每档最多 3 次评估 + 起始计数余量,超出即状态损坏
            assert!(
                evaluations <= 3 * (start + 1) + 8,
                "恢复步进失控: {evaluations} 次评估后仍在 {level}"
            );
        }
        assert_eq!(dial.capacity_ratio(), 1.0);
    }
}
