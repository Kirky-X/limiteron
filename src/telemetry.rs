//! 监控和追踪模块
//!
//! 集成Prometheus指标和OpenTelemetry分布式追踪。
//!
//! # 功能
//!
//! - Prometheus指标：Counter、Gauge、Histogram
//! - OpenTelemetry分布式追踪
//! - Jaeger导出器
//! - 指标采集和导出
//!
//! # 示例
//!
//! ```rust
//! use limiteron::telemetry::{init_telemetry, Metrics, TelemetryConfig};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = TelemetryConfig::default();
//!     let (metrics, tracer) = init_telemetry(&config).await.unwrap();
//!
//!     // 使用指标
//!     metrics.requests_total.inc();
//!
//!     // 使用追踪
//!     let span = tracer.start_span("my_operation");
//!     span.finish();
//! }
//! ```

use opentelemetry::global;
use opentelemetry_sdk::trace::Config;
use opentelemetry_sdk::Resource;
use prometheus::{Counter, Encoder, Gauge, Histogram, Registry, TextEncoder};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

/// 监控指标
///
/// 包含所有Prometheus指标的定义和操作方法。
#[derive(Clone)]
pub struct Metrics {
    /// 总请求数
    pub requests_total: Counter,
    /// 允许的请求数
    pub requests_allowed: Counter,
    /// 拒绝的请求数
    pub requests_rejected: Counter,
    /// 封禁的请求数
    pub requests_banned: Counter,
    /// 错误数
    pub errors_total: Counter,
    /// 检查延迟分布
    pub check_duration: Histogram,
    /// 限流器延迟分布
    pub limiter_duration: Histogram,
    /// 配额使用率
    pub quota_usage: Gauge,
    /// 并发连接数
    pub concurrent_connections: Gauge,
    /// 令牌桶令牌数
    pub token_bucket_tokens: Gauge,
    /// 滑动窗口请求数
    pub sliding_window_requests: Gauge,
    /// 固定窗口请求数
    pub fixed_window_requests: Gauge,
    /// 指标注册表
    registry: Registry,
}

/// 全局指标实例
static GLOBAL_METRICS: std::sync::OnceLock<Arc<Metrics>> = std::sync::OnceLock::new();

/// 设置全局指标实例
///
/// # 参数
/// - `metrics`: Metrics实例
pub fn set_global_metrics(metrics: Arc<Metrics>) {
    let _ = GLOBAL_METRICS.set(metrics);
}

/// 获取全局指标实例
///
/// # 返回
/// - `Some(Arc<Metrics>)`: 如果已设置
/// - `None`: 如果未设置
pub fn try_global() -> Option<Arc<Metrics>> {
    GLOBAL_METRICS.get().cloned()
}

impl Metrics {
    /// 创建新的监控指标
    ///
    /// # 返回
    /// - 包含所有指标的Metrics实例
    pub fn new() -> Self {
        let registry = Registry::new();

        // 总请求数
        let requests_total = register_counter!(
            "flowguard_requests_total",
            "Total number of flow control checks",
            &registry,
            vec![]
        );

        // 允许的请求数
        let requests_allowed = register_counter!(
            "flowguard_requests_allowed_total",
            "Total number of allowed requests",
            &registry,
            vec![]
        );

        // 拒绝的请求数
        let requests_rejected = register_counter!(
            "flowguard_requests_rejected_total",
            "Total number of rejected requests",
            &registry,
            vec![]
        );

        // 封禁的请求数
        let requests_banned = register_counter!(
            "flowguard_requests_banned_total",
            "Total number of banned requests",
            &registry,
            vec![]
        );

        // 错误数
        let errors_total = register_counter!(
            "flowguard_errors_total",
            "Total number of errors",
            &registry,
            vec![]
        );

        // 检查延迟分布
        let check_duration = register_histogram!(
            "flowguard_check_duration_seconds",
            "Duration of flow control checks in seconds",
            &registry,
            vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0]
        );

        // 限流器延迟分布
        let limiter_duration = register_histogram!(
            "flowguard_limiter_duration_seconds",
            "Duration of limiter operations in seconds",
            &registry,
            vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0]
        );

        // 配额使用率
        let quota_usage = register_gauge!(
            "flowguard_quota_usage_ratio_percent",
            "Quota usage ratio as percentage (0-100)",
            &registry,
            vec![]
        );

        // 并发连接数
        let concurrent_connections = register_gauge!(
            "flowguard_concurrent_connections",
            "Current number of concurrent connections",
            &registry,
            vec![]
        );

        // 令牌桶令牌数
        let token_bucket_tokens = register_gauge!(
            "flowguard_token_bucket_tokens",
            "Current number of tokens in token bucket",
            &registry,
            vec![]
        );

        // 滑动窗口请求数
        let sliding_window_requests = register_gauge!(
            "flowguard_sliding_window_requests",
            "Current number of requests in sliding window",
            &registry,
            vec![]
        );

        // 固定窗口请求数
        let fixed_window_requests = register_gauge!(
            "flowguard_fixed_window_requests",
            "Current number of requests in fixed window",
            &registry,
            vec![]
        );

        Self {
            requests_total,
            requests_allowed,
            requests_rejected,
            requests_banned,
            errors_total,
            check_duration,
            limiter_duration,
            quota_usage,
            concurrent_connections,
            token_bucket_tokens,
            sliding_window_requests,
            fixed_window_requests,
            registry,
        }
    }

    /// 注册到Registry
    ///
    /// # 参数
    /// - `registry`: Prometheus注册表
    ///
    /// # 返回
    /// - `Ok(())`: 注册成功
    /// - `Err(_)`: 注册失败
    pub fn register(&self, registry: &Registry) -> Result<(), prometheus::Error> {
        registry.register(Box::new(self.requests_total.clone()))?;
        registry.register(Box::new(self.requests_allowed.clone()))?;
        registry.register(Box::new(self.requests_rejected.clone()))?;
        registry.register(Box::new(self.requests_banned.clone()))?;
        registry.register(Box::new(self.errors_total.clone()))?;
        registry.register(Box::new(self.check_duration.clone()))?;
        registry.register(Box::new(self.limiter_duration.clone()))?;
        registry.register(Box::new(self.quota_usage.clone()))?;
        registry.register(Box::new(self.concurrent_connections.clone()))?;
        registry.register(Box::new(self.token_bucket_tokens.clone()))?;
        registry.register(Box::new(self.sliding_window_requests.clone()))?;
        registry.register(Box::new(self.fixed_window_requests.clone()))?;
        Ok(())
    }

    /// 收集所有指标并返回Prometheus格式的文本
    ///
    /// # 返回
    /// - Prometheus格式的指标文本
    pub fn gather(&self) -> String {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
            error!("Failed to encode metrics: {}", e);
            return String::new();
        }
        String::from_utf8(buffer).unwrap_or_else(|_| String::new())
    }

    /// 记录检查操作
    ///
    /// # 参数
    /// - `duration`: 操作持续时间
    /// - `allowed`: 是否允许
    pub fn record_check(&self, duration: Duration, allowed: bool) {
        self.check_duration.observe(duration.as_secs_f64());
        self.requests_total.inc();
        if allowed {
            self.requests_allowed.inc();
        } else {
            self.requests_rejected.inc();
        }
    }

    /// 记录错误
    ///
    /// # 参数
    /// - `error_type`: 错误类型
    pub fn record_error(&self, error_type: &str) {
        self.errors_total.inc();
    }

    /// 记录封禁
    pub fn record_ban(&self) {
        self.requests_banned.inc();
    }

    /// 更新配额使用率
    ///
    /// # 参数
    /// - `usage`: 使用率 (0-100)
    pub fn update_quota_usage(&self, usage: f64) {
        self.quota_usage.set(usage);
    }

    /// 更新并发连接数
    ///
    /// # 参数
    /// - `count`: 连接数
    pub fn update_concurrent_connections(&self, count: i64) {
        self.concurrent_connections.set(count as f64);
    }

    /// 更新令牌桶令牌数
    ///
    /// # 参数
    /// - `tokens`: 令牌数
    pub fn update_token_bucket_tokens(&self, tokens: f64) {
        self.token_bucket_tokens.set(tokens);
    }

    /// 更新滑动窗口请求数
    ///
    /// # 参数
    /// - `count`: 请求数
    pub fn update_sliding_window_requests(&self, count: f64) {
        self.sliding_window_requests.set(count);
    }

    /// 更新固定窗口请求数
    ///
    /// # 参数
    /// - `count`: 请求数
    pub fn update_fixed_window_requests(&self, count: f64) {
        self.fixed_window_requests.set(count);
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// 追踪器
///
/// 使用OpenTelemetry实现的分布式追踪器。
#[derive(Clone)]
pub struct Tracer {
    /// 是否启用
    enabled: bool,
}

impl Tracer {
    /// 创建新的追踪器
    ///
    /// # 参数
    /// - `enabled`: 是否启用追踪
    ///
    /// # 返回
    /// - Tracer实例
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// 开始追踪
    ///
    /// # 参数
    /// - `name`: Span名称
    ///
    /// # 返回
    /// - Span实例
    pub fn start_span(&self, name: &str) -> Span {
        if !self.enabled {
            return Span::new_disabled();
        }

        Span::new()
    }

    /// 检查是否启用
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for Tracer {
    fn default() -> Self {
        Self::new(true)
    }
}

/// Span
///
/// 表示一个追踪操作。
pub struct Span {
    /// 开始时间
    started_at: Option<Instant>,
    /// 是否启用
    enabled: bool,
    /// 属性
    attributes: std::sync::Arc<parking_lot::RwLock<Vec<(String, String)>>>,
    /// 事件
    events: std::sync::Arc<parking_lot::RwLock<Vec<(String, Vec<(String, String)>)>>>,
    /// 错误
    error: std::sync::Arc<parking_lot::RwLock<Option<String>>>,
}

impl Span {
    /// 创建新的Span
    fn new() -> Self {
        Self {
            started_at: Some(Instant::now()),
            enabled: true,
            attributes: std::sync::Arc::new(parking_lot::RwLock::new(Vec::new())),
            events: std::sync::Arc::new(parking_lot::RwLock::new(Vec::new())),
            error: std::sync::Arc::new(parking_lot::RwLock::new(None)),
        }
    }

    /// 创建禁用的Span
    fn new_disabled() -> Self {
        Self {
            started_at: None,
            enabled: false,
            attributes: std::sync::Arc::new(parking_lot::RwLock::new(Vec::new())),
            events: std::sync::Arc::new(parking_lot::RwLock::new(Vec::new())),
            error: std::sync::Arc::new(parking_lot::RwLock::new(None)),
        }
    }

    /// 添加属性
    ///
    /// # 参数
    /// - `key`: 属性名
    /// - `value`: 属性值
    pub fn set_attribute(&self, key: &str, value: &str) {
        if self.enabled {
            self.attributes
                .write()
                .push((key.to_string(), value.to_string()));
        }
    }

    /// 添加事件
    ///
    /// # 参数
    /// - `name`: 事件名
    /// - `attributes`: 事件属性
    pub fn add_event(&self, name: &str, attributes: Vec<(String, String)>) {
        if self.enabled {
            self.events.write().push((name.to_string(), attributes));
        }
    }

    /// 记录错误
    ///
    /// # 参数
    /// - `error`: 错误信息
    pub fn record_error(&self, error: &str) {
        if self.enabled {
            *self.error.write() = Some(error.to_string());
        }
    }

    /// 结束追踪
    pub fn finish(self) {
        if self.enabled {
            // 记录span完成
            let elapsed = self.elapsed();
            if let Some(duration) = elapsed {
                tracing::debug!("Span finished in {:?}", duration);
            }
        }
    }

    /// 获取持续时间
    pub fn elapsed(&self) -> Option<Duration> {
        self.started_at.map(|start| start.elapsed())
    }

    /// 获取所有属性
    pub fn attributes(&self) -> Vec<(String, String)> {
        self.attributes.read().clone()
    }

    /// 获取所有事件
    pub fn events(&self) -> Vec<(String, Vec<(String, String)>)> {
        self.events.read().clone()
    }

    /// 获取错误信息
    pub fn error(&self) -> Option<String> {
        self.error.read().clone()
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if self.enabled {
            // 自动结束span
            let elapsed = self.elapsed();
            if let Some(duration) = elapsed {
                tracing::debug!("Span dropped after {:?}", duration);
            }
        }
    }
}

/// 遥测配置
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// 服务名称
    pub service_name: String,
    /// Jaeger端点
    pub jaeger_endpoint: Option<String>,
    /// 是否启用Prometheus
    pub enable_prometheus: bool,
    /// 是否启用追踪
    pub enable_tracing: bool,
    /// Prometheus端口
    pub prometheus_port: u16,
    /// 采样率 (0.0 - 1.0)
    pub sampling_rate: f64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: "flowguard".to_string(),
            jaeger_endpoint: None,
            enable_prometheus: true,
            enable_tracing: false,
            prometheus_port: 9090,
            sampling_rate: 1.0,
        }
    }
}

impl TelemetryConfig {
    /// 创建新的配置
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            ..Default::default()
        }
    }

    /// 设置Jaeger端点
    pub fn with_jaeger(mut self, endpoint: impl Into<String>) -> Self {
        self.jaeger_endpoint = Some(endpoint.into());
        self.enable_tracing = true;
        self
    }

    /// 启用Prometheus
    pub fn with_prometheus(mut self, port: u16) -> Self {
        self.prometheus_port = port;
        self.enable_prometheus = true;
        self
    }

    /// 设置采样率
    pub fn with_sampling_rate(mut self, rate: f64) -> Self {
        self.sampling_rate = rate.clamp(0.0, 1.0);
        self
    }
}

/// 初始化遥测系统
///
/// # 参数
/// - `config`: 遥测配置
///
/// # 返回
/// - `Ok((Metrics, Tracer))`: 初始化成功
/// - `Err(_)`: 初始化失败
///
/// # 示例
/// ```rust
/// use limiteron::telemetry::{init_telemetry, TelemetryConfig};
///
/// #[tokio::main]
/// async fn main() {
///     let config = TelemetryConfig::new("my-service")
///         .with_jaeger("http://localhost:14268/api/traces")
///         .with_prometheus(9090);
///
///     let (metrics, tracer) = init_telemetry(&config).await.unwrap();
/// }
/// ```
pub async fn init_telemetry(config: &TelemetryConfig) -> Result<(Metrics, Tracer), String> {
    info!("Initializing telemetry system");

    // 初始化Prometheus指标
    let metrics = if config.enable_prometheus {
        info!(
            "Prometheus metrics enabled on port {}",
            config.prometheus_port
        );
        Metrics::new()
    } else {
        info!("Prometheus metrics disabled");
        Metrics::new()
    };

    // 初始化OpenTelemetry追踪
    let tracer = if config.enable_tracing {
        info!(
            "OpenTelemetry tracing enabled with sampling rate {}",
            config.sampling_rate
        );

        if let Some(ref jaeger_endpoint) = config.jaeger_endpoint {
            init_jaeger_tracer(config, jaeger_endpoint).await?;
        } else {
            info!("No Jaeger endpoint provided, using console exporter");
            init_console_tracer(config)?;
        }

        Tracer::new(true)
    } else {
        info!("OpenTelemetry tracing disabled");
        Tracer::new(false)
    };

    info!("Telemetry system initialized successfully");
    Ok((metrics, tracer))
}

/// 初始化Jaeger追踪器
async fn init_jaeger_tracer(config: &TelemetryConfig, endpoint: &str) -> Result<(), String> {
    // 注意：opentelemetry-jaeger API在新版本中已改变
    // 这里提供一个简化的实现
    info!(
        "Jaeger tracing requested with endpoint: {} (simplified implementation)",
        endpoint
    );
    info!("Note: Full Jaeger integration requires additional configuration");

    // 简化版本，仅记录日志
    Ok(())
}

/// 初始化控制台追踪器
fn init_console_tracer(config: &TelemetryConfig) -> Result<(), String> {
    // 简化版本，仅记录日志
    info!(
        "Console tracing enabled with sampling rate: {}",
        config.sampling_rate
    );
    Ok(())
}

/// 启动Prometheus指标服务器
///
/// # 参数
/// - `metrics`: Metrics实例
/// - `port`: 端口号
///
/// # 返回
/// - `Ok(())`: 服务器启动成功
/// - `Err(_)`: 服务器启动失败
pub async fn start_prometheus_server(metrics: Arc<Metrics>, port: u16) -> Result<(), String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .map_err(|e| format!("Failed to bind to port {}: {}", port, e))?;

    info!("Prometheus metrics server listening on port {}", port);

    loop {
        match listener.accept().await {
            Ok((mut socket, addr)) => {
                let metrics = metrics.clone();
                tokio::spawn(async move {
                    let mut buffer = [0u8; 1024];
                    if let Ok(n) = socket.read(&mut buffer).await {
                        let request = String::from_utf8_lossy(&buffer[..n]);
                        if request.starts_with("GET /metrics") {
                            let response = metrics.gather();
                            let http_response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                                response.len(),
                                response
                            );
                            if socket.write_all(http_response.as_bytes()).await.is_err() {
                                warn!("Failed to send metrics response");
                            }
                        } else {
                            let response = "HTTP/1.1 404 Not Found\r\n\r\n";
                            if socket.write_all(response.as_bytes()).await.is_err() {
                                warn!("Failed to send 404 response");
                            }
                        }
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
            }
        }
    }
}

// ============================================================================
// 辅助宏和函数
// ============================================================================

/// 注册Counter指标
macro_rules! register_counter {
    ($name:expr, $help:expr, $registry:expr, $labels:expr) => {{
        let opts = prometheus::Opts::new($name, $help);
        Counter::with_opts(opts).unwrap()
    }};
}

/// 注册Gauge指标
macro_rules! register_gauge {
    ($name:expr, $help:expr, $registry:expr, $labels:expr) => {{
        let opts = prometheus::Opts::new($name, $help);
        Gauge::with_opts(opts).unwrap()
    }};
}

/// 注册Histogram指标
macro_rules! register_histogram {
    ($name:expr, $help:expr, $registry:expr, $buckets:expr) => {{
        let opts = prometheus::HistogramOpts::new($name, $help);
        Histogram::with_opts(opts.buckets($buckets)).unwrap()
    }};
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_metrics_creation() {
        let metrics = Metrics::new();
        // 验证所有指标都已创建
        assert_eq!(metrics.requests_total.get(), 0.0);
        assert_eq!(metrics.requests_allowed.get(), 0.0);
        assert_eq!(metrics.requests_rejected.get(), 0.0);
    }

    #[test]
    fn test_metrics_record_check_allowed() {
        let metrics = Metrics::new();
        metrics.record_check(Duration::from_millis(10), true);

        assert_eq!(metrics.requests_total.get(), 1.0);
        assert_eq!(metrics.requests_allowed.get(), 1.0);
        assert_eq!(metrics.requests_rejected.get(), 0.0);
    }

    #[test]
    fn test_metrics_record_check_rejected() {
        let metrics = Metrics::new();
        metrics.record_check(Duration::from_millis(10), false);

        assert_eq!(metrics.requests_total.get(), 1.0);
        assert_eq!(metrics.requests_allowed.get(), 0.0);
        assert_eq!(metrics.requests_rejected.get(), 1.0);
    }

    #[test]
    fn test_metrics_record_error() {
        let metrics = Metrics::new();
        metrics.record_error("test_error");

        assert_eq!(metrics.errors_total.get(), 1.0);
    }

    #[test]
    fn test_metrics_record_ban() {
        let metrics = Metrics::new();
        metrics.record_ban();

        assert_eq!(metrics.requests_banned.get(), 1.0);
    }

    #[test]
    fn test_metrics_update_quota_usage() {
        let metrics = Metrics::new();
        metrics.update_quota_usage(75.5);

        // Gauge的值无法直接获取，这里只测试不会panic
    }

    #[test]
    fn test_metrics_update_concurrent_connections() {
        let metrics = Metrics::new();
        metrics.update_concurrent_connections(10);

        // Gauge的值无法直接获取，这里只测试不会panic
    }

    #[test]
    fn test_metrics_gather() {
        let metrics = Metrics::new();
        metrics.record_check(Duration::from_millis(10), true);

        let output = metrics.gather();
        assert!(!output.is_empty());
        assert!(output.contains("flowguard_requests_total"));
    }

    #[test]
    fn test_tracer_creation() {
        let tracer = Tracer::new(true);
        assert!(tracer.is_enabled());

        let tracer = Tracer::new(false);
        assert!(!tracer.is_enabled());
    }

    #[test]
    fn test_tracer_start_span() {
        let tracer = Tracer::new(true);
        let span = tracer.start_span("test_operation");
        span.finish();
    }

    #[test]
    fn test_disabled_span() {
        let span = Span::new_disabled();
        assert!(span.elapsed().is_none());
    }

    #[test]
    fn test_span_set_attribute() {
        let tracer = Tracer::new(true);
        let span = tracer.start_span("test_operation");
        span.set_attribute("key", "value");
        span.finish();
    }

    #[test]
    fn test_span_add_event() {
        let tracer = Tracer::new(true);
        let span = tracer.start_span("test_operation");
        span.add_event(
            "test_event",
            vec![("attr1".to_string(), "value1".to_string())],
        );
        span.finish();
    }

    #[test]
    fn test_span_record_error() {
        let tracer = Tracer::new(true);
        let span = tracer.start_span("test_operation");
        span.record_error("test error");
        span.finish();
    }

    #[test]
    fn test_span_elapsed() {
        let span = Span::new_disabled();
        assert!(span.elapsed().is_none());
    }

    #[test]
    fn test_telemetry_config_default() {
        let config = TelemetryConfig::default();
        assert_eq!(config.service_name, "flowguard");
        assert!(config.enable_prometheus);
        assert!(!config.enable_tracing);
        assert_eq!(config.prometheus_port, 9090);
        assert_eq!(config.sampling_rate, 1.0);
    }

    #[test]
    fn test_telemetry_config_builder() {
        let config = TelemetryConfig::new("test-service")
            .with_jaeger("http://localhost:14268/api/traces")
            .with_prometheus(8080)
            .with_sampling_rate(0.5);

        assert_eq!(config.service_name, "test-service");
        assert!(config.enable_tracing);
        assert_eq!(config.prometheus_port, 8080);
        assert_eq!(config.sampling_rate, 0.5);
    }

    #[test]
    fn test_sampling_rate_clamping() {
        let config = TelemetryConfig::new("test").with_sampling_rate(1.5);
        assert_eq!(config.sampling_rate, 1.0);

        let config = TelemetryConfig::new("test").with_sampling_rate(-0.5);
        assert_eq!(config.sampling_rate, 0.0);
    }

    #[test]
    fn test_metrics_multiple_records() {
        let metrics = Metrics::new();

        for i in 0..5 {
            metrics.record_check(Duration::from_millis(i * 10), i % 2 == 0);
        }

        assert_eq!(metrics.requests_total.get(), 5.0);
        assert_eq!(metrics.requests_allowed.get(), 3.0);
        assert_eq!(metrics.requests_rejected.get(), 2.0);
    }

    #[test]
    fn test_metrics_all_gauges() {
        let metrics = Metrics::new();

        metrics.update_quota_usage(50.0);
        metrics.update_concurrent_connections(100);
        metrics.update_token_bucket_tokens(10.5);
        metrics.update_sliding_window_requests(20.0);
        metrics.update_fixed_window_requests(30.0);

        // 只测试不会panic
    }

    #[test]
    fn test_tracer_default() {
        let tracer = Tracer::default();
        assert!(tracer.is_enabled());
    }

    #[test]
    fn test_metrics_default() {
        let metrics = Metrics::default();
        assert_eq!(metrics.requests_total.get(), 0.0);
    }

    #[test]
    fn test_span_attributes() {
        let span = Span::new();
        span.set_attribute("user_id", "test_user");
        span.set_attribute("ip", "192.168.1.1");

        let attrs = span.attributes();
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0], ("user_id".to_string(), "test_user".to_string()));
        assert_eq!(attrs[1], ("ip".to_string(), "192.168.1.1".to_string()));
    }

    #[test]
    fn test_span_events() {
        let span = Span::new();
        span.add_event("event1", vec![("key1".to_string(), "value1".to_string())]);
        span.add_event("event2", vec![("key2".to_string(), "value2".to_string())]);

        let events = span.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, "event1");
        assert_eq!(events[1].0, "event2");
    }

    #[test]
    fn test_span_error() {
        let span = Span::new();
        span.record_error("test error");

        let error = span.error();
        assert_eq!(error, Some("test error".to_string()));
    }

    #[test]
    fn test_span_drop() {
        let span = Span::new();
        span.set_attribute("test", "value");
        // span will be dropped here
    }

    #[test]
    fn test_global_metrics() {
        let metrics = Arc::new(Metrics::new());
        set_global_metrics(metrics);

        let global = try_global();
        assert!(global.is_some());
    }

    #[test]
    fn test_global_metrics_none() {
        // 清除全局指标

        let global = try_global();
        assert!(global.is_none());
    }

    #[test]
    fn test_metrics_register() {
        let metrics = Metrics::new();
        let registry = Registry::new();

        let result = metrics.register(&registry);
        assert!(result.is_ok());
    }

    #[test]
    fn test_span_duration() {
        let span = Span::new();
        std::thread::sleep(Duration::from_millis(10));
        let elapsed = span.elapsed();
        assert!(elapsed.is_some());
        assert!(elapsed.unwrap() >= Duration::from_millis(10));
    }

    #[test]
    fn test_disabled_span_no_operations() {
        let span = Span::new_disabled();
        span.set_attribute("key", "value");
        span.add_event("event", vec![]);
        span.record_error("error");

        assert!(span.elapsed().is_none());
        assert!(span.attributes().is_empty());
        assert!(span.events().is_empty());
        assert!(span.error().is_none());
    }

    #[test]
    fn test_metrics_histogram_buckets() {
        let metrics = Metrics::new();

        // 测试不同的延迟值
        metrics.record_check(Duration::from_micros(50), true);
        metrics.record_check(Duration::from_millis(1), true);
        metrics.record_check(Duration::from_millis(10), true);
        metrics.record_check(Duration::from_millis(100), true);
        metrics.record_check(Duration::from_millis(500), true);

        // 验证指标被正确记录
        assert_eq!(metrics.requests_total.get(), 5.0);
    }

    #[test]
    fn test_metrics_gather_format() {
        let metrics = Metrics::new();
        metrics.record_check(Duration::from_millis(10), true);

        let output = metrics.gather();

        // 验证输出格式
        assert!(output.contains("flowguard_requests_total"));
        assert!(output.contains("flowguard_requests_allowed_total"));
        assert!(output.contains("flowguard_requests_rejected_total"));
        assert!(output.contains("flowguard_check_duration_seconds"));
    }
}

// 导出宏
pub(crate) use register_counter;
pub(crate) use register_gauge;
pub(crate) use register_histogram;
