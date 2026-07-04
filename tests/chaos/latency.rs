//! 延迟注入模块
//!
//! 提供可配置的延迟注入功能,模拟各种网络延迟场景。
//!
//! ## 延迟分布
//! - `Constant`: 固定延迟
//! - `Uniform`: 均匀分布延迟
//! - `Exponential`: 指数分布延迟 (模拟真实网络)
//! - `Jitter`: 带抖动的延迟 (模拟网络波动)

use parking_lot::Mutex;
use std::time::Duration;

/// 延迟分布类型
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum LatencyDistribution {
    /// 固定延迟
    Constant(Duration),
    /// 均匀分布 [min, max]
    Uniform { min: Duration, max: Duration },
    /// 指数分布 (平均延迟)
    Exponential { mean: Duration },
    /// 带抖动的延迟 (基础延迟 ± 抖动范围)
    Jitter {
        base: Duration,
        jitter_range: Duration,
    },
}

/// 延迟注入器状态
struct InjectorState {
    /// 操作计数器
    operation_count: u64,
    /// 累计注入延迟
    total_injected_ns: u64,
}

/// 延迟注入器
///
/// 提供多种延迟分布模型,用于模拟网络延迟。
///
/// # 示例
///
/// ```ignore
/// use limiteron_chaos::latency::{LatencyInjector, LatencyDistribution};
/// use std::time::Duration;
///
/// // 指数分布延迟,平均100ms
/// let injector = LatencyInjector::new(LatencyDistribution::Exponential {
///     mean: Duration::from_millis(100),
/// });
///
/// // 注入延迟
/// injector.inject().await;
/// ```
pub struct LatencyInjector {
    /// 延迟分布
    distribution: LatencyDistribution,
    /// 注入器状态
    state: Mutex<InjectorState>,
    /// 是否启用
    enabled: parking_lot::RwLock<bool>,
}

impl LatencyInjector {
    /// 创建新的延迟注入器
    pub fn new(distribution: LatencyDistribution) -> Self {
        Self {
            distribution,
            state: Mutex::new(InjectorState {
                operation_count: 0,
                total_injected_ns: 0,
            }),
            enabled: parking_lot::RwLock::new(true),
        }
    }

    /// 创建禁用的注入器
    pub fn disabled() -> Self {
        Self {
            distribution: LatencyDistribution::Constant(Duration::ZERO),
            state: Mutex::new(InjectorState {
                operation_count: 0,
                total_injected_ns: 0,
            }),
            enabled: parking_lot::RwLock::new(false),
        }
    }

    /// 启用/禁用注入
    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.write() = enabled;
    }

    /// 检查是否启用
    pub fn is_enabled(&self) -> bool {
        *self.enabled.read()
    }

    /// 注入延迟
    pub async fn inject(&self) -> Duration {
        if !self.is_enabled() {
            return Duration::ZERO;
        }

        let delay = self.calculate_delay();

        // 先在锁内更新状态，然后释放锁再 await，避免 await_holding_lock
        {
            let mut state = self.state.lock();
            state.operation_count += 1;
            state.total_injected_ns += delay.as_nanos() as u64;
        }

        if delay > Duration::ZERO {
            tokio::time::sleep(delay).await;
        }

        delay
    }

    /// 计算延迟 (不实际sleep)
    pub fn calculate_delay(&self) -> Duration {
        let state = self.state.lock();
        let op_count = state.operation_count.wrapping_add(1);
        drop(state);

        match &self.distribution {
            LatencyDistribution::Constant(d) => *d,
            LatencyDistribution::Uniform { min, max } => {
                let range_ns = max.as_nanos() as u64 - min.as_nanos() as u64;
                if range_ns == 0 {
                    return *min;
                }
                let rand_val = Self::rand_from_count(op_count);
                let offset = rand_val % range_ns;
                Duration::from_nanos(min.as_nanos() as u64 + offset)
            }
            LatencyDistribution::Exponential { mean } => {
                // 使用逆变换采样生成指数分布
                let mean_ns = mean.as_nanos() as u64;
                if mean_ns == 0 {
                    return Duration::ZERO;
                }
                let rand_val = Self::rand_from_count(op_count);
                // 简化指数分布: 使用线性近似
                let delay_ns =
                    (rand_ns_to_uniform(rand_val) as u128 * mean_ns as u128) / u64::MAX as u128;
                Duration::from_nanos(delay_ns as u64)
            }
            LatencyDistribution::Jitter { base, jitter_range } => {
                let jitter_ns = jitter_range.as_nanos() as u64;
                if jitter_ns == 0 {
                    return *base;
                }
                let rand_val = Self::rand_from_count(op_count);
                let offset = rand_val % jitter_ns;
                // 基础延迟 ± 一半抖动范围
                let half_jitter = jitter_ns / 2;
                let actual_jitter = offset.abs_diff(half_jitter);
                let delay_ns = base.as_nanos() as u64;
                Duration::from_nanos(delay_ns.saturating_add(actual_jitter))
            }
        }
    }

    /// 获取统计信息
    pub fn stats(&self) -> LatencyStats {
        let state = self.state.lock();
        LatencyStats {
            total_operations: state.operation_count,
            total_injected: Duration::from_nanos(state.total_injected_ns),
            avg_latency: Duration::from_nanos(
                state
                    .total_injected_ns
                    .checked_div(state.operation_count)
                    .unwrap_or(0),
            ),
        }
    }

    /// 从操作计数生成伪随机数
    fn rand_from_count(seed: u64) -> u64 {
        let a: u64 = 6364136223846793005;
        let c: u64 = 1442695040888963407;
        seed.wrapping_mul(a).wrapping_add(c)
    }
}

/// 延迟统计信息
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LatencyStats {
    /// 总操作数
    pub total_operations: u64,
    /// 累计注入延迟
    pub total_injected: Duration,
    /// 平均延迟
    pub avg_latency: Duration,
}

/// 将均匀分布随机数转换为指数分布
fn rand_ns_to_uniform(rand_ns: u64) -> u64 {
    // 简化实现: 使用线性缩放
    // 真实实现应该使用 -ln(1-u) * mean
    if rand_ns == u64::MAX {
        return u64::MAX;
    }
    // 使用简单的非线性映射模拟指数分布
    let u = rand_ns as f64 / u64::MAX as f64;
    // 近似: -ln(1-u)
    let exp_val = if u >= 1.0 {
        10.0 // 截断
    } else {
        -(1.0 - u).ln()
    };
    // 归一化到 0-10 范围,然后映射回 0-u64::MAX
    ((exp_val / 10.0) * u64::MAX as f64) as u64
}

/// 便捷函数: 创建常见延迟场景
///
/// 创建低延迟抖动 (1-5ms)
pub fn low_jitter() -> LatencyInjector {
    LatencyInjector::new(LatencyDistribution::Jitter {
        base: Duration::from_millis(1),
        jitter_range: Duration::from_millis(4),
    })
}

/// 创建中等网络延迟 (50-200ms)
pub fn moderate_network_latency() -> LatencyInjector {
    LatencyInjector::new(LatencyDistribution::Uniform {
        min: Duration::from_millis(50),
        max: Duration::from_millis(200),
    })
}

/// 创建高延迟/网络分区场景 (500-2000ms)
pub fn high_latency_network_partition() -> LatencyInjector {
    LatencyInjector::new(LatencyDistribution::Uniform {
        min: Duration::from_millis(500),
        max: Duration::from_millis(2000),
    })
}

/// 创建指数分布延迟 (平均100ms)
#[allow(dead_code)]
pub fn exponential_latency(mean_ms: u64) -> LatencyInjector {
    LatencyInjector::new(LatencyDistribution::Exponential {
        mean: Duration::from_millis(mean_ms),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_constant_delay() {
        let injector =
            LatencyInjector::new(LatencyDistribution::Constant(Duration::from_millis(50)));

        let start = Instant::now();
        injector.inject().await;
        let elapsed = start.elapsed();

        assert!(
            elapsed >= Duration::from_millis(50),
            "Expected at least 50ms, got {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_uniform_delay_range() {
        let injector = LatencyInjector::new(LatencyDistribution::Uniform {
            min: Duration::from_millis(10),
            max: Duration::from_millis(100),
        });

        let mut min_observed = Duration::from_secs(100);
        let mut max_observed = Duration::ZERO;

        for _ in 0..10 {
            let delay = injector.calculate_delay();
            min_observed = min_observed.min(delay);
            max_observed = max_observed.max(delay);
        }

        // 延迟应该在范围内
        assert!(min_observed >= Duration::from_millis(10));
        assert!(max_observed <= Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_disabled_injector() {
        let injector = LatencyInjector::disabled();

        let start = Instant::now();
        let delay = injector.inject().await;
        let elapsed = start.elapsed();

        assert_eq!(delay, Duration::ZERO);
        assert!(elapsed < Duration::from_millis(10));
    }

    #[tokio::test]
    async fn test_injector_stats() {
        let injector =
            LatencyInjector::new(LatencyDistribution::Constant(Duration::from_millis(10)));

        for _ in 0..5 {
            injector.inject().await;
        }

        let stats = injector.stats();
        assert_eq!(stats.total_operations, 5);
        assert!(stats.avg_latency >= Duration::from_millis(10));
    }

    #[tokio::test]
    async fn test_toggle_enabled() {
        let injector =
            LatencyInjector::new(LatencyDistribution::Constant(Duration::from_millis(100)));

        // 默认启用
        assert!(injector.is_enabled());

        // 禁用
        injector.set_enabled(false);
        assert!(!injector.is_enabled());

        let start = Instant::now();
        injector.inject().await;
        let elapsed = start.elapsed();

        // 禁用时应该立即返回
        assert!(elapsed < Duration::from_millis(50));

        // 重新启用
        injector.set_enabled(true);
        assert!(injector.is_enabled());
    }

    #[test]
    fn test_convenience_functions() {
        let low = low_jitter();
        assert!(low.is_enabled());

        let moderate = moderate_network_latency();
        assert!(moderate.is_enabled());

        let high = high_latency_network_partition();
        assert!(high.is_enabled());
    }
}
