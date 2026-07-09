//! Telemetry 示例
//!
//! 演示 limiteron 的遥测系统：Prometheus 指标采集、OpenTelemetry 分布式追踪、
//! Span 属性/事件/错误记录、指标导出。
//!
//! # 涵盖 API
//!
//! - `TelemetryConfig`（`new`、`with_jaeger`、`with_prometheus`、`with_sampling_rate`）
//! - `init_telemetry(config)` -> `(Metrics, Tracer)`
//! - `Tracer`（`new`、`start_span`、`is_enabled`）
//! - `Span`（`set_attribute`、`add_event`、`record_error`、`finish`、`elapsed`、
//!   `attributes`、`events`、`error`）
//! - `Metrics`（`new`、`gather`、`record_check`、`record_error`、`record_ban`、
//!   `update_quota_usage`、`update_concurrent_connections`、`update_token_bucket_tokens`、
//!   `update_sliding_window_requests`、`update_fixed_window_requests`）
//!
//! # 运行方式
//!
//! ```bash
//! cargo run --bin telemetry_demo --features telemetry
//! ```
//!
//! # 注意
//!
//! - 仅启用 `telemetry` feature 时，`Metrics` 为无操作实现（方法仍可调用）
//! - 同时启用 `monitoring` feature 时，`Metrics` 为完整 Prometheus 实现
//! - `init_telemetry` 在 `enable_tracing=true` 时会尝试初始化 tracing subscriber，
//!   重复初始化会被安全忽略

use limiteron::telemetry::{init_telemetry, Metrics, TelemetryConfig, Tracer};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Telemetry Demo ===\n");

    demo_config_builder();
    demo_init_telemetry().await?;
    demo_tracer_and_span().await?;
    demo_metrics_recording().await?;
    demo_metrics_gather().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

/// 演示 TelemetryConfig 构建器模式
fn demo_config_builder() {
    println!("--- TelemetryConfig Builder ---");

    // 默认配置
    let default_config = TelemetryConfig::default();
    println!(
        "默认配置: service={}, prometheus_enabled={}, tracing_enabled={}",
        default_config.service_name,
        default_config.enable_prometheus,
        default_config.enable_tracing
    );

    // 自定义服务名
    let config = TelemetryConfig::new("my-api-service");
    println!(
        "自定义服务名: service={}, sampling_rate={}",
        config.service_name, config.sampling_rate
    );

    // 完整构建器链
    let full_config = TelemetryConfig::new("production-service")
        .with_jaeger("http://localhost:14268/api/traces")
        .with_prometheus(9091)
        .with_sampling_rate(0.5);
    println!(
        "完整配置: service={}, jaeger={:?}, prometheus_port={}, sampling_rate={}",
        full_config.service_name,
        full_config.jaeger_endpoint,
        full_config.prometheus_port,
        full_config.sampling_rate
    );

    // 采样率会被 clamp 到 [0.0, 1.0]
    let clamped = TelemetryConfig::new("test").with_sampling_rate(2.0);
    assert_eq!(clamped.sampling_rate, 1.0);
    let clamped_low = TelemetryConfig::new("test").with_sampling_rate(-0.5);
    assert_eq!(clamped_low.sampling_rate, 0.0);
    println!("采样率 clamp 验证通过: 2.0 -> 1.0, -0.5 -> 0.0");

    println!();
}

/// 演示 init_telemetry 初始化
async fn demo_init_telemetry() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- init_telemetry ---");

    // 默认配置（不启用追踪）
    let config = TelemetryConfig::new("demo-service");
    let (metrics, tracer) = init_telemetry(&config).await?;
    println!(
        "默认初始化: tracer_enabled={}, metrics_type=noop",
        tracer.is_enabled()
    );
    assert!(!tracer.is_enabled(), "默认配置应禁用追踪");

    // 启用追踪（通过 with_jaeger 自动设置 enable_tracing=true）
    // 简化模式下不实际连接 Jaeger 服务，仅初始化 tracing subscriber
    let tracing_config = TelemetryConfig::new("traced-service")
        .with_sampling_rate(0.1)
        .with_jaeger("http://localhost:14268/api/traces");
    let (_metrics2, tracer2) = init_telemetry(&tracing_config).await?;
    println!("启用追踪后: tracer_enabled={}", tracer2.is_enabled());
    assert!(tracer2.is_enabled(), "启用追踪后 tracer 应为 enabled");

    // 验证 Metrics 可用
    let _metrics3 = Metrics::new();
    println!("Metrics::new() 调用成功");

    // 验证 Tracer::new
    let enabled_tracer = Tracer::new(true);
    let disabled_tracer = Tracer::new(false);
    println!(
        "Tracer::new(true).is_enabled()={}, Tracer::new(false).is_enabled()={}",
        enabled_tracer.is_enabled(),
        disabled_tracer.is_enabled()
    );

    // 防止 unused variable 警告
    let _ = (metrics,);

    println!();
    Ok(())
}

/// 演示 Tracer 和 Span 的使用
async fn demo_tracer_and_span() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Tracer & Span ---");

    let tracer = Tracer::new(true);

    // 创建 span
    let span = tracer.start_span("demo_operation");
    println!("Span 创建成功");

    // 设置属性
    span.set_attribute("service.name", "demo-service");
    span.set_attribute("request.id", "req-12345");
    span.set_attribute("user.id", "user-67890");
    println!("已设置 3 个属性");

    // 添加事件
    span.add_event(
        "processing_started",
        vec![("timestamp".to_string(), "2026-07-01T00:00:00Z".to_string())],
    );
    span.add_event(
        "cache_miss",
        vec![("key".to_string(), "user:profile".to_string())],
    );
    println!("已添加 2 个事件");

    // 记录错误（演示用，实际操作成功）
    span.record_error("connection_timeout");
    println!("已记录错误: connection_timeout");

    // 读取 span 信息（必须在 finish 前读取，因为 finish 消费 span）
    let attributes = span.attributes();
    let events = span.events();
    let error = span.error();
    let elapsed = span.elapsed();

    println!(
        "Span 信息: attributes_count={}, events_count={}, error={:?}, has_elapsed={}",
        attributes.len(),
        events.len(),
        error,
        elapsed.is_some()
    );

    // 打印属性详情
    for (key, value) in &attributes {
        println!("  属性: {} = {}", key, value);
    }

    // 打印事件详情
    for (name, event_attrs) in &events {
        println!("  事件: {} ({} 个属性)", name, event_attrs.len());
    }

    // 结束 span（消耗 span）
    span.finish();
    println!("Span 已结束");

    // 禁用的 tracer 创建的 span
    let disabled_tracer = Tracer::new(false);
    let disabled_span = disabled_tracer.start_span("disabled_op");
    // 对禁用 span 的操作都是无操作
    disabled_span.set_attribute("key", "value");
    disabled_span.add_event("event", vec![]);
    disabled_span.record_error("error");

    // 禁用 span 的属性应为空
    let disabled_attrs = disabled_span.attributes();
    let disabled_events = disabled_span.events();
    let disabled_error = disabled_span.error();
    println!(
        "禁用 Span: attributes={}, events={}, error={:?}",
        disabled_attrs.len(),
        disabled_events.len(),
        disabled_error
    );
    assert!(disabled_attrs.is_empty(), "禁用 span 不应有属性");
    assert!(disabled_events.is_empty(), "禁用 span 不应有事件");
    assert!(disabled_error.is_none(), "禁用 span 不应有错误");
    disabled_span.finish();

    println!();
    Ok(())
}

/// 演示 Metrics 指标记录
async fn demo_metrics_recording() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Metrics Recording ---");

    let metrics = Metrics::new();

    // 记录检查操作（允许）
    metrics.record_check(Duration::from_millis(5), true);
    metrics.record_check(Duration::from_millis(3), true);
    metrics.record_check(Duration::from_millis(10), true);
    println!("记录 3 次允许的检查");

    // 记录检查操作（拒绝）
    metrics.record_check(Duration::from_millis(8), false);
    metrics.record_check(Duration::from_millis(15), false);
    println!("记录 2 次拒绝的检查");

    // 记录错误
    metrics.record_error("storage_error");
    metrics.record_error("timeout_error");
    metrics.record_error("permission_denied");
    println!("记录 3 次错误");

    // 记录封禁
    metrics.record_ban();
    metrics.record_ban();
    println!("记录 2 次封禁");

    // 更新配额使用率
    metrics.update_quota_usage(75.5);
    println!("更新配额使用率: 75.5%");

    // 更新并发连接数
    metrics.update_concurrent_connections(42);
    println!("更新并发连接数: 42");

    // 更新令牌桶令牌数
    metrics.update_token_bucket_tokens(95.0);
    println!("更新令牌桶令牌数: 95.0");

    // 更新滑动窗口请求数
    metrics.update_sliding_window_requests(128.0);
    println!("更新滑动窗口请求数: 128.0");

    // 更新固定窗口请求数
    metrics.update_fixed_window_requests(256.0);
    println!("更新固定窗口请求数: 256.0");

    println!("所有指标记录方法调用成功");
    println!();
    Ok(())
}

/// 演示 Metrics 收集和导出
async fn demo_metrics_gather() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Metrics Gather ---");

    let metrics = Metrics::new();

    // 记录一些指标
    metrics.record_check(Duration::from_micros(500), true);
    metrics.record_check(Duration::from_micros(800), false);
    metrics.record_error("test_error");
    metrics.record_ban();
    metrics.update_quota_usage(50.0);
    metrics.update_concurrent_connections(10);

    // 收集指标
    let gathered = metrics.gather();

    // 在无 monitoring feature 时，gather() 返回空字符串
    // 在有 monitoring feature 时，返回 Prometheus 格式文本
    if gathered.is_empty() {
        println!("Metrics gather 返回空（monitoring feature 未启用，使用无操作实现）");
        println!("提示: 使用 --features full 启用完整 Prometheus 指标导出");
    } else {
        println!("Metrics gather 成功，输出长度: {} 字节", gathered.len());
        // 打印前几行作为示例
        let preview_lines: Vec<&str> = gathered.lines().take(10).collect();
        println!("前 10 行预览:");
        for line in preview_lines {
            println!("  {}", line);
        }
    }

    // Metrics 实现 Clone
    let metrics_clone = metrics.clone();
    let _gathered_clone = metrics_clone.gather();
    println!("Metrics Clone 验证通过");

    // Metrics 实现 Default
    let _default_metrics = Metrics::default();
    println!("Metrics Default 验证通过");

    println!();
    Ok(())
}
