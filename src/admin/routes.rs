// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Route definitions

use crate::i18n::t;
use ahash::AHashMap;
use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, RETRY_AFTER},
    },
    middleware::from_fn,
    routing::{delete, get, post, put},
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{
    config::{
        AdminApiConfig, AdminRole, MIN_API_KEY_LEN, default_rate_limit_for, role_key_is_acceptable,
    },
    handlers,
    server::LimiteronState,
};

/// 通过 request extensions 传递的鉴权 operator 身份
///
/// middleware 在通过 API key 鉴权后，根据 `AdminApiConfig::api_key_operators`
/// 解析对应 operator 身份并写入 request extensions。handlers 优先使用此值，
/// 不再信任 JSON body 中的 `operator` 字段，防止身份伪造。
#[derive(Debug, Clone)]
pub struct OperatorIdentity(pub String);

/// 通过 request extensions 传递的鉴权角色
///
/// middleware 鉴权通过后按 `AdminApiConfig::api_key_roles` 解析角色写入
/// extensions；handlers 可据此区分调用者权限级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatorRole(pub AdminRole);

/// 端点所需角色（基于 HTTP 方法）
///
/// GET/HEAD 为只读（viewer 可访问）；其余方法（POST/PUT/DELETE 等）为
/// 写操作，需要 admin。探针路径（/healthz /readyz /metrics）在更早的
/// bypass 分支放行，不进入本判定。
pub(crate) fn role_allows(role: AdminRole, method: &http::Method) -> bool {
    if method == http::Method::GET || method == http::Method::HEAD {
        role.allows_read()
    } else {
        role.allows_write()
    }
}

/// per-client rate limit bucket 类型
///
/// key = (group, client_ip)，value = (request_count, window_start Instant,
/// window Duration)。窗口长度随 entry 存储：各组窗口可不同，用「当前组」
/// 的窗口判定所有组 entry 是否过期会让长窗口组的计数被短窗口组提前清零
/// （限流被跨组绕过）。
/// 提取为 type alias 以避免 clippy::type_complexity 警告。
type RateBucketMap = AHashMap<(String, String), (u64, Instant, Duration)>;
type RateBuckets = Arc<Mutex<RateBucketMap>>;

/// 补强：per-client bucket 内存上限。
///
/// 原实现从不删除过期 entry，攻击者用轮换源 IP 可令 map 单调膨胀至 OOM（内存耗尽 DoS）。
/// 此处设硬上限：超过则先清扫已过期窗口的 entry；若清扫后仍超限，移除最旧（最小 window_start）的 entry。
const RATE_BUCKET_MAX_ENTRIES: usize = 10_000;

/// 常量时间字符串比较（凭证比对，避免计时侧信道）
pub(crate) fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}

/// 锁定 rate_buckets mutex，从中毒状态恢复而非 panic。
///
/// std::sync::Mutex 在持锁期间 panic 时会"中毒"。原实现 `.lock().expect(...)`
/// 会在中毒时 panic，导致 admin API 整体不可用——对于 per-client 速率分桶
/// 这样的非关键、内存态、按窗口重置的数据，这是过度反应。
///
/// 此函数恢复中毒 mutex 的内部数据（可能已不一致）并记录 warn 日志，
/// 让运维介入排查根因。窗口过期后计数器自然重置，影响可控。
fn lock_rate_buckets(buckets: &Mutex<RateBucketMap>) -> std::sync::MutexGuard<'_, RateBucketMap> {
    buckets.lock().unwrap_or_else(|poisoned| {
        log::warn!(
            target: "admin-api",
            "rate_buckets mutex was poisoned by a previous panic; \
             recovering inner state (counters may be stale until window reset)"
        );
        poisoned.into_inner()
    })
}

/// K8s 探针/指标端点 bypass 路径
///
/// 这些端点跳过 API key 认证：K8s kubelet 探针与 Prometheus 抓取器
/// 不携带管理凭证；端点本身只读（/metrics 仅暴露流量指标）。
///
/// `/healthz` `/readyz` 完全 bypass 速率限制——K8s 探针被限流会触发
/// Pod 重启；`/metrics` 仍走速率限制（免凭证反复抓取会放大为资源消耗），
/// 默认额度高于业务端点（Prometheus 按目标批量抓取）。
const PROBE_PATHS: [&str; 3] = ["/healthz", "/readyz", "/metrics"];

/// 指标端点路径（免认证但受限流）
const METRICS_PATH: &str = "/metrics";

fn is_probe_path(path: &str) -> bool {
    PROBE_PATHS.contains(&path)
}

/// 按路径前缀分组
///
/// 用于按端点分组应用不同的速率限制策略：
/// - `/api/v1/ban*` → "ban"（默认 100/min）
/// - `/api/v1/quota*` → "quota"（默认 50/min）
/// - `/metrics` → "metrics"（默认 600/min）
/// - 其他 → "default"（默认 200/min）
///
/// 各组默认额度取自 `config::default_rate_limit_for`（与配置默认 map 同源）；
/// 配置只覆盖部分分组时，未覆盖的分组回退到同一额度，不会因缺项而放宽。
fn group_for_path(path: &str) -> &'static str {
    if path.starts_with("/api/v1/ban") {
        "ban"
    } else if path.starts_with("/api/v1/quota") {
        "quota"
    } else if path == METRICS_PATH {
        "metrics"
    } else {
        "default"
    }
}

pub fn create_router(state: LimiteronState, config: &AdminApiConfig) -> Router {
    let mut router = Router::new()
        // K8s 探针与指标端点（bypass 认证）
        .route("/healthz", get(handlers::healthz))
        .route("/readyz", get(handlers::readyz))
        .route("/metrics", get(handlers::metrics))
        // 系统状态
        .route("/api/v1/status", get(handlers::get_status))
        // 封禁管理
        .route("/api/v1/ban", post(handlers::create_ban))
        .route("/api/v1/ban/{target}", delete(handlers::delete_ban))
        // 配额管理
        .route("/api/v1/quota/{user_id}", put(handlers::update_quota))
        // 熔断器状态
        .route(
            "/api/v1/status/circuit-breaker",
            get(handlers::get_circuit_breaker_status),
        )
        // Governor 运行时自省
        .route("/api/v1/introspect", get(handlers::introspect))
        // 规则热更新（原子换配置）
        .route("/api/v1/config", post(handlers::apply_config))
        // 批量检查（N key 一次决策）
        .route("/api/v1/check/batch", post(handlers::check_batch))
        // 批量令牌预取
        .route("/api/v1/tokens/prefetch", post(handlers::prefetch_tokens))
        .with_state(state);

    let api_key = config.api_key.clone();
    let operator_mapping = config.api_key_operators.clone();
    let role_mapping = config.api_key_roles.clone();
    // 主 api_key 的强度校验与 AdminServer::start/into_router 同口径：
    // 校验失败（空 key / 长度 <16）时主 key 不得成为有效凭证——否则直接
    // 调用本函数（绕过 AdminServer）的部署可用 1 字符主 key 构造出
    // 接受该 key 的路由。校验不通过即拒绝主 key（fail closed），并显性
    // 记错；空主 key 的畸形凭证（"Bearer "）另行在鉴权处排除。
    let main_key_accepted = match config.validate() {
        Ok(()) => config.primary_key_is_acceptable(),
        Err(e) => {
            log::error!(
                target: "admin-api",
                "admin API config invalid, the primary api_key is rejected: {e}"
            );
            false
        }
    };
    // 角色映射中的 key 同样是可直接登录管理面的活凭证：空白 key 会退化为
    // "Bearer " 形如畸形凭证，一律剔除；短于最小长度的 key 与主 key 同口径
    // fail closed——不作有效凭证（过短的 key 可被穷举，映射为角色后即获
    // 管理面访问权），并显性记错（只报角色，不输出 key 原文）。
    let mut valid_keys = Vec::new();
    for (key, role) in &config.api_key_roles {
        if key.trim().is_empty() {
            continue;
        }
        if !role_key_is_acceptable(key) {
            log::error!(
                target: "admin-api",
                "api_key_roles contains a key shorter than {} characters (role {}); it is rejected",
                MIN_API_KEY_LEN,
                role.as_str()
            );
            continue;
        }
        valid_keys.push(key.clone());
    }
    let rate_limits = config.rate_limits.clone();
    // per-client rate buckets，key = (group, client_ip)
    //
    // 原实现按 group 全局共享计数器，单个恶意客户端耗尽配额后
    // 所有合法管理员也被限制（DoS 放大器）。此处改为按 (group, client_ip)
    // 分桶，每个客户端在每组内有独立配额。
    //
    // client_ip 来源：`ConnectInfo<SocketAddr>`（TCP 连接远端）。
    // admin API 通常直接暴露，不信任 X-Forwarded-For（可伪造）。
    // 无 connect info（如测试 oneshot）回退到 "unknown" 共享桶。
    let rate_buckets: RateBuckets = Arc::new(Mutex::new(AHashMap::new()));
    router = router.layer(from_fn(
        move |mut req: Request<Body>, next: axum::middleware::Next| {
            let api_key = api_key.clone();
            let operator_mapping = operator_mapping.clone();
            let role_mapping = role_mapping.clone();
            let valid_keys = valid_keys.clone();
            let rate_limits = rate_limits.clone();
            let rate_buckets = rate_buckets.clone();
            async move {
                let path = req.uri().path();
                let is_probe = is_probe_path(path);
                // 探针端点（healthz/readyz）完全 bypass 速率限制与认证：
                // K8s 探针被限流会触发 Pod 重启
                if is_probe && path != METRICS_PATH {
                    return next.run(req).await;
                }

                // 速率限制检查（在鉴权之前，防止暴力破解和 DDoS）
                let group = group_for_path(path);
                // 提取 client IP 用于 per-client 分桶
                let client_ip = req
                    .extensions()
                    .get::<ConnectInfo<SocketAddr>>()
                    .map(|ci| ci.0.ip().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let (max, window_secs) = rate_limits
                    .get(group)
                    .copied()
                    .unwrap_or_else(|| default_rate_limit_for(group));
                let window = Duration::from_secs(window_secs);
                let now = Instant::now();
                {
                    let mut buckets = lock_rate_buckets(&rate_buckets);
                    // 补强：先清扫已过期 entry，防止 map 无限膨胀（OOM DoS）。
                    // 过期判定用 entry 自带的窗口：各组窗口长度可不同，
                    // 用当前组的窗口判定所有组会提前清零长窗口组的计数。
                    buckets.retain(|_, (_, start, w)| now.duration_since(*start) < *w);
                    // 清扫后若仍超容量上限，淘汰最旧（window_start 最小）的 entry。
                    if buckets.len() >= RATE_BUCKET_MAX_ENTRIES
                        && let Some(oldest) = buckets
                            .iter()
                            .min_by_key(|(_, (_, start, _))| *start)
                            .map(|(k, _)| k.clone())
                    {
                        buckets.remove(&oldest);
                    }
                    let key = (group.to_string(), client_ip);
                    let entry = buckets.entry(key).or_insert((0, now, window));
                    if now.duration_since(entry.1) >= entry.2 {
                        // 窗口过期，重置计数（保留本组窗口长度）
                        let entry_window = entry.2;
                        *entry = (1, now, entry_window);
                    } else if entry.0 < max {
                        entry.0 += 1;
                    } else {
                        // 超限：返回 429 + Retry-After（响应体经 FTL 目录本地化，
                        // 当前 locale → en 回退）
                        let retry_after = entry.2 - now.duration_since(entry.1);
                        let mut resp = axum::response::Response::new(Body::from(
                            crate::i18n::t_simple("rate-limit-exceeded"),
                        ));
                        *resp.status_mut() = StatusCode::TOO_MANY_REQUESTS;
                        if let Ok(val) = retry_after
                            .as_secs()
                            .to_string()
                            .parse::<http::HeaderValue>()
                        {
                            resp.headers_mut().insert(RETRY_AFTER, val);
                        }
                        return resp;
                    }
                }

                // /metrics 免 API key 认证（Prometheus 抓取器不携带管理凭证），
                // 但已受上方速率限制约束（免凭证反复抓取会放大为资源消耗）
                if is_probe {
                    return next.run(req).await;
                }

                // 鉴权
                let auth_header = req
                    .headers()
                    .get(AUTHORIZATION)
                    .and_then(|v| v.to_str().ok());

                let expected = format!("Bearer {}", api_key);
                let token = match auth_header {
                    // 空主 key 不得作为有效凭证：否则 "Bearer " 形如畸形凭证
                    // 可通过鉴权（validate() 仅在 enabled 时要求非空，直接
                    // 调用 create_router 可绕过该校验）；主 key 未通过强度
                    // 校验（过短）时同样不作为有效凭证
                    Some(token)
                        if (main_key_accepted && constant_time_eq(token, &expected))
                            || valid_keys
                                .iter()
                                .any(|k| constant_time_eq(token, &format!("Bearer {k}"))) =>
                    {
                        Some(token.to_string())
                    }
                    _ => None,
                };
                let Some(token) = token else {
                    let mut resp = axum::response::Response::new(Body::from(
                        crate::i18n::t_simple("admin-api-invalid-key"),
                    ));
                    *resp.status_mut() = StatusCode::UNAUTHORIZED;
                    return resp;
                };

                // API key 鉴权通过后，
                // 将 operator 身份从 mapping 解析并写入 request extensions。
                // 必须用请求中实际提交的 token（而非全局单一 api_key）查映射，
                // 否则多 key 部署下所有 key 都落到同一 operator，丧失身份隔离。
                // mapping 为空 → 回退到默认 "admin-api"（向后兼容），记录 warn。
                let raw_key = token.strip_prefix("Bearer ").unwrap_or(&token).to_string();
                let operator = operator_mapping.get(&raw_key).cloned().unwrap_or_else(|| {
                    log::warn!(
                        target: "admin-api",
                        "{}",
                        t("admin-api-key-operator-fallback", &[])
                    );
                    "admin-api".to_string()
                });
                req.extensions_mut().insert(OperatorIdentity(operator));

                // RBAC：解析角色 → 端点授权（越权 403，与凭证无效 401 区分）。
                // 未在角色映射中的合法 key 默认 admin（向后兼容单 key 部署）。
                let role = role_mapping
                    .get(&raw_key)
                    .copied()
                    .unwrap_or(AdminRole::Admin);
                if !role_allows(role, req.method()) {
                    log::warn!(
                        target: "admin-api",
                        "{}",
                        t(
                            "admin-rbac-denied",
                            &[
                                ("role", role.as_str().to_string()),
                                ("method", req.method().to_string()),
                                ("path", req.uri().path().to_string()),
                            ],
                        )
                    );
                    let mut resp = axum::response::Response::new(Body::from(
                        crate::i18n::t_simple("admin-rbac-insufficient-role"),
                    ));
                    *resp.status_mut() = StatusCode::FORBIDDEN;
                    return resp;
                }
                req.extensions_mut().insert(OperatorRole(role));

                next.run(req).await
            }
        },
    ));

    router
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::AdminApiConfig;
    use crate::admin::handlers::tests::make_state;
    #[cfg(feature = "ban-manager")]
    use crate::admin::handlers::tests::{TestDeps, make_governor, make_state_with};
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header::AUTHORIZATION};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// 构造带 BanManager 的 LimiteronState（用于封禁相关测试）
    #[cfg(feature = "ban-manager")]
    async fn make_state_with_ban_manager() -> LimiteronState {
        use crate::BanManager;

        let ban_manager = Arc::new(
            BanManager::new()
                .await
                .expect("BanManager::new should succeed"),
        );
        make_state_with(
            Arc::new(make_governor().await),
            TestDeps {
                ban_manager: Some(ban_manager),
                ..Default::default()
            },
        )
    }

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
    ///
    /// 消费方测试组均经 ban-manager 门控，helper 随之门控。
    #[cfg(feature = "ban-manager")]
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

    // ========================================================================
    // rate_buckets Mutex 中毒恢复
    //
    // 验证策略：
    // 1. 构造 Mutex 并通过持锁时 panic 使其中毒
    // 2. 调用 lock_rate_buckets 验证不 panic 而是恢复（返回可用 guard）
    // 3. 验证恢复后的 guard 可正常读写
    // ========================================================================

    #[test]
    fn test_medium_002_lock_recovers_from_poisoned_mutex() {
        let mutex: Mutex<RateBucketMap> = Mutex::new(AHashMap::new());

        // 先毒化 mutex：持锁期间 panic
        let result = std::panic::catch_unwind(|| {
            let _guard = mutex.lock().unwrap();
            panic!("intentional poison for MEDIUM-002 test");
        });
        assert!(
            result.is_err(),
            "precondition: mutex should already be poisoned"
        );

        // lock_rate_buckets 应从中毒状态恢复，不 panic
        let mut guard = lock_rate_buckets(&mutex);
        assert!(guard.is_empty(), "the recovered guard must access the map");
        guard.insert(
            ("default".to_string(), "127.0.0.1".to_string()),
            (1, Instant::now(), Duration::from_secs(60)),
        );
        assert_eq!(guard.len(), 1, "the recovered guard must write normally");
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
        assert!(!constant_time_eq("abc", "and"));
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
    async fn test_create_ban_duplicate_increments_ban_times() {
        // ban-5 修复回归：重复封禁同一目标时 ban_times 正确递增
        // （旧实现依赖 history 快照，MemoryBanStorage 不跟踪历史导致
        // 计数恒为 1；现在由存储层在已存值上原子 +1）
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

        // 第二次创建同一目标 - 计数原子递增
        let resp = app.oneshot(make_create_ban_request(body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let resp_body = resp.into_body().collect().await.unwrap().to_bytes();
        let json2: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json2["data"]["ban_times"].as_u64().unwrap(), 2);
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
    // operator 身份必须由 API key mapping 决定，
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
        let governor = Arc::new(make_governor().await);
        let state = LimiteronState {
            governor,
            ban_manager: Some(ban_manager),
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
            #[cfg(feature = "prometheus")]
            metrics: None,
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
            "operator must come from the API key mapping (admin-alice), not the body (admin-bob)"
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
        let governor = Arc::new(make_governor().await);
        let state = LimiteronState {
            governor,
            ban_manager: Some(ban_manager),
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
            #[cfg(feature = "prometheus")]
            metrics: None,
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
            "without a mapping configured, fall back to the default 'admin-api' instead of the body operator"
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

        let governor = Arc::new(make_governor().await);
        let state = LimiteronState {
            governor,
            ban_manager: Some(ban_manager),
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
            #[cfg(feature = "prometheus")]
            metrics: None,
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
            "delete_ban operator must come from the API key mapping (admin-alice), not the body (admin-bob)"
        );
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_delete_ban_authorization_denied_returns_403() {
        use crate::BanManager;
        use crate::authorization::SimpleAuthorizationProvider;
        use crate::ban::{BanSource, BanTarget};
        use std::sync::Arc;

        // API key → "admin-bob" 映射；授权提供者只允许 "admin-alice"
        let config = AdminApiConfig::new("test-api-key-16chars!!")
            .with_api_key_operator("test-api-key-16chars!!", "admin-bob");

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

        // 预先用被授权的 operator 建封：确保 403 来自授权拒绝，
        // 而不是"目标无封禁"（404）
        let target = BanTarget::Ip("198.51.100.7".to_string());
        ban_manager
            .create_ban(
                target.clone(),
                "unban 授权测试".to_string(),
                BanSource::Manual {
                    operator: "admin-alice".to_string(),
                },
                serde_json::json!({}),
                Some(std::time::Duration::from_secs(3600)),
            )
            .await
            .unwrap();

        let governor = Arc::new(make_governor().await);
        let state = LimiteronState {
            governor,
            ban_manager: Some(ban_manager),
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
            #[cfg(feature = "prometheus")]
            metrics: None,
        };
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/ban/198.51.100.7")
            .method("DELETE")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .header("Content-Type", "application/json")
            .body(Body::from(r#"{"reason":"unban"}"#.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // 403 = 授权拒绝；500 = 授权错误被误映射为内部错误
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "an operator without remove_ban permission must get 403, not 500"
        );
    }

    // ========================================================================
    // Admin API 速率限制
    //
    // 验证策略：配置小限制值，发送超过限制的请求，断言第 N+1 次返回 429。
    // 速率限制在鉴权之前执行，防止暴力破解和 DDoS。
    // ========================================================================

    #[tokio::test]
    async fn test_vuln_0002_rate_limit_returns_429_after_max() {
        let state = make_state().await;
        // 配置 default 分组限制为 3 次/60s
        let config =
            AdminApiConfig::new("test-api-key-16chars!!").with_rate_limit("default", 3, 60);
        let app = create_router(state, &config);

        // 前 3 次成功（200 OK）
        for i in 0..3 {
            let req = Request::builder()
                .uri("/api/v1/status")
                .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
                .body(Body::empty())
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "request {} should succeed within rate limit",
                i
            );
        }

        // 第 4 次被速率限制（429）
        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "the 4th request must be rate limited"
        );
    }

    #[tokio::test]
    async fn test_vuln_0002_rate_limit_includes_retry_after_header() {
        let state = make_state().await;
        // 配置 default 分组限制为 1 次/60s
        let config =
            AdminApiConfig::new("test-api-key-16chars!!").with_rate_limit("default", 1, 60);
        let app = create_router(state, &config);

        // 第 1 次成功
        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // 第 2 次被限制
        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        // 验证 Retry-After header 存在且值 > 0
        let retry_after = resp
            .headers()
            .get(RETRY_AFTER)
            .expect("429 response must contain the Retry-After header");
        let retry_secs: u64 = retry_after.to_str().unwrap().parse().unwrap();
        assert!(
            retry_secs > 0,
            "Retry-After must be positive (remaining window time), got: {}",
            retry_secs
        );
    }

    #[tokio::test]
    async fn test_vuln_0002_rate_limit_groups_independent() {
        let state = make_state().await;
        // ban 和 default 都限制为 2 次/60s
        let config = AdminApiConfig::new("test-api-key-16chars!!")
            .with_rate_limit("ban", 2, 60)
            .with_rate_limit("default", 2, 60);
        let app = create_router(state, &config);

        // 发 2 次 status 请求（default 分组），耗尽 default 配额
        for _ in 0..2 {
            let req = Request::builder()
                .uri("/api/v1/status")
                .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
                .body(Body::empty())
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }

        // default 分组已耗尽，第 3 次 status 返回 429
        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "default group quota exhaustion must return 429"
        );

        // ban 分组未耗尽，ban 请求不应返回 429（独立计数）
        let req = Request::builder()
            .uri("/api/v1/ban")
            .method("POST")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"target":{"type":"ip","value":"1.2.3.4"},"reason":"test"}"#.to_string(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "ban group must count independently from the default group and not return 429"
        );
    }

    // ========================================================================
    // 测试：per-client rate limit buckets
    //
    // 验证策略：
    // 1. 不同 client_ip 的请求有独立计数器（一个耗尽不影响另一个）
    // 2. 相同 client_ip 的请求共享计数器（per-client 而非 per-request）
    // 3. 无 ConnectInfo 时回退到 "unknown" 共享桶（向后兼容测试场景）
    // 4. per-client 隔离在 group 维度仍然成立
    // ========================================================================

    #[tokio::test]
    async fn test_high_001_per_client_isolation_different_ips() {
        let state = make_state().await;
        // 配置 default 分组限制为 2 次/60s
        let config =
            AdminApiConfig::new("test-api-key-16chars!!").with_rate_limit("default", 2, 60);
        let app = create_router(state, &config);

        // client A: 127.0.0.1，发送 2 次请求耗尽配额
        let client_a: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        for i in 0..2 {
            let mut req = Request::builder()
                .uri("/api/v1/status")
                .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
                .body(Body::empty())
                .unwrap();
            req.extensions_mut().insert(ConnectInfo(client_a));
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "client A request {} should succeed within its own bucket",
                i
            );
        }

        // client A 第 3 次被限制（429）
        let mut req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(client_a));
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "client A's 3rd request must be rate limited (own quota exhausted)"
        );

        // client B: 192.168.1.1，发送请求应成功（独立计数）
        let client_b: SocketAddr = "192.168.1.1:5678".parse().unwrap();
        let mut req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(client_b));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "client B must count independently from client A and not be limited by client A's exhaustion"
        );
    }

    #[tokio::test]
    async fn test_high_001_same_ip_shares_bucket_across_ports() {
        let state = make_state().await;
        // 配置 default 分组限制为 2 次/60s
        let config =
            AdminApiConfig::new("test-api-key-16chars!!").with_rate_limit("default", 2, 60);
        let app = create_router(state, &config);

        // 同一 client IP 不同端口（应视为同一客户端，因为按 IP 分桶）
        let client_port1: SocketAddr = "127.0.0.1:1111".parse().unwrap();
        let client_port2: SocketAddr = "127.0.0.1:2222".parse().unwrap();

        // 第 1 次请求（端口 1111）
        let mut req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(client_port1));
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // 第 2 次请求（端口 2222，同 IP 不同端口）
        let mut req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(client_port2));
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // 第 3 次请求（同 IP）应被限制（共享计数）
        let mut req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(client_port1));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "same IP with different ports must share one rate bucket (bucketed by IP, not by socket)"
        );
    }

    #[tokio::test]
    async fn test_high_001_unknown_fallback_when_no_connect_info() {
        let state = make_state().await;
        // 配置 default 分组限制为 1 次/60s
        let config =
            AdminApiConfig::new("test-api-key-16chars!!").with_rate_limit("default", 1, 60);
        let app = create_router(state, &config);

        // 不注入 ConnectInfo（模拟测试或无 connect info 的场景）
        // 第 1 次成功
        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // 第 2 次被限制（共享 "unknown" 桶）
        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "without ConnectInfo, fall back to a shared 'unknown' bucket"
        );
    }

    #[tokio::test]
    async fn test_high_001_per_client_isolation_across_groups() {
        let state = make_state().await;
        // 配置 ban=1, default=1
        let config = AdminApiConfig::new("test-api-key-16chars!!")
            .with_rate_limit("ban", 1, 60)
            .with_rate_limit("default", 1, 60);
        let app = create_router(state, &config);

        let client: SocketAddr = "10.0.0.1:9999".parse().unwrap();

        // 用 ban 请求耗尽 ban 组配额（POST /api/v1/ban）
        // ban-manager feature 未启用时返回 503，但不影响速率限制计数（middleware 在鉴权后、handler 前计数）
        let mut req = Request::builder()
            .uri("/api/v1/ban")
            .method("POST")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"target":{"type":"ip","value":"1.2.3.4"},"reason":"test"}"#.to_string(),
            ))
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(client));
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "the first ban request must not be rate limited"
        );

        // 同一 client 的 default 组请求应仍能成功（不同 group 独立计数）
        let mut req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(client));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "different groups of the same client must count independently"
        );
    }

    // ========================================================================
    // 跨组限流窗口不互清
    //
    // 回归场景：default 组窗口 600s、ban 组窗口 1s。default 组配额耗尽后，
    // ban 组请求（窗口 1s）不得把 default 组的 entry 当过期 entry 清掉——
    // 否则长窗口组的计数被短窗口组提前清零，限流被绕过。
    // ========================================================================

    #[tokio::test]
    async fn test_rate_limit_group_windows_do_not_clear_each_other() {
        let state = make_state().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!")
            .with_rate_limit("default", 2, 600)
            .with_rate_limit("ban", 2, 1);
        let app = create_router(state, &config);

        // default 组耗尽配额（2 次）
        for i in 0..2 {
            let req = Request::builder()
                .uri("/api/v1/status")
                .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
                .body(Body::empty())
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "default request {i} should pass"
            );
        }

        // 等 ban 组窗口（1s）过期：default 组窗口（600s）未过期
        tokio::time::sleep(Duration::from_millis(1100)).await;

        // ban 组请求触发过期清扫（该组窗口为 1s）
        let req = Request::builder()
            .uri("/api/v1/ban")
            .method("POST")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"target":{"type":"ip","value":"1.2.3.4"},"reason":"test"}"#.to_string(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "the ban group must not be limited by its own fresh bucket"
        );

        // default 组配额仍耗尽：长窗口组的计数未被短窗口组清零
        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "the long-window group's counter must survive the short-window group's sweep"
        );
    }

    // ========================================================================
    // /metrics 独立限流（免认证但受限流，超限 429）
    // ========================================================================

    #[tokio::test]
    async fn test_metrics_endpoint_has_its_own_rate_limit() {
        let state = make_state().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!")
            .with_rate_limit("metrics", 2, 60)
            .with_rate_limit("default", 1, 60);
        let app = create_router(state, &config);

        // /metrics 免认证：前 2 次抓取成功
        for i in 0..2 {
            let req = Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "metrics scrape {i} should succeed without credentials"
            );
        }

        // 第 3 次超出 metrics 组配额 → 429
        let req = Request::builder()
            .uri("/metrics")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "the metrics group quota must be enforced independently"
        );

        // metrics 组耗尽不影响 default 组
        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "the metrics group exhaustion must not affect the default group"
        );
    }

    // ========================================================================
    // 主 api_key 强度校验（直接调用 create_router 时同样生效）
    // ========================================================================

    #[tokio::test]
    async fn test_create_router_rejects_short_primary_api_key() {
        let state = make_state().await;
        // 1 字符主 key：AdminServer::start/into_router 会拒绝，直接调用
        // create_router 也不得把它当作有效凭证
        let config = AdminApiConfig::new("k");
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer k")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "a primary api_key shorter than 16 characters must not authenticate"
        );
    }

    #[tokio::test]
    async fn test_create_router_accepts_valid_primary_api_key() {
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
    }

    // ========================================================================
    // 角色映射 key 的强度（与公开辅助函数 is_valid_api_key 同口径）
    // ========================================================================

    #[tokio::test]
    async fn test_create_router_rejects_short_role_key() {
        let state = make_state().await;
        // 1 字符角色 key：映射到 admin 后即获管理面访问权，可被穷举，
        // 与主 key 同口径 fail closed——不得作为有效凭证
        let config =
            AdminApiConfig::new("primary-key-16chars!!!").with_api_key_role("k", AdminRole::Admin);
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer k")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "a role-mapped api_key shorter than 16 characters must not authenticate"
        );
    }

    #[tokio::test]
    async fn test_create_router_rejects_short_role_key_with_viewer_role() {
        let state = make_state().await;
        // 短角色 key 一律拒绝，与所映射的角色无关（viewer 同样不得通过）
        let config =
            AdminApiConfig::new("primary-key-16chars!!!").with_api_key_role("k", AdminRole::Viewer);
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer k")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "a short role-mapped api_key must not authenticate for any role"
        );
    }

    #[tokio::test]
    async fn test_create_router_accepts_role_key_at_minimum_length() {
        let state = make_state().await;
        // 恰好 16 字符的角色 key：达到下限，仍是有效凭证
        let config = AdminApiConfig::new("primary-key-16chars!!!")
            .with_api_key_role("0123456789abcdef", AdminRole::Viewer);
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer 0123456789abcdef")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "a role-mapped api_key of exactly 16 characters must still authenticate"
        );
    }

    // ========================================================================
    // 部分 rate_limits 配置的回退额度（不得弱于文档值）
    // ========================================================================

    #[tokio::test]
    async fn test_partial_rate_limits_fall_back_to_documented_ban_quota() {
        let state = make_state().await;
        // 配置只覆盖 "default" 组：ban / quota 组缺席配置，必须回退到
        // 文档默认额度（ban=100/min, quota=50/min），而非统一的 200/min
        let mut rate_limits = AHashMap::new();
        rate_limits.insert("default".to_string(), (200, 60));
        let config = AdminApiConfig {
            api_key: "test-api-key-16chars!!".to_string(),
            enabled: true,
            rate_limits,
            ..Default::default()
        };
        let app = create_router(state, &config);

        // ban 组：前 100 次放行，第 101 次超限（回退额度为 100/min）
        for i in 0..100 {
            let req = Request::builder()
                .uri("/api/v1/ban")
                .method("POST")
                .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"target":{"type":"ip","value":"1.2.3.4"},"reason":"test"}"#.to_string(),
                ))
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_ne!(
                resp.status(),
                StatusCode::TOO_MANY_REQUESTS,
                "ban request {i} must stay within the documented 100/min fallback"
            );
        }
        let req = Request::builder()
            .uri("/api/v1/ban")
            .method("POST")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"target":{"type":"ip","value":"1.2.3.4"},"reason":"test"}"#.to_string(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "the missing ban group must fall back to 100/min, not the generic 200/min"
        );

        // quota 组：前 50 次放行，第 51 次超限（回退额度为 50/min）
        for i in 0..50 {
            let req = Request::builder()
                .uri("/api/v1/quota/user-1")
                .method("PUT")
                .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"limit":10}"#.to_string()))
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_ne!(
                resp.status(),
                StatusCode::TOO_MANY_REQUESTS,
                "quota request {i} must stay within the documented 50/min fallback"
            );
        }
        let req = Request::builder()
            .uri("/api/v1/quota/user-1")
            .method("PUT")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .header("Content-Type", "application/json")
            .body(Body::from(r#"{"limit":10}"#.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "the missing quota group must fall back to 50/min, not the generic 200/min"
        );
    }

    #[tokio::test]
    async fn test_partial_rate_limits_keep_configured_group_untouched() {
        let state = make_state().await;
        // 配置只覆盖 "default" 组且额度极低：ban 组缺席配置，回退到
        // 文档额度 100/min（不受 default 组的低额度影响）
        let mut rate_limits = AHashMap::new();
        rate_limits.insert("default".to_string(), (1, 60));
        let config = AdminApiConfig {
            api_key: "test-api-key-16chars!!".to_string(),
            enabled: true,
            rate_limits,
            ..Default::default()
        };
        let app = create_router(state, &config);

        for i in 0..100 {
            let req = Request::builder()
                .uri("/api/v1/ban")
                .method("POST")
                .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"target":{"type":"ip","value":"1.2.3.4"},"reason":"test"}"#.to_string(),
                ))
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_ne!(
                resp.status(),
                StatusCode::TOO_MANY_REQUESTS,
                "the missing ban group must keep its own 100/min budget (request {i})"
            );
        }

        // 第 101 次触发 ban 组自身的回退额度（100/min），与 default 组的 1/min 无关
        let req = Request::builder()
            .uri("/api/v1/ban")
            .method("POST")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"target":{"type":"ip","value":"1.2.3.4"},"reason":"test"}"#.to_string(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "the missing ban group must fall back to 100/min regardless of the default group's quota"
        );
    }

    // ========================================================================
    // Admin RBAC（admin/viewer 角色矩阵，越权 403）
    // ========================================================================

    #[tokio::test]
    async fn test_t605_viewer_key_can_read_status() {
        let state = make_state().await;
        let config = AdminApiConfig::new("primary-key-16chars!!!")
            .with_api_key_role("viewer-key-16chars!!", AdminRole::Viewer);
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer viewer-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "viewer read endpoint should pass"
        );
    }

    #[tokio::test]
    async fn test_t605_viewer_key_write_ban_returns_403() {
        let state = make_state().await;
        let config = AdminApiConfig::new("primary-key-16chars!!!")
            .with_api_key_role("viewer-key-16chars!!", AdminRole::Viewer);
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/ban")
            .method("POST")
            .header(AUTHORIZATION, "Bearer viewer-key-16chars!!")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"target":{"type":"ip","value":"1.2.3.4"},"reason":"x"}"#.to_string(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "viewer write endpoint must 403"
        );
    }

    #[tokio::test]
    async fn test_t605_viewer_key_delete_ban_returns_403() {
        let state = make_state().await;
        let config = AdminApiConfig::new("primary-key-16chars!!!")
            .with_api_key_role("viewer-key-16chars!!", AdminRole::Viewer);
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/ban/1.2.3.4")
            .method("DELETE")
            .header(AUTHORIZATION, "Bearer viewer-key-16chars!!")
            .header("Content-Type", "application/json")
            .body(Body::from(r#"{"reason":"x"}"#.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "viewer DELETE must 403"
        );
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_t605_admin_key_write_ban_succeeds_and_viewer_cannot() {
        let state = make_state_with_ban_manager().await;
        let config = AdminApiConfig::new("primary-key-16chars!!!")
            .with_api_key_role("viewer-key-16chars!!", AdminRole::Viewer);
        let app = create_router(state, &config);

        // admin（主 key，未在角色映射中 → 默认 admin）写封禁 → 201
        let req = Request::builder()
            .uri("/api/v1/ban")
            .method("POST")
            .header(AUTHORIZATION, "Bearer primary-key-16chars!!!")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"target":{"type":"ip","value":"198.51.100.9"},"reason":"rbac"}"#.to_string(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "admin writing a ban should succeed"
        );

        // viewer 同端点 → 403
        let req = Request::builder()
            .uri("/api/v1/ban")
            .method("POST")
            .header(AUTHORIZATION, "Bearer viewer-key-16chars!!")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"target":{"type":"ip","value":"198.51.100.10"},"reason":"rbac"}"#.to_string(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_t605_unknown_key_still_401() {
        let state = make_state().await;
        let config = AdminApiConfig::new("primary-key-16chars!!!")
            .with_api_key_role("viewer-key-16chars!!", AdminRole::Viewer);
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/status")
            .header(AUTHORIZATION, "Bearer rogue-key-not-registered")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "unregistered key should 401"
        );
    }

    #[test]
    fn test_t605_role_allows_matrix() {
        use axum::http::Method;
        // GET/HEAD：admin/viewer 均放行
        assert!(role_allows(AdminRole::Admin, &Method::GET));
        assert!(role_allows(AdminRole::Viewer, &Method::GET));
        assert!(role_allows(AdminRole::Viewer, &Method::HEAD));
        // 写方法：仅 admin
        assert!(role_allows(AdminRole::Admin, &Method::POST));
        assert!(!role_allows(AdminRole::Viewer, &Method::POST));
        assert!(!role_allows(AdminRole::Viewer, &Method::PUT));
        assert!(!role_allows(AdminRole::Viewer, &Method::DELETE));
    }

    // ========================================================================
    // Governor 自省 API（JSON）
    // ========================================================================

    #[tokio::test]
    async fn test_t608_introspect_returns_json_snapshot() {
        let state = make_state().await;
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let app = create_router(state, &config);

        let req = Request::builder()
            .uri("/api/v1/introspect")
            .header(AUTHORIZATION, "Bearer test-api-key-16chars!!")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "introspection endpoint should be 200"
        );

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // 规则清单（共享夹具 make_valid_config 含 test_rule）
        let rules = json["rules"].as_array().expect("rules array");
        assert!(
            rules.iter().any(|r| r["id"] == "test_rule"),
            "introspection must include the configured rules: {json}"
        );
        // 决策链清单（含统计字段）
        assert!(json["chains"].is_array(), "chains should be an array");
        assert!(
            json["stats"]["total_requests"].is_u64(),
            "stats should be inlined"
        );
        assert!(
            json["l1_cache"]["enabled"].is_boolean(),
            "l1_cache status should be inlined"
        );
        assert!(
            json["health"]["storage_healthy"].is_boolean(),
            "health status should be inlined"
        );
        // jq 可解析的结构化 JSON（字段齐全）
        assert!(json["config_version"].is_string());
        assert!(json["is_shutdown"].is_boolean());
    }

    #[tokio::test]
    async fn test_t608_introspect_reflects_requests() {
        let state = make_state().await;
        // 触发一次请求让 stats 变化
        let ctx = crate::matchers::RequestContext::new().with_path("/x");
        let _ = state.governor.check(&ctx).await;

        let snapshot = state.governor.introspect().await;
        assert!(
            snapshot.stats.total_requests >= 1,
            "introspection stats must reflect requests that happened"
        );
        assert!(
            snapshot
                .rules
                .iter()
                .any(|r| r.limiters.iter().any(|k| k == "token_bucket")),
            "rule limiter type should output the kind name"
        );
    }
}
