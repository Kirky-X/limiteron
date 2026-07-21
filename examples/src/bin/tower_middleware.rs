// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Tower Middleware 示例
//!
//! 演示如何将 Governor 流量控制集成到 Tower Service 处理链中。
//!
//! # 涵盖 API
//!
//! - `RateLimitConfig`（`new`、`with_return_429_on_reject`、`with_reject_body` 等）
//! - `RateLimitLayer::new(governor, config)` / `with_converter`
//! - `RateLimitService` 实现 Tower `Service<Request<B>>` trait
//! - `IntoRequestContext` trait (自定义转换器)
//! - `RateLimitHeaderValues` 响应头注入
//!
//! # 运行方式
//!
//! ```bash
//! cargo run --bin tower_middleware --features tower-middleware
//! ```

use http::{Request, Response, StatusCode};
use limiteron::Governor;
use limiteron::middleware::{IntoRequestContext, RateLimitConfig, RateLimitLayer};
use limiteron::tower::{Layer, Service};
use std::sync::Arc;
use std::task::{Context, Poll};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Tower Middleware Demo ===\n");

    demo_config_builder();
    demo_layer_creation().await;
    demo_service_call().await?;
    demo_custom_converter().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

/// 演示 RateLimitConfig 配置构建器
fn demo_config_builder() {
    println!("--- 1. RateLimitConfig Builder ---\n");

    let config = RateLimitConfig::new()
        .with_return_429_on_reject(true)
        .with_return_403_on_ban(true)
        .with_reject_body("Too Many Requests")
        .with_ban_body("Forbidden")
        .with_skip_health_checks(true)
        .with_health_check_path("/api/health");

    println!("  return_429_on_reject: {}", config.return_429_on_reject);
    println!("  return_403_on_ban:    {}", config.return_403_on_ban);
    println!("  reject_body:          {:?}", config.reject_body);
    println!("  ban_body:             {:?}", config.ban_body);
    println!("  skip_health_checks:   {}", config.skip_health_checks);
    println!("  health_check_paths:   {:?}", config.health_check_paths);

    // 测试健康检查路径判断
    println!(
        "\n  is_health_check_path('/health'):      {}",
        config.is_health_check_path("/health")
    );
    println!(
        "  is_health_check_path('/api/health'):   {}",
        config.is_health_check_path("/api/health")
    );
    println!(
        "  is_health_check_path('/api/users'):    {}",
        config.is_health_check_path("/api/users")
    );

    // 默认配置
    let default_config = RateLimitConfig::default();
    println!("\n  Default config:");
    println!(
        "    return_429_on_reject: {}",
        default_config.return_429_on_reject
    );
    println!(
        "    health_check_paths:   {:?}",
        default_config.health_check_paths
    );
    println!();
}

/// 演示 RateLimitLayer 创建
async fn demo_layer_creation() {
    println!("--- 2. RateLimitLayer Creation ---\n");

    let governor = Arc::new(Governor::new().await);
    let config = RateLimitConfig::default();

    // 方式 1：使用默认转换器（DefaultRequestContextConverter 内部使用）
    let layer = RateLimitLayer::new(governor.clone(), config.clone());
    println!("  Layer created with new() (default converter)");

    // 方式 2：使用自定义转换器
    let layer_with_converter = RateLimitLayer::with_converter(governor, config, CustomConverter);
    println!("  Layer created with with_converter() (custom converter)");
    let _ = layer_with_converter;
    let _ = layer;
    println!();
}

/// 演示通过 Service trait 处理请求
async fn demo_service_call() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("--- 3. Service Call ---\n");

    let governor = Arc::new(Governor::new().await);
    let config = RateLimitConfig::default();
    let layer = RateLimitLayer::new(governor, config);

    // 创建一个简单的内部服务（始终返回 200 OK）
    let inner_service = OkService;
    let mut service = layer.layer(inner_service);

    // 构建一个 HTTP 请求
    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/users")
        .header("X-User-Id", "user-001")
        .header("X-Real-IP", "192.168.1.10")
        .body(())
        .expect("request should build");

    // 调用服务（OkService 始终就绪，无需 poll_ready）
    let response: Response<String> = service.call(request).await?;
    println!("  Response status: {}", response.status());
    println!("  Response body: {}", response.body());

    // 构建健康检查请求（应该跳过限流）
    let health_request = Request::builder()
        .method("GET")
        .uri("/health")
        .body(())
        .expect("health request should build");
    let health_response: Response<String> = service.call(health_request).await?;
    println!("  Health check status: {}", health_response.status());
    println!();
    Ok(())
}

/// 演示自定义 IntoRequestContext 转换器
async fn demo_custom_converter() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("--- 4. Custom IntoRequestContext ---\n");

    let governor = Arc::new(Governor::new().await);
    let config = RateLimitConfig::default();
    let layer = RateLimitLayer::with_converter(governor, config, CustomConverter);
    let mut service = layer.layer(OkService);

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/data")
        .header("X-Tenant-Id", "tenant-abc")
        .header("X-Forwarded-For", "203.0.113.50, 10.0.0.1")
        .body(())
        .expect("request should build");

    let response: Response<String> = service.call(request).await?;
    println!("  Custom converter response status: {}", response.status());
    println!("  (Custom converter extracted X-Tenant-Id and X-Forwarded-For)");
    println!();
    Ok(())
}

/// 自定义请求转换器：从自定义头提取信息
#[derive(Clone)]
struct CustomConverter;

impl<B> IntoRequestContext<B> for CustomConverter {
    fn into_request_context(&self, request: &Request<B>) -> limiteron::matchers::RequestContext {
        let mut ctx = limiteron::matchers::RequestContext::new()
            .with_path(request.uri().path())
            .with_method(request.method().as_str());

        // 从自定义头提取租户 ID
        if let Some(tenant) = request.headers().get("x-tenant-id") {
            if let Ok(value) = tenant.to_str() {
                ctx = ctx.with_header("X-Tenant-Id", value);
            }
        }

        // 从 X-Forwarded-For 提取 IP
        if let Some(forwarded) = request.headers().get("x-forwarded-for") {
            if let Ok(value) = forwarded.to_str() {
                if let Some(first_ip) = value.split(',').next() {
                    ctx = ctx.with_client_ip(first_ip.trim());
                }
            }
        }
        ctx
    }
}

/// 简单的内部服务：始终返回 200 OK
#[derive(Clone)]
struct OkService;

impl<B> Service<Request<B>> for OkService {
    type Response = Response<String>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = std::future::Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let path = req.uri().path().to_string();
        std::future::ready(Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/plain")
            .body(format!("OK: {}", path))
            .expect("response should build")))
    }
}
