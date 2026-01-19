//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 增强监控指标和告警系统
//!
//! 实现实时监控、性能指标收集和智能告警功能。

use crate::telemetry::Tracer;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicBool};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn, error};

/// 告警级别
#[derive(Debug, Clone, PartialEq)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

/// 告警配置
#[derive(Debug, Clone)]
pub struct AlertConfig {
    /// CPU 使用率告警阈值（0.0-1.0）
    cpu_threshold: f64,
    
    /// 内存使用率告警阈值（0.0-1.0）
    memory_threshold: f64,
    
    /// 请求延迟告警阈值（毫秒）
    latency_threshold_ms: u64,
    
    /// 错误率告警阈值（0.0-0.05）
    error_rate_threshold: f64,
    
    /// 告警冷却时间
    alert_cooldown: Duration,
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
    latency_samples: std::sync::Mutex<LatencySamples>,
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
            latency_samples: std::sync::Mutex::new(LatencySamples::new(1000)), // 保存最近1000个样本
        }
    }

    /// 获取指标快照
    pub fn snapshot(&self) -> MetricsSnapshot {
        let total = self.total_requests.load(std::sync::atomic::Ordering::Relaxed);
        let failed = self.failed_requests.load(std::sync::atomic::Ordering::Relaxed);

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
            cpu_usage: 0.0, // TODO: 实现实际的 CPU 使用率监控
            memory_usage: 0.0, // TODO: 实现实际的内存使用率监控
        }
    }
}

/// 监控系统
pub struct MonitoringSystem {
    /// 性能指标
    metrics: Arc<PerformanceMetrics>,

    /// 告警配置
    alert_config: AlertConfig,

    /// 告警状态
    alert_in_progress: Arc<AtomicBool>,

    /// 最后告警时间
    last_alert_time: Arc<std::sync::Mutex<Instant>>,

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
            last_alert_time: Arc::new(std::sync::Mutex::new(Instant::now())),
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
        let latency = timer.finish();
        self.metrics.successful_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.metrics.total_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.update_latency_stats(latency.as_millis() as u64);

        debug!("请求成功: {}，延迟: {}ms", timer.request_id, latency.as_millis());
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
        if let Ok(mut samples) = self.metrics.latency_samples.lock() {
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

        // 检查各种告警条件
        if metrics.cache_hit_rate < 0.8 {
            alerts.push(AlertLevel::Warning);
        }

        if metrics.error_rate > self.alert_config.error_rate_threshold {
            alerts.push(AlertLevel::Critical);
        }

        if metrics.avg_latency_ms > self.alert_config.latency_threshold_ms {
            alerts.push(AlertLevel::Warning);
        }

        if metrics.cpu_usage > self.alert_config.cpu_threshold {
            alerts.push(AlertLevel::Critical);
        }

        if metrics.memory_usage > self.alert_config.memory_threshold {
            alerts.push(AlertLevel::Warning);
        }

        alerts
    }

    /// 处理告警
    pub async fn handle_alerts(&self, alerts: &[AlertLevel]) {
        if alerts.is_empty() {
            return;
        }

        let now = Instant::now();
        let last_alert = *self.last_alert_time.lock().unwrap();
        let cooldown_elapsed = now.duration_since(last_alert);

        let should_alert = alerts.iter().any(|level| {
            matches!(level, AlertLevel::Critical) ||
                (matches!(level, AlertLevel::Warning) && cooldown_elapsed >= self.alert_config.alert_cooldown)
        });

        if !should_alert {
            return;
        }

        // 更新最后告警时间
        *self.last_alert_time.lock().unwrap() = now;

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

        self.send_alert_notifications(&alerts).await;
    }

    /// 格式化告警级别
    pub fn format_alert_level(level: &AlertLevel) -> String {
        match level {
            AlertLevel::Info => "INFO".to_string(),
            AlertLevel::Warning => "WARNING".to_string(),
            AlertLevel::Critical => "CRITICAL".to_string(),
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
            cpu_threshold: 0.8,
            memory_threshold: 0.7,
            latency_threshold_ms: 100,
            error_rate_threshold: 0.05,
            alert_cooldown: Duration::from_secs(60),
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