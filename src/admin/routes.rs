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

/// vuln-0001 修复：通过 request extensions 传递的鉴权 operator 身份
///
/// middleware 在通过 API key 鉴权后，根据 `AdminApiConfig::api_key_operators`
/// 解析对应 operator 身份并写入 request extensions。handlers 优先使用此值，
/// 不再信任 JSON body 中的 `operator` 字段，防止身份伪造。
#[derive(Debug, Clone)]
pub struct OperatorIdentity(pub String);

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
    let operator_mapping = config.api_key_operators.clone();
    router = router.layer(from_fn(
        move |mut req: Request<Body>, next: axum::middleware::Next| {
            let api_key = api_key.clone();
            let operator_mapping = operator_mapping.clone();
            async move {
                let auth_header = req
                    .headers()
                    .get(AUTHORIZATION)
                    .and_then(|v| v.to_str().ok());

                let expected = format!("Bearer {}", api_key);
                match auth_header {
                    Some(token) if constant_time_eq(token, &expected) => {
                        // vuln-0001 修复：API key 鉴权通过后，
                        // 将 operator 身份从 mapping 解析并写入 request extensions。
                        // mapping 为空 → 回退到默认 "admin-api"（向后兼容），记录 warn。
                        let operator =
                            operator_mapping.get(&api_key).cloned().unwrap_or_else(|| {
                                log::warn!(
                                    target: "admin-api",
                                    "API key 未配置 operator 映射，回退到默认 'admin-api'；\
                                     建议通过 AdminApiConfig::with_api_key_operator 配置显式映射\
                                     以防止 operator 身份伪造"
                                );
                                "admin-api".to_string()
                            });
                        req.extensions_mut().insert(OperatorIdentity(operator));
                        next.run(req).await
                    }
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

    // ========================================================================
    // vuln-0001 修复测试：operator 身份必须由 API key mapping 决定，
    // body 中的 operator 字段必须被忽略。
    //
    // 验证策略：使用 SimpleAuthorizationProvider 作为 oracle。
    // provider 只允许 mapping 中的 operator（或默认 "admin-api"）。
    // 如果 handler 错误地使用 body 中的 operator，授权会失败返回 403；
    // 使用 mapping/默认 operator 则返回 201/200。
    // 注意：存储层（BanRecord）不持久化 BanSource，故无法通过读回 source 验证，
    // 必须通过授权链路的行为差异（201 vs 403）来验证。
    // ========================================================================

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_vuln_0001_operator_from_api_key_mapping_ignores_body() {
        use crate::BanManager;
        use crate::authorization::SimpleAuthorizationProvider;
        use std::sync::Arc;

        // 配置 API key → "admin-alice" 映射
        let config = AdminApiConfig::new("test-api-key-16chars!!")
            .with_api_key_operator("test-api-key-16chars!!", "admin-alice");

        // AuthorizationProvider 只允许 "admin-alice"：
        // - 使用 mapping 的 "admin-alice" → 授权通过 → 201
        // - 使用 body 的 "admin-bob" → 授权失败 → 403
        let auth_provider = Arc::new(SimpleAuthorizationProvider::new(vec![
            "admin-alice".to_string(),
        ]));
        let ban_manager = Arc::new(
            BanManager::builder()
                .with_authorization_provider(auth_provider)
                .build()
                .await
                .unwrap(),
        );
        let governor = Arc::new(crate::admin::make_governor().await);
        let state = AppState {
            governor,
            ban_manager: Some(ban_manager),
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
        };
        let app = create_router(state, &config);

        // body 中谎称 operator 是 "admin-bob"，应被忽略
        // 实际 operator 应为 mapping 中的 "admin-alice"
        let body = r#"{"target":{"type":"ip","value":"198.51.100.1"},"reason":"vuln-0001 测试","operator":"admin-bob"}"#;
        let resp = app.oneshot(make_create_ban_request(body)).await.unwrap();
        // 201 = 使用 mapping 的 "admin-alice" 通过授权
        // 403 = 使用 body 的 "admin-bob" 授权失败（漏洞未修复）
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "operator 必须来自 API key mapping（admin-alice），而非 body（admin-bob）"
        );
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_vuln_0001_operator_falls_back_to_admin_api_when_mapping_absent() {
        use crate::BanManager;
        use crate::authorization::SimpleAuthorizationProvider;
        use std::sync::Arc;

        // 未配置 operator mapping → 回退到默认 "admin-api"（向后兼容）
        let config = AdminApiConfig::new("test-api-key-16chars!!");

        // AuthorizationProvider 只允许 "admin-api"：
        // - 使用默认 "admin-api" → 授权通过 → 201
        // - 使用 body 的 "admin-evil" → 授权失败 → 403
        let auth_provider = Arc::new(SimpleAuthorizationProvider::new(vec![
            "admin-api".to_string(),
        ]));
        let ban_manager = Arc::new(
            BanManager::builder()
                .with_authorization_provider(auth_provider)
                .build()
                .await
                .unwrap(),
        );
        let governor = Arc::new(crate::admin::make_governor().await);
        let state = AppState {
            governor,
            ban_manager: Some(ban_manager),
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
        };
        let app = create_router(state, &config);

        // body 中谎称 operator 是 "admin-evil"
        let body = r#"{"target":{"type":"ip","value":"198.51.100.2"},"reason":"fallback 测试","operator":"admin-evil"}"#;
        let resp = app.oneshot(make_create_ban_request(body)).await.unwrap();
        // 201 = 使用默认 "admin-api" 通过授权
        // 403 = 使用 body 的 "admin-evil" 授权失败（漏洞未修复）
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "未配置 mapping 时应回退到默认 'admin-api'，而非 body 中的 operator"
        );
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_vuln_0001_delete_ban_uses_mapped_operator() {
        use crate::BanManager;
        use crate::authorization::SimpleAuthorizationProvider;
        use crate::ban::{BanSource, BanTarget};
        use std::sync::Arc;

        // 配置 API key → "admin-alice" 映射
        let config = AdminApiConfig::new("test-api-key-16chars!!")
            .with_api_key_operator("test-api-key-16chars!!", "admin-alice");

        // AuthorizationProvider 只允许 "admin-alice"
        let auth_provider = Arc::new(SimpleAuthorizationProvider::new(vec![
            "admin-alice".to_string(),
        ]));
        let ban_manager = Arc::new(
            BanManager::builder()
                .with_authorization_provider(auth_provider)
                .build()
                .await
                .unwrap(),
        );

        // 先用 "admin-alice" 直接调用 create_ban 创建封禁（通过 auth）
        let target = BanTarget::Ip("198.51.100.3".to_string());
        ban_manager
            .create_ban(
                target.clone(),
                "vuln-0001 unban 测试".to_string(),
                BanSource::Manual {
                    operator: "admin-alice".to_string(),
                },
                serde_json::json!({}),
                Some(std::time::Duration::from_secs(3600)),
            )
            .await
            .unwrap();

        let governor = Arc::new(crate::admin::make_governor().await);
        let state = AppState {
            governor,
            ban_manager: Some(ban_manager),
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
        };
        let app = create_router(state, &config);

        // body 中谎称 operator 是 "admin-bob"，应被忽略
        let req = Request::builder()
            .uri("/api/v1/ban/198.51.100.3")
            .method("DELETE")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"reason":"unban","operator":"admin-bob"}"#.to_string(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // 200 = 使用 mapping 的 "admin-alice" 通过授权
        // 403 = 使用 body 的 "admin-bob" 授权失败（漏洞未修复）
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "delete_ban 的 operator 必须来自 API key mapping（admin-alice），而非 body（admin-bob）"
        );
    }
}
