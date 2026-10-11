// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! e2e：K8s 探针与指标端点
//!
//! 验证 admin API 的三个运维端点（bypass 认证）：
//! - `GET /healthz`（存活探针）→ 200
//! - `GET /readyz`（就绪探针，聚合 `Governor::health_status()`）→ 200/503
//! - `GET /metrics`（Prometheus 文本格式）→ 200
//!
//! 三端点均**不需要** Authorization header（K8s kubelet 探针与抓取器不携带
//! 管理凭证），其余业务端点仍保持 401 语义。

#![cfg(feature = "admin-api")]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

const API_KEY: &str = "probe-e2e-api-key-16ch!";

async fn make_app() -> axum::Router {
    use limiteron::admin::{AdminApiConfig, AdminServer};
    let storage: std::sync::Arc<dyn limiteron::storage::Storage> =
        std::sync::Arc::new(limiteron::storage::MemoryStorage::new());
    let ban_storage: std::sync::Arc<dyn limiteron::storage::BanStorage> =
        std::sync::Arc::new(limiteron::storage::MemoryBanStorage::new());
    let governor = std::sync::Arc::new(
        limiteron::Governor::builder()
            .with_config(test_config())
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("governor ok"),
    );
    let server = AdminServer::new(governor, AdminApiConfig::new(API_KEY));
    server.into_router().expect("admin config valid")
}

fn test_config() -> limiteron::config::FlowControlConfig {
    use limiteron::config::{Action, ActionConfig, GlobalConfig, LimiterConfig, Matcher, Rule};
    limiteron::config::FlowControlConfig {
        version: "0.1.0".to_string(),
        global: GlobalConfig::default(),
        rules: vec![Rule {
            id: "probe_e2e_rule".to_string(),
            name: "Probe E2E Rule".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 100,
                refill_rate: 10,
            }],
            action: ActionConfig {
                on_exceed: Action::Reject,
                ban: None,
            },
        }],
    }
}

async fn get(path: &str) -> (StatusCode, String) {
    let app = make_app().await;
    let resp = app
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).to_string())
}

#[tokio::test]
async fn test_healthz_returns_200_without_auth() {
    let (status, body) = get("/healthz").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "/healthz should be 200 without auth"
    );
    assert!(
        body.contains("ok") || body.contains("healthy"),
        "/healthz body should indicate liveness, actual: {body}"
    );
}

#[tokio::test]
async fn test_readyz_returns_200_when_governor_healthy() {
    let (status, body) = get("/readyz").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "/readyz for a healthy Governor should be 200, actual: {body}"
    );
    assert!(
        body.contains("ready") || body.contains("healthy"),
        "/readyz body should indicate readiness, actual: {body}"
    );
}

#[tokio::test]
async fn test_metrics_returns_200_prometheus_format() {
    let (status, body) = get("/metrics").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "/metrics should be 200 without auth"
    );
    // Prometheus 文本格式以 HELP/TYPE 注释或样本行呈现；无监控 feature 时
    // 也必须返回合法的空 exposition（注释行）。
    assert!(
        body.starts_with('#') || body.contains("_total") || body.contains("limiteron"),
        "/metrics should be Prometheus text format, actual: {body}"
    );
}

#[tokio::test]
async fn test_probe_endpoints_bypass_auth_but_business_endpoints_still_require_it() {
    let app = make_app().await;
    // 业务端点无认证 → 401
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 探针端点带错误凭证也应 200（bypass 语义：探针不读凭证）
    for path in ["/healthz", "/readyz", "/metrics"] {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header(
                        axum::http::header::AUTHORIZATION,
                        "Bearer totally-invalid-key",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "{path} should bypass auth");
    }
}

#[cfg(feature = "prometheus")]
#[tokio::test]
async fn test_metrics_with_prometheus_feature_renders_registered_counters() {
    use limiteron::telemetry::Metrics;
    // 记录一次指标后经 /metrics 渲染（走全局 metrics 注入路径）
    let metrics = std::sync::Arc::new(Metrics::new());
    metrics.record_check(std::time::Duration::from_millis(1), true);
    limiteron::telemetry::set_global_metrics(metrics.clone());

    let (status, body) = get("/metrics").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("flowguard_check_duration_seconds"),
        "with prometheus enabled, /metrics should render the registered histogram (first 200 bytes): {}",
        body.chars().take(200).collect::<String>()
    );
}
