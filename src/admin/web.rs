// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 只读管理 Web UI（`admin-ui` feature）
//!
//! 内嵌单页（无构建链，vanilla HTML+JS）展示限流 / 熔断 / 配额状态。
//!
//! ## 机械只读守卫（三层，新增写端点即红灯）
//!
//! 1. **显式路由表**：[`ROUTE_TABLE`] 是路径清单的唯一事实来源，Router
//!    由它构建且只经 `get()` 注册；守卫测试断言表内全部路由为 GET。
//! 2. **注册完整性**：守卫测试枚举 Router 实际注册的路径集合与
//!    [`ROUTE_TABLE`] 对比，防止数组与注册漂移。
//! 3. **类型窄化**：数据 handler 只接受 `Arc<dyn ReadOnlySnapshotSource>`
//!    （[`AdminService`] 的只读投影 trait，blanket 委托三个只读方法），
//!    本模块类型面上不存在可调用的写方法。
//!
//! 另有请求方法级守卫：对每个注册路径发 POST/PUT/DELETE 断言 405。
//!
//! ## 绑定安全
//!
//! 默认绑定 `127.0.0.1`（仅本机访问）。**本 UI 默认无认证**——改为非回环
//! 地址会将未鉴权的管理快照（规则、统计、健康、封禁维度元数据）暴露给
//! 同网段所有主机；`WebUiServer::start` 对非回环绑定显性发出
//! `tracing::warn!`，运维必须以前置反向代理（认证 + TLS）暴露。详见
//! README「安全」与 `docs/API_REFERENCE.md` 对应节。
//!
//! **访问控制面（[`WebUiAccess`]）**：路由层已内置两项可选控制，均为
//! opt-in（默认行为与既有部署逐位一致）：
//! - **Host 校验**（[`HostCheck::Strict`]）：默认放行 IP 字面量、
//!   `localhost` 与显式配置的 host，拒绝其余 DNS 名——DNS rebinding
//!   依赖浏览器以攻击者控制的域名访问回环地址，域名收敛是唯一可靠的
//!   收敛面。默认 [`HostCheck::Permissive`] 不校验 Host（保持既有行为）；
//!   改用严格模式会打断"按域名访问"的部署（含保留 Host 的反向代理），
//!   此类部署须把域名列入 [`WebUiAccess::allowed_hosts`]。Host 取
//!   `Host` 头，缺失时回退 URI authority（HTTP/2 只有 `:authority`），
//!   两者皆缺即拒绝。
//! - **API key 鉴权**（[`WebUiAccess::api_key`]）：配置后要求
//!   `Authorization: Bearer <key>`（scheme 大小写不敏感），缺失或不符
//!   返回 401；key 须非空且长度 ≥ 16（与 `AdminApiConfig` 同口径）。
//!   静态页 `/` 不含数据，豁免凭证校验（Host 校验仍覆盖）；数据端点
//!   鉴权失败按客户端 IP 限流（窗口内失败次数达上限即 429）。
//!
//! **已知边界（登记）**：默认（宽松 Host + 无鉴权）下仍存在 DNS
//! rebinding 的信息泄露面（攻击者诱导浏览器以恶意域名解析到回环地址
//! 读取快照）。缓解依据：端点全部 GET 只读、无状态变更；快照中的封禁
//! 清单已降敏（目标按 `logging::redaction` 口径脱敏、封禁原因剔除），
//! 残留泄露面限于规则拓扑与统计。彻底闭合需启用严格 Host 校验
//! （`WebUiAccess::strict()`）或由前置反向代理强制 Host 白名单。
//!
//! feature 决策记录：`admin-ui` 默认关闭、不入 `full` preset（UI 面与
//! `openapi`/`admin-client` 同口径——文档与工具面不进默认编译）。

use super::server::LimiteronState;
use super::service::AdminService;
#[cfg(feature = "circuit-breaker")]
use super::service::AdminServiceError;
use ahash::AHashMap;
use axum::{
    Json, Router,
    body::Body,
    extract::{ConnectInfo, Request},
    http::{
        StatusCode,
        header::{AUTHORIZATION, HOST, RETRY_AFTER},
    },
    middleware::Next,
    middleware::from_fn,
    response::{Html, IntoResponse},
    routing::get,
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 只读快照源：[`AdminService`] 的类型级只读投影
///
/// blanket 委托到 [`AdminService`] 的三个只读方法；Web UI 数据 handler
/// 仅接受本 trait 对象，类型面上无法触达写端点（守卫 3）。
#[async_trait::async_trait]
pub trait ReadOnlySnapshotSource: Send + Sync {
    /// 系统状态
    async fn status(&self) -> super::handlers::SystemStatus;

    /// 运行时自省快照
    async fn introspect(&self) -> serde_json::Value;

    /// 熔断器状态
    #[cfg(feature = "circuit-breaker")]
    async fn circuit_breaker_status(
        &self,
    ) -> Result<super::handlers::CircuitBreakerStatus, AdminServiceError>;
}

#[async_trait::async_trait]
impl<T: AdminService + ?Sized> ReadOnlySnapshotSource for T {
    async fn status(&self) -> super::handlers::SystemStatus {
        AdminService::status(self).await
    }

    async fn introspect(&self) -> serde_json::Value {
        AdminService::introspect(self).await
    }

    #[cfg(feature = "circuit-breaker")]
    async fn circuit_breaker_status(
        &self,
    ) -> Result<super::handlers::CircuitBreakerStatus, AdminServiceError> {
        AdminService::circuit_breaker_status(self).await
    }
}

/// 内嵌单页路径（静态页，不含数据）
const STATIC_INDEX_PATH: &str = "/";

/// Web UI 路由表（守卫 1：路径清单的唯一事实来源，全部 GET）
///
/// 新增端点必须在此登记且仅可为 GET——守卫测试断言本表不含非 GET
/// 语义，且 Router 实际注册集合与本表一致（守卫 2）。
pub(crate) const ROUTE_TABLE: [&str; 3] = [STATIC_INDEX_PATH, "/snapshot", "/circuit-breaker"];

/// Web UI 的 Host 校验策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HostCheck {
    /// 宽松（默认，向后兼容）：不校验 Host 头
    ///
    /// 保留既有部署行为；DNS rebinding 面未被路由层收敛，须由前置反向
    /// 代理强制 Host 白名单。
    #[default]
    Permissive,
    /// 严格：仅放行 IP 字面量、`localhost` 与显式配置的 host
    ///
    /// 其余 DNS 名一律 421（DNS rebinding 的收敛面）。
    Strict,
}

/// Web UI 访问控制配置（Host 校验 + 可选 API key）
///
/// 默认值与既有行为逐位一致（宽松 Host、无鉴权）。两项控制独立启用：
/// 严格 Host 校验收敛 DNS rebinding 面，API key 收敛"任何能访问该端口
/// 的人都能读快照"的面。
#[derive(Clone, Default)]
pub struct WebUiAccess {
    /// 额外允许的 Host（不含端口，大小写不敏感）
    pub allowed_hosts: Vec<String>,
    /// 访问 API key（`Some` 时要求 `Authorization: Bearer <key>`）
    pub api_key: Option<String>,
    /// Host 校验策略
    pub host_check: HostCheck,
}

// 手动实现 Debug 并脱敏 api_key：派生的 Debug 会以明文打印 `api_key`，
// 误用 `println!("{access:?}")` 即泄露 Web UI 凭证（同 AdminApiConfig 口径）
impl std::fmt::Debug for WebUiAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebUiAccess")
            .field("allowed_hosts", &self.allowed_hosts)
            .field(
                "api_key",
                &self
                    .api_key
                    .as_ref()
                    .map(|key| format!("<redacted; {} chars>", key.len())),
            )
            .field("host_check", &self.host_check)
            .finish()
    }
}

impl WebUiAccess {
    /// 严格 Host 校验（放行 IP 字面量、`localhost` 与显式配置的 host）
    #[must_use]
    pub fn strict() -> Self {
        Self {
            host_check: HostCheck::Strict,
            ..Self::default()
        }
    }

    /// 追加允许的 Host（不含端口）
    #[must_use]
    pub fn with_allowed_hosts<I, S>(mut self, hosts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allowed_hosts.extend(hosts.into_iter().map(Into::into));
        self
    }

    /// 要求 API key 鉴权（`Authorization: Bearer <key>`）
    ///
    /// key 须非空且长度 ≥ [`crate::admin::config::MIN_API_KEY_LEN`]（与
    /// `AdminApiConfig` 同口径）：空 key 会退化成「空串相等」的畸形凭证，
    /// 过短 key 可被在线穷举。违规 key 立即 panic——构造期失败优于运行期
    /// 默默放行弱凭证。
    #[must_use]
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        let api_key = api_key.into();
        assert!(
            !api_key.is_empty() && api_key.len() >= crate::admin::config::MIN_API_KEY_LEN,
            "web UI api_key must be non-empty and at least {} characters",
            crate::admin::config::MIN_API_KEY_LEN
        );
        self.api_key = Some(api_key);
        self
    }
}

/// 取出 Host 头中的主机部分（去端口，IPv6 去方括号）
fn host_part(host_header: &str) -> String {
    let trimmed = host_header.trim();
    if let Some(rest) = trimmed.strip_prefix('[') {
        // IPv6 字面量：[::1]:9091 —— 括号必须正确闭合，其后只允许可选
        // 端口；畸形字面量（如 "[::1]evil.example.com"）整段判无效，
        // 否则会被截成合法 IP 字面量而误放行。
        let Some((host, tail)) = rest.split_once(']') else {
            return String::new();
        };
        let valid_tail = tail.is_empty()
            || (tail.starts_with(':')
                && tail.len() > 1
                && tail[1..].chars().all(|c| c.is_ascii_digit()));
        if !valid_tail {
            return String::new();
        }
        return host.to_ascii_lowercase();
    }
    match trimmed.split_once(':') {
        Some((host, _port)) => host.to_ascii_lowercase(),
        None => trimmed.to_ascii_lowercase(),
    }
}

/// Host 头是否被放行
///
/// 严格模式放行：IP 字面量（任意地址，含 loopback 与内网直连地址）、
/// `localhost`、显式配置的 host；其余 DNS 名拒绝（DNS rebinding 面）。
fn host_allowed(host_header: &str, allowed_hosts: &[String]) -> bool {
    let host = host_part(host_header);
    if host.is_empty() {
        return false;
    }
    if host == "localhost" || host.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    allowed_hosts
        .iter()
        .any(|allowed| host_part(allowed) == host)
}

/// 鉴权失败限流桶：key = client IP，value = (失败次数, 窗口起点)
type AuthFailureBuckets = Arc<Mutex<AHashMap<String, (u64, Instant)>>>;

/// 鉴权失败限流：单客户端窗口内允许的最大失败次数
///
/// 仅在配置了 API key 时生效——无凭证可猜时无需限流。达上限后不再消耗
/// 凭证比对（在线穷举被阻断），窗口过后计数自然重置。
const AUTH_FAILURE_MAX: u64 = 10;

/// 鉴权失败限流窗口
const AUTH_FAILURE_WINDOW: Duration = Duration::from_secs(60);

/// 鉴权失败限流桶的条目上限
///
/// 攻击者可用轮换源 IP 令桶表单调膨胀至 OOM（内存耗尽 DoS）；超限时
/// 淘汰最旧（窗口起点最早）的条目（同 `routes.rs` 口径）。
const AUTH_FAILURE_MAX_ENTRIES: usize = 10_000;

/// 锁定鉴权失败桶；中毒时恢复内部状态而非 panic（同 routes.rs 口径：
/// 内存态计数器不值得让整站不可用）
fn lock_auth_failures(
    buckets: &AuthFailureBuckets,
) -> std::sync::MutexGuard<'_, AHashMap<String, (u64, Instant)>> {
    buckets
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 该客户端的鉴权失败次数是否已达上限
///
/// 已达上限时返回窗口剩余时长（供 429 的 `Retry-After`）。
fn auth_failures_exhausted(
    buckets: &AuthFailureBuckets,
    client: &str,
    max: u64,
    window: Duration,
) -> Option<Duration> {
    let now = Instant::now();
    let mut guard = lock_auth_failures(buckets);
    // 清扫过期 entry（桶表随源 IP 轮换单调增长，须有界）
    guard.retain(|_, (_, start)| now.duration_since(*start) < window);
    let entry = guard.get(client)?;
    if entry.0 >= max {
        Some(window.saturating_sub(now.duration_since(entry.1)))
    } else {
        None
    }
}

/// 记录一次鉴权失败
fn record_auth_failure(buckets: &AuthFailureBuckets, client: &str, window: Duration) {
    let now = Instant::now();
    let mut guard = lock_auth_failures(buckets);
    guard.retain(|_, (_, start)| now.duration_since(*start) < window);
    // 超容量上限时淘汰最旧条目（清扫后仍超限才淘汰）
    if guard.len() >= AUTH_FAILURE_MAX_ENTRIES
        && let Some(oldest) = guard
            .iter()
            .min_by_key(|(_, (_, start))| *start)
            .map(|(client, _)| client.clone())
    {
        guard.remove(&oldest);
    }
    guard.entry(client.to_string()).or_insert((0, now)).0 += 1;
}

/// 客户端 IP（per-client 分桶键）
///
/// 来源 `ConnectInfo<SocketAddr>`（TCP 连接远端），不信任
/// `X-Forwarded-For`（可伪造）；无 connect info 时回退共享桶 `"unknown"`。
fn client_ip(req: &Request<Body>) -> String {
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connect_info| connect_info.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// 解析 Bearer 凭证（scheme 大小写不敏感，RFC 7235 §2.1）
///
/// 返回 `None` 表示不是 Bearer 形态；返回空串表示畸形凭证
/// （`"Bearer "` 不得作为有效凭证——空 token 不参与比对）。
fn bearer_token(header: &str) -> Option<&str> {
    let (scheme, token) = header.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    Some(token)
}

/// 访问控制中间件：Host 校验 + 限流 + API key 鉴权
///
/// 命中即拒绝（421 / 429 / 401），不触达 handler。
async fn enforce_access(
    access: &WebUiAccess,
    buckets: &AuthFailureBuckets,
    req: Request<Body>,
    next: Next,
) -> axum::response::Response {
    if access.host_check == HostCheck::Strict {
        // Host 头优先；HTTP/2（h2c）只带 `:authority`（映射到 URI
        // authority）而无 Host 头——不回退则严格模式在 h2c 形态下整条
        // 被绕过。两者皆缺（或 Host 头非 ASCII）一律拒绝。
        let host: Option<&str> = match req.headers().get(HOST) {
            Some(value) => value.to_str().ok(),
            None => req.uri().authority().map(|authority| authority.as_str()),
        };
        if !host.is_some_and(|host| host_allowed(host, &access.allowed_hosts)) {
            return (StatusCode::MISDIRECTED_REQUEST, "Host header not allowed").into_response();
        }
    }

    // 静态页（"/"）不含数据，仅豁免凭证校验；Host 校验仍覆盖全部路由。
    // 数据端点（/snapshot、/circuit-breaker）必须携带有效凭证。
    if let Some(expected) = &access.api_key
        && req.uri().path() != STATIC_INDEX_PATH
    {
        let client = client_ip(&req);
        // 限流置于凭证校验之前：窗口内失败次数达上限即 429，不再消耗
        // 一次凭证比对（阻断在线穷举）。
        if let Some(remaining) =
            auth_failures_exhausted(buckets, &client, AUTH_FAILURE_MAX, AUTH_FAILURE_WINDOW)
        {
            let mut resp = axum::response::Response::new(Body::from(
                "Too many failed authentication attempts",
            ));
            *resp.status_mut() = StatusCode::TOO_MANY_REQUESTS;
            if let Ok(value) = remaining
                .as_secs()
                .max(1)
                .to_string()
                .parse::<axum::http::HeaderValue>()
            {
                resp.headers_mut().insert(RETRY_AFTER, value);
            }
            return resp;
        }

        let presented = req
            .headers()
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(bearer_token);
        // key 强度校验与 AdminApiConfig 同口径：空/过短的 key 不是有效
        // 凭证（空 key 会与空 token 相等；短 key 可被穷举）。
        let key_acceptable =
            !expected.is_empty() && expected.len() >= crate::admin::config::MIN_API_KEY_LEN;
        let ok = key_acceptable
            && presented.is_some_and(|token| {
                !token.is_empty() && crate::admin::routes::constant_time_eq(token, expected)
            });
        if !ok {
            record_auth_failure(buckets, &client, AUTH_FAILURE_WINDOW);
            return (
                StatusCode::UNAUTHORIZED,
                crate::i18n::t_simple("admin-api-invalid-key"),
            )
                .into_response();
        }
    }

    next.run(req).await
}

/// 构建 Web UI Router（全部 GET；数据源经类型窄化的只读投影）
///
/// 注册完全由 [`ROUTE_TABLE`] 驱动：表是路径清单的唯一事实来源——多注册
/// 结构上不可能（无表外 route 调用），漏注册由守卫测试的 GET 探测红灯。
pub fn create_router(snapshot_source: Arc<dyn ReadOnlySnapshotSource>) -> Router {
    create_router_with_access(snapshot_source, WebUiAccess::default())
}

/// 构建 Web UI Router（带访问控制：Host 校验 + 可选 API key）
///
/// [`create_router`] 等价于本函数传入 [`WebUiAccess::default`]（宽松
/// Host、无鉴权——既有行为）。
pub fn create_router_with_access(
    snapshot_source: Arc<dyn ReadOnlySnapshotSource>,
    access: WebUiAccess,
) -> Router {
    let mut router = Router::new();
    for path in ROUTE_TABLE {
        let source = snapshot_source.clone();
        router = router.route(
            path,
            get(move || async move { serve_snapshot(&source, path).await }),
        );
    }
    // 共享句柄：每请求只 clone Arc，不再逐请求复制 allowed_hosts 向量与
    // api_key 字符串（含密钥明文拷贝）
    let layer_access = Arc::new(access);
    let layer_buckets: AuthFailureBuckets = Arc::new(Mutex::new(AHashMap::new()));
    router.layer(from_fn(move |req: Request<Body>, next: Next| {
        let access = layer_access.clone();
        let buckets = layer_buckets.clone();
        async move { enforce_access(&access, &buckets, req, next).await }
    }))
}

/// 快照中的封禁清单降敏（Web UI 无鉴权）
///
/// `/snapshot` 不挂鉴权中间件，原始封禁目标（IP / 用户 ID / MAC）与封禁
/// 原因（操作者原文，可能含内部描述）不得原样出现在响应里。降敏后只
/// 保留聚合面：目标按 `BanTarget` 的 type 标签脱敏（与
/// [`crate::logging::redact_ban_target`] 同口径），原因文本整体剔除；
/// 封禁次数、是否手动、到期时间保留。带鉴权的 `/api/v1/introspect`
/// 不经此降敏。
fn redact_ban_targets(body: &mut serde_json::Value) {
    let Some(items) = body.get_mut("active_bans").and_then(|v| v.as_array_mut()) else {
        return;
    };
    for item in items {
        if let Some(obj) = item.as_object_mut() {
            obj.remove("reason");
        }
        let Some(target) = item.get_mut("target") else {
            continue;
        };
        let Some(kind) = target
            .get("type")
            .and_then(|t| t.as_str())
            .map(str::to_owned)
        else {
            continue;
        };
        let Some(value) = target.get_mut("value") else {
            continue;
        };
        match kind.as_str() {
            "ip" => redact_string_value(value, |s| crate::logging::redact_ip(Some(s))),
            "user" => redact_string_value(value, |s| crate::logging::redact_user_id(Some(s))),
            "mac" | "cidr" => {
                redact_string_value(value, |s| crate::logging::redact_basic(Some(s)));
            }
            "geo" => {
                if let serde_json::Value::Object(map) = value {
                    if let Some(serde_json::Value::String(code)) = map.get_mut("country_code") {
                        let masked = crate::logging::redact_basic(Some(code.as_str()));
                        *code = masked;
                    }
                } else {
                    redact_json_strings(value);
                }
            }
            // 未知变体（新增 BanTarget 变体或异常结构）默认遮蔽：字符串走
            // redact_basic，对象形态递归遮蔽字符串叶子——fail closed 优先于
            // 原样输出未知字段。
            _ => redact_json_strings(value),
        }
    }
}

/// 按类型脱敏字符串形态的封禁目标值
///
/// 形态非字符串（异常结构）时退回默认脱敏，不原样输出。
fn redact_string_value(value: &mut serde_json::Value, mask: impl Fn(&str) -> String) {
    match value {
        serde_json::Value::String(s) => {
            let masked = mask(s.as_str());
            *s = masked;
        }
        other => redact_json_strings(other),
    }
}

/// 递归遮蔽 JSON 中的字符串叶子（未知封禁变体的默认脱敏）
fn redact_json_strings(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => {
            *s = crate::logging::redact_basic(Some(s.as_str()));
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(redact_json_strings),
        serde_json::Value::Object(map) => map.values_mut().for_each(redact_json_strings),
        _ => {}
    }
}

/// 按路径分派的只读响应（`/` 返回内嵌单页，其余返回快照 JSON）
async fn serve_snapshot(
    source: &Arc<dyn ReadOnlySnapshotSource>,
    path: &str,
) -> axum::response::Response {
    match path {
        "/" => axum::response::IntoResponse::into_response(Html(INDEX_HTML)),
        "/snapshot" => {
            // 无鉴权面：封禁清单先降敏再输出（目标脱敏 + 原因剔除）
            let mut body = source.introspect().await;
            redact_ban_targets(&mut body);
            axum::response::IntoResponse::into_response(Json(body))
        }
        #[cfg(feature = "circuit-breaker")]
        "/circuit-breaker" => {
            let body = match source.circuit_breaker_status().await {
                Ok(status) => serde_json::json!({
                    "success": true,
                    "message": crate::i18n::t_simple("admin-ui-message-ok"),
                    "data": status,
                }),
                Err(e) => serde_json::json!({
                    "success": false,
                    "message": e.to_string(),
                    "data": serde_json::Value::Null,
                }),
            };
            axum::response::IntoResponse::into_response(Json(body))
        }
        #[cfg(not(feature = "circuit-breaker"))]
        "/circuit-breaker" => {
            axum::response::IntoResponse::into_response(Json(serde_json::json!({
                "success": false,
                "message": crate::i18n::t_simple("admin-ui-circuit-breaker-not-configured"),
                "data": serde_json::Value::Null,
            })))
        }
        // 路由表驱动注册下不可达；显性留痕防表与分派漂移
        other => axum::response::IntoResponse::into_response(Json(serde_json::json!({
            "success": false,
            "message": crate::i18n::t(
                "admin-ui-unhandled-route",
                &[("path", other.to_string())],
            ),
        }))),
    }
}

/// 内嵌单页（无构建链）：fetch `/snapshot` 与 `/circuit-breaker` 渲染
pub(crate) const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Limiteron admin snapshot (read-only)</title>
<style>
  :root { color-scheme: light dark; }
  body { font-family: system-ui, sans-serif; margin: 2rem auto; max-width: 60rem; padding: 0 1rem; }
  h1 { font-size: 1.3rem; } h2 { font-size: 1.05rem; margin-top: 1.8rem; }
  table { border-collapse: collapse; width: 100%; margin-top: .6rem; }
  th, td { border: 1px solid #8884; padding: .35rem .55rem; text-align: left; font-size: .88rem; }
  th { background: #8881; }
  .meta { color: #888; font-size: .8rem; }
  .ok { color: #1a7f37; } .bad { color: #c0392b; }
  code { background: #8882; padding: .05rem .3rem; border-radius: 3px; }
  #keybox { margin: .8rem 0; }
  #keybox input { padding: .25rem .4rem; }
</style>
</head>
<body>
<h1>Limiteron admin snapshot (read-only)</h1>
<p class="meta">This page issues GET requests only; auto-refresh <span id="refresh">5</span>s. Data sources: /snapshot, /circuit-breaker.</p>
<div id="keybox" hidden>
  <label for="apikey">API key</label>
  <input id="apikey" type="password" autocomplete="off" placeholder="Bearer API key">
  <button id="keysave" type="button">Save</button>
  <span class="bad" id="keymsg"></span>
</div>
<h2>Aggregate statistics</h2>
<table id="stats"><thead><tr><th>Metric</th><th>Value</th></tr></thead><tbody></tbody></table>
<h2>Health</h2>
<table id="health"><thead><tr><th>Component</th><th>Status</th></tr></thead><tbody></tbody></table>
<h2>Rules and decision chains</h2>
<table id="rules"><thead><tr><th>Rule</th><th>Priority</th><th>Matcher</th><th>Limiter</th><th>Nodes</th></tr></thead><tbody></tbody></table>
<h2>Circuit breaker</h2>
<table id="cb"><thead><tr><th>State</th><th>Failure rate</th><th>Slow call rate</th></tr></thead><tbody></tbody></table>
<script>
// API key 存放于 sessionStorage（不落 URL、不进浏览器历史与反代日志）；
// 数据端点要求 Authorization: Bearer <key>，缺失/不符返回 401。
function storedKey() {
  try { return sessionStorage.getItem('webUiKey') || ''; } catch (e) { return ''; }
}
function saveKey(key) {
  try { sessionStorage.setItem('webUiKey', key); } catch (e) { /* 不可用时仅本次会话生效 */ }
}
function authHeaders() {
  const key = storedKey();
  return key ? { 'Authorization': 'Bearer ' + key } : {};
}
function showKeyBox(message) {
  const box = document.querySelector('#keybox');
  if (box) box.hidden = false;
  const msg = document.querySelector('#keymsg');
  if (msg) msg.textContent = message || '';
}
async function getJson(path) {
  const resp = await fetch(path, { headers: authHeaders() });
  if (resp.status === 401) {
    showKeyBox('API key required (sent as Authorization: Bearer <key>)');
    throw new Error('API key required');
  }
  return await resp.json();
}
document.querySelector('#keysave').addEventListener('click', () => {
  const input = document.querySelector('#apikey');
  const key = (input.value || '').trim();
  if (!key) return;
  saveKey(key);
  input.value = '';
  document.querySelector('#keybox').hidden = true;
  refresh();
});
function put(id, rows) {
  const tb = document.querySelector(id + ' tbody');
  tb.innerHTML = '';
  for (const [k, v, cls] of rows) {
    const tr = document.createElement('tr');
    const td1 = document.createElement('td'); td1.textContent = k;
    const td2 = document.createElement('td'); td2.textContent = String(v);
    if (cls) td2.className = cls;
    tr.append(td1, td2); tb.append(tr);
  }
}
async function refresh() {
  try {
    const snap = await getJson('/snapshot');
    const s = snap.stats || {};
    put('#stats', [
      ['Total requests', s.total_requests ?? '-'],
      ['Rejected requests', s.rejected_requests ?? '-'],
      ['Banned requests', s.banned_requests ?? '-'],
      ['Active limit keys', s.active_keys ?? '-'],
      ['Config version', snap.config_version ?? '-'],
      ['Shutdown', snap.is_shutdown ?? '-', snap.is_shutdown ? 'bad' : 'ok'],
    ]);
    const h = snap.health || {};
    const healthy = (v) => v === true || v === 'healthy';
    put('#health', [
      ['storage', h.storage_healthy ?? '-', healthy(h.storage_healthy) ? 'ok' : 'bad'],
      ['ban_storage', h.ban_storage_healthy ?? '-', healthy(h.ban_storage_healthy) ? 'ok' : 'bad'],
      ['cache', h.cache_healthy ?? '-', healthy(h.cache_healthy) ? 'ok' : 'bad'],
      ['background_tasks', h.background_tasks_alive ?? '-', healthy(h.background_tasks_alive) ? 'ok' : 'bad'],
    ]);
    const rules = snap.rules || [];
    const chains = snap.chains || [];
    const chainNodes = (id) => {
      const c = chains.find((x) => x.rule_id === id || x.id === id);
      return c ? (c.node_count ?? (c.nodes || []).length) : '-';
    };
    put('#rules', rules.map((r) => [
      (r.id ?? '') + ' / ' + (r.name ?? ''),
      r.priority ?? '-',
      r.matcher_count ?? (r.matchers || []).length,
      r.limiter_kinds ? r.limiter_kinds.join(', ') : ((r.limiters || []).length),
      chainNodes(r.id),
    ]));
    try {
      const cb = await getJson('/circuit-breaker');
      const d = cb.data || {};
      put('#cb', [[
        d.state ?? 'unavailable', d.failure_rate ?? '-',
        d.slow_call_rate ?? '-',
      ].slice(0, 1).map((v) => [ 'state', v, v === 'Open' ? 'bad' : 'ok' ])[0],
      ['failure_rate', d.failure_rate ?? '-'],
      ['slow_call_rate', d.slow_call_rate ?? '-']]);
    } catch { put('#cb', [['state', 'unavailable', 'bad']]); }
  } catch (e) {
    put('#health', [['fetch', String(e), 'bad']]);
  }
}
refresh();
setInterval(refresh, 5000);
</script>
</body>
</html>
"#;

/// Web UI 配置（绑定默认 127.0.0.1，仅本机可见）
#[derive(Debug, Clone)]
pub struct WebUiConfig {
    /// 绑定地址；默认 `127.0.0.1`。改为非回环地址 = 无认证快照对同网段
    /// 暴露，必须前置反向代理（认证 + TLS）
    pub host: String,
    /// 绑定端口（默认 9091，与 admin API 默认端口 9090 区分）
    pub port: u16,
}

impl Default for WebUiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 9091,
        }
    }
}

impl WebUiConfig {
    /// 绑定地址是否为回环
    #[must_use]
    pub fn is_loopback_binding(&self) -> bool {
        self.host
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(matches!(self.host.as_str(), "localhost"))
    }

    /// 绑定目标 `host:port`
    #[must_use]
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// 只读 Web UI 服务器
pub struct WebUiServer {
    state: LimiteronState,
    config: WebUiConfig,
    access: WebUiAccess,
}

impl WebUiServer {
    /// 以应用状态与配置构造
    #[must_use]
    pub fn new(state: LimiteronState, config: WebUiConfig) -> Self {
        Self {
            state,
            config,
            access: WebUiAccess::default(),
        }
    }

    /// 设置访问控制（Host 校验 + 可选 API key）
    #[must_use]
    pub fn with_access(mut self, access: WebUiAccess) -> Self {
        self.access = access;
        self
    }

    /// 绑定配置
    #[must_use]
    pub fn address(&self) -> String {
        self.config.address()
    }

    /// 启动服务器（阻塞当前任务）
    ///
    /// 非回环绑定发出无认证暴露警示日志（不阻断——部署者可能确有内网
    /// 可信环境需求，但警示必须留痕，见模块文档）。
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.config.is_loopback_binding() {
            tracing::warn!(
                address = %self.config.address(),
                "web UI bound to a non-loopback address: the snapshot endpoints are UNAUTHENTICATED; \
                 expose only behind an authenticating reverse proxy (TLS terminated there)"
            );
        }
        let router = create_router_with_access(Arc::new(self.state.service()), self.access.clone());
        let listener = tokio::net::TcpListener::bind(self.config.address()).await?;
        log::info!(
            target: "admin-ui",
            "read-only web UI listening on {} (auth: {}; data: GET-only snapshots)",
            self.config.address(),
            if self.access.api_key.is_some() {
                "api-key"
            } else {
                "none"
            }
        );
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await?;
        Ok(())
    }

    /// 构建 Router（测试与外部 runner 挂载用）
    pub fn into_router(&self) -> Router {
        create_router_with_access(Arc::new(self.state.service()), self.access.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::LimiteronState;
    use crate::admin::handlers::tests::make_state;
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use tower::ServiceExt;

    /// 守卫 1：路由表全部为 GET 语义（表即事实来源，新增非 GET 即红灯）
    #[test]
    fn route_table_is_get_only() {
        // 表中路径即为路由清单；本模块仅经 get() 注册（编译期保证），
        // 此处断言表规模与已知面一致，防路径静默增删
        assert_eq!(ROUTE_TABLE.len(), 3);
        assert!(ROUTE_TABLE.contains(&"/"));
        assert!(ROUTE_TABLE.contains(&"/snapshot"));
        assert!(ROUTE_TABLE.contains(&"/circuit-breaker"));
    }

    /// 守卫 2：路由表每个路径 GET 可达（注册完整性——漏注册即红灯）
    #[tokio::test]
    async fn route_table_paths_are_all_registered_and_serve() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default());
        let router = server.into_router();
        for path in ROUTE_TABLE {
            let response = router
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "route table path {path} is not registered (table drifted from Router)"
            );
        }
    }

    /// 方法级守卫：对每个注册路径发 POST/PUT/DELETE，全部 405
    #[tokio::test]
    async fn non_get_methods_are_rejected_on_every_path() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default());
        let router = server.into_router();
        for path in ROUTE_TABLE {
            for method in [Method::POST, Method::PUT, Method::DELETE] {
                let response = router
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method(method.clone())
                            .uri(path)
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    StatusCode::METHOD_NOT_ALLOWED,
                    "{method} {path} must be rejected (read-only guard)"
                );
            }
        }
    }

    /// 数据端点语义：/snapshot 返回 introspect 投影；GET / 返回 HTML
    #[tokio::test]
    async fn get_endpoints_serve_readonly_snapshots() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default());
        let router = server.into_router();

        let index = router
            .clone()
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(index.status(), StatusCode::OK);
        let body = axum::body::to_bytes(index.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(
            html.contains("read-only"),
            "the embedded single page should state its read-only nature"
        );
        assert!(
            !html.contains("POST") && !html.contains("DELETE"),
            "the page script must not contain write method calls"
        );

        let snapshot = router
            .oneshot(Request::get("/snapshot").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(snapshot.status(), StatusCode::OK);
        let body = axum::body::to_bytes(snapshot.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json.get("stats").is_some(),
            "the snapshot should include the stats surface"
        );
    }

    #[test]
    fn binding_defaults_to_loopback_and_flags_non_loopback() {
        let config = WebUiConfig::default();
        assert!(config.is_loopback_binding());
        assert_eq!(config.address(), "127.0.0.1:9091");

        let exposed = WebUiConfig {
            host: "0.0.0.0".to_string(),
            port: 8080,
        };
        assert!(!exposed.is_loopback_binding());
    }

    /// 类型窄化烟测：Arc<dyn ReadOnlySnapshotSource> 可用（编译期守卫 3
    /// 的对象存在性确认）
    #[tokio::test]
    async fn readonly_projection_serves_from_state() {
        let state: LimiteronState = make_state().await;
        let source: Arc<dyn ReadOnlySnapshotSource> = Arc::new(state.service());
        let status = source.status().await;
        assert_eq!(status.total_requests, 0);
        let snapshot = source.introspect().await;
        assert!(snapshot.get("stats").is_some());
    }

    /// 快照降敏：封禁原因整体剔除，目标按类型掩码
    ///
    /// 无鉴权面（`/snapshot`）不得原样输出封禁目标与封禁原因。
    #[test]
    fn snapshot_redaction_removes_reason_and_masks_target() {
        let mut body = serde_json::json!({
            "stats": {"total_requests": 1},
            "active_bans": [
                {
                    "target": {"type": "ip", "value": "192.168.1.100"},
                    "reason": "internal incident #42",
                    "ban_times": 2,
                    "is_manual": true
                },
                {
                    "target": {"type": "user", "value": "user-1234"},
                    "reason": "abuse",
                    "ban_times": 1
                },
                {
                    "target": {"type": "mac", "value": "00:1a:2b:3c:4d:5e"},
                    "reason": "stolen device"
                },
                {
                    "target": {"type": "geo", "value": {"country_code": "CN"}},
                    "reason": "region block"
                }
            ]
        });

        redact_ban_targets(&mut body);

        let bans = body["active_bans"]
            .as_array()
            .expect("active_bans must stay an array");
        assert_eq!(bans.len(), 4);
        for ban in bans {
            assert!(
                ban.get("reason").is_none(),
                "the ban reason must be removed from the unauthenticated snapshot: {ban}"
            );
        }
        assert_eq!(bans[0]["target"]["value"], "192.168.***.***");
        assert_eq!(bans[1]["target"]["value"], "us***34");
        assert_eq!(bans[2]["target"]["value"], "00***5e");
        assert_eq!(bans[3]["target"]["value"]["country_code"], "***");
        // 非敏感维度保留
        assert_eq!(bans[0]["ban_times"], 2);
        assert_eq!(bans[0]["is_manual"], true);
    }

    /// 降敏对缺字段/异常结构不 panic（快照结构随版本演进）
    #[test]
    fn snapshot_redaction_tolerates_unexpected_shapes() {
        let mut body = serde_json::json!({
            "active_bans": [
                {"target": {"type": "ip"}},
                {"target": {"value": "1.2.3.4"}},
                {"target": {"type": "ip", "value": 42}},
                {},
                "not-an-object"
            ]
        });
        redact_ban_targets(&mut body);
        assert_eq!(body["active_bans"].as_array().unwrap().len(), 5);
    }

    /// Host 判定：IP 字面量与 localhost 放行，DNS 名默认不放行
    #[test]
    fn host_check_policy() {
        let allowed = ["admin.example.com".to_string()];
        assert!(host_allowed("127.0.0.1:9091", &allowed));
        assert!(host_allowed("localhost:9091", &allowed));
        assert!(host_allowed("[::1]:9091", &allowed));
        assert!(host_allowed("192.168.1.10:9091", &allowed));
        assert!(host_allowed("admin.example.com", &allowed));
        assert!(host_allowed("ADMIN.EXAMPLE.COM", &allowed));
        assert!(!host_allowed("evil.example.com", &allowed));
        assert!(!host_allowed("127.0.0.1.evil.com", &allowed));
        assert!(!host_allowed("", &allowed));
    }

    /// 严格 Host 校验：DNS 名（DNS rebinding 面）被拒，IP 字面量放行
    #[tokio::test]
    async fn strict_host_check_rejects_dns_name_host() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default())
            .with_access(WebUiAccess::strict().with_allowed_hosts(["admin.example.com"]));
        let router = server.into_router();

        let rejected = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/snapshot")
                    .header("Host", "evil.example.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            rejected.status(),
            StatusCode::MISDIRECTED_REQUEST,
            "a DNS-name Host must be rejected in strict mode"
        );

        let allowed = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/snapshot")
                    .header("Host", "127.0.0.1:9091")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);

        let configured = router
            .oneshot(
                Request::builder()
                    .uri("/snapshot")
                    .header("Host", "admin.example.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(configured.status(), StatusCode::OK);
    }

    /// 默认（宽松）模式不校验 Host：既有部署行为不变
    #[tokio::test]
    async fn permissive_host_check_keeps_existing_behavior() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default());
        let router = server.into_router();

        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/snapshot")
                    .header("Host", "any.example.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// API key 鉴权：配置后缺失/错误凭证一律 401，正确凭证放行
    #[tokio::test]
    async fn api_key_auth_gates_the_snapshot() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default())
            .with_access(WebUiAccess::default().with_api_key("web-ui-key-16chars!!"));
        let router = server.into_router();

        let anonymous = router
            .clone()
            .oneshot(Request::get("/snapshot").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

        let wrong = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/snapshot")
                    .header("Authorization", "Bearer wrong-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

        let authorized = router
            .oneshot(
                Request::builder()
                    .uri("/snapshot")
                    .header("Authorization", "Bearer web-ui-key-16chars!!")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authorized.status(), StatusCode::OK);
    }

    /// 启动烟测：随机回环端口真实 serve，HTTP GET 可达
    #[tokio::test]
    async fn server_serves_on_loopback() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let state = make_state().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let config = WebUiConfig {
            host: "127.0.0.1".to_string(),
            port: addr.port(),
        };
        let server = WebUiServer::new(state, config);
        let handle = tokio::spawn(async move { server.start().await });

        // 就绪轮询后发真实 HTTP 请求
        let mut attempt = 0;
        let mut stream = loop {
            attempt += 1;
            match tokio::net::TcpStream::connect(addr).await {
                Ok(s) => break s,
                Err(_) if attempt < 50 => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await
                }
                Err(e) => panic!("server never became reachable: {e}"),
            }
        };
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        let text = String::from_utf8_lossy(&buf);
        assert!(text.starts_with("HTTP/1.1 200 OK"), "got: {text}");

        handle.abort();
    }

    /// Debug 脱敏：api_key 不得以明文出现在 Debug 输出中
    #[test]
    fn web_ui_access_debug_redacts_api_key() {
        let access = WebUiAccess::default().with_api_key("web-ui-key-16chars!!");
        let rendered = format!("{access:?}");
        assert!(
            !rendered.contains("web-ui-key-16chars!!"),
            "the api key must not be printed in clear text: {rendered}"
        );
        assert!(
            rendered.contains("<redacted"),
            "the Debug output must mark the api key as redacted: {rendered}"
        );
    }

    /// 严格 Host 校验：既无 Host 头也无 URI authority 一律 421
    #[tokio::test]
    async fn strict_host_check_rejects_request_without_host_or_authority() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default())
            .with_access(WebUiAccess::strict().with_allowed_hosts(["admin.example.com"]));
        let router = server.into_router();

        let rejected = router
            .oneshot(Request::get("/snapshot").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            rejected.status(),
            StatusCode::MISDIRECTED_REQUEST,
            "a request without any host information must be rejected in strict mode"
        );
    }

    /// 严格 Host 校验：h2c 形态（只有 :authority，无 Host 头）同样受白名单约束
    #[tokio::test]
    async fn strict_host_check_uses_uri_authority_when_host_header_is_absent() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default())
            .with_access(WebUiAccess::strict().with_allowed_hosts(["admin.example.com"]));
        let router = server.into_router();

        let rejected = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("http://evil.example.com/snapshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            rejected.status(),
            StatusCode::MISDIRECTED_REQUEST,
            "the URI authority must be checked when the Host header is absent (h2c shape)"
        );

        let allowed = router
            .oneshot(
                Request::builder()
                    .uri("http://admin.example.com/snapshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            allowed.status(),
            StatusCode::OK,
            "a whitelisted URI authority must pass the strict host check"
        );
    }

    /// 空 key 不得配置：空串会与畸形凭证（"Bearer "）相等
    #[test]
    #[should_panic(expected = "web UI api_key must be non-empty")]
    fn with_api_key_rejects_empty_key() {
        let _ = WebUiAccess::default().with_api_key("");
    }

    /// 过短 key 可被在线穷举，构造期即拒绝
    #[test]
    #[should_panic(expected = "web UI api_key must be non-empty")]
    fn with_api_key_rejects_short_key() {
        let _ = WebUiAccess::default().with_api_key("short-key");
    }

    /// 空 key + "Bearer " 畸形凭证不得通过鉴权
    ///
    /// 公开字段直写可绕过构造器校验，鉴权处必须 fail closed——否则空串
    /// 相等（constant_time_eq("", "") 为真）即构成真实绕过。
    #[tokio::test]
    async fn empty_api_key_and_malformed_bearer_are_rejected() {
        let state = make_state().await;
        let access = WebUiAccess {
            api_key: Some(String::new()),
            ..WebUiAccess::default()
        };
        let server = WebUiServer::new(state, WebUiConfig::default()).with_access(access);
        let router = server.into_router();

        let malformed = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/snapshot")
                    .header("Authorization", "Bearer ")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            malformed.status(),
            StatusCode::UNAUTHORIZED,
            "an empty api key must not accept a malformed credential"
        );

        let empty_token = router
            .oneshot(
                Request::builder()
                    .uri("/snapshot")
                    .header("Authorization", "Bearer")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            empty_token.status(),
            StatusCode::UNAUTHORIZED,
            "a credential without a token must not authenticate"
        );
    }

    /// 弱 key（长度不足）不得作为有效凭证（绕过构造器直写公开字段亦然）
    #[tokio::test]
    async fn weak_api_key_cannot_authenticate() {
        let state = make_state().await;
        let access = WebUiAccess {
            api_key: Some("k".to_string()),
            ..WebUiAccess::default()
        };
        let server = WebUiServer::new(state, WebUiConfig::default()).with_access(access);
        let router = server.into_router();

        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/snapshot")
                    .header("Authorization", "Bearer k")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "a key shorter than the minimum length must not authenticate"
        );
    }

    /// 静态页豁免凭证校验：页面不含数据须可达，数据端点仍需凭证
    #[tokio::test]
    async fn static_page_is_served_without_api_key_but_data_endpoints_are_not() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default())
            .with_access(WebUiAccess::default().with_api_key("web-ui-key-16chars!!"));
        let router = server.into_router();

        let index = router
            .clone()
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            index.status(),
            StatusCode::OK,
            "the static page carries no data and must stay reachable without a credential"
        );

        let snapshot = router
            .clone()
            .oneshot(Request::get("/snapshot").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            snapshot.status(),
            StatusCode::UNAUTHORIZED,
            "the snapshot endpoint must still require the api key"
        );

        let breaker = router
            .oneshot(
                Request::get("/circuit-breaker")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            breaker.status(),
            StatusCode::UNAUTHORIZED,
            "the circuit breaker endpoint must still require the api key"
        );
    }

    /// 页面侧须提供取 key 入口并带上 Authorization 头
    ///
    /// 启用 API key 后，浏览器必须能取到数据——页面须有 key 入口
    /// （sessionStorage，不落 URL）并在 fetch 上带 Bearer 凭证。
    #[test]
    fn page_provides_api_key_entry_and_bearer_header() {
        assert!(
            INDEX_HTML.contains("sessionStorage"),
            "the page must offer a key entry backed by sessionStorage"
        );
        assert!(
            INDEX_HTML.contains("'Authorization': 'Bearer '"),
            "the page's fetches must carry the Bearer credential"
        );
        assert!(
            INDEX_HTML.contains("API key required"),
            "the page must render an explicit state when a key is required"
        );
    }

    /// 鉴权失败限流：窗口内失败次数达上限后 429（在线穷举被阻断）
    #[tokio::test]
    async fn auth_failures_are_rate_limited() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default())
            .with_access(WebUiAccess::default().with_api_key("web-ui-key-16chars!!"));
        let router = server.into_router();

        for i in 0..AUTH_FAILURE_MAX {
            let resp = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/snapshot")
                        .header("Authorization", "Bearer wrong-key")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::UNAUTHORIZED,
                "failed attempt {i} must be rejected with 401"
            );
        }

        let limited = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/snapshot")
                    .header("Authorization", "Bearer wrong-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            limited.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "the attempt after the failure quota must be rate limited"
        );
        assert!(
            limited.headers().get("Retry-After").is_some(),
            "the 429 response must carry a Retry-After header"
        );

        // 静态页不含数据，不参与凭证校验（不计入失败额度）
        let index = router
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(index.status(), StatusCode::OK);
    }

    /// 鉴权失败限流按客户端分桶：一个客户端耗尽额度不影响其他客户端
    #[tokio::test]
    async fn auth_failure_limit_is_per_client() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default())
            .with_access(WebUiAccess::default().with_api_key("web-ui-key-16chars!!"));
        let router = server.into_router();

        let client_a: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        for _ in 0..AUTH_FAILURE_MAX {
            let mut req = Request::builder()
                .uri("/snapshot")
                .header("Authorization", "Bearer wrong-key")
                .body(Body::empty())
                .unwrap();
            req.extensions_mut().insert(ConnectInfo(client_a));
            let resp = router.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        }

        let mut req = Request::builder()
            .uri("/snapshot")
            .header("Authorization", "Bearer wrong-key")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(client_a));
        let limited = router.clone().oneshot(req).await.unwrap();
        assert_eq!(
            limited.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "the exhausted client must be rate limited"
        );

        let client_b: SocketAddr = "192.168.1.1:5678".parse().unwrap();
        let mut req = Request::builder()
            .uri("/snapshot")
            .header("Authorization", "Bearer wrong-key")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(client_b));
        let other = router.oneshot(req).await.unwrap();
        assert_eq!(
            other.status(),
            StatusCode::UNAUTHORIZED,
            "another client must count independently from the exhausted one"
        );
    }

    /// 畸形 IPv6 字面量不得被当作合法 IP 放行
    #[test]
    fn malformed_ipv6_literals_are_rejected() {
        assert!(
            !host_allowed("[::1]evil.example.com", &[]),
            "a bracketed literal with trailing garbage must not be accepted as an IP"
        );
        assert!(
            !host_allowed("[::1", &[]),
            "an unterminated bracketed literal must not be accepted"
        );
        assert!(
            host_allowed("[::1]:9091", &[]),
            "a well-formed IPv6 literal must still be accepted"
        );
    }

    /// 未知封禁变体默认脱敏（fail closed：新增变体不得原样输出）
    #[test]
    fn unknown_ban_target_types_are_redacted() {
        let mut body = serde_json::json!({
            "active_bans": [
                {"target": {"type": "future-variant", "value": "sensitive-target-value"}},
                {"target": {"type": "future-variant", "value": {"nested": "sensitive-target-value"}}}
            ]
        });

        redact_ban_targets(&mut body);

        assert_ne!(
            body["active_bans"][0]["target"]["value"], "sensitive-target-value",
            "an unknown target variant must be masked by default"
        );
        assert_ne!(
            body["active_bans"][1]["target"]["value"]["nested"], "sensitive-target-value",
            "an unknown target variant with an object shape must be masked by default"
        );
    }

    /// 漂移守卫：BanTarget 每个变体的封禁目标都须被遮蔽
    ///
    /// 变体清单为穷尽 match——新增变体时此处编译失败，强制同步脱敏口径
    /// （web.rs 的 match 不得与 `logging::redact_ban_target` 长期漂移）。
    #[test]
    fn ban_target_redaction_covers_every_variant() {
        for target in ban_target_samples() {
            let mut body = serde_json::json!({ "active_bans": [{ "target": target }] });
            let raw_leaves = string_leaves(&body["active_bans"][0]["target"]["value"]);
            redact_ban_targets(&mut body);
            let masked_leaves = string_leaves(&body["active_bans"][0]["target"]["value"]);

            assert_eq!(
                raw_leaves.len(),
                masked_leaves.len(),
                "the redaction must preserve the value shape"
            );
            for (raw, masked) in raw_leaves.iter().zip(masked_leaves.iter()) {
                assert_ne!(
                    masked, raw,
                    "every ban target variant must be masked: {raw} -> {masked}"
                );
                assert!(
                    !masked.contains(raw.as_str()),
                    "the raw ban target must not survive redaction: {raw} -> {masked}"
                );
            }
        }
    }

    /// BanTarget 全变体样本
    ///
    /// `sample` 为穷尽 match：枚举新增变体时编译失败（漂移守卫红灯）。
    fn ban_target_samples() -> Vec<crate::storage::BanTarget> {
        use crate::storage::BanTarget;

        fn sample(target: &BanTarget) -> BanTarget {
            match target {
                BanTarget::Ip(_) => BanTarget::Ip("192.168.1.100".to_string()),
                BanTarget::UserId(_) => BanTarget::UserId("user-1234".to_string()),
                BanTarget::Mac(_) => BanTarget::Mac("00:1a:2b:3c:4d:5e".to_string()),
                BanTarget::Geo { .. } => BanTarget::Geo {
                    country_code: "CN".to_string(),
                },
                BanTarget::Cidr(_) => BanTarget::Cidr("10.0.0.0/8".to_string()),
            }
        }

        vec![
            sample(&BanTarget::Ip(String::new())),
            sample(&BanTarget::UserId(String::new())),
            sample(&BanTarget::Mac(String::new())),
            sample(&BanTarget::Geo {
                country_code: String::new(),
            }),
            sample(&BanTarget::Cidr(String::new())),
        ]
    }

    /// 收集 JSON 值中的全部字符串叶子
    fn string_leaves(value: &serde_json::Value) -> Vec<String> {
        let mut leaves = Vec::new();
        collect_string_leaves(value, &mut leaves);
        leaves
    }

    fn collect_string_leaves(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(s) => out.push(s.clone()),
            serde_json::Value::Array(items) => {
                for item in items {
                    collect_string_leaves(item, out);
                }
            }
            serde_json::Value::Object(map) => {
                for leaf in map.values() {
                    collect_string_leaves(leaf, out);
                }
            }
            _ => {}
        }
    }

    /// Bearer scheme 大小写不敏感（RFC 7235 §2.1）
    #[tokio::test]
    async fn bearer_scheme_is_case_insensitive() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default())
            .with_access(WebUiAccess::default().with_api_key("web-ui-key-16chars!!"));
        let router = server.into_router();

        for scheme in ["bearer", "BEARER", "BeArEr"] {
            let resp = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/snapshot")
                        .header("Authorization", format!("{scheme} web-ui-key-16chars!!"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "the {scheme} scheme must be accepted (RFC 7235 case-insensitive)"
            );
        }

        let other_scheme = router
            .oneshot(
                Request::builder()
                    .uri("/snapshot")
                    .header("Authorization", "Basic web-ui-key-16chars!!")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            other_scheme.status(),
            StatusCode::UNAUTHORIZED,
            "a non-Bearer scheme must not authenticate"
        );
    }

    /// 共享访问控制句柄（Arc）后，鉴权与 Host 校验仍然生效
    ///
    /// 访问控制改为 Arc 共享后每请求只 clone 句柄——本用例守住"重构不得
    /// 削弱访问控制"这一不变量。
    #[tokio::test]
    async fn shared_access_handle_keeps_enforcing_access_control() {
        let state = make_state().await;
        let server = WebUiServer::new(state, WebUiConfig::default())
            .with_access(WebUiAccess::strict().with_api_key("web-ui-key-16chars!!"));
        let router = server.into_router();

        let bad_host = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/snapshot")
                    .header("Host", "evil.example.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad_host.status(), StatusCode::MISDIRECTED_REQUEST);

        let anonymous = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/snapshot")
                    .header("Host", "127.0.0.1:9091")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

        let authorized = router
            .oneshot(
                Request::builder()
                    .uri("/snapshot")
                    .header("Host", "127.0.0.1:9091")
                    .header("Authorization", "Bearer web-ui-key-16chars!!")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authorized.status(), StatusCode::OK);
    }
}
