//! 混沌测试模块
//!
//! 提供故障注入、延迟注入等混沌测试基础设施,验证系统在异常条件下的行为。
//!
//! ## 混沌场景
//! 1. 存储间歇性失败 - 模拟Redis连接丢失
//! 2. 高延迟注入 - 模拟网络分区
//! 3. 突发流量 - 模拟DDoS场景
//! 4. 时钟偏移 - 测试时钟边界情况
//! 5. 级联故障 - 多个组件同时故障
//!
//! ## 使用方法
//! ```ignore
//! use limiteron_chaos::FaultInjectionStorage;
//!
//! let fault_storage = FaultInjectionStorage::builder()
//!     .with_inner(storage)
//!     .with_failure_rate(0.1)  // 10% 失败率
//!     .with_latency_range(100, 500)  // 100-500ms 延迟
//!     .build();
//! ```

pub mod fault_injection;
pub mod latency;
pub mod scenarios;

pub use fault_injection::FaultPattern;

/// 混沌测试配置
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChaosConfig {
    /// 故障注入率 (0.0 - 1.0)
    pub failure_rate: f64,
    /// 延迟范围 (ms)
    pub latency_min_ms: u64,
    pub latency_max_ms: u64,
    /// 故障模式
    pub fault_pattern: FaultPattern,
    /// 测试持续时间 (秒)
    pub duration_secs: u64,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            failure_rate: 0.05,
            latency_min_ms: 0,
            latency_max_ms: 100,
            fault_pattern: FaultPattern::Random,
            duration_secs: 30,
        }
    }
}

/// 混沌测试结果
#[derive(Debug)]
pub struct ChaosTestResult {
    /// 总请求数
    pub total_requests: u64,
    /// 成功请求数
    pub successful_requests: u64,
    /// 失败请求数
    pub failed_requests: u64,
    /// 超时请求数
    pub timed_out_requests: u64,
    /// 平均延迟 (ms)
    pub avg_latency_ms: f64,
    /// P99 延迟 (ms)
    pub p99_latency_ms: f64,
    /// 最大延迟 (ms)
    pub max_latency_ms: f64,
}

impl ChaosTestResult {
    /// 成功率
    #[allow(dead_code)]
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            return 0.0;
        }
        self.successful_requests as f64 / self.total_requests as f64
    }

    /// 失败率
    #[allow(dead_code)]
    pub fn failure_rate(&self) -> f64 {
        1.0 - self.success_rate()
    }
}
