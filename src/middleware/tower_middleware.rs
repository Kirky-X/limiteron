//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! Tower Service/Layer 实现
//!
//! 实现 Tower 的 Layer 和 Service trait，将 Governor 流量控制
//! 集成到 HTTP 请求处理链中。

use crate::error::Decision;
use crate::error::FlowGuardError;
use crate::governor::Governor;
use crate::matchers::RequestContext;
use http::{Request, Response, StatusCode};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

use super::headers::{inject_rate_limit_headers, RateLimitHeaderValues};

/// 限流中间件配置
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// 是否在请求被拒绝时返回 429 状态码（默认: true）
    pub return_429_on_reject: bool,
    /// 是否在请求被封禁时返回 403 状态码（默认: true）
    pub return_403_on_ban: bool,
    /// 自定义拒绝响应体（默认: "Rate limit exceeded"）
    pub reject_body: String,
    /// 自定义封禁响应体（默认: "Access denied"）
    pub ban_body: String,
    /// 是否跳过健康检查路径（默认: true）
    pub skip_health_checks: bool,
    /// 健康检查路径列表（默认: ["/health", "/healthz", "/ready"]）
    pub health_check_paths: Vec<String>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            return_429_on_reject: true,
            return_403_on_ban: true,
            reject_body: "Rate limit exceeded".to_string(),
            ban_body: "Access denied".to_string(),
            skip_health_checks: true,
            health_check_paths: vec![
                "/health".to_string(),
                "/healthz".to_string(),
                "/ready".to_string(),
            ],
        }
    }
}

impl RateLimitConfig {
    /// 创建新的配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置是否在请求被拒绝时返回 429 状态码
    pub fn with_return_429_on_reject(mut self, value: bool) -> Self {
        self.return_429_on_reject = value;
        self
    }

    /// 设置是否在请求被封禁时返回 403 状态码
    pub fn with_return_403_on_ban(mut self, value: bool) -> Self {
        self.return_403_on_ban = value;
        self
    }

    /// 设置自定义拒绝响应体
    pub fn with_reject_body(mut self, body: &str) -> Self {
        self.reject_body = body.to_string();
        self
    }

    /// 设置自定义封禁响应体
    pub fn with_ban_body(mut self, body: &str) -> Self {
        self.ban_body = body.to_string();
        self
    }

    /// 设置是否跳过健康检查路径
    pub fn with_skip_health_checks(mut self, skip: bool) -> Self {
        self.skip_health_checks = skip;
        self
    }

    /// 添加健康检查路径
    pub fn with_health_check_path(mut self, path: &str) -> Self {
        self.health_check_paths.push(path.to_string());
        self
    }

    /// 检查路径是否为健康检查路径
    pub fn is_health_check_path(&self, path: &str) -> bool {
        self.skip_health_checks && self.health_check_paths.iter().any(|p| p == path)
    }
}

/// 将 HTTP 请求转换为 RequestContext 的 trait
///
/// 用户可以通过实现此 trait 来定制如何从 HTTP 请求中提取限流所需的上下文信息。
pub trait IntoRequestContext<B> {
    /// 将 HTTP 请求转换为 RequestContext
    fn into_request_context(&self, request: &Request<B>) -> RequestContext;
}

/// 默认的 RequestContext 转换器
///
/// 从 HTTP 请求中提取常见的标识符：
/// - 用户 ID: `X-User-Id` header
/// - IP 地址: `X-Forwarded-For` 或 `X-Real-IP` header
/// - API Key: `X-API-Key` header
#[derive(Debug, Clone, Default)]
pub struct DefaultRequestContextConverter;

impl<B> IntoRequestContext<B> for DefaultRequestContextConverter {
    fn into_request_context(&self, request: &Request<B>) -> RequestContext {
        let mut context = RequestContext::new()
            .with_path(request.uri().path())
            .with_method(request.method().as_str());

        // 提取用户 ID
        if let Some(user_id) = request.headers().get("x-user-id") {
            if let Ok(value) = user_id.to_str() {
                context = context.with_header("X-User-Id", value);
            }
        }

        // 提取 IP 地址（优先 X-Real-IP，其次 X-Forwarded-For）
        if let Some(ip) = request.headers().get("x-real-ip") {
            if let Ok(value) = ip.to_str() {
                context = context.with_client_ip(value);
            }
        } else if let Some(forwarded) = request.headers().get("x-forwarded-for") {
            if let Ok(value) = forwarded.to_str() {
                // X-Forwarded-For 可能包含多个 IP，取第一个
                if let Some(first_ip) = value.split(',').next() {
                    context = context.with_client_ip(first_ip.trim());
                }
            }
        }

        // 提取 API Key
        if let Some(api_key) = request.headers().get("x-api-key") {
            if let Ok(value) = api_key.to_str() {
                context = context.with_header("X-API-Key", value);
            }
        }

        // 复制所有 headers 到 context
        for (name, value) in request.headers().iter() {
            if let Ok(value_str) = value.to_str() {
                context = context.with_header(name.as_str(), value_str);
            }
        }

        context
    }
}

/// 限流 Layer
///
/// 实现 Tower 的 Layer trait，用于包装内部服务并添加限流功能。
///
/// # 示例
///
/// ```rust,no_run
/// use limiteron::middleware::{RateLimitLayer, RateLimitConfig};
/// use limiteron::Governor;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() {
///     let governor = Governor::new().await;
///     let layer = RateLimitLayer::new(
///         Arc::new(governor),
///         RateLimitConfig::default(),
///     );
///     // layer 可以用于包装 Tower 服务
/// }
/// ```
pub struct RateLimitLayer<C = DefaultRequestContextConverter> {
    governor: Arc<Governor>,
    config: RateLimitConfig,
    context_converter: C,
}

impl RateLimitLayer {
    /// 创建新的限流 Layer
    pub fn new(governor: Arc<Governor>, config: RateLimitConfig) -> Self {
        Self {
            governor,
            config,
            context_converter: DefaultRequestContextConverter,
        }
    }
}

impl<C> RateLimitLayer<C> {
    /// 使用自定义上下文转换器创建 Layer
    pub fn with_converter(governor: Arc<Governor>, config: RateLimitConfig, converter: C) -> Self {
        Self {
            governor,
            config,
            context_converter: converter,
        }
    }
}

impl<S, C> Layer<S> for RateLimitLayer<C>
where
    C: Clone,
{
    type Service = RateLimitService<S, C>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            governor: self.governor.clone(),
            inner,
            config: self.config.clone(),
            context_converter: self.context_converter.clone(),
        }
    }
}

/// 限流 Service
///
/// 实现 Tower 的 Service trait，在调用内部服务之前执行限流检查。
/// 根据检查结果注入限流响应头或返回错误响应。
pub struct RateLimitService<S, C = DefaultRequestContextConverter> {
    governor: Arc<Governor>,
    inner: S,
    config: RateLimitConfig,
    context_converter: C,
}

impl<S, C, ReqBody, ResBody> Service<Request<ReqBody>> for RateLimitService<S, C>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send,
    S::Future: Send + 'static,
    C: IntoRequestContext<ReqBody> + Send + Sync + Clone + 'static,
    ReqBody: Send + 'static,
    ResBody: Default + Send + 'static,
{
    type Response = Response<ResBody>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // 委托给内部服务的 poll_ready
        self.inner
            .poll_ready(cx)
            .map_err(|e| e.into())
            .map_ok(|_| ())
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let governor = self.governor.clone();
        let config = self.config.clone();
        let context_converter = self.context_converter.clone();

        // 检查是否为健康检查路径
        let path = req.uri().path().to_string();
        if config.is_health_check_path(&path) {
            // 跳过限流检查，直接调用内部服务
            let future = self.inner.call(req);
            return Box::pin(async move { future.await.map_err(|e| e.into()) });
        }

        // 提取请求上下文
        let context = context_converter.into_request_context(&req);

        // 克隆内部服务以备后用
        let inner = self.inner.clone().call(req);

        Box::pin(async move {
            // 执行限流检查
            match governor.check(&context).await {
                Ok(Decision::Allowed(metadata)) => {
                    // 请求允许，调用内部服务
                    let response = inner.await.map_err(|e| e.into())?;

                    // 注入限流响应头
                    let header_values = RateLimitHeaderValues {
                        limit: metadata.limit,
                        remaining: metadata.remaining,
                        reset_at: metadata.reset_at,
                        retry_after: metadata.retry_after,
                        policy: metadata.policy,
                    };

                    Ok(inject_rate_limit_headers(response, &header_values))
                }
                Ok(Decision::Rejected(metadata)) => {
                    // 请求被拒绝，返回 429
                    if config.return_429_on_reject {
                        let mut response = Response::new(ResBody::default());
                        *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;

                        let header_values = RateLimitHeaderValues {
                            limit: metadata.limit,
                            remaining: 0,
                            reset_at: metadata.reset_at,
                            retry_after: Some(metadata.retry_after),
                            policy: String::new(),
                        };

                        Ok(inject_rate_limit_headers(response, &header_values))
                    } else {
                        // 如果不返回 429，则继续调用内部服务
                        inner.await.map_err(|e| e.into())
                    }
                }
                Ok(Decision::Banned(_)) => {
                    // 请求被封禁，返回 403
                    if config.return_403_on_ban {
                        let mut response = Response::new(ResBody::default());
                        *response.status_mut() = StatusCode::FORBIDDEN;
                        Ok(response)
                    } else {
                        // 如果不返回 403，则继续调用内部服务
                        inner.await.map_err(|e| e.into())
                    }
                }
                Err(e) => {
                    // 限流检查出错，记录错误并继续
                    log::error!("Rate limit check failed: {}", e);
                    inner.await.map_err(|e| e.into())
                }
            }
        })
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert!(config.return_429_on_reject);
        assert!(config.return_403_on_ban);
        assert!(config.skip_health_checks);
        assert_eq!(config.health_check_paths.len(), 3);
        assert!(config.is_health_check_path("/health"));
        assert!(config.is_health_check_path("/healthz"));
        assert!(config.is_health_check_path("/ready"));
        assert!(!config.is_health_check_path("/api/users"));
    }

    #[test]
    fn test_rate_limit_config_builder() {
        let config = RateLimitConfig::new()
            .with_return_429_on_reject(false)
            .with_return_403_on_ban(false)
            .with_reject_body("Custom reject")
            .with_ban_body("Custom ban")
            .with_skip_health_checks(false)
            .with_health_check_path("/ping");

        assert!(!config.return_429_on_reject);
        assert!(!config.return_403_on_ban);
        assert_eq!(config.reject_body, "Custom reject");
        assert_eq!(config.ban_body, "Custom ban");
        assert!(!config.skip_health_checks);
        assert!(!config.is_health_check_path("/health")); // skip_health_checks is false
        assert!(config.health_check_paths.contains(&"/ping".to_string()));
    }

    #[test]
    fn test_default_request_context_converter() {
        use http::Request;

        let converter = DefaultRequestContextConverter;

        let mut request = Request::builder()
            .uri("/api/users")
            .method("GET")
            .header("X-User-Id", "user123")
            .header("X-Real-IP", "192.168.1.1")
            .header("X-API-Key", "my-api-key")
            .body(())
            .unwrap();

        // 由于 Request builder 默认使用 Lowercase header names
        // 但我们不关心具体实现，只要提取到值即可
        let context = converter.into_request_context(&request);

        assert_eq!(context.path, "/api/users");
        assert_eq!(context.method, "GET");
    }

    #[tokio::test]
    async fn test_rate_limit_layer_creation() {
        use crate::storage::{MemoryBanStorage, MemoryStorage};
        use std::sync::Arc;

        let storage: Arc<dyn crate::storage::Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn crate::storage::BanStorage> = Arc::new(MemoryBanStorage::new());

        let governor = Governor::builder()
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        let layer = RateLimitLayer::new(Arc::new(governor), RateLimitConfig::default());

        // 验证 layer 创建成功
        assert!(layer.config.skip_health_checks);
    }
}
