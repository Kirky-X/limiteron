//! 混沌场景定义
//!
//! 定义常见的混沌测试场景,用于验证系统在异常条件下的行为。
//!
//! ## 场景列表
//! 1. `StorageIntermittentFailure` - 存储间歇性失败
//! 2. `HighLatencyInjection` - 高延迟注入
//! 3. `BurstTraffic` - 突发流量
//! 4. `ClockSkew` - 时钟偏移
//! 5. `CascadingFailure` - 级联故障

use super::fault_injection::{FaultInjectionStorage, FaultPattern};
use super::ChaosTestResult;
use limiteron::clock::MockClock;
use limiteron::error::StorageError;
use limiteron::limiters::{Limiter, TokenBucketLimiter};
use limiteron::storage::MemoryStorage;
use limiteron::{Storage, StorageCreate};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 混沌场景 trait
#[async_trait::async_trait]
pub trait ChaosScenario: Send + Sync {
    /// 场景名称
    fn name(&self) -> &str;

    /// 场景描述
    #[allow(dead_code)]
    fn description(&self) -> &str;

    /// 运行场景
    async fn run(&self) -> ChaosTestResult;
}

/// 存储间歇性失败场景
///
/// 模拟Redis连接丢失或网络抖动,验证系统在存储不可用时的降级行为。
pub struct StorageIntermittentFailure {
    /// 失败率 (0.0 - 1.0)
    pub failure_rate: f64,
    /// 请求数量
    pub request_count: usize,
    /// 限流器类型
    #[allow(dead_code)]
    pub limiter_type: LimiterType,
}

/// 限流器类型枚举
#[derive(Clone)]
#[allow(dead_code)]
pub enum LimiterType {
    TokenBucket {
        capacity: u64,
        refill_rate: u64,
    },
    FixedWindow {
        window_size: Duration,
        max_requests: u64,
    },
}

#[async_trait::async_trait]
impl ChaosScenario for StorageIntermittentFailure {
    fn name(&self) -> &str {
        "storage_intermittent_failure"
    }

    fn description(&self) -> &str {
        "模拟存储间歇性失败,验证限流器在存储不可用时的容错能力"
    }

    async fn run(&self) -> ChaosTestResult {
        let inner = Arc::new(MemoryStorage::create_storage());
        let fault_storage = Arc::new(FaultInjectionStorage::new(
            inner,
            self.failure_rate,
            FaultPattern::Random,
            0..0,
        ));

        // 由于限流器本身不直接使用Storage trait,我们测试故障注入对系统的影响
        // 这里创建一个简单的场景: 限流器 + 故障存储的组合行为
        let mut result = ChaosTestResult {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            timed_out_requests: 0,
            avg_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            max_latency_ms: 0.0,
        };

        let mut latencies = Vec::with_capacity(self.request_count);

        for i in 0..self.request_count {
            let start = Instant::now();
            let storage_result = fault_storage
                .set(&format!("key_{}", i), "value", Some(60))
                .await;
            let elapsed = start.elapsed();
            latencies.push(elapsed.as_millis() as f64);

            result.total_requests += 1;
            match storage_result {
                Ok(()) => result.successful_requests += 1,
                Err(StorageError::TimeoutError(_)) => result.failed_requests += 1,
                Err(_) => result.failed_requests += 1,
            }
        }

        // 计算延迟统计
        if !latencies.is_empty() {
            latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
            result.avg_latency_ms = latencies.iter().sum::<f64>() / latencies.len() as f64;
            result.max_latency_ms = *latencies
                .iter()
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap();

            let p99_idx = (latencies.len() as f64 * 0.99) as usize;
            result.p99_latency_ms = latencies[p99_idx.min(latencies.len() - 1)];
        }

        result
    }
}

/// 高延迟注入场景
///
/// 模拟网络分区或慢速存储,验证系统在延迟增加时的超时和降级行为。
pub struct HighLatencyInjection {
    /// 延迟范围 (ms)
    pub latency_min_ms: u64,
    pub latency_max_ms: u64,
    /// 请求数量
    pub request_count: usize,
}

#[async_trait::async_trait]
impl ChaosScenario for HighLatencyInjection {
    fn name(&self) -> &str {
        "high_latency_injection"
    }

    fn description(&self) -> &str {
        "模拟高延迟网络环境,验证系统在延迟增加时的行为"
    }

    async fn run(&self) -> ChaosTestResult {
        let inner = Arc::new(MemoryStorage::create_storage());
        let fault_storage = Arc::new(FaultInjectionStorage::new(
            inner,
            0.0, // 不注入失败,只注入延迟
            FaultPattern::Random,
            self.latency_min_ms..self.latency_max_ms,
        ));

        let mut result = ChaosTestResult {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            timed_out_requests: 0,
            avg_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            max_latency_ms: 0.0,
        };

        let mut latencies = Vec::with_capacity(self.request_count);
        let timeout = Duration::from_millis(self.latency_max_ms + 500);

        for i in 0..self.request_count {
            let start = Instant::now();

            // 使用超时控制
            let storage_result = tokio::time::timeout(
                timeout,
                fault_storage.set(&format!("key_{}", i), "value", None),
            )
            .await;

            let elapsed = start.elapsed();
            latencies.push(elapsed.as_millis() as f64);

            result.total_requests += 1;
            match storage_result {
                Ok(Ok(())) => result.successful_requests += 1,
                Ok(Err(_)) => result.failed_requests += 1,
                Err(_) => result.timed_out_requests += 1,
            }
        }

        // 计算延迟统计
        if !latencies.is_empty() {
            latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
            result.avg_latency_ms = latencies.iter().sum::<f64>() / latencies.len() as f64;
            result.max_latency_ms = *latencies
                .iter()
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap();

            let p99_idx = (latencies.len() as f64 * 0.99) as usize;
            result.p99_latency_ms = latencies[p99_idx.min(latencies.len() - 1)];
        }

        result
    }
}

/// 突发流量场景
///
/// 模拟DDoS或流量突发,验证限流器在极端压力下的行为。
pub struct BurstTraffic {
    /// 正常QPS
    pub normal_qps: usize,
    /// 突发QPS倍数
    pub burst_multiplier: usize,
    /// 正常持续时间 (秒)
    pub normal_duration_secs: u64,
    /// 突发持续时间 (秒)
    pub burst_duration_secs: u64,
}

#[async_trait::async_trait]
impl ChaosScenario for BurstTraffic {
    fn name(&self) -> &str {
        "burst_traffic"
    }

    fn description(&self) -> &str {
        "模拟突发流量高峰,验证限流器在极端压力下的保护能力"
    }

    async fn run(&self) -> ChaosTestResult {
        let limiter = Arc::new(TokenBucketLimiter::new(100, 10));
        let mut result = ChaosTestResult {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            timed_out_requests: 0,
            avg_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            max_latency_ms: 0.0,
        };

        let mut latencies = Vec::new();

        // 正常流量阶段
        let normal_requests = self.normal_qps * self.normal_duration_secs as usize;
        for _ in 0..normal_requests {
            let start = Instant::now();
            let allowed = limiter.allow(1).await.unwrap();
            let elapsed = start.elapsed();
            latencies.push(elapsed.as_millis() as f64);

            result.total_requests += 1;
            if allowed {
                result.successful_requests += 1;
            } else {
                result.failed_requests += 1;
            }
        }

        // 突发流量阶段
        let burst_requests =
            self.normal_qps * self.burst_multiplier * self.burst_duration_secs as usize;
        for _ in 0..burst_requests {
            let start = Instant::now();
            let allowed = limiter.allow(1).await.unwrap();
            let elapsed = start.elapsed();
            latencies.push(elapsed.as_millis() as f64);

            result.total_requests += 1;
            if allowed {
                result.successful_requests += 1;
            } else {
                result.failed_requests += 1;
            }
        }

        // 计算延迟统计
        if !latencies.is_empty() {
            latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
            result.avg_latency_ms = latencies.iter().sum::<f64>() / latencies.len() as f64;
            result.max_latency_ms = *latencies
                .iter()
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap();

            let p99_idx = (latencies.len() as f64 * 0.99) as usize;
            result.p99_latency_ms = latencies[p99_idx.min(latencies.len() - 1)];
        }

        result
    }
}

/// 时钟偏移场景
///
/// 模拟时钟回拨或跳跃,验证时间敏感算法的正确性。
pub struct ClockSkew {
    /// 初始容量
    pub capacity: u64,
    /// 补充速率
    pub refill_rate: u64,
    /// 时钟跳跃 (秒,正数前进,负数回拨)
    pub clock_jump_seconds: i64,
}

#[async_trait::async_trait]
impl ChaosScenario for ClockSkew {
    fn name(&self) -> &str {
        "clock_skew"
    }

    fn description(&self) -> &str {
        "模拟时钟偏移(跳跃/回拨),验证限流器对时钟异常的容错能力"
    }

    async fn run(&self) -> ChaosTestResult {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn limiteron::clock::Clock> = mock_clock.clone();
        let limiter = TokenBucketLimiter::with_clock(self.capacity, self.refill_rate, clock);

        let mut result = ChaosTestResult {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            timed_out_requests: 0,
            avg_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            max_latency_ms: 0.0,
        };

        // 先消耗一些令牌
        for _ in 0..self.capacity {
            let allowed = limiter.allow(1).await.unwrap();
            result.total_requests += 1;
            if allowed {
                result.successful_requests += 1;
            } else {
                result.failed_requests += 1;
            }
        }

        // 时钟跳跃
        if self.clock_jump_seconds > 0 {
            mock_clock.advance(Duration::from_secs(self.clock_jump_seconds as u64));
        }
        // 注意: MockClock 不支持回拨,这里只测试前进

        // 时钟跳跃后再请求
        for _ in 0..self.capacity {
            let allowed = limiter.allow(1).await.unwrap();
            result.total_requests += 1;
            if allowed {
                result.successful_requests += 1;
            } else {
                result.failed_requests += 1;
            }
        }

        result
    }
}

/// 级联故障场景
///
/// 模拟多个组件同时故障,验证系统的整体韧性。
pub struct CascadingFailure {
    /// 存储失败率
    pub storage_failure_rate: f64,
    /// 延迟注入 (ms)
    pub latency_ms: u64,
    /// 请求数量
    pub request_count: usize,
}

#[async_trait::async_trait]
impl ChaosScenario for CascadingFailure {
    fn name(&self) -> &str {
        "cascading_failure"
    }

    fn description(&self) -> &str {
        "模拟存储失败+高延迟的级联故障,验证系统多重压力下的行为"
    }

    async fn run(&self) -> ChaosTestResult {
        let inner = Arc::new(MemoryStorage::create_storage());
        let fault_storage = Arc::new(FaultInjectionStorage::new(
            inner,
            self.storage_failure_rate,
            FaultPattern::Random,
            self.latency_ms..self.latency_ms + 50,
        ));

        let mut result = ChaosTestResult {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            timed_out_requests: 0,
            avg_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            max_latency_ms: 0.0,
        };

        let mut latencies = Vec::with_capacity(self.request_count);
        let timeout = Duration::from_millis(self.latency_ms + 1000);

        for i in 0..self.request_count {
            let start = Instant::now();

            let storage_result = tokio::time::timeout(
                timeout,
                fault_storage.set(&format!("key_{}", i), "value", None),
            )
            .await;

            let elapsed = start.elapsed();
            latencies.push(elapsed.as_millis() as f64);

            result.total_requests += 1;
            match storage_result {
                Ok(Ok(())) => result.successful_requests += 1,
                Ok(Err(_)) => result.failed_requests += 1,
                Err(_) => result.timed_out_requests += 1,
            }
        }

        // 计算延迟统计
        if !latencies.is_empty() {
            latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
            result.avg_latency_ms = latencies.iter().sum::<f64>() / latencies.len() as f64;
            result.max_latency_ms = *latencies
                .iter()
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap();

            let p99_idx = (latencies.len() as f64 * 0.99) as usize;
            result.p99_latency_ms = latencies[p99_idx.min(latencies.len() - 1)];
        }

        result
    }
}

/// 混沌测试运行器
///
/// 运行一个或多个混沌场景,收集结果。
pub struct ChaosTestRunner {
    /// 场景列表
    scenarios: Vec<Box<dyn ChaosScenario>>,
}

impl ChaosTestRunner {
    /// 创建新的测试运行器
    pub fn new() -> Self {
        Self {
            scenarios: Vec::new(),
        }
    }

    /// 添加场景
    pub fn add_scenario<S: ChaosScenario + 'static>(mut self, scenario: S) -> Self {
        self.scenarios.push(Box::new(scenario));
        self
    }

    /// 运行所有场景
    pub async fn run_all(&self) -> Vec<(&str, ChaosTestResult)> {
        let mut results = Vec::with_capacity(self.scenarios.len());

        for scenario in &self.scenarios {
            let name = scenario.name();
            let result = scenario.run().await;
            results.push((name, result));
        }

        results
    }

    /// 运行指定名称的场景
    pub async fn run_scenario(&self, name: &str) -> Option<ChaosTestResult> {
        for scenario in &self.scenarios {
            if scenario.name() == name {
                return Some(scenario.run().await);
            }
        }
        None
    }

    /// 获取场景数量
    pub fn scenario_count(&self) -> usize {
        self.scenarios.len()
    }
}

impl Default for ChaosTestRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_storage_intermittent_failure() {
        let scenario = StorageIntermittentFailure {
            failure_rate: 0.3,
            request_count: 100,
            limiter_type: LimiterType::TokenBucket {
                capacity: 100,
                refill_rate: 10,
            },
        };

        let result = scenario.run().await;

        assert_eq!(result.total_requests, 100);
        assert!(result.successful_requests > 0);
        assert!(result.failed_requests > 0);
        // 30% 失败率,应该有大约 60-80 成功, 20-40 失败
        assert!(
            result.successful_requests >= 50 && result.successful_requests <= 90,
            "Expected 50-90 successful, got {}",
            result.successful_requests
        );
    }

    #[tokio::test]
    async fn test_high_latency_injection() {
        let scenario = HighLatencyInjection {
            latency_min_ms: 10,
            latency_max_ms: 50,
            request_count: 20,
        };

        let result = scenario.run().await;

        assert_eq!(result.total_requests, 20);
        assert_eq!(result.successful_requests, 20); // 无失败,只有延迟
        assert!(result.avg_latency_ms >= 10.0);
        assert!(result.max_latency_ms <= 50.0 + 10.0); // 允许一些误差
    }

    #[tokio::test]
    async fn test_burst_traffic() {
        let scenario = BurstTraffic {
            normal_qps: 10,
            burst_multiplier: 5,
            normal_duration_secs: 1,
            burst_duration_secs: 1,
        };

        let result = scenario.run().await;

        // 总共 10 + 50 = 60 请求
        assert_eq!(result.total_requests, 60);
        // 限流器容量100,速率10/s, 应该允许大部分
        assert!(result.successful_requests > 0);
    }

    #[tokio::test]
    async fn test_clock_skew() {
        let scenario = ClockSkew {
            capacity: 10,
            refill_rate: 100,
            clock_jump_seconds: 5,
        };

        let result = scenario.run().await;

        assert_eq!(result.total_requests, 20); // 两轮各10个
                                               // 第一轮消耗10个, 第二轮时钟跳跃后补充,应该全部允许
        assert!(result.successful_requests >= 10);
    }

    #[tokio::test]
    async fn test_cascading_failure() {
        let scenario = CascadingFailure {
            storage_failure_rate: 0.2,
            latency_ms: 20,
            request_count: 50,
        };

        let result = scenario.run().await;

        assert_eq!(result.total_requests, 50);
        assert!(result.successful_requests > 0);
        assert!(result.failed_requests > 0);
        assert!(result.avg_latency_ms >= 20.0);
    }

    #[tokio::test]
    async fn test_chaos_test_runner() {
        let runner = ChaosTestRunner::new()
            .add_scenario(StorageIntermittentFailure {
                failure_rate: 0.1,
                request_count: 20,
                limiter_type: LimiterType::TokenBucket {
                    capacity: 100,
                    refill_rate: 10,
                },
            })
            .add_scenario(ClockSkew {
                capacity: 5,
                refill_rate: 10,
                clock_jump_seconds: 1,
            });

        assert_eq!(runner.scenario_count(), 2);

        let results = runner.run_all().await;
        assert_eq!(results.len(), 2);

        // 验证可以单独运行
        let single = runner.run_scenario("clock_skew").await;
        assert!(single.is_some());

        let nonexistent = runner.run_scenario("nonexistent").await;
        assert!(nonexistent.is_none());
    }
}
