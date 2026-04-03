//! 路由定义

use axum::{
    response::IntoResponse,
    routing::{delete, get, put},
    Router,
};

use super::{config::AdminApiConfig, handlers, server::AppState};

/// 创建路由
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

    // 如果配置了API Key,添加认证中间件
    if config.api_key.is_some() {
        use axum::{
            body::Body,
            http::{header::AUTHORIZATION, Request, StatusCode},
            middleware::from_fn,
        };

        let api_key = config.api_key.clone().unwrap();
        router = router.layer(from_fn(
            move |req: Request<Body>, next: axum::middleware::Next| {
                let api_key = api_key.clone();
                async move {
                    let auth_header = req
                        .headers()
                        .get(AUTHORIZATION)
                        .and_then(|v| v.to_str().ok());

                    match auth_header {
                        Some(token) if token == format!("Bearer {}", api_key) => {
                            next.run(req).await
                        }
                        _ => {
                            let mut resp =
                                axum::response::Response::new(Body::from("Invalid API key"));
                            *resp.status_mut() = StatusCode::UNAUTHORIZED;
                            resp
                        }
                    }
                }
            },
        ));
    }

    router
}
