//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 增强监控指标和告警系统
//!
//! 实现实时监控、性能指标收集和智能告警功能。

use super::Tracer;
use parking_lot::{Mutex as ParkingMutex, RwLock as ParkingRwLock};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicBool};
use std::time::{Duration, Instant};
use log::{debug, info, warn, error};

/// 告警级别
#[derive(Debug, Clone, PartialEq)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
pub struct AlertThresholdF64 {
    warning: f64,
    critical: f64,
}

#[derive(Debug, Clone)]
pub struct AlertThresholdU64 {
    warning: u64,
    critical: u64,
}

/// 告警配置
#[derive(Debug, Clone)]
pub struct AlertConfig {
    cpu_thresholds: AlertThresholdF64,
    memory_thresholds: AlertThresholdF64,
    latency_thresholds_ms: AlertThresholdU64,
    error_rate_thresholds: AlertThresholdF64,
    cache_hit_rate_thresholds: AlertThresholdF64,
    alert_cooldown: Duration,
    jitter_suppression_count: u32,
}

#[derive(Debug, Default)]
struct MetricAlertState {
    last_level: Option<AlertLevel>,
    consecutive: u32,
}

#[derive(Debug, Default)]
struct AlertState {
    cpu: MetricAlertState,
    memory: MetricAlertState,
    latency: MetricAlertState,
    error_rate: MetricAlertState,
    cache_hit_rate: MetricAlertState,
}

/// 性能指标快照
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub avg_latency_ms: u64,
    pub p95_latency_ms: u64,
    pub p99_latency_ms: u64,
    pub concurrent_requests: u64,
    pub cache_hit_rate: f64,
    pub circuit_breaker_trips: u64,
    pub active_connections: u64,
    pub error_rate: f64,
    pub cpu_usage: f64,
    pub memory_usage: f64,
}

/// 延迟样本收集器
///
/// 用于计算真正的 P95/P99 延迟百分位数
struct LatencySamples {
    samples: Vec<u64>,
    max_samples: usize,
}

impl std::fmt::Debug for LatencySamples {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LatencySamples")
            .field("samples_count", &self.samples.len())
            .field("max_samples", &self.max_samples)
            .finish()
    }
}

impl Default for LatencySamples {
    fn default() -> Self {
        Self {
            samples: Vec::new(),
            max_samples: 10000,
        }
    }
}

impl LatencySamples {
    fn new(max_samples: usize) -> Self {
        Self {
            samples: Vec::with_capacity(max_samples),
            max_samples,
        }
    }

    fn add_sample(&mut self, latency_ms: u64) {
        if self.samples.len() >= self.max_samples {
            // 如果样本已满，移除最旧的样本（FIFO）
            self.samples.remove(0);
        }
        self.samples.push(latency_ms);
    }

    /// 计算百分位数
    fn percentile(&self, p: f64) -> u64 {
        if self.samples.is_empty() {
            return 0;
        }

        let mut sorted = self.samples.clone();
        sorted.sort_unstable();

        let index = ((sorted.len() as f64) * p / 100.0) as usize;
        sorted.get(index).copied().unwrap_or(0)
    }

    fn p95(&self) -> u64 {
        self.percentile(95.0)
    }

    fn p99(&self) -> u64 {
        self.percentile(99.0)
    }
}

#[derive(Debug, Default)]
struct SystemMetricsSampler {
    last_total: u64,
    last_idle: u64,
}

impl SystemMetricsSampler {
    fn read_cpu_usage(&mut self) -> f64 {
        let content = match std::fs::read_to_string("/proc/stat") {
            Ok(value) => value,
            Err(_) => return 0.0,
        };

        let mut parts = match content.lines().next() {
            Some(line) => line.split_whitespace(),
            None => return 0.0,
        };

        let label = match parts.next() {
            Some(value) => value,
            None => return 0.0,
        };

        if label != "cpu" {
            return 0.0;
        }

        let mut total: u64 = 0;
        let mut idle: u64 = 0;

        for (index, value) in parts.enumerate() {
            let parsed: u64 = match value.parse() {
                Ok(v) => v,
                Err(_) => return 0.0,
            };

            total = total.saturating_add(parsed);
            if index == 3 {
                idle = idle.saturating_add(parsed);
            }
            if index == 4 {
                idle = idle.saturating_add(parsed);
            }
        }

        if self.last_total == 0 {
            self.last_total = total;
            self.last_idle = idle;
            return 0.0;
        }

        let total_delta = total.saturating_sub(self.last_total);
        let idle_delta = idle.saturating_sub(self.last_idle);
        self.last_total = total;
        self.last_idle = idle;

        if total_delta == 0 {
            return 0.0;
        }

        let usage = 1.0 - (idle_delta as f64 / total_delta as f64);
        usage.clamp(0.0, 1.0)
    }
}

fn read_memory_usage() -> f64 {
    let content = match std::fs::read_to_string("/proc/meminfo") {
        Ok(value) => value,
        Err(_) => return 0.0,
    };

    let mut total_kb: u64 = 0;
    let mut available_kb: u64 = 0;

    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            if let Some(value) = line.split_whitespace().nth(1) {
                if let Ok(parsed) = value.parse::<u64>() {
                    total_kb = parsed;
                }
            }
        }

        if line.starts_with("MemAvailable:") {
            if let Some(value) = line.split_whitespace().nth(1) {
                if let Ok(parsed) = value.parse::<u64>() {
                    available_kb = parsed;
                }
            }
        }
    }

    if total_kb == 0 {
        return 0.0;
    }

    let used_kb = total_kb.saturating_sub(available_kb);
    (used_kb as f64 / total_kb as f64).clamp(0.0, 1.0)
}

/// 性能指标
#[derive(Debug, Default)]
pub struct PerformanceMetrics {
    /// 请求总数
    total_requests: AtomicU64,

    /// 成功请求数
    successful_requests: AtomicU64,

    /// 失败请求数
    failed_requests: AtomicU64,

    /// 平均请求延迟
    avg_latency_ms: AtomicU64,

    /// P95 延迟
    p95_latency_ms: AtomicU64,

    /// P99 延迟
    p99_latency_ms: AtomicU64,

    /// 并发请求数
    concurrent_requests: AtomicU64,

    /// 缓存命中率
    cache_hit_rate: f64,

    /// 熔断器触发次数
    circuit_breaker_trips: AtomicU64,

    /// 当前活跃连接数
    active_connections: AtomicU64,

    /// 延迟样本（用于计算真正的百分位数）
    latency_samples: ParkingMutex<LatencySamples>,

    system_sampler: ParkingMutex<SystemMetricsSampler>,
}
impl PerformanceMetrics {
    /// 创建新的性能指标
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            successful_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            avg_latency_ms: AtomicU64::new(0),
            p95_latency_ms: AtomicU64::new(0),
            p99_latency_ms: AtomicU64::new(0),
            concurrent_requests: AtomicU64::new(0),
            cache_hit_rate: 0.0,
            circuit_breaker_trips: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
            latency_samples: ParkingMutex::new(LatencySamples::new(1000)), // 保存最近1000个样本
            system_sampler: ParkingMutex::new(SystemMetricsSampler::default()),
        }
    }

    /// 获取指标快照
    pub fn snapshot(&self) -> MetricsSnapshot {
        let total = self.total_requests.load(std::sync::atomic::Ordering::Relaxed);
        let failed = self.failed_requests.load(std::sync::atomic::Ordering::Relaxed);
        let cpu_usage = self.system_sampler.lock().read_cpu_usage();
        let memory_usage = read_memory_usage();

        MetricsSnapshot {
            total_requests: total,
            successful_requests: self.successful_requests.load(std::sync::atomic::Ordering::Relaxed),
            failed_requests: failed,
            avg_latency_ms: self.avg_latency_ms.load(std::sync::atomic::Ordering::Relaxed),
            p95_latency_ms: self.p95_latency_ms.load(std::sync::atomic::Ordering::Relaxed),
            p99_latency_ms: self.p99_latency_ms.load(std::sync::atomic::Ordering::Relaxed),
            concurrent_requests: self.concurrent_requests.load(std::sync::atomic::Ordering::Relaxed),
            cache_hit_rate: self.cache_hit_rate,
            circuit_breaker_trips: self.circuit_breaker_trips.load(std::sync::atomic::Ordering::Relaxed),
            active_connections: self.active_connections.load(std::sync::atomic::Ordering::Relaxed),
            error_rate: if total > 0 { failed as f64 / total as f64 } else { 0.0 },
            cpu_usage,
            memory_usage,
        }
    }
}

/// 监控系统
#[allow(dead_code)]
pub struct MonitoringSystem {
    /// 性能指标
    metrics: Arc<PerformanceMetrics>,

    /// 告警配置
    alert_config: AlertConfig,

    /// 告警状态
    alert_in_progress: Arc<AtomicBool>,

    /// 最后告警时间
    last_alert_time: Arc<ParkingMutex<Instant>>,

    alert_state: Arc<ParkingRwLock<AlertState>>,

    /// 遥踪器
    tracer: Arc<Tracer>,
}

impl MonitoringSystem {
    /// 创建新的监控告警系统
    pub fn new(
        metrics: Arc<PerformanceMetrics>,
        tracer: Arc<Tracer>,
        alert_config: AlertConfig,
    ) -> Self {
        Self {
            metrics,
            tracer,
            alert_config,
            alert_in_progress: Arc::new(AtomicBool::new(false)),
            last_alert_time: Arc::new(ParkingMutex::new(Instant::now())),
            alert_state: Arc::new(ParkingRwLock::new(AlertState::default())),
        }
    }

    /// 记录请求开始
    pub fn record_request_start(&self, request_id: &str) -> RequestTimer {
        RequestTimer::new(
            request_id.to_string(),
            self.metrics.clone(),
            self.tracer.clone(),
        )
    }

    /// 记录请求成功
    pub fn record_request_success(&self, timer: RequestTimer) -> Duration {
        let request_id = timer.request_id.clone();
        let latency = timer.finish();
        self.metrics.successful_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.metrics.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.update_latency_stats(latency.as_millis() as u64);

        debug!("请求成功: {}，延迟: {}ms", request_id, latency.as_millis());
        latency
    }

    /// 记录请求失败
    pub fn record_request_failure(&self, timer: RequestTimer) {
        self.metrics.failed_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.metrics.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        debug!("请求失败: {}", timer.request_id);
    }

    /// 更新延迟统计
    fn update_latency_stats(&self, latency_ms: u64) {
        let current_avg = self.metrics.avg_latency_ms.load(std::sync::atomic::Ordering::Relaxed);
        let new_avg = ((current_avg * 9) + latency_ms) / 10;
        self.metrics.avg_latency_ms.store(new_avg, std::sync::atomic::Ordering::Relaxed);

        // 添加延迟样本
        {
            let mut samples = self.metrics.latency_samples.lock();
            samples.add_sample(latency_ms);

            // 计算真正的 P95 和 P99
            let p95 = samples.p95();
            let p99 = samples.p99();

            self.metrics.p95_latency_ms.store(p95, std::sync::atomic::Ordering::Relaxed);
            self.metrics.p99_latency_ms.store(p99, std::sync::atomic::Ordering::Relaxed);
        }

        debug!("更新延迟统计: P95={}ms, P99={}ms",
            self.metrics.p95_latency_ms.load(std::sync::atomic::Ordering::Relaxed),
            self.metrics.p99_latency_ms.load(std::sync::atomic::Ordering::Relaxed)
        );
    }

    /// 检查告警条件
    pub fn check_alerts(&self) -> Vec<AlertLevel> {
        let mut alerts = Vec::new();

        let metrics = self.metrics.snapshot();
        let mut state = self.alert_state.write();

        let cpu_level = Self::evaluate_threshold_f64(
            metrics.cpu_usage,
            &self.alert_config.cpu_thresholds,
            true,
        );
        if let Some(level) =
            Self::apply_jitter(cpu_level, &mut state.cpu, &self.alert_config)
        {
            alerts.push(level);
        }

        let memory_level = Self::evaluate_threshold_f64(
            metrics.memory_usage,
            &self.alert_config.memory_thresholds,
            true,
        );
        if let Some(level) =
            Self::apply_jitter(memory_level, &mut state.memory, &self.alert_config)
        {
            alerts.push(level);
        }

        let latency_level =
            Self::evaluate_threshold_u64(metrics.avg_latency_ms, &self.alert_config.latency_thresholds_ms);
        if let Some(level) =
            Self::apply_jitter(latency_level, &mut state.latency, &self.alert_config)
        {
            alerts.push(level);
        }

        let error_rate_level = Self::evaluate_threshold_f64(
            metrics.error_rate,
            &self.alert_config.error_rate_thresholds,
            true,
        );
        if let Some(level) =
            Self::apply_jitter(error_rate_level, &mut state.error_rate, &self.alert_config)
        {
            alerts.push(level);
        }

        let cache_hit_rate_level = Self::evaluate_threshold_f64(
            metrics.cache_hit_rate,
            &self.alert_config.cache_hit_rate_thresholds,
            false,
        );
        if let Some(level) = Self::apply_jitter(
            cache_hit_rate_level,
            &mut state.cache_hit_rate,
            &self.alert_config,
        ) {
            alerts.push(level);
        }

        alerts
    }

    /// 处理告警
    pub async fn handle_alerts(&self, alerts: &[AlertLevel]) {
        if alerts.is_empty() {
            return;
        }

        let now = Instant::now();
        let last_alert = *self.last_alert_time.lock();
        let cooldown_elapsed = now.duration_since(last_alert);

        let should_alert = alerts.iter().any(|level| {
            matches!(level, AlertLevel::Critical) ||
                (matches!(level, AlertLevel::Warning) && cooldown_elapsed >= self.alert_config.alert_cooldown)
        });

        if !should_alert {
            return;
        }

        // 更新最后告警时间
        *self.last_alert_time.lock() = now;

        // 记录告警
        for level in alerts {
            match level {
                AlertLevel::Critical => {
                    error!("发送严重告警: {}", Self::format_alert_level(level));
                    debug!("严重告警级别: {}", Self::format_alert_level(level));
                }
                AlertLevel::Warning => {
                    warn!("发送警告告警: {}", Self::format_alert_level(level));
                    debug!("警告告警级别: {}", Self::format_alert_level(level));
                }
                AlertLevel::Info => {
                    info!("发送信息告警: {}", Self::format_alert_level(level));
                    debug!("信息告警级别: {}", Self::format_alert_level(level));
                }
            }
        }

        self.send_alert_notifications(alerts).await;
    }

    /// 格式化告警级别
    pub fn format_alert_level(level: &AlertLevel) -> String {
        match level {
            AlertLevel::Info => "INFO".to_string(),
            AlertLevel::Warning => "WARNING".to_string(),
            AlertLevel::Critical => "CRITICAL".to_string(),
        }
    }

    fn evaluate_threshold_f64(
        value: f64,
        thresholds: &AlertThresholdF64,
        higher_is_worse: bool,
    ) -> Option<AlertLevel> {
        if higher_is_worse {
            if value >= thresholds.critical {
                Some(AlertLevel::Critical)
            } else if value >= thresholds.warning {
                Some(AlertLevel::Warning)
            } else {
                None
            }
        } else if value <= thresholds.critical {
            Some(AlertLevel::Critical)
        } else if value <= thresholds.warning {
            Some(AlertLevel::Warning)
        } else {
            None
        }
    }

    fn evaluate_threshold_u64(value: u64, thresholds: &AlertThresholdU64) -> Option<AlertLevel> {
        if value >= thresholds.critical {
            Some(AlertLevel::Critical)
        } else if value >= thresholds.warning {
            Some(AlertLevel::Warning)
        } else {
            None
        }
    }

    fn apply_jitter(
        level: Option<AlertLevel>,
        state: &mut MetricAlertState,
        config: &AlertConfig,
    ) -> Option<AlertLevel> {
        let Some(level) = level else {
            state.last_level = None;
            state.consecutive = 0;
            return None;
        };

        if state.last_level.as_ref() == Some(&level) {
            state.consecutive = state.consecutive.saturating_add(1);
        } else {
            state.last_level = Some(level.clone());
            state.consecutive = 1;
        }

        if matches!(level, AlertLevel::Critical) {
            return Some(level);
        }

        let required = config.jitter_suppression_count.max(1);
        if state.consecutive >= required {
            Some(level)
        } else {
            None
        }
    }

    /// 发送告警通知
    async fn send_alert_notifications(&self, alerts: &[AlertLevel]) {
        // 这里可以实现邮件、Slack、Webhook 等通知
        for level in alerts {
            match level {
                AlertLevel::Critical => {
                    error!("发送严重告警: {}", Self::format_alert_level(level));
                }
                AlertLevel::Warning => {
                    warn!("发送警告告警: {}", Self::format_alert_level(level));
                }
                AlertLevel::Info => {
                    info!("发送信息告警: {}", Self::format_alert_level(level));
                }
            }
        }
    }
}

/// 请求计时器
#[allow(dead_code)]
pub struct RequestTimer {
    request_id: String,
    start_time: Instant,
    metrics: Arc<PerformanceMetrics>,
    tracer: Arc<Tracer>,
}

impl RequestTimer {
    pub fn new(
        request_id: String,
        metrics: Arc<PerformanceMetrics>,
        tracer: Arc<Tracer>,
    ) -> Self {
        Self {
            request_id,
            start_time: Instant::now(),
            metrics,
            tracer,
        }
    }

    pub fn finish(self) -> Duration {
        let duration = self.start_time.elapsed();

        debug!("请求完成: {}，耗时: {:?}", self.request_id, duration);

        duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_monitoring_system() {
        let metrics = Arc::new(PerformanceMetrics::default());
        let tracer = Arc::new(Tracer::new(false));
        let alert_config = AlertConfig {
            cpu_thresholds: AlertThresholdF64 {
                warning: 0.8,
                critical: 0.95,
            },
            memory_thresholds: AlertThresholdF64 {
                warning: 0.7,
                critical: 0.9,
            },
            latency_thresholds_ms: AlertThresholdU64 {
                warning: 100,
                critical: 300,
            },
            error_rate_thresholds: AlertThresholdF64 {
                warning: 0.03,
                critical: 0.05,
            },
            cache_hit_rate_thresholds: AlertThresholdF64 {
                warning: 0.8,
                critical: 0.6,
            },
            alert_cooldown: Duration::from_secs(60),
            jitter_suppression_count: 1,
        };
        let monitoring = MonitoringSystem::new(metrics, tracer, alert_config);

        // 模拟一些请求
        for i in 0..10 {
            let timer = monitoring.record_request_start(&format!("test_{}", i));

            // 模拟成功请求
            tokio::time::sleep(Duration::from_millis(10)).await;
            monitoring.record_request_success(timer);
        }

        // 模拟失败请求
        for i in 0..3 {
            let timer = monitoring.record_request_start(&format!("test_fail_{}", i));
            tokio::time::sleep(Duration::from_millis(50)).await;
            monitoring.record_request_failure(timer);
        }

        // 等待统计稳定
        tokio::time::sleep(Duration::from_millis(100)).await;

        let snapshot = monitoring.metrics.snapshot();
        assert_eq!(snapshot.successful_requests, 10);
        assert_eq!(snapshot.failed_requests, 3);
        assert_eq!(snapshot.total_requests, 13);

        // 测试告警触发
        // 故意制造高延迟请求
        let slow_timer = monitoring.record_request_start(&format!("slow_test_1"));
        tokio::time::sleep(Duration::from_millis(200)).await;
        monitoring.record_request_success(slow_timer);

        let alerts = monitoring.check_alerts();
        assert!(!alerts.is_empty());
        assert!(alerts.contains(&AlertLevel::Warning));
    }
}
