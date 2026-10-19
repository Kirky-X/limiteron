// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 增强监控指标和告警系统
//!
//! 实现实时监控、性能指标收集和智能告警功能。

use super::Tracer;
use crate::i18n::t;
use log::{debug, error, info, warn};
use oxcache::infra::metrics::{
    CacheOpResult, CacheOpType, CacheOperation, HistogramData, MetricValue,
    OPERATION_LATENCY_HISTOGRAM,
};
use oxcache::{CacheLayer, UnifiedMetricsRecorder};
use parking_lot::{Mutex as ParkingMutex, RwLock as ParkingRwLock};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

/// limiteron 请求指标在 oxcache service 维度的归因名（R9 service 标签）
const METRICS_SERVICE: &str = "limiteron";

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
    /// 平均请求延迟（毫秒）：延迟直方图 sum/count 均值
    pub avg_latency_ms: u64,
    /// P95 延迟（毫秒）——直方图桶近似值
    ///
    /// 环排序精确值已随 `LatencySamples` 删除：延迟样本经 oxcache
    /// 标准命名直方图（`oxcache_operation_duration_seconds`）承载，
    /// 分位数由累积桶插值估计，精度受桶边界限制（design D1）。
    pub p95_latency_ms: u64,
    /// P99 延迟（毫秒）——直方图桶近似值（降级原因同 `p95_latency_ms`）
    pub p99_latency_ms: u64,
    pub concurrent_requests: u64,
    pub cache_hit_rate: f64,
    pub circuit_breaker_trips: u64,
    pub active_connections: u64,
    pub error_rate: f64,
    pub cpu_usage: f64,
    pub memory_usage: f64,
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
        if line.starts_with("MemTotal:")
            && let Some(value) = line.split_whitespace().nth(1)
            && let Ok(parsed) = value.parse::<u64>()
        {
            total_kb = parsed;
        }

        if line.starts_with("MemAvailable:")
            && let Some(value) = line.split_whitespace().nth(1)
            && let Ok(parsed) = value.parse::<u64>()
        {
            available_kb = parsed;
        }
    }

    if total_kb == 0 {
        return 0.0;
    }

    let used_kb = total_kb.saturating_sub(available_kb);
    (used_kb as f64 / total_kb as f64).clamp(0.0, 1.0)
}

/// 性能指标
///
/// 请求计数与延迟经 oxcache [`UnifiedMetricsRecorder`] 承载（service 归因
/// `limiteron`）：成功请求为 op 命中计数、失败请求为 op 未命中计数，延迟
/// 样本写入 oxcache 标准命名延迟直方图（`oxcache_operation_duration_seconds`）。
/// p95/p99 由直方图累积桶插值近似（非环排序精确值，design D1）。
#[derive(Debug)]
pub struct PerformanceMetrics {
    /// 请求计数与延迟直方图的承载 recorder（全局 unified metrics +
    /// `limiteron` service 维度归因）
    recorder: UnifiedMetricsRecorder,

    /// 并发请求数
    concurrent_requests: AtomicU64,

    /// 缓存命中率
    cache_hit_rate: f64,

    /// 熔断器触发次数
    circuit_breaker_trips: AtomicU64,

    /// 当前活跃连接数
    active_connections: AtomicU64,

    system_sampler: ParkingMutex<SystemMetricsSampler>,
}

impl PerformanceMetrics {
    /// 创建新的性能指标
    pub fn new() -> Self {
        Self {
            recorder: UnifiedMetricsRecorder::global_tagged(METRICS_SERVICE),
            concurrent_requests: AtomicU64::new(0),
            cache_hit_rate: 0.0,
            circuit_breaker_trips: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
            system_sampler: ParkingMutex::new(SystemMetricsSampler::default()),
        }
    }

    /// 创建使用隔离 recorder 的指标（仅测试）
    ///
    /// 计数不落进程级全局 unified metrics——绝对值断言的测试若接全局，
    /// reset 与快照之间的窗口会被并行测试的记账污染（曾以 failed=5≠3 偶发）。
    #[cfg(test)]
    pub(crate) fn new_isolated() -> Self {
        Self {
            recorder: UnifiedMetricsRecorder::new(),
            ..Self::new()
        }
    }

    /// 记录一次成功请求（op 命中计数，service 归因 `limiteron`）
    fn record_request_success(&self) {
        self.recorder.metrics().record_operation_with_service(
            &CacheOperation {
                layer: CacheLayer::L1,
                op_type: CacheOpType::Get,
                result: CacheOpResult::Hit,
            },
            METRICS_SERVICE,
        );
    }

    /// 记录一次失败请求（op 未命中计数；失败不带延迟样本）
    fn record_request_failure(&self) {
        self.recorder.metrics().record_operation_with_service(
            &CacheOperation {
                layer: CacheLayer::L1,
                op_type: CacheOpType::Get,
                result: CacheOpResult::Miss,
            },
            METRICS_SERVICE,
        );
    }
    /// 记录一次延迟样本（毫秒）到 oxcache 标准命名延迟直方图
    fn record_latency_sample(&self, latency_ms: u64) {
        self.recorder
            .metrics()
            .record_latency_seconds(latency_ms as f64 / 1000.0);
    }

    /// 获取指标快照
    pub fn snapshot(&self) -> MetricsSnapshot {
        let counters = self.recorder.metrics().get_counters();
        let total = counters.total_operations;
        let failed = counters.l1_misses;
        let cpu_usage = self.system_sampler.lock().read_cpu_usage();
        let memory_usage = read_memory_usage();
        let (avg_latency_ms, p95_latency_ms, p99_latency_ms) = self.latency_stats_ms();

        MetricsSnapshot {
            total_requests: total,
            successful_requests: counters.l1_hits,
            failed_requests: failed,
            avg_latency_ms,
            p95_latency_ms,
            p99_latency_ms,
            concurrent_requests: self
                .concurrent_requests
                .load(std::sync::atomic::Ordering::Relaxed),
            cache_hit_rate: self.cache_hit_rate,
            circuit_breaker_trips: self
                .circuit_breaker_trips
                .load(std::sync::atomic::Ordering::Relaxed),
            active_connections: self
                .active_connections
                .load(std::sync::atomic::Ordering::Relaxed),
            error_rate: if total > 0 {
                failed as f64 / total as f64
            } else {
                0.0
            },
            cpu_usage,
            memory_usage,
        }
    }

    /// 从标准命名延迟直方图计算 avg/p95/p99（毫秒）
    ///
    /// 无延迟样本时三项均为 0。
    fn latency_stats_ms(&self) -> (u64, u64, u64) {
        let dynamic = self.recorder.metrics().get_dynamic_metrics();
        let Some(MetricValue::Histogram(hist)) = dynamic.get(OPERATION_LATENCY_HISTOGRAM) else {
            return (0, 0, 0);
        };
        let avg = if hist.count > 0 {
            (hist.sum / hist.count as f64 * 1000.0).round() as u64
        } else {
            0
        };
        (
            avg,
            histogram_quantile_ms(hist, 95.0),
            histogram_quantile_ms(hist, 99.0),
        )
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// 从延迟直方图累积桶插值估计分位数（毫秒）
///
/// `rank = p/100 × count`：定位首个累积计数 ≥ rank 的桶，在
/// [前一边界, 本边界] 区间线性插值；rank 超出全部桶时以样本最大值
/// 为近似上界。分位数为桶近似值——精确环排序已随 `LatencySamples`
/// 删除（design D1、R-telemetry-monitoring-002）。
fn histogram_quantile_ms(hist: &HistogramData, p: f64) -> u64 {
    if hist.count == 0 {
        return 0;
    }
    let rank = (p / 100.0) * hist.count as f64;
    let mut lower = 0.0;
    let mut cum_prev = 0u64;
    for &(boundary, cum) in &hist.buckets {
        if (cum as f64) >= rank {
            let span = cum.saturating_sub(cum_prev) as f64;
            let fraction = if span > 0.0 {
                ((rank - cum_prev as f64) / span).clamp(0.0, 1.0)
            } else {
                1.0
            };
            return ((lower + fraction * (boundary - lower)) * 1000.0).round() as u64;
        }
        cum_prev = cum;
        lower = boundary;
    }
    (hist.max * 1000.0).round() as u64
}

/// 监控系统
pub struct MonitoringSystem {
    /// 性能指标
    metrics: Arc<PerformanceMetrics>,

    /// 告警配置
    alert_config: AlertConfig,

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
        self.metrics.record_request_success();
        self.update_latency_stats(latency.as_millis() as u64);

        debug!(
            "request succeeded: {}, latency: {}ms",
            request_id,
            latency.as_millis()
        );
        latency
    }

    /// 记录请求失败
    pub fn record_request_failure(&self, timer: RequestTimer) {
        self.metrics.record_request_failure();

        debug!("request failed: {}", timer.request_id);
    }

    /// 更新延迟统计：延迟样本写入 oxcache 标准命名直方图，
    /// p95/p99 由快照侧的累积桶插值近似（见 `MetricsSnapshot` doc）
    fn update_latency_stats(&self, latency_ms: u64) {
        self.metrics.record_latency_sample(latency_ms);

        debug!(
            "latency stats updated: {}ms (histogram bucket approximation)",
            latency_ms
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
        if let Some(level) = Self::apply_jitter(cpu_level, &mut state.cpu, &self.alert_config) {
            alerts.push(level);
        }

        let memory_level = Self::evaluate_threshold_f64(
            metrics.memory_usage,
            &self.alert_config.memory_thresholds,
            true,
        );
        if let Some(level) = Self::apply_jitter(memory_level, &mut state.memory, &self.alert_config)
        {
            alerts.push(level);
        }

        let latency_level = Self::evaluate_threshold_u64(
            metrics.avg_latency_ms,
            &self.alert_config.latency_thresholds_ms,
        );
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
            matches!(level, AlertLevel::Critical)
                || (matches!(level, AlertLevel::Warning)
                    && cooldown_elapsed >= self.alert_config.alert_cooldown)
        });

        if !should_alert {
            return;
        }

        // 更新最后告警时间
        *self.last_alert_time.lock() = now;

        // 告警日志由 send_alert_notifications 统一输出（J5：此前两处
        // 对同一告警输出完全相同的消息，每条告警必然记两次日志）

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
                    error!(
                        "{}",
                        t(
                            "alert-sent-critical",
                            &[("level", Self::format_alert_level(level).to_string())],
                        )
                    );
                }
                AlertLevel::Warning => {
                    warn!(
                        "{}",
                        t(
                            "alert-sent-warning",
                            &[("level", Self::format_alert_level(level).to_string())],
                        )
                    );
                }
                AlertLevel::Info => {
                    info!(
                        "{}",
                        t(
                            "alert-sent-info",
                            &[("level", Self::format_alert_level(level).to_string())],
                        )
                    );
                }
            }
        }
    }
}

/// 请求计时器
pub struct RequestTimer {
    request_id: String,
    start_time: Instant,
}

impl RequestTimer {
    pub fn new(
        request_id: String,
        _metrics: Arc<PerformanceMetrics>,
        _tracer: Arc<Tracer>,
    ) -> Self {
        Self {
            request_id,
            start_time: Instant::now(),
        }
    }

    pub fn finish(self) -> Duration {
        let duration = self.start_time.elapsed();

        debug!(
            "request completed: {}, elapsed: {:?}",
            self.request_id, duration
        );

        duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[serial_test::serial]
    async fn test_monitoring_system() {
        reset_global_metrics();
        let metrics = Arc::new(PerformanceMetrics::new_isolated());
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
                warning: 15,
                critical: 30,
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
        let slow_timer = monitoring.record_request_start("slow_test_1");
        tokio::time::sleep(Duration::from_millis(200)).await;
        monitoring.record_request_success(slow_timer);

        let alerts = monitoring.check_alerts();
        assert!(!alerts.is_empty());
        assert!(alerts.contains(&AlertLevel::Warning));
    }

    // -- oxcache UnifiedMetricsRecorder 承载语义锚 --

    /// 计数与延迟断言依赖全局 unified 指标状态：序列化并先复位，
    /// 避免并行测试相互污染（oxcache recorder 为进程级共享收集器）
    fn reset_global_metrics() {
        oxcache::infra::convenience::reset();
    }

    fn alert_config_with(jitter: u32) -> AlertConfig {
        AlertConfig {
            cpu_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            memory_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            latency_thresholds_ms: AlertThresholdU64 {
                warning: 999999,
                critical: 9999999,
            },
            error_rate_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            cache_hit_rate_thresholds: AlertThresholdF64 {
                warning: -1.0,
                critical: -2.0,
            },
            alert_cooldown: Duration::ZERO,
            jitter_suppression_count: jitter,
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_request_counts_recorded_via_recorder() {
        reset_global_metrics();
        let metrics = Arc::new(PerformanceMetrics::new_isolated());
        let tracer = Arc::new(Tracer::new(false));
        let monitoring = MonitoringSystem::new(metrics.clone(), tracer, alert_config_with(1));

        for i in 0..10 {
            let timer = monitoring.record_request_start(&format!("ok_{}", i));
            monitoring.record_request_success(timer);
        }
        for i in 0..3 {
            let timer = monitoring.record_request_start(&format!("fail_{}", i));
            monitoring.record_request_failure(timer);
        }

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.successful_requests, 10);
        assert_eq!(snapshot.failed_requests, 3);
        assert_eq!(snapshot.total_requests, 13);
        assert!((snapshot.error_rate - 3.0 / 13.0).abs() < 1e-9);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_percentiles_are_histogram_bucket_approximations() {
        reset_global_metrics();
        let metrics = Arc::new(PerformanceMetrics::new_isolated());
        let tracer = Arc::new(Tracer::new(false));
        let monitoring = MonitoringSystem::new(metrics.clone(), tracer, alert_config_with(1));

        // 20 条 120ms 已知量级延迟：分位数必须落在对应直方图桶
        // 边界区间 (100ms, 250ms] 内，而非环排序精确值 120
        for _ in 0..20 {
            monitoring.update_latency_stats(120);
        }

        let snapshot = metrics.snapshot();
        assert!(
            snapshot.p95_latency_ms > 100 && snapshot.p95_latency_ms <= 250,
            "p95={}ms should fall in the (100, 250] bucket range",
            snapshot.p95_latency_ms
        );
        assert!(
            snapshot.p99_latency_ms > 100 && snapshot.p99_latency_ms <= 250,
            "p99={}ms should fall in the (100, 250] bucket range",
            snapshot.p99_latency_ms
        );
        assert!(
            snapshot.avg_latency_ms > 50 && snapshot.avg_latency_ms <= 150,
            "avg={}ms should be close to 120ms",
            snapshot.avg_latency_ms
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_export_prometheus_format_contains_latency_histogram() {
        // 本测试验证「记录 → 全局收集器 → Prometheus 导出」管道：
        // 必须接全局（隔离实例的样本不会进 export）；断言为 contains 类，
        // 对并行记账污染免疫
        let metrics = Arc::new(PerformanceMetrics::default());
        let tracer = Arc::new(Tracer::new(false));
        let monitoring = MonitoringSystem::new(metrics, tracer, alert_config_with(1));

        for _ in 0..20 {
            monitoring.update_latency_stats(120);
        }

        let output = oxcache::export_prometheus_format();
        assert!(
            output.contains("oxcache_operation_duration_seconds_histogram_sum"),
            "the export must contain the latency histogram sum"
        );
        assert!(
            output.contains("oxcache_operation_duration_seconds_histogram_count"),
            "the export must contain the latency histogram count"
        );
        assert!(
            output.contains("oxcache_operation_duration_seconds_histogram_bucket"),
            "the export must contain the latency histogram bucket"
        );
    }

    // -- Threshold evaluation tests --

    #[test]
    fn test_evaluate_threshold_f64_higher_is_worse() {
        let t = AlertThresholdF64 {
            warning: 0.7,
            critical: 0.9,
        };

        // below warning -> None
        assert!(MonitoringSystem::evaluate_threshold_f64(0.5, &t, true).is_none());
        // above warning -> Warning
        assert_eq!(
            MonitoringSystem::evaluate_threshold_f64(0.8, &t, true),
            Some(AlertLevel::Warning)
        );
        // above critical -> Critical
        assert_eq!(
            MonitoringSystem::evaluate_threshold_f64(0.95, &t, true),
            Some(AlertLevel::Critical)
        );
        // exactly at warning -> Warning
        assert_eq!(
            MonitoringSystem::evaluate_threshold_f64(0.7, &t, true),
            Some(AlertLevel::Warning)
        );
        // exactly at critical -> Critical
        assert_eq!(
            MonitoringSystem::evaluate_threshold_f64(0.9, &t, true),
            Some(AlertLevel::Critical)
        );
    }

    #[test]
    fn test_evaluate_threshold_f64_lower_is_worse() {
        let t = AlertThresholdF64 {
            warning: 0.7,
            critical: 0.4,
        };

        // above warning -> None
        assert!(MonitoringSystem::evaluate_threshold_f64(0.8, &t, false).is_none());
        // below warning -> Warning
        assert_eq!(
            MonitoringSystem::evaluate_threshold_f64(0.6, &t, false),
            Some(AlertLevel::Warning)
        );
        // below critical -> Critical
        assert_eq!(
            MonitoringSystem::evaluate_threshold_f64(0.3, &t, false),
            Some(AlertLevel::Critical)
        );
    }

    #[test]
    fn test_evaluate_threshold_u64_all_levels() {
        let t = AlertThresholdU64 {
            warning: 100,
            critical: 200,
        };

        // below warning -> None
        assert!(MonitoringSystem::evaluate_threshold_u64(50, &t).is_none());
        // above warning -> Warning
        assert_eq!(
            MonitoringSystem::evaluate_threshold_u64(150, &t),
            Some(AlertLevel::Warning)
        );
        // above critical -> Critical
        assert_eq!(
            MonitoringSystem::evaluate_threshold_u64(250, &t),
            Some(AlertLevel::Critical)
        );
        // exactly at critical -> Critical
        assert_eq!(
            MonitoringSystem::evaluate_threshold_u64(200, &t),
            Some(AlertLevel::Critical)
        );
    }

    // -- Apply jitter tests --

    #[test]
    fn test_apply_jitter_none_level() {
        let config = AlertConfig {
            cpu_thresholds: AlertThresholdF64 {
                warning: 0.8,
                critical: 0.95,
            },
            memory_thresholds: AlertThresholdF64 {
                warning: 0.7,
                critical: 0.9,
            },
            latency_thresholds_ms: AlertThresholdU64 {
                warning: 15,
                critical: 30,
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
            jitter_suppression_count: 3,
        };
        let mut state = MetricAlertState {
            last_level: Some(AlertLevel::Warning),
            consecutive: 5,
        };
        let result = MonitoringSystem::apply_jitter(None, &mut state, &config);
        assert!(result.is_none());
        assert!(state.last_level.is_none());
        assert_eq!(state.consecutive, 0);
    }

    #[test]
    fn test_apply_jitter_critical_bypass() {
        let config = AlertConfig {
            cpu_thresholds: AlertThresholdF64 {
                warning: 0.8,
                critical: 0.95,
            },
            memory_thresholds: AlertThresholdF64 {
                warning: 0.7,
                critical: 0.9,
            },
            latency_thresholds_ms: AlertThresholdU64 {
                warning: 15,
                critical: 30,
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
            jitter_suppression_count: 5,
        };
        let mut state = MetricAlertState::default();
        // Critical should bypass jitter suppression
        let result =
            MonitoringSystem::apply_jitter(Some(AlertLevel::Critical), &mut state, &config);
        assert_eq!(result, Some(AlertLevel::Critical));
        assert_eq!(state.consecutive, 1);
    }

    #[test]
    fn test_apply_jitter_consecutive_tracking() {
        let config = AlertConfig {
            cpu_thresholds: AlertThresholdF64 {
                warning: 0.8,
                critical: 0.95,
            },
            memory_thresholds: AlertThresholdF64 {
                warning: 0.7,
                critical: 0.9,
            },
            latency_thresholds_ms: AlertThresholdU64 {
                warning: 15,
                critical: 30,
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
            jitter_suppression_count: 3,
        };
        let mut state = MetricAlertState::default();

        // First occurrence of Warning -> not enough consecutive
        let r1 = MonitoringSystem::apply_jitter(Some(AlertLevel::Warning), &mut state, &config);
        assert!(r1.is_none());
        assert_eq!(state.consecutive, 1);
        assert_eq!(state.last_level, Some(AlertLevel::Warning));

        // Second consecutive -> still not enough (need 3)
        let r2 = MonitoringSystem::apply_jitter(Some(AlertLevel::Warning), &mut state, &config);
        assert!(r2.is_none());
        assert_eq!(state.consecutive, 2);

        // Third consecutive -> threshold met
        let r3 = MonitoringSystem::apply_jitter(Some(AlertLevel::Warning), &mut state, &config);
        assert_eq!(r3, Some(AlertLevel::Warning));
        assert_eq!(state.consecutive, 3);
    }

    #[test]
    fn test_apply_jitter_level_change_resets_counter() {
        let config = AlertConfig {
            cpu_thresholds: AlertThresholdF64 {
                warning: 0.8,
                critical: 0.95,
            },
            memory_thresholds: AlertThresholdF64 {
                warning: 0.7,
                critical: 0.9,
            },
            latency_thresholds_ms: AlertThresholdU64 {
                warning: 15,
                critical: 30,
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
            jitter_suppression_count: 3,
        };
        let mut state = MetricAlertState::default();

        // Build up consecutive warnings
        MonitoringSystem::apply_jitter(Some(AlertLevel::Warning), &mut state, &config);
        MonitoringSystem::apply_jitter(Some(AlertLevel::Warning), &mut state, &config);
        assert_eq!(state.consecutive, 2);

        // Level changes -> counter resets to 1
        let r = MonitoringSystem::apply_jitter(Some(AlertLevel::Critical), &mut state, &config);
        assert_eq!(r, Some(AlertLevel::Critical));
        assert_eq!(state.consecutive, 1);
        assert_eq!(state.last_level, Some(AlertLevel::Critical));
    }

    // -- Format alert level tests --

    #[test]
    fn test_format_alert_level_all() {
        assert_eq!(
            MonitoringSystem::format_alert_level(&AlertLevel::Info),
            "INFO"
        );
        assert_eq!(
            MonitoringSystem::format_alert_level(&AlertLevel::Warning),
            "WARNING"
        );
        assert_eq!(
            MonitoringSystem::format_alert_level(&AlertLevel::Critical),
            "CRITICAL"
        );
    }

    // -- handle_alerts tests --

    #[tokio::test]
    async fn test_handle_alerts_empty() {
        let metrics = Arc::new(PerformanceMetrics::new_isolated());
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
                warning: 15,
                critical: 30,
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
        monitoring.handle_alerts(&[]).await;
    }

    #[tokio::test]
    async fn test_handle_alerts_critical_bypasses_cooldown() {
        let metrics = Arc::new(PerformanceMetrics::new_isolated());
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
                warning: 15,
                critical: 30,
            },
            error_rate_thresholds: AlertThresholdF64 {
                warning: 0.03,
                critical: 0.05,
            },
            cache_hit_rate_thresholds: AlertThresholdF64 {
                warning: 0.8,
                critical: 0.6,
            },
            alert_cooldown: Duration::from_secs(3600),
            jitter_suppression_count: 1,
        };
        let monitoring = MonitoringSystem::new(metrics, tracer, alert_config);
        monitoring.handle_alerts(&[AlertLevel::Critical]).await;
    }

    #[tokio::test]
    async fn test_handle_alerts_all_levels() {
        let metrics = Arc::new(PerformanceMetrics::new_isolated());
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
                warning: 15,
                critical: 30,
            },
            error_rate_thresholds: AlertThresholdF64 {
                warning: 0.03,
                critical: 0.05,
            },
            cache_hit_rate_thresholds: AlertThresholdF64 {
                warning: 0.8,
                critical: 0.6,
            },
            alert_cooldown: Duration::ZERO,
            jitter_suppression_count: 1,
        };
        let monitoring = MonitoringSystem::new(metrics, tracer, alert_config);
        monitoring
            .handle_alerts(&[AlertLevel::Info, AlertLevel::Warning, AlertLevel::Critical])
            .await;
    }

    // -- send_alert_notifications test --

    #[tokio::test]
    async fn test_send_alert_notifications_all_levels() {
        let metrics = Arc::new(PerformanceMetrics::new_isolated());
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
                warning: 15,
                critical: 30,
            },
            error_rate_thresholds: AlertThresholdF64 {
                warning: 0.03,
                critical: 0.05,
            },
            cache_hit_rate_thresholds: AlertThresholdF64 {
                warning: 0.8,
                critical: 0.6,
            },
            alert_cooldown: Duration::ZERO,
            jitter_suppression_count: 1,
        };
        let monitoring = MonitoringSystem::new(metrics, tracer, alert_config);
        monitoring
            .send_alert_notifications(&[
                AlertLevel::Info,
                AlertLevel::Warning,
                AlertLevel::Critical,
            ])
            .await;
    }

    // -- check_alerts comprehensive tests --

    #[test]
    fn test_check_alerts_all_normal_no_alerts() {
        let metrics = Arc::new(PerformanceMetrics::new_isolated());
        let tracer = Arc::new(Tracer::new(false));
        // thresholds that should never trigger on a normal system
        let alert_config = AlertConfig {
            cpu_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            memory_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            latency_thresholds_ms: AlertThresholdU64 {
                warning: 999999,
                critical: 9999999,
            },
            error_rate_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            cache_hit_rate_thresholds: AlertThresholdF64 {
                warning: -1.0,
                critical: -2.0,
            },
            alert_cooldown: Duration::from_secs(0),
            jitter_suppression_count: 1,
        };
        let monitoring = MonitoringSystem::new(metrics, tracer, alert_config);
        let alerts = monitoring.check_alerts();
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_check_alerts_cache_hit_rate_trigger() {
        let metrics = Arc::new(PerformanceMetrics::new_isolated());
        let tracer = Arc::new(Tracer::new(false));
        // Only cache_hit_rate threshold is triggerable
        let alert_config = AlertConfig {
            cpu_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            memory_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            latency_thresholds_ms: AlertThresholdU64 {
                warning: 999999,
                critical: 9999999,
            },
            error_rate_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            cache_hit_rate_thresholds: AlertThresholdF64 {
                warning: 0.8,
                critical: 0.6,
            },
            alert_cooldown: Duration::from_secs(0),
            jitter_suppression_count: 1,
        };
        let monitoring = MonitoringSystem::new(metrics, tracer, alert_config);
        // Default cache_hit_rate is 0.0, which is <= 0.6 critical (lower_is_worse)
        let alerts = monitoring.check_alerts();
        assert!(!alerts.is_empty());
        assert!(alerts.contains(&AlertLevel::Critical));
    }

    #[test]
    #[serial_test::serial]
    fn test_check_alerts_error_rate_critical() {
        reset_global_metrics();
        let metrics = Arc::new(PerformanceMetrics::new_isolated());
        let tracer = Arc::new(Tracer::new(false));
        // Trigger only error rate
        let alert_config = AlertConfig {
            cpu_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            memory_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            latency_thresholds_ms: AlertThresholdU64 {
                warning: 999999,
                critical: 9999999,
            },
            error_rate_thresholds: AlertThresholdF64 {
                warning: 0.1,
                critical: 0.2,
            },
            cache_hit_rate_thresholds: AlertThresholdF64 {
                warning: -1.0,
                critical: -2.0,
            },
            alert_cooldown: Duration::from_secs(0),
            jitter_suppression_count: 1,
        };
        let monitoring = MonitoringSystem::new(metrics, tracer, alert_config);

        // record failures to create high error rate
        let timer = monitoring.record_request_start("fail1");
        monitoring.record_request_failure(timer);
        let timer = monitoring.record_request_start("fail2");
        monitoring.record_request_failure(timer);
        let timer = monitoring.record_request_start("fail3");
        monitoring.record_request_failure(timer);

        // error_rate = 3/3 = 1.0 >= 0.2 -> Critical
        let alerts = monitoring.check_alerts();
        assert!(!alerts.is_empty());
        assert!(alerts.contains(&AlertLevel::Critical));
    }

    #[test]
    #[serial_test::serial]
    fn test_check_alerts_latency_critical() {
        reset_global_metrics();
        let metrics = Arc::new(PerformanceMetrics::new_isolated());
        let tracer = Arc::new(Tracer::new(false));
        let alert_config = AlertConfig {
            cpu_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            memory_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            latency_thresholds_ms: AlertThresholdU64 {
                warning: 5,
                critical: 10,
            },
            error_rate_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            cache_hit_rate_thresholds: AlertThresholdF64 {
                warning: -1.0,
                critical: -2.0,
            },
            alert_cooldown: Duration::from_secs(0),
            jitter_suppression_count: 1,
        };
        let monitoring = MonitoringSystem::new(metrics, tracer, alert_config);

        // Record a slow request to bump avg latency
        // avg = (0*9 + 200)/10 = 20 >= critical=10
        let timer = monitoring.record_request_start("slow");
        std::thread::sleep(Duration::from_millis(200));
        monitoring.record_request_success(timer);

        let alerts = monitoring.check_alerts();
        assert!(!alerts.is_empty());
        assert!(alerts.contains(&AlertLevel::Critical));
    }

    // -- PerformanceMetrics tests --

    #[test]
    #[serial_test::serial]
    fn test_performance_metrics_new() {
        reset_global_metrics();
        let metrics = PerformanceMetrics::new();
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_requests, 0);
        assert_eq!(snapshot.successful_requests, 0);
        assert_eq!(snapshot.failed_requests, 0);
        assert_eq!(snapshot.avg_latency_ms, 0);
        assert_eq!(snapshot.error_rate, 0.0);
    }

    #[test]
    #[serial_test::serial]
    fn test_snapshot_error_rate_with_total() {
        reset_global_metrics();
        let metrics = Arc::new(PerformanceMetrics::new_isolated());
        // Manually set values through recorded events
        // fail 3, no success -> total=3
        let tracer = Arc::new(Tracer::new(false));
        let dummy_config = AlertConfig {
            cpu_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            memory_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            latency_thresholds_ms: AlertThresholdU64 {
                warning: 999999,
                critical: 9999999,
            },
            error_rate_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            cache_hit_rate_thresholds: AlertThresholdF64 {
                warning: -1.0,
                critical: -2.0,
            },
            alert_cooldown: Duration::from_secs(0),
            jitter_suppression_count: 1,
        };
        let monitoring = MonitoringSystem::new(metrics.clone(), tracer, dummy_config);
        monitoring.record_request_failure(monitoring.record_request_start("f1"));
        monitoring.record_request_failure(monitoring.record_request_start("f2"));
        monitoring.record_request_failure(monitoring.record_request_start("f3"));

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_requests, 3);
        assert_eq!(snapshot.failed_requests, 3);
        assert!(snapshot.error_rate > 0.0);
    }

    // -- RequestTimer test --

    #[test]
    fn test_request_timer_finish() {
        let metrics = Arc::new(PerformanceMetrics::new_isolated());
        let tracer = Arc::new(Tracer::new(false));
        let timer = RequestTimer::new("test_timer".to_string(), metrics, tracer);
        let duration = timer.finish();
        assert!(duration >= Duration::ZERO);
    }

    // -- Read memory/cpu functions (existence tests) --

    #[test]
    fn test_read_memory_usage() {
        let usage = read_memory_usage();
        assert!(usage >= 0.0);
        assert!(usage <= 1.0);
    }

    #[test]
    fn test_system_sampler_initially_zero() {
        let mut sampler = SystemMetricsSampler::default();
        // First call returns 0.0 since last_total == 0
        let usage = sampler.read_cpu_usage();
        assert_eq!(usage, 0.0);
        // Second call should compute a real value (system is running)
        let usage2 = sampler.read_cpu_usage();
        assert!(usage2 >= 0.0);
        assert!(usage2 <= 1.0);
    }

    // -- Debug formatting tests --

    #[test]
    fn test_metrics_snapshot_debug_clone() {
        let snapshot = MetricsSnapshot {
            total_requests: 100,
            successful_requests: 90,
            failed_requests: 10,
            avg_latency_ms: 50,
            p95_latency_ms: 100,
            p99_latency_ms: 200,
            concurrent_requests: 5,
            cache_hit_rate: 0.85,
            circuit_breaker_trips: 1,
            active_connections: 42,
            error_rate: 0.1,
            cpu_usage: 0.5,
            memory_usage: 0.6,
        };
        let cloned = snapshot.clone();
        assert_eq!(cloned.total_requests, 100);
        assert_eq!(cloned.error_rate, 0.1);
    }

    #[test]
    #[serial_test::serial]
    fn test_check_alerts_multiple_checks() {
        reset_global_metrics();
        let metrics = Arc::new(PerformanceMetrics::new_isolated());
        let tracer = Arc::new(Tracer::new(false));
        let alert_config = AlertConfig {
            cpu_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            memory_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            latency_thresholds_ms: AlertThresholdU64 {
                warning: 5,
                critical: 10,
            },
            error_rate_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            cache_hit_rate_thresholds: AlertThresholdF64 {
                warning: -1.0,
                critical: -2.0,
            },
            alert_cooldown: Duration::from_secs(0),
            jitter_suppression_count: 1,
        };
        let monitoring = MonitoringSystem::new(metrics, tracer, alert_config);

        // No alerts initially
        assert!(monitoring.check_alerts().is_empty());

        // Add a slow request (avg = (0*9 + 200)/10 = 20 >= critical=10)
        let timer = monitoring.record_request_start("slow");
        std::thread::sleep(Duration::from_millis(200));
        monitoring.record_request_success(timer);

        // Now latency alert should trigger
        let alerts = monitoring.check_alerts();
        assert!(!alerts.is_empty());
    }

    /// 覆盖 check_alerts 中 cpu (line 471) 和 memory (line 481) 的 alerts.push(level) 路径
    /// 设置 critical = 0.0 使得任何 cpu/memory 使用率都触发 Critical 告警
    #[test]
    fn test_check_alerts_cpu_memory_critical() {
        let metrics = Arc::new(PerformanceMetrics::new_isolated());
        let tracer = Arc::new(Tracer::new(false));
        let alert_config = AlertConfig {
            // critical = 0.0 使得 cpu_usage >= 0.0 恒为 true -> Critical
            cpu_thresholds: AlertThresholdF64 {
                warning: 0.0,
                critical: 0.0,
            },
            // critical = 0.0 使得 memory_usage >= 0.0 恒为 true -> Critical
            memory_thresholds: AlertThresholdF64 {
                warning: 0.0,
                critical: 0.0,
            },
            // 其他阈值设为不触发
            latency_thresholds_ms: AlertThresholdU64 {
                warning: 999_999,
                critical: 9_999_999,
            },
            error_rate_thresholds: AlertThresholdF64 {
                warning: 2.0,
                critical: 3.0,
            },
            cache_hit_rate_thresholds: AlertThresholdF64 {
                warning: -1.0,
                critical: -2.0,
            },
            alert_cooldown: Duration::from_secs(0),
            jitter_suppression_count: 1,
        };
        let monitoring = MonitoringSystem::new(metrics, tracer, alert_config);

        // cpu_usage 首次调用返回 0.0，0.0 >= 0.0 触发 Critical
        // memory_usage 从 /proc/meminfo 读取非零值，>= 0.0 触发 Critical
        // Critical 级别在 apply_jitter 中直接返回 Some(level)，无需累积
        let alerts = monitoring.check_alerts();
        assert!(
            alerts.contains(&AlertLevel::Critical),
            "expected Critical alert from cpu/memory, got: {:?}",
            alerts
        );
    }
}
