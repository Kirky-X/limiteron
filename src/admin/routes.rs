//! Route definitions

use axum::{
    body::Body,
    http::{header::AUTHORIZATION, Request, StatusCode},
    middleware::from_fn,
    routing::{delete, get, put},
    Router,
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
    use crate::admin::config::AdminApiConfig;
    use crate::admin::server::AppState;
    use crate::config::types::{
        Action, ActionConfig, FlowControlConfig, LimiterConfig, Matcher, Rule,
    };
    use crate::storage::{BanStorage, MemoryBanStorage, MemoryStorage, Storage};
    use crate::Governor;
    use axum::body::Body;
    use axum::http::{header::AUTHORIZATION, Request, StatusCode};
    use http_body_util::BodyExt;
    use std::sync::Arc;
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

    /// 构造包含至少一条规则的合法 FlowControlConfig（Governor::new() 默认配置无规则会 panic）
    fn make_valid_config() -> FlowControlConfig {
        FlowControlConfig {
            version: "0.1.0".to_string(),
            global: crate::config::types::GlobalConfig::default(),
            rules: vec![Rule {
                id: "test_rule".to_string(),
                name: "Test Rule".to_string(),
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

    /// 构造可用的 Governor 实例（避免 Governor::new() 的空配置 panic）
    async fn make_governor() -> Governor {
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
        Governor::builder()
            .with_config(make_valid_config())
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed with valid config")
    }

    async fn make_state() -> AppState {
        let governor = Arc::new(make_governor().await);
        AppState {
            governor,
            #[cfg(feature = "ban-manager")]
            ban_manager: None,
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
        }
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
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["data"]["key"].as_str().unwrap(), "my-test-key");
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
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(!json["success"].as_bool().unwrap());
    }
}
