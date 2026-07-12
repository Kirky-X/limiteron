// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
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
//!     metrics.record_check(std::time::Duration::from_millis(1), true);
//!
//!     // 使用追踪
//!     let span = tracer.start_span("my_operation");
//!     span.finish();
//! }
//! ```

// 子模块
pub mod monitoring;

// 重新导出 monitoring 模块的公共类型
pub use monitoring::{
    AlertConfig, AlertLevel, AlertThresholdF64, AlertThresholdU64, MetricsSnapshot,
};

#[cfg(feature = "monitoring")]
use log::error;
use log::{info, warn};
#[cfg(feature = "monitoring")]
use prometheus::{Counter, Encoder, Gauge, Histogram, HistogramOpts, Registry, TextEncoder};
use std::sync::Arc;
use std::time::{Duration, Instant};
#[cfg(feature = "telemetry")]
use tracing_subscriber;

// 实现模块
mod telemetry_impl;

// 重新导出实现模块的公共函数
#[cfg(feature = "telemetry")]
pub use telemetry_impl::init_telemetry;
#[cfg(feature = "monitoring")]
pub use telemetry_impl::{set_global_metrics, try_global};

#[cfg(not(feature = "monitoring"))]
#[derive(Clone, Default)]
pub struct Metrics;

/// 监控指标
///
#[cfg(feature = "monitoring")]
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
#[cfg(feature = "monitoring")]
static GLOBAL_METRICS: std::sync::OnceLock<Arc<Metrics>> = std::sync::OnceLock::new();

/// 追踪器
///
/// 使用OpenTelemetry实现的分布式追踪器。
#[derive(Clone)]
pub struct Tracer {
    /// 是否启用
    enabled: bool,
}

/// Span
///
/// 表示一个追踪操作。
#[allow(clippy::type_complexity)]
pub struct Span {
    /// 开始时间
    started_at: Option<Instant>,
    /// 是否启用
    enabled: bool,
    /// 属性（使用 Mutex 代替 RwLock，简化并发控制）
    attributes: std::sync::Arc<tokio::sync::Mutex<Vec<(String, String)>>>,
    /// 事件（使用 Mutex 代替 RwLock，简化并发控制）
    events: std::sync::Arc<tokio::sync::Mutex<Vec<(String, Vec<(String, String)>)>>>,
    /// 错误（使用 Mutex 代替 RwLock，简化并发控制）
    error: std::sync::Arc<tokio::sync::Mutex<Option<String>>>,
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
