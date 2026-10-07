// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Tower Service/Layer 实现
//!
//! 实现 Tower 的 Layer 和 Service trait，将 Governor 流量控制
//! 集成到 HTTP 请求处理链中。

use crate::error::Decision;
use crate::governor::Governor;
use crate::limiters::Limiter;
use crate::matchers::RequestContext;
use http::{Request, Response, StatusCode};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::Service;
use tower_layer::Layer;

use super::headers::{RateLimitHeaderValues, inject_rate_limit_headers};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

fn into_box_error<E: Into<BoxError>>(e: E) -> BoxError {
    e.into()
}

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
            // 默认响应体走 FTL 目录（当前 locale → en 回退；键 rate-limit-exceeded /
            // access-denied），显式覆盖仍可经 with_reject_body/with_ban_body 注入
            reject_body: crate::i18n::t_simple("rate-limit-exceeded"),
            ban_body: crate::i18n::t_simple("access-denied"),
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
    #[allow(clippy::wrong_self_convention)]
    fn into_request_context(&self, request: &Request<B>) -> RequestContext;
}

/// 拒绝响应的输入快照
///
/// 字段缺省（`None`）表示该信息在当前路径不可得：Governor 决策路径
/// 三项齐全；直驱静态限流器的快速路径（[`KeyedRateLimitLayer`]）无
/// 决策链元数据，构造缺省值。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RejectInfo {
    /// 限流上限（RateLimit-Limit）
    pub limit: Option<u64>,
    /// 窗口重置时间戳（Unix 秒，RateLimit-Reset）
    pub reset_at: Option<u64>,
    /// 重试等待秒数（Retry-After）
    pub retry_after: Option<u64>,
}

/// 拒绝响应工厂
///
/// 把「拒绝时如何渲染响应」从决策逻辑中解耦：Service 负责判定，
/// 实现方负责响应形态（状态码/响应体/头部）。
pub trait RejectResponder<ResBody> {
    /// 为一次拒绝构造 HTTP 响应
    fn reject_response(&self, info: RejectInfo) -> Response<ResBody>;
}

/// 默认拒绝响应工厂：IETF 限流头 + 429
///
/// 输出与历史 `RateLimitService` 内联构造逐字节一致：状态 429、
/// 响应体取默认值、注入 `RateLimit-Limit` / `RateLimit-Remaining=0` /
/// `RateLimit-Reset` / `Retry-After`（policy 空则不写 `RateLimit-Policy`）。
/// 信息缺省时不伪造零值头（裸 429）。
#[derive(Debug, Clone, Copy, Default)]
pub struct IetfHttpResponder;

impl<ResBody> RejectResponder<ResBody> for IetfHttpResponder
where
    ResBody: Default,
{
    fn reject_response(&self, info: RejectInfo) -> Response<ResBody> {
        let mut response = Response::new(ResBody::default());
        *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
        if let (Some(limit), Some(reset_at)) = (info.limit, info.reset_at) {
            let header_values = RateLimitHeaderValues {
                limit,
                remaining: 0,
                reset_at,
                retry_after: info.retry_after,
                policy: String::new(),
            };
            return inject_rate_limit_headers(response, &header_values);
        }
        response
    }
}

/// 直连对端地址扩展：宿主框架（如 axum `into_make_service_with_connect_info`）
/// 注入后，可信代理判定与直连 IP 提取才可用。
#[derive(Debug, Clone, Copy)]
pub struct PeerAddr(pub std::net::SocketAddr);

/// 默认的 RequestContext 转换器
///
/// 从 HTTP 请求中提取常见的标识符：
/// - 用户 ID: `X-User-Id` header
/// - IP 地址: `X-Forwarded-For` 或 `X-Real-IP` header（仅可信代理）
/// - API Key: `X-API-Key` header
///
/// # 可信代理
///
/// 历史教训：曾无条件采信 X-Forwarded-For/X-Real-IP 首值——
/// 伪造头可绕过 IP 限流或陷害他人触发封禁。现仅当直连对端 ∈ trusted_proxies
/// 时采信转发头；默认空列表 = 不采信，IP 以 PeerAddr 扩展为准）
#[derive(Debug, Clone, Default)]
pub struct DefaultRequestContextConverter {
    /// 可信代理网段；空 = 不采信任何转发头
    trusted_proxies: Vec<ipnet::IpNet>,
}

impl DefaultRequestContextConverter {
    /// 配置可信代理网段（如 ["10.0.0.0/8"]）
    pub fn with_trusted_proxies(mut self, proxies: Vec<ipnet::IpNet>) -> Self {
        self.trusted_proxies = proxies;
        self
    }

    /// 直连对端是否可信
    fn peer_trusted(&self, peer: &PeerAddr) -> bool {
        !self.trusted_proxies.is_empty()
            && self
                .trusted_proxies
                .iter()
                .any(|net| net.contains(&peer.0.ip()))
    }
}

impl<B> IntoRequestContext<B> for DefaultRequestContextConverter {
    fn into_request_context(&self, request: &Request<B>) -> RequestContext {
        let mut context = RequestContext::new()
            .with_path(request.uri().path())
            .with_method(request.method().as_str());

        // 提取用户 ID
        if let Some(user_id) = request.headers().get("x-user-id")
            && let Ok(value) = user_id.to_str()
        {
            context = context.with_header("X-User-Id", value);
        }

        // 提取 IP 地址：
        // - 直连对端（PeerAddr 扩展）可信且配置了可信网段 → 采信 X-Real-IP/X-Forwarded-For 首值
        // - 其余情况（含默认空配置）→ 用对端地址，不采信可伪造的转发头
        let peer = request.extensions().get::<PeerAddr>().copied();
        let trust_forwarded = peer.as_ref().map(|p| self.peer_trusted(p)).unwrap_or(false);

        let mut forwarded_ip: Option<String> = None;
        if trust_forwarded {
            if let Some(ip) = request.headers().get("x-real-ip")
                && let Ok(value) = ip.to_str()
            {
                forwarded_ip = Some(value.to_string());
            } else if let Some(forwarded) = request.headers().get("x-forwarded-for")
                && let Ok(value) = forwarded.to_str()
                && let Some(first_ip) = value.split(',').next()
            {
                forwarded_ip = Some(first_ip.trim().to_string());
            }
        }

        if let Some(ip) = forwarded_ip {
            context = context.with_client_ip(&ip);
        } else if let Some(p) = peer {
            context = context.with_client_ip(&p.0.ip().to_string());
        }

        // 提取 API Key
        if let Some(api_key) = request.headers().get("x-api-key")
            && let Ok(value) = api_key.to_str()
        {
            context = context.with_header("X-API-Key", value);
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
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let governor = Governor::new().await?;
///     let layer = RateLimitLayer::new(
///         Arc::new(governor),
///         RateLimitConfig::default(),
///     );
///     // layer 可以用于包装 Tower 服务
///     Ok(())
/// }
/// ```
pub struct RateLimitLayer<C = DefaultRequestContextConverter, R = IetfHttpResponder> {
    governor: Arc<Governor>,
    config: RateLimitConfig,
    context_converter: C,
    responder: R,
}

impl RateLimitLayer {
    /// 创建新的限流 Layer
    pub fn new(governor: Arc<Governor>, config: RateLimitConfig) -> Self {
        Self {
            governor,
            config,
            context_converter: DefaultRequestContextConverter::default(),
            responder: IetfHttpResponder,
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
            responder: IetfHttpResponder,
        }
    }
}

impl<C, R> RateLimitLayer<C, R> {
    /// 替换拒绝响应工厂（上下文转换器维度保持不变）
    ///
    /// 与 [`Self::with_converter`] 正交可组合：自定义转换器与自定义
    /// 响应工厂可任意搭配。
    pub fn with_responder<R2>(self, responder: R2) -> RateLimitLayer<C, R2> {
        RateLimitLayer {
            governor: self.governor,
            config: self.config,
            context_converter: self.context_converter,
            responder,
        }
    }
}

impl<S, C, R> Layer<S> for RateLimitLayer<C, R>
where
    C: Clone,
    R: Clone,
{
    type Service = RateLimitService<S, C, R>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            governor: self.governor.clone(),
            inner,
            config: self.config.clone(),
            context_converter: self.context_converter.clone(),
            responder: self.responder.clone(),
        }
    }
}

/// 限流 Service
///
/// 实现 Tower 的 Service trait，在调用内部服务之前执行限流检查。
/// 根据检查结果注入限流响应头或返回错误响应。
pub struct RateLimitService<S, C = DefaultRequestContextConverter, R = IetfHttpResponder> {
    governor: Arc<Governor>,
    inner: S,
    config: RateLimitConfig,
    context_converter: C,
    responder: R,
}

impl<S, C, R, ReqBody, ResBody> Service<Request<ReqBody>> for RateLimitService<S, C, R>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send,
    S::Future: Send + 'static,
    C: IntoRequestContext<ReqBody> + Send + Sync + Clone + 'static,
    R: RejectResponder<ResBody> + Send + Sync + Clone + 'static,
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
            .map_err(into_box_error)
            .map_ok(|_| ())
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let governor = self.governor.clone();
        let config = self.config.clone();
        let context_converter = self.context_converter.clone();
        let responder = self.responder.clone();

        // 检查是否为健康检查路径
        let path = req.uri().path().to_string();
        if config.is_health_check_path(&path) {
            // 跳过限流检查，直接调用内部服务
            let future = self.inner.call(req);
            return Box::pin(async move { future.await.map_err(into_box_error) });
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
                    let response = inner.await.map_err(into_box_error)?;

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
                    // 请求被拒绝，经响应工厂构造响应（默认 IetfHttpResponder
                    // 的输出与历史内联构造逐字节一致）
                    if config.return_429_on_reject {
                        Ok(responder.reject_response(RejectInfo {
                            limit: Some(metadata.limit),
                            reset_at: Some(metadata.reset_at),
                            retry_after: Some(metadata.retry_after),
                        }))
                    } else {
                        // 如果不返回 429，则继续调用内部服务
                        inner.await.map_err(into_box_error)
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
                        inner.await.map_err(into_box_error)
                    }
                }
                Err(e) => {
                    // 限流检查出错，记录错误并继续
                    log::error!("Rate limit check failed: {}", e);
                    inner.await.map_err(into_box_error)
                }
            }
        })
    }
}

/// 从 HTTP 请求提取限流 key
///
/// 返回 `None` 表示请求不携带可用 key：无法按 key 归账，直通不限流
/// （该取舍由 [`KeyedRateLimitLayer`] 文档声明）。
///
/// 返回 `Cow` 以允许借用实现（如直接引用头值）避免每请求堆分配；
/// 需要拼接/规范化 key 的实现可返回 `Cow::Owned`。
pub trait RequestKey<B> {
    /// 提取限流 key（如用户 ID、API Key、对端地址字符串）
    fn request_key<'a>(&self, request: &'a Request<B>) -> Option<std::borrow::Cow<'a, str>>;
}

/// Header 提取器：取单个请求头原值作为限流 key（零堆分配）
///
/// 头缺失或值非可见 ASCII 时返回 `None`（直通）。
///
/// # 键长与基数警示
///
/// 头值是攻击者完全可控的输入。超长值按 `Self::max_key_len` 截断
/// （截断后不同长 key 可能碰撞到同一桶，属保守方向——共享配额，
/// 不会绕过限流）；高基数（海量不同取值）头部会放大支持 per-key
/// 账本的限流器内存占用，务必搭配有界账本限流器使用：
/// `QuotaLimiter` 带 1 万 key 跟踪上限（超限触发过期清理），
/// `InMemoryDistributedLimiter` 的计数表无界；而
/// `TokenBucketLimiter` / `FixedWindowLimiter` / `ConcurrencyLimiter`
/// 为单实例共享桶，key 不参与记账，无放大面。
#[derive(Debug, Clone)]
pub struct HeaderKeyExtractor {
    header: &'static str,
    max_key_len: usize,
}

impl HeaderKeyExtractor {
    /// 默认最大 key 长度（字节数）
    pub const DEFAULT_MAX_KEY_LEN: usize = 128;

    /// 以请求头名构造提取器（如 `"x-api-key"`；匹配大小写不敏感），
    /// 键长上限取 [`Self::DEFAULT_MAX_KEY_LEN`]
    pub fn new(header: &'static str) -> Self {
        Self {
            header,
            max_key_len: Self::DEFAULT_MAX_KEY_LEN,
        }
    }

    /// 自定义键长上限（字节数；0 视为不限长）
    pub fn with_max_key_len(mut self, max_key_len: usize) -> Self {
        self.max_key_len = max_key_len;
        self
    }
}

impl<B> RequestKey<B> for HeaderKeyExtractor {
    fn request_key<'a>(&self, request: &'a Request<B>) -> Option<std::borrow::Cow<'a, str>> {
        let value = request.headers().get(self.header)?.to_str().ok()?;
        // to_str() 已保证可见 ASCII（单字节字符），字节截断必在字符边界
        if self.max_key_len == 0 || value.len() <= self.max_key_len {
            return Some(std::borrow::Cow::Borrowed(value));
        }
        Some(std::borrow::Cow::Borrowed(&value[..self.max_key_len]))
    }
}

/// 按 key 直驱静态限流器的轻量 Tower Layer
///
/// 库内首条绕过 Governor 的快速路径：不做规则匹配、决策链与存储适配，
/// 单个静态限流器实例按 key 直驱（如 [`crate::limiters::QuotaLimiter`]、
/// `ConcurrencyLimiter` 等任何实现 [`Limiter`] 的类型），适合单规则
/// 热点（API Key 配额、全局限流头直挂）。完整决策能力（多规则匹配、
/// 封禁、审计）仍以 [`RateLimitLayer`] 的 Governor 路径为准。
///
/// 语义：
/// - key 缺失（[`RequestKey`] 返回 `None`）→ 直通不限流
/// - `Limiter::check` 通过 → 直通；拒绝/出错 → 经 [`RejectResponder`]
///   构造响应（[`RejectInfo`] 为缺省值，快速路径不产生决策链元数据）
pub struct KeyedRateLimitLayer<L, K, R = IetfHttpResponder> {
    limiter: Arc<L>,
    key_extractor: K,
    responder: R,
}

impl<L, K> KeyedRateLimitLayer<L, K> {
    /// 创建 keyed 快速路径 Layer（默认 IETF 拒绝响应）
    pub fn new(limiter: Arc<L>, key_extractor: K) -> Self {
        Self {
            limiter,
            key_extractor,
            responder: IetfHttpResponder,
        }
    }
}

impl<L, K, R> KeyedRateLimitLayer<L, K, R> {
    /// 使用自定义拒绝响应工厂创建 keyed 快速路径 Layer
    pub fn with_responder(limiter: Arc<L>, key_extractor: K, responder: R) -> Self {
        Self {
            limiter,
            key_extractor,
            responder,
        }
    }
}

impl<S, L, K, R> Layer<S> for KeyedRateLimitLayer<L, K, R>
where
    K: Clone,
    R: Clone,
{
    type Service = KeyedRateLimitService<S, L, K, R>;

    fn layer(&self, inner: S) -> Self::Service {
        KeyedRateLimitService {
            limiter: self.limiter.clone(),
            inner,
            key_extractor: self.key_extractor.clone(),
            responder: self.responder.clone(),
        }
    }
}

/// [`KeyedRateLimitLayer`] 产出的 Tower Service
pub struct KeyedRateLimitService<S, L, K, R = IetfHttpResponder> {
    limiter: Arc<L>,
    inner: S,
    key_extractor: K,
    responder: R,
}

impl<S, L, K, R, ReqBody, ResBody> Service<Request<ReqBody>> for KeyedRateLimitService<S, L, K, R>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send,
    S::Future: Send + 'static,
    L: Limiter + 'static,
    K: RequestKey<ReqBody> + Send + Sync + Clone + 'static,
    R: RejectResponder<ResBody> + Send + Sync + Clone + 'static,
    ReqBody: Send + 'static,
    ResBody: Default + Send + 'static,
{
    type Response = Response<ResBody>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner
            .poll_ready(cx)
            .map_err(into_box_error)
            .map_ok(|_| ())
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let limiter = self.limiter.clone();
        let responder = self.responder.clone();
        let key_extractor = self.key_extractor.clone();
        // 拒绝是本快速路径的常态产出：inner.call 推迟到检查通过后才发起，
        // 拒绝路径零 inner 开销。key 提取在 block 内完成——Cow 借用 req，
        // 与 req 同居于此 future，借用免分配且所有权自洽。
        let mut inner = self.inner.clone();
        Box::pin(async move {
            // key 缺失：无法按 key 归账，直通不限流
            let Some(key) = key_extractor.request_key(&req) else {
                return inner.call(req).await.map_err(into_box_error);
            };
            match limiter.check(&key).await {
                Ok(()) => inner.call(req).await.map_err(into_box_error),
                // 拒绝与检查错误均经响应工厂上报（快速路径
                // 不区分错误类别，避免把存储故障渲染为放行）
                Err(_) => Ok(responder.reject_response(RejectInfo::default())),
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
    use crate::config::{Action, ActionConfig, FlowControlConfig, LimiterConfig, Matcher, Rule};
    use crate::error::LimiteronError;
    use crate::storage::{MemoryBanStorage, MemoryStorage};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Clone)]
    struct MockService;

    impl<B> Service<Request<B>> for MockService {
        type Response = Response<()>;
        type Error = BoxError;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request<B>) -> Self::Future {
            Box::pin(async { Ok(Response::new(())) })
        }
    }

    async fn make_governor(
        config: FlowControlConfig,
        l1_enabled: bool,
    ) -> (Arc<Governor>, Arc<dyn crate::storage::BanStorage>) {
        let storage: Arc<dyn crate::storage::Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn crate::storage::BanStorage> = Arc::new(MemoryBanStorage::new());
        let bs = ban_storage.clone();
        let gov = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .with_l1_cache_enabled(l1_enabled)
            .build()
            .await
            .expect("Governor build");
        (Arc::new(gov), bs)
    }

    fn gen_config(capacity: u64, refill_rate: u64) -> FlowControlConfig {
        FlowControlConfig {
            rules: vec![Rule {
                id: "r".into(),
                name: "R".into(),
                priority: 100,
                matchers: vec![Matcher::User {
                    user_ids: vec!["*".into()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity,
                    refill_rate,
                }],
                action: ActionConfig {
                    on_exceed: Action::Reject,
                    ban: None,
                },
            }],
            ..FlowControlConfig::default()
        }
    }

    fn make_req(path: &str, user: &str) -> Request<()> {
        Request::builder()
            .uri(path)
            .method("GET")
            .header("X-User-Id", user)
            .body(())
            .unwrap()
    }

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

        let converter = DefaultRequestContextConverter::default();

        let request = Request::builder()
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
        use crate::config::{
            Action, ActionConfig, FlowControlConfig, LimiterConfig, Matcher, Rule,
        };
        use crate::storage::{MemoryBanStorage, MemoryStorage};
        use std::sync::Arc;

        let config = FlowControlConfig {
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
            ..FlowControlConfig::default()
        };

        let storage: Arc<dyn crate::storage::Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn crate::storage::BanStorage> = Arc::new(MemoryBanStorage::new());

        let governor = Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed");

        let layer = RateLimitLayer::new(Arc::new(governor), RateLimitConfig::default());

        assert!(layer.config.skip_health_checks);
    }

    #[test]
    fn test_into_box_error_with_string() {
        let result = into_box_error("custom error".to_string());
        assert_eq!(result.to_string(), "custom error");
    }

    #[test]
    fn test_default_converter_x_forwarded_for() {
        // 可信代理语义：对端 ∈ trusted_proxies 时采信 XFF 首值
        let c = DefaultRequestContextConverter::default()
            .with_trusted_proxies(vec!["10.0.0.0/8".parse().unwrap()]);
        let mut req = Request::builder()
            .uri("/api")
            .method("GET")
            .header("X-Forwarded-For", "10.0.0.1")
            .body(())
            .unwrap();
        req.extensions_mut()
            .insert(PeerAddr("10.0.0.254:1000".parse().unwrap()));
        let ctx = c.into_request_context(&req);
        assert_eq!(ctx.client_ip.as_deref(), Some("10.0.0.1"));
    }

    #[test]
    fn test_default_converter_x_forwarded_for_multiple_ips() {
        let c = DefaultRequestContextConverter::default()
            .with_trusted_proxies(vec!["10.0.0.0/8".parse().unwrap()]);
        let mut req = Request::builder()
            .uri("/api")
            .method("GET")
            .header("X-Forwarded-For", "192.168.1.1, 10.0.0.1, 172.16.0.1")
            .body(())
            .unwrap();
        req.extensions_mut()
            .insert(PeerAddr("10.0.0.254:1000".parse().unwrap()));
        let ctx = c.into_request_context(&req);
        assert_eq!(ctx.client_ip.as_deref(), Some("192.168.1.1"));
    }

    #[test]
    fn test_default_converter_x_real_ip_overrides_forwarded_for() {
        let c = DefaultRequestContextConverter::default()
            .with_trusted_proxies(vec!["10.0.0.0/8".parse().unwrap()]);
        let mut req = Request::builder()
            .uri("/api")
            .method("GET")
            .header("X-Real-IP", "192.168.1.1")
            .header("X-Forwarded-For", "10.0.0.1")
            .body(())
            .unwrap();
        req.extensions_mut()
            .insert(PeerAddr("10.0.0.254:1000".parse().unwrap()));
        let ctx = c.into_request_context(&req);
        assert_eq!(ctx.client_ip.as_deref(), Some("192.168.1.1"));
    }

    #[test]
    fn test_default_converter_no_ip_headers() {
        let c = DefaultRequestContextConverter::default();
        let req = Request::builder()
            .uri("/api")
            .method("GET")
            .body(())
            .unwrap();
        let ctx = c.into_request_context(&req);
        assert!(ctx.client_ip.is_none());
    }

    #[test]
    fn test_default_converter_headers_iterated() {
        let c = DefaultRequestContextConverter::default();
        let req = Request::builder()
            .uri("/api")
            .method("GET")
            .header("X-Custom-Header", "custom-value")
            .body(())
            .unwrap();
        let ctx = c.into_request_context(&req);
        assert_eq!(ctx.headers.get("x-custom-header").unwrap(), "custom-value");
    }

    #[tokio::test]
    async fn test_rate_limit_layer_with_converter() {
        #[derive(Clone)]
        struct CustomConverter;
        impl<B> IntoRequestContext<B> for CustomConverter {
            fn into_request_context(&self, _req: &Request<B>) -> RequestContext {
                RequestContext::new()
                    .with_path("/custom")
                    .with_method("POST")
            }
        }
        let (gov, _) = make_governor(gen_config(100, 10), true).await;
        let layer =
            RateLimitLayer::with_converter(gov, RateLimitConfig::default(), CustomConverter);
        let mut svc: RateLimitService<MockService, CustomConverter> = layer.layer(MockService);
        let resp: Response<()> = svc.call(make_req("/api", "u")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_layer_creates_service() {
        let (gov, _) = make_governor(gen_config(100, 10), true).await;
        let layer = RateLimitLayer::new(gov, RateLimitConfig::default());
        let _svc: RateLimitService<MockService> = layer.layer(MockService);
    }

    #[tokio::test]
    async fn test_service_health_check_path() {
        let (gov, _) = make_governor(gen_config(1, 10), false).await;
        let ctx = RequestContext::new()
            .with_path("/api")
            .with_method("GET")
            .with_header("x-user-id", "u");
        assert!(matches!(
            gov.check(&ctx).await.unwrap(),
            Decision::Allowed(_)
        ));

        let mut svc = RateLimitLayer::new(gov, RateLimitConfig::default()).layer(MockService);
        let resp: Response<()> = svc.call(make_req("/health", "u")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().get("RateLimit-Limit").is_none());
    }

    #[tokio::test]
    async fn test_service_allowed() {
        let (gov, _) = make_governor(gen_config(100, 10), false).await;
        let mut svc = RateLimitLayer::new(gov, RateLimitConfig::default()).layer(MockService);
        let resp: Response<()> = svc.call(make_req("/api", "u")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get("RateLimit-Limit").unwrap(), "0");
        assert_eq!(resp.headers().get("RateLimit-Remaining").unwrap(), "0");
        assert!(resp.headers().get("Retry-After").is_none());
    }

    #[tokio::test]
    async fn test_service_rejected_with_429() {
        let (gov, _) = make_governor(gen_config(1, 10), false).await;
        let ctx = RequestContext::new()
            .with_path("/api")
            .with_method("GET")
            .with_header("x-user-id", "u");
        assert!(matches!(
            gov.check(&ctx).await.unwrap(),
            Decision::Allowed(_)
        ));

        let mut svc = RateLimitLayer::new(gov, RateLimitConfig::default()).layer(MockService);
        let resp: Response<()> = svc.call(make_req("/api", "u")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(resp.headers().get("RateLimit-Remaining").unwrap(), "0");
        // TokenBucket(capacity=1, refill=10) 拒绝后决策链按桶快照回填：下一令牌
        // 0.1s 后可用，secs 截断为 0 再 max(1)，故 Retry-After 为 "1" 而非兜底 60。
        assert_eq!(resp.headers().get("Retry-After").unwrap(), "1");
    }

    #[tokio::test]
    async fn test_service_rejected_pass_through() {
        let (gov, _) = make_governor(gen_config(1, 10), false).await;
        let ctx = RequestContext::new()
            .with_path("/api")
            .with_method("GET")
            .with_header("x-user-id", "u");
        assert!(matches!(
            gov.check(&ctx).await.unwrap(),
            Decision::Allowed(_)
        ));

        let cfg = RateLimitConfig {
            return_429_on_reject: false,
            ..RateLimitConfig::default()
        };
        let mut svc = RateLimitLayer::new(gov, cfg).layer(MockService);
        let resp: Response<()> = svc.call(make_req("/api", "u")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().get("RateLimit-Limit").is_none());
    }

    #[cfg(feature = "parallel-checker")]
    #[tokio::test]
    async fn test_service_banned_with_403() {
        use crate::storage::BanTarget;
        use chrono::Utc;

        let (gov, ban_storage) = make_governor(gen_config(100, 10), false).await;
        let record = crate::storage::BanRecord {
            target: BanTarget::UserId("u".into()),
            ban_times: 1,
            duration: std::time::Duration::from_secs(3600),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            is_manual: true,
            reason: "test".into(),
        };
        ban_storage.save(&record).await.unwrap();

        let mut svc = RateLimitLayer::new(gov, RateLimitConfig::default()).layer(MockService);
        let resp: Response<()> = svc.call(make_req("/api", "u")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(resp.headers().get("RateLimit-Limit").is_none());
    }

    #[cfg(feature = "parallel-checker")]
    #[tokio::test]
    async fn test_service_banned_pass_through() {
        use crate::storage::BanTarget;
        use chrono::Utc;

        let (gov, ban_storage) = make_governor(gen_config(100, 10), false).await;
        let record = crate::storage::BanRecord {
            target: BanTarget::UserId("u".into()),
            ban_times: 1,
            duration: std::time::Duration::from_secs(3600),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            is_manual: true,
            reason: "test".into(),
        };
        ban_storage.save(&record).await.unwrap();

        let cfg = RateLimitConfig {
            return_403_on_ban: false,
            ..RateLimitConfig::default()
        };
        let mut svc = RateLimitLayer::new(gov, cfg).layer(MockService);
        let resp: Response<()> = svc.call(make_req("/api", "u")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().get("RateLimit-Limit").is_none());
    }

    // poll_ready 成功路径覆盖（lines 263-268）
    #[tokio::test]
    async fn test_service_poll_ready_ok() {
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        let (gov, _) = make_governor(gen_config(100, 10), false).await;
        let mut svc = RateLimitLayer::new(gov, RateLimitConfig::default()).layer(MockService);
        let poll =
            <RateLimitService<MockService> as Service<Request<()>>>::poll_ready(&mut svc, &mut cx);
        match poll {
            Poll::Ready(Ok(())) => {}
            _ => panic!("expected Ready(Ok(()))"),
        }
    }

    // poll_ready 错误路径覆盖（line 267 map_err 分支）
    #[derive(Clone)]
    struct ErrorMockService;

    impl<B> Service<Request<B>> for ErrorMockService {
        type Response = Response<()>;
        type Error = BoxError;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Err("service not ready".into()))
        }

        fn call(&mut self, _req: Request<B>) -> Self::Future {
            Box::pin(async { Ok(Response::new(())) })
        }
    }

    #[tokio::test]
    async fn test_service_poll_ready_error() {
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        let (gov, _) = make_governor(gen_config(100, 10), false).await;
        let mut svc = RateLimitLayer::new(gov, RateLimitConfig::default()).layer(ErrorMockService);
        let poll = <RateLimitService<ErrorMockService> as Service<Request<()>>>::poll_ready(
            &mut svc, &mut cx,
        );
        match poll {
            Poll::Ready(Err(_)) => {}
            _ => panic!("expected Ready(Err)"),
        }
    }

    // ========================================================================
    // RejectResponder:拒绝响应工厂(默认 IetfHttpResponder = 历史硬编码构造)
    // ========================================================================

    #[test]
    fn test_reject_responder_default_snapshot() {
        // 默认工厂输出与历史 Service::call 内联构造逐字节一致:
        // 429 + body 默认 + RateLimit-{Limit,Remaining=0,Reset} + Retry-After,
        // policy 空 → 无 RateLimit-Policy 头
        let got: Response<()> = IetfHttpResponder.reject_response(RejectInfo {
            limit: Some(7),
            reset_at: Some(1234567890),
            retry_after: Some(9),
        });

        // 历史内联构造(照抄原 Service::call Rejected 分支)
        let mut expected = Response::new(());
        *expected.status_mut() = StatusCode::TOO_MANY_REQUESTS;
        let expected = inject_rate_limit_headers(
            expected,
            &RateLimitHeaderValues {
                limit: 7,
                remaining: 0,
                reset_at: 1234567890,
                retry_after: Some(9),
                policy: String::new(),
            },
        );

        assert_eq!(got.status(), expected.status());
        assert_eq!(got.headers().len(), expected.headers().len());
        for name in [
            "RateLimit-Limit",
            "RateLimit-Remaining",
            "RateLimit-Reset",
            "Retry-After",
            "RateLimit-Policy",
        ] {
            assert_eq!(
                got.headers().get(name),
                expected.headers().get(name),
                "header {name} 不一致"
            );
        }
        assert_eq!(got.headers().get("RateLimit-Limit").unwrap(), "7");
        assert_eq!(got.headers().get("RateLimit-Remaining").unwrap(), "0");
        assert_eq!(got.headers().get("RateLimit-Reset").unwrap(), "1234567890");
        assert_eq!(got.headers().get("Retry-After").unwrap(), "9");
    }

    #[test]
    fn test_reject_responder_default_sparse_info_bare_429() {
        // 信息缺省(直驱静态限流器路径)→ 裸 429,不伪造零值头
        let got: Response<()> = IetfHttpResponder.reject_response(RejectInfo::default());
        assert_eq!(got.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(got.headers().get("RateLimit-Limit").is_none());
        assert!(got.headers().get("RateLimit-Remaining").is_none());
        assert!(got.headers().get("RateLimit-Reset").is_none());
        assert!(got.headers().get("Retry-After").is_none());
    }

    /// 自定义拒绝响应工厂:拒绝时走工厂;允许路径不经工厂
    #[tokio::test]
    async fn test_service_rejected_uses_custom_responder() {
        #[derive(Clone)]
        struct CustomResponder {
            calls: Arc<AtomicUsize>,
        }
        impl RejectResponder<()> for CustomResponder {
            fn reject_response(&self, _info: RejectInfo) -> Response<()> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                let mut resp = Response::new(());
                *resp.status_mut() = StatusCode::IM_A_TEAPOT;
                resp.headers_mut().insert(
                    "X-Custom-Responder",
                    http::header::HeaderValue::from_static("yes"),
                );
                resp
            }
        }

        let (gov, _) = make_governor(gen_config(1, 10), false).await;
        let calls = Arc::new(AtomicUsize::new(0));
        let mut svc = RateLimitLayer::new(gov, RateLimitConfig::default())
            .with_responder(CustomResponder {
                calls: calls.clone(),
            })
            .layer(MockService);

        // 第一次:允许 → 不经工厂
        let resp: Response<()> = svc.call(make_req("/api", "u")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        // 第二次:耗尽拒绝 → 自定义工厂生效(默认 IETF 头被替换)
        {
            let resp: Response<()> = svc.call(make_req("/api", "u")).await.unwrap();
            assert_eq!(resp.status(), StatusCode::IM_A_TEAPOT);
            assert_eq!(resp.headers().get("X-Custom-Responder").unwrap(), "yes");
            assert!(resp.headers().get("RateLimit-Limit").is_none());
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    /// return_429_on_reject=false:直通路径不触发工厂
    #[tokio::test]
    async fn test_service_responder_not_called_when_429_disabled() {
        #[derive(Clone)]
        struct CountingResponder {
            calls: Arc<AtomicUsize>,
        }
        impl RejectResponder<()> for CountingResponder {
            fn reject_response(&self, _info: RejectInfo) -> Response<()> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Response::new(())
            }
        }

        let (gov, _) = make_governor(gen_config(1, 10), false).await;
        let calls = Arc::new(AtomicUsize::new(0));
        let cfg = RateLimitConfig {
            return_429_on_reject: false,
            ..RateLimitConfig::default()
        };
        let mut svc = RateLimitLayer::new(gov, cfg)
            .with_responder(CountingResponder {
                calls: calls.clone(),
            })
            .layer(MockService);

        let resp: Response<()> = svc.call(make_req("/api", "u")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(calls.load(Ordering::SeqCst), 0, "直通路径不应触发工厂");
    }

    // ========================================================================
    // KeyedRateLimitLayer:绕过 Governor 的静态限流器直驱快速路径
    // ========================================================================

    #[derive(Clone)]
    struct StaticLimiter {
        allow: Arc<AtomicBool>,
    }

    #[async_trait::async_trait]
    impl crate::limiters::Limiter for StaticLimiter {
        async fn allow(&self, _cost: u64) -> Result<bool, LimiteronError> {
            Ok(self.allow.load(Ordering::SeqCst))
        }
    }

    #[tokio::test]
    async fn test_keyed_layer_allows_then_rejects_via_default_responder() {
        let limiter = StaticLimiter {
            allow: Arc::new(AtomicBool::new(true)),
        };
        let mut svc = KeyedRateLimitLayer::new(
            Arc::new(limiter.clone()),
            HeaderKeyExtractor::new("x-api-key"),
        )
        .layer(MockService);

        // 带 key 且允许 → 直通
        let req = Request::builder()
            .uri("/api")
            .method("GET")
            .header("X-API-Key", "k1")
            .body(())
            .unwrap();
        let resp: Response<()> = svc.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // 限流器拒绝 → 默认工厂 429(信息缺省 → 无 RateLimit 头)
        limiter.allow.store(false, Ordering::SeqCst);
        let req = Request::builder()
            .uri("/api")
            .method("GET")
            .header("X-API-Key", "k1")
            .body(())
            .unwrap();
        let resp: Response<()> = svc.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(resp.headers().get("RateLimit-Limit").is_none());
    }

    #[tokio::test]
    async fn test_keyed_layer_passes_through_without_key() {
        // 无 key 请求无法按 key 归账 → 直通不限流(语义在文档声明)
        let limiter = StaticLimiter {
            allow: Arc::new(AtomicBool::new(false)),
        };
        let mut svc =
            KeyedRateLimitLayer::new(Arc::new(limiter), HeaderKeyExtractor::new("x-api-key"))
                .layer(MockService);

        let req = Request::builder().uri("/api").method("GET").body(());
        let resp: Response<()> = svc.call(req.unwrap()).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_keyed_layer_custom_responder_with_retry_after() {
        #[derive(Clone)]
        struct RetryAfterResponder {
            calls: Arc<AtomicUsize>,
        }
        impl RejectResponder<()> for RetryAfterResponder {
            fn reject_response(&self, info: RejectInfo) -> Response<()> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                let mut resp = Response::new(());
                *resp.status_mut() = StatusCode::TOO_MANY_REQUESTS;
                if let Some(secs) = info.retry_after {
                    resp.headers_mut().insert(
                        "Retry-After",
                        http::header::HeaderValue::from_str(&secs.to_string()).unwrap(),
                    );
                }
                resp
            }
        }

        let limiter = StaticLimiter {
            allow: Arc::new(AtomicBool::new(false)),
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let mut svc = KeyedRateLimitLayer::with_responder(
            Arc::new(limiter),
            HeaderKeyExtractor::new("x-api-key"),
            RetryAfterResponder {
                calls: calls.clone(),
            },
        )
        .layer(MockService);

        let req = Request::builder()
            .uri("/api")
            .method("GET")
            .header("X-API-Key", "k1")
            .body(())
            .unwrap();
        let resp: Response<()> = svc.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    // ========================================================================
    // HeaderKeyExtractor:键长截断与借用语义
    // ========================================================================

    #[test]
    fn test_header_key_extractor_borrows_short_values() {
        use std::borrow::Cow;
        let extractor = HeaderKeyExtractor::new("x-api-key");
        let req = Request::builder()
            .uri("/api")
            .method("GET")
            .header("X-API-Key", "short-key")
            .body(())
            .unwrap();
        let key = extractor.request_key(&req).unwrap();
        assert!(matches!(key, Cow::Borrowed(_)), "短值应借用零分配");
        assert_eq!(&*key, "short-key");
    }

    #[test]
    fn test_header_key_extractor_truncates_long_values() {
        let extractor = HeaderKeyExtractor::new("x-api-key");
        let long = "a".repeat(300);
        let req = Request::builder()
            .uri("/api")
            .method("GET")
            .header("X-API-Key", &long)
            .body(())
            .unwrap();
        let key = extractor.request_key(&req).unwrap();
        assert_eq!(key.len(), HeaderKeyExtractor::DEFAULT_MAX_KEY_LEN);
        assert_eq!(&*key, &long[..HeaderKeyExtractor::DEFAULT_MAX_KEY_LEN]);

        // 上限 0 = 不限长
        let unlimited = extractor.with_max_key_len(0);
        let key = unlimited.request_key(&req).unwrap();
        assert_eq!(key.len(), 300);
    }

    #[test]
    fn test_header_key_extractor_non_ascii_value_passes_through() {
        // HTTP 头值仅允许可见 ASCII:to_str 失败 → None → 直通不限流
        let extractor = HeaderKeyExtractor::new("x-api-key");
        let req = Request::builder()
            .uri("/api")
            .method("GET")
            .header(
                "X-API-Key",
                http::header::HeaderValue::from_bytes("中文用户".as_bytes()).unwrap(),
            )
            .body(())
            .unwrap();
        assert!(extractor.request_key(&req).is_none());
    }

    #[test]
    fn test_header_key_extractor_missing_header_is_none() {
        let extractor = HeaderKeyExtractor::new("x-api-key");
        let req = Request::builder()
            .uri("/api")
            .method("GET")
            .body(())
            .unwrap();
        assert!(extractor.request_key(&req).is_none());
    }

    // ========================================================================
    // Layer 组合面:自定义转换器与自定义响应工厂正交
    // ========================================================================

    #[tokio::test]
    async fn test_layer_converter_and_responder_combine() {
        #[derive(Clone)]
        struct FixedUserConverter;
        impl<B> IntoRequestContext<B> for FixedUserConverter {
            fn into_request_context(&self, _req: &Request<B>) -> RequestContext {
                RequestContext::new()
                    .with_path("/api")
                    .with_method("GET")
                    .with_header("x-user-id", "combo-user")
            }
        }
        #[derive(Clone)]
        struct TeapotResponder;
        impl RejectResponder<()> for TeapotResponder {
            fn reject_response(&self, _info: RejectInfo) -> Response<()> {
                let mut resp = Response::new(());
                *resp.status_mut() = StatusCode::IM_A_TEAPOT;
                resp
            }
        }

        let (gov, _) = make_governor(gen_config(1, 10), false).await;
        let mut svc =
            RateLimitLayer::with_converter(gov, RateLimitConfig::default(), FixedUserConverter)
                .with_responder(TeapotResponder)
                .layer(MockService);

        // 自定义转换器归账同一用户:第 1 次允许、第 2 次经自定义工厂拒绝
        let resp: Response<()> = svc.call(make_req("/api", "anyone")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let resp: Response<()> = svc.call(make_req("/api", "anyone")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::IM_A_TEAPOT);
    }
}
