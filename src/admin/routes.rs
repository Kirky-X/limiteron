// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Route definitions

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header::AUTHORIZATION},
    middleware::from_fn,
    routing::{delete, get, post, put},
};

use super::{config::AdminApiConfig, handlers, server::AppState};

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}

pub fn create_router(state: AppState, config: &AdminApiConfig) -> Router {
    let mut router = Router::new()
        // 系统状态
        .route("/api/v1/status", get(handlers::get_status))
        // 限流器状态
        .route(
            "/api/v1/status/limiter/{key}",
            get(handlers::get_limiter_status),
        )
        // 封禁管理
        .route("/api/v1/ban", post(handlers::create_ban))
        .route("/api/v1/ban/{target}", delete(handlers::delete_ban))
        // 配额管理
        .route("/api/v1/quota/{tenant_id}", put(handlers::update_quota))
        // 熔断器状态
        .route(
            "/api/v1/status/circuit-breaker",
            get(handlers::get_circuit_breaker_status),
        )
        .with_state(state);

    let api_key = config.api_key.clone();
    router = router.layer(from_fn(
        move |req: Request<Body>, next: axum::middleware::Next| {
            let api_key = api_key.clone();
            async move {
                let auth_header = req
                    .headers()
                    .get(AUTHORIZATION)
                    .and_then(|v| v.to_str().ok());

                let expected = format!("Bearer {}", api_key);
                match auth_header {
                    Some(token) if constant_time_eq(token, &expected) => next.run(req).await,
                    _ => {
                        let mut resp = axum::response::Response::new(Body::from("Invalid API key"));
                        *resp.status_mut() = StatusCode::UNAUTHORIZED;
                        resp
                    }
                }
            }
        },
    ));

    router
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::AdminApiConfig;
    use crate::admin::{make_state, make_state_with_ban_manager};
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header::AUTHORIZATION};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn constant_time_eq(a: &str, b: &str) -> bool {
        if a.len() != b.len() {
            return false;
        }
        let mut result = 0u8;
        for (x, y) in a.bytes().zip(b.bytes()) {
            result |= x ^ y;
        }
        result == 0
    }

    /// 构造 POST /api/v1/ban 请求
    fn make_create_ban_request(body: &str) -> Request<Body> {
        Request::builder()
            .uri("/api/v1/ban")
            .method("POST")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[test]
    fn test_constant_time_eq_equal_strings() {
        assert!(constant_time_eq("hello", "hello"));
        assert!(constant_time_eq("", ""));
        assert!(constant_time_eq("Bearer abc123", "Bearer abc123"));
    }

    #[test]
    fn test_constant_time_eq_different_lengths() {
        assert!(!constant_time_eq("hello", "hell"));
        assert!(!constant_time_eq("a", "ab"));
        assert!(!constant_time_eq("", "a"));
    }

    #[test]
    fn test_constant_time_eq_different_content_same_length() {
        assert!(!constant_time_eq("hello", "world"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("Bearer xyz", "Bearer abc"));
    }

    #[tokio::test]
    async fn test_router_rejects_missing_auth_header() {
        let state = make_state().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/status")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_router_rejects_wrong_api_key() {
        let state = make_state().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer wrong-key")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_router_accepts_valid_api_key() {
        let state = make_state().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["success"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_router_status_endpoint_returns_data() {
        let state = make_state().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["data"]["total_requests"].as_u64().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_router_limiter_status_endpoint() {
        let state = make_state().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/status/limiter/my-test-key")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // 端点尚未实现，返回 501 Not Implemented
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(!json["success"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_router_circuit_breaker_endpoint() {
        let state = make_state().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/status/circuit-breaker")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // circuit_breaker=None → 503 SERVICE_UNAVAILABLE
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(!json["success"].as_bool().unwrap());
    }

    // ========================================================================
    // POST /api/v1/ban - create_ban 端点测试
    // ========================================================================

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_create_ban_ip_success() {
        let state = make_state_with_ban_manager().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        let body = r#"{"target":{"type":"ip","value":"192.168.1.100"},"reason":"恶意请求"}"#;
        let resp = app.oneshot(make_create_ban_request(body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["success"].as_bool().unwrap());
        assert!(json["data"]["id"].as_str().is_some());
        assert_eq!(json["data"]["ban_times"].as_u64().unwrap(), 1);
        assert!(json["data"]["is_manual"].as_bool().unwrap());
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_create_ban_user_success() {
        let state = make_state_with_ban_manager().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        let body = r#"{"target":{"type":"user","value":"user123"},"reason":"滥用行为"}"#;
        let resp = app.oneshot(make_create_ban_request(body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["success"].as_bool().unwrap());
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_create_ban_mac_success() {
        let state = make_state_with_ban_manager().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        let body = r#"{"target":{"type":"mac","value":"00:1a:2b:3c:4d:5e"},"reason":"设备封禁"}"#;
        let resp = app.oneshot(make_create_ban_request(body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["success"].as_bool().unwrap());
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_create_ban_geo_success() {
        let state = make_state_with_ban_manager().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        let body = r#"{"target":{"type":"geo","value":{"country_code":"CN"}},"reason":"地区封禁"}"#;
        let resp = app.oneshot(make_create_ban_request(body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["success"].as_bool().unwrap());
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_create_ban_invalid_ip_returns_400() {
        let state = make_state_with_ban_manager().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        // 无效 IP（含字母）应触发 ValidationError → 400
        let body = r#"{"target":{"type":"ip","value":"999.999.999.999"},"reason":"测试"}"#;
        let resp = app.oneshot(make_create_ban_request(body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(!json["success"].as_bool().unwrap());
        assert!(json["message"].as_str().unwrap().contains("IP"));
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_create_ban_invalid_geo_country_code_returns_400() {
        let state = make_state_with_ban_manager().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        // 无效国家码（小写）应触发 ValidationError → 400
        let body = r#"{"target":{"type":"geo","value":{"country_code":"cn"}},"reason":"测试"}"#;
        let resp = app.oneshot(make_create_ban_request(body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_create_ban_no_manager_returns_503() {
        // ban_manager=None 应返回 503 SERVICE_UNAVAILABLE
        let state = make_state().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        let body = r#"{"target":{"type":"ip","value":"1.2.3.4"},"reason":"测试"}"#;
        let resp = app.oneshot(make_create_ban_request(body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(!json["success"].as_bool().unwrap());
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_create_ban_with_custom_duration() {
        let state = make_state_with_ban_manager().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        let body = r#"{"target":{"type":"ip","value":"10.0.0.5"},"reason":"自定义时长","duration_secs":3600}"#;
        let resp = app.oneshot(make_create_ban_request(body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["success"].as_bool().unwrap());
        // expires_at 应为非零时间戳
        assert!(json["data"]["expires_at"].as_i64().unwrap() > 0);
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_create_ban_with_operator() {
        let state = make_state_with_ban_manager().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        let body = r#"{"target":{"type":"ip","value":"172.16.0.1"},"reason":"测试 operator","operator":"admin-alice"}"#;
        let resp = app.oneshot(make_create_ban_request(body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_create_ban_duplicate_returns_same_ban_times() {
        // 注意：MemoryBanStorage::get_history() 总是返回 None（不跟踪历史），
        // 所以重复封禁同一目标时 ban_times 每次都为 1。
        // 此测试验证重复创建不会报错，且返回 201。
        let state = make_state_with_ban_manager().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        let body = r#"{"target":{"type":"ip","value":"203.0.113.10"},"reason":"重复封禁"}"#;

        // 第一次创建
        let resp = app
            .clone()
            .oneshot(make_create_ban_request(body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let resp_body = resp.into_body().collect().await.unwrap().to_bytes();
        let json1: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json1["data"]["ban_times"].as_u64().unwrap(), 1);

        // 第二次创建同一目标 - MemoryBanStorage 限制下 ban_times 仍为 1
        let resp = app.oneshot(make_create_ban_request(body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let resp_body = resp.into_body().collect().await.unwrap().to_bytes();
        let json2: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json2["data"]["ban_times"].as_u64().unwrap(), 1);
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_create_ban_missing_reason_returns_422() {
        let state = make_state_with_ban_manager().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        // JSON 合法但缺必填字段 reason → serde 反序列化失败 → 422
        // （axum 区分 JSON 语法错误=400 与反序列化错误=422）
        let body = r#"{"target":{"type":"ip","value":"1.2.3.4"}}"#;
        let resp = app.oneshot(make_create_ban_request(body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_create_ban_invalid_json_returns_400() {
        let state = make_state_with_ban_manager().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        // 非法 JSON → axum Json extractor 返回 400
        let body = r#"{"target":{"type":"ip","value":"1.2.3.4"},"reason":"测试""#;
        let resp = app.oneshot(make_create_ban_request(body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_create_ban_requires_auth() {
        let state = make_state_with_ban_manager().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        // 缺少 Authorization header → 401
        let req = Request::builder()
            .uri("/api/v1/ban")
            .method("POST")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"target":{"type":"ip","value":"1.2.3.4"},"reason":"测试"}"#.to_string(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
