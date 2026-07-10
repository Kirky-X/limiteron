//! 故障注入存储层
//!
//! 包装真实存储,随机注入故障以测试系统的容错能力。
//!
//! ## 故障模式
//! - `Random`: 随机故障,每次操作有固定概率失败
//! - `Intermittent`: 间歇性故障,周期性地失败和恢复
//! - `Continuous`: 持续故障,一旦触发就一直失败直到恢复
//! - `Bursty`: 突发性故障,短时间内大量失败

use async_trait::async_trait;
use limiteron::Storage;
use limiteron::error::StorageError;
use parking_lot::Mutex;
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

/// 故障模式
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub enum FaultPattern {
    /// 随机故障 - 每次操作有固定概率失败
    #[default]
    Random,
    /// 间歇性故障 - 周期性失败和恢复
    Intermittent {
        /// 故障周期 (操作次数)
        cycle_length: u64,
        /// 每个周期内失败的操作数
        fail_count: u64,
    },
    /// 突发性故障 - 短时间突发大量失败
    Bursty {
        /// 突发概率 (0.0 - 1.0)
        burst_probability: f64,
        /// 突发持续时间 (操作次数)
        burst_duration: u64,
    },
}

/// 故障注入状态
struct FaultState {
    /// 操作计数器
    operation_count: u64,
    /// 是否在突发状态中
    in_burst: bool,
    /// 突发剩余操作数
    burst_remaining: u64,
}

/// 故障注入存储层
///
/// 包装内部存储,按配置的模式注入故障。
///
/// # 示例
///
/// ```ignore
/// use limiteron_chaos::FaultInjectionStorage;
///
/// let fault_storage = FaultInjectionStorage::new(
///     inner_storage,
///     0.1,  // 10% 失败率
///     FaultPattern::Random,
///     50..200,  // 延迟范围
/// );
/// ```
pub struct FaultInjectionStorage {
    /// 内部存储
    inner: Arc<dyn Storage>,
    /// 基础失败率 (0.0 - 1.0)
    failure_rate: f64,
    /// 延迟注入范围 (ms)
    latency_range: Range<u64>,
    /// 故障模式
    fault_pattern: FaultPattern,
    /// 故障状态 (内部可变性)
    state: Mutex<FaultState>,
    /// 随机数生成器种子 (用于可重现测试)
    #[allow(dead_code)]
    seed: u64,
}

impl FaultInjectionStorage {
    /// 创建新的故障注入存储
    pub fn new(
        inner: Arc<dyn Storage>,
        failure_rate: f64,
        fault_pattern: FaultPattern,
        latency_range: Range<u64>,
    ) -> Self {
        Self {
            inner,
            failure_rate: failure_rate.clamp(0.0, 1.0),
            latency_range,
            fault_pattern,
            state: Mutex::new(FaultState {
                operation_count: 0,
                in_burst: false,
                burst_remaining: 0,
            }),
            seed: 42, // 默认固定种子用于可重现测试
        }
    }

    /// 创建带有自定义种子的故障注入存储
    #[allow(dead_code)]
    pub fn with_seed(
        inner: Arc<dyn Storage>,
        failure_rate: f64,
        fault_pattern: FaultPattern,
        latency_range: Range<u64>,
        seed: u64,
    ) -> Self {
        Self {
            inner,
            failure_rate: failure_rate.clamp(0.0, 1.0),
            latency_range,
            fault_pattern,
            state: Mutex::new(FaultState {
                operation_count: 0,
                in_burst: false,
                burst_remaining: 0,
            }),
            seed,
        }
    }

    /// 构建器模式创建
    #[allow(dead_code)]
    pub fn builder() -> FaultInjectionBuilder {
        FaultInjectionBuilder::default()
    }

    /// 判断是否应该注入故障
    fn should_fail(&self) -> bool {
        let mut state = self.state.lock();
        state.operation_count += 1;
        let op_count = state.operation_count;

        match self.fault_pattern {
            FaultPattern::Random => {
                // 简单的线性同余生成器
                let rand_val = self.simple_rand(op_count);
                (rand_val as f64 / u64::MAX as f64) < self.failure_rate
            }
            FaultPattern::Intermittent {
                cycle_length,
                fail_count,
            } => {
                let pos_in_cycle = op_count % cycle_length;
                pos_in_cycle < fail_count
            }
            FaultPattern::Bursty {
                burst_probability,
                burst_duration,
            } => {
                // 检查是否应该进入突发状态
                if !state.in_burst {
                    let rand_val = self.simple_rand(op_count);
                    if (rand_val as f64 / u64::MAX as f64) < burst_probability {
                        state.in_burst = true;
                        state.burst_remaining = burst_duration;
                    }
                }

                if state.in_burst {
                    state.burst_remaining -= 1;
                    if state.burst_remaining == 0 {
                        state.in_burst = false;
                    }
                    true
                } else {
                    false
                }
            }
        }
    }

    /// 生成随机延迟
    fn get_delay(&self) -> Duration {
        let rand_val = self.simple_rand(self.state.lock().operation_count.wrapping_mul(7));
        let range_size = self.latency_range.end - self.latency_range.start;
        let delay_ms = if range_size == 0 {
            self.latency_range.start
        } else {
            self.latency_range.start + (rand_val % range_size)
        };
        Duration::from_millis(delay_ms)
    }

    /// 简单的随机数生成 (线性同余)
    fn simple_rand(&self, seed: u64) -> u64 {
        let a: u64 = 6364136223846793005;
        let c: u64 = 1442695040888963407;
        seed.wrapping_mul(a).wrapping_add(c)
    }

    /// 获取内部存储的引用
    #[allow(dead_code)]
    pub fn inner(&self) -> &dyn Storage {
        self.inner.as_ref()
    }
}

#[async_trait]
impl Storage for FaultInjectionStorage {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        // 注入延迟
        if self.latency_range.end > 0 {
            tokio::time::sleep(self.get_delay()).await;
        }

        // 注入故障
        if self.should_fail() {
            return Err(StorageError::TimeoutError(
                "Injected fault: storage get operation failed".into(),
            ));
        }

        self.inner.get(key).await
    }

    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError> {
        // 注入延迟
        if self.latency_range.end > 0 {
            tokio::time::sleep(self.get_delay()).await;
        }

        // 注入故障
        if self.should_fail() {
            return Err(StorageError::TimeoutError(
                "Injected fault: storage set operation failed".into(),
            ));
        }

        self.inner.set(key, value, ttl).await
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        // 注入延迟
        if self.latency_range.end > 0 {
            tokio::time::sleep(self.get_delay()).await;
        }

        // 注入故障
        if self.should_fail() {
            return Err(StorageError::TimeoutError(
                "Injected fault: storage delete operation failed".into(),
            ));
        }

        self.inner.delete(key).await
    }
}

/// 故障注入存储构建器
#[derive(Default)]
#[allow(dead_code)]
pub struct FaultInjectionBuilder {
    inner: Option<Arc<dyn Storage>>,
    failure_rate: f64,
    latency_min: u64,
    latency_max: u64,
    fault_pattern: FaultPattern,
    seed: u64,
}

#[allow(dead_code)]
impl FaultInjectionBuilder {
    /// 设置内部存储
    pub fn with_inner(mut self, inner: Arc<dyn Storage>) -> Self {
        self.inner = Some(inner);
        self
    }

    /// 设置失败率 (0.0 - 1.0)
    pub fn with_failure_rate(mut self, rate: f64) -> Self {
        self.failure_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// 设置延迟范围 (ms)
    pub fn with_latency_range(mut self, min: u64, max: u64) -> Self {
        self.latency_min = min;
        self.latency_max = max;
        self
    }

    /// 设置故障模式
    pub fn with_fault_pattern(mut self, pattern: FaultPattern) -> Self {
        self.fault_pattern = pattern;
        self
    }

    /// 设置随机种子
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// 构建故障注入存储
    pub fn build(self) -> FaultInjectionStorage {
        let inner = self.inner.expect("inner storage is required");
        FaultInjectionStorage::with_seed(
            inner,
            self.failure_rate,
            self.fault_pattern,
            self.latency_min..self.latency_max,
            self.seed,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use limiteron::storage::MemoryStorage;

    #[tokio::test]
    async fn test_fault_injection_random_failures() {
        let inner = Arc::new(MemoryStorage::create_storage());
        let fault_storage = FaultInjectionStorage::new(
            inner,
            0.5, // 50% 失败率
            FaultPattern::Random,
            0..0, // 无延迟
        );

        let mut failures = 0u64;
        let attempts = 100;

        for i in 0..attempts {
            let result = fault_storage
                .set(&format!("key_{}", i), "value", None)
                .await;
            if result.is_err() {
                failures += 1;
            }
        }

        // 50% 失败率, 100次尝试应该有大约 30-70 次失败
        assert!(
            (30..=70).contains(&failures),
            "Expected 30-70 failures, got {}",
            failures
        );
    }

    #[tokio::test]
    async fn test_fault_injection_intermittent_failures() {
        let inner = Arc::new(MemoryStorage::create_storage());
        let fault_storage = FaultInjectionStorage::new(
            inner,
            0.0,
            FaultPattern::Intermittent {
                cycle_length: 10,
                fail_count: 3,
            },
            0..0,
        );

        // 前10次操作应该有3次失败
        let mut failures = 0u64;
        for i in 0..10 {
            let result = fault_storage
                .set(&format!("key_{}", i), "value", None)
                .await;
            if result.is_err() {
                failures += 1;
            }
        }

        assert_eq!(failures, 3, "Expected 3 failures in first cycle");
    }

    #[tokio::test]
    async fn test_fault_injection_no_faults() {
        let inner = Arc::new(MemoryStorage::create_storage());
        let fault_storage =
            FaultInjectionStorage::new(inner.clone(), 0.0, FaultPattern::Random, 0..0);

        // 0% 失败率,所有操作应该成功
        for i in 0..100 {
            fault_storage
                .set(&format!("key_{}", i), "value", None)
                .await
                .unwrap();
        }

        // 验证数据确实在内部存储中
        let value = inner.get("key_0").await.unwrap();
        assert_eq!(value, Some("value".to_string()));
    }

    #[tokio::test]
    async fn test_fault_injection_latency() {
        use std::time::Instant;

        let inner = Arc::new(MemoryStorage::create_storage());
        let fault_storage = FaultInjectionStorage::new(
            inner,
            0.0,
            FaultPattern::Random,
            50..100, // 50-100ms 延迟
        );

        let start = Instant::now();
        let _ = fault_storage.set("key", "value", None).await;
        let elapsed = start.elapsed();

        // 应该有至少50ms延迟
        assert!(
            elapsed >= Duration::from_millis(50),
            "Expected at least 50ms latency, got {:?}",
            elapsed
        );
    }
}
