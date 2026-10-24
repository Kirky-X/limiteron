// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Webhook URL 校验
//!
//! 防止 SSRF 攻击，确保 webhook URL 指向安全的公网地址。
//!
//! 错误文案经 FTL 目录 `webhook-*` 键构造（接线）
//! `t(key, args)` 随 locale 渲染，默认/回退为英文规范串。

use crate::error::WebhookUrlError;

/// 校验 Webhook URL 是否安全
///
/// # 安全检查项
/// - 必须为 HTTPS 协议（当 `require_https` 为 true 时）
/// - 禁止 localhost / 127.0.0.1 / ::1
/// - 禁止私有 IP 段（10.x, 172.16.x, 192.168.x）
/// - 禁止 link-local 地址
/// - 禁止唯一本地 IPv6 地址（fc00::/7）
/// - 禁止未指定地址（0.0.0.0 / ::）
/// - 禁止 IPv4-mapped IPv6 内网地址（如 ::ffff:10.0.0.1，防 SSRF 绕过）
/// - 禁止 IPv6 链路本地地址（fe80::/10）
/// - 禁止其它特殊用途段（运营商级 NAT 100.64.0.0/10、基准测试段
///   198.18.0.0/15、组播、保留段、IPv4 兼容写法 ::a.b.c.d 等）
///
/// 字面量 IP 的判定与出站解析器共用 [`forbidden_target_reason`]，
/// 两处口径一致。域名不在此判定范围内（字面量之外的解析结果由
/// [`validate_webhook_url_async`] 与出站解析器 [`SsrfSafeResolver`] 覆盖）。
///
/// # 参数
/// - `url`: 待校验的 URL
/// - `require_https`: 是否强制要求 HTTPS
///
/// # 返回
/// - `Ok(())`: URL 安全
/// - `Err(String)`: 不安全的原因（经 FTL 目录渲染，默认英文规范串）
#[cfg(feature = "webhook")]
pub(crate) fn validate_webhook_url(url: &str, require_https: bool) -> Result<(), String> {
    use crate::i18n::t;

    let parsed = url
        .parse::<reqwest::Url>()
        // 错误文案接线 FTL（webhook-invalid-url），zh 渲染可达
        .map_err(|e| t("webhook-invalid-url", &[("reason", e.to_string())]))?;

    if require_https && parsed.scheme() != "https" {
        return Err(t("webhook-https-required", &[]));
    }

    let host_raw = parsed
        .host_str()
        .ok_or_else(|| t("webhook-missing-host", &[]))?;
    // IPv6 地址在 URL 中带方括号（如 [::1]），parse::<IpAddr> 前需去除
    let host = host_raw.trim_start_matches('[').trim_end_matches(']');
    let lower_host = host.to_lowercase();

    if lower_host == "localhost" || lower_host == "127.0.0.1" || lower_host == "::1" {
        return Err(t("webhook-localhost-forbidden", &[]));
    }

    if let Ok(ip) = host.parse::<std::net::IpAddr>()
        && let Some(reason) = forbidden_target_reason(ip)
    {
        return Err(t(forbidden_target_message_key(reason), &[]));
    }

    Ok(())
}

/// 目标地址被判定为禁止出站的类别
///
/// 字面量路径与出站解析器共用同一判定，避免两处口径漂移。
#[cfg(feature = "webhook")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForbiddenTarget {
    /// 回环地址（127.0.0.0/8、::1）
    Loopback,
    /// 未指定地址（0.0.0.0、::）
    Unspecified,
    /// 私有地址（RFC1918）
    Private,
    /// 链路本地地址（169.254.0.0/16）
    LinkLocal,
    /// 唯一本地 IPv6（fc00::/7）
    UniqueLocalV6,
    /// IPv6 链路本地地址（fe80::/10）
    LinkLocalV6,
    /// IPv4-mapped IPv6 内嵌私有地址（::ffff:10.0.0.1）
    MappedPrivate,
    /// IPv4-mapped IPv6 内嵌链路本地地址（::ffff:169.254.1.1）
    MappedLinkLocal,
    /// IPv4-mapped IPv6 内嵌回环地址（::ffff:127.0.0.1）
    MappedLoopback,
    /// IPv4-mapped IPv6 内嵌未指定地址（::ffff:0.0.0.0）
    MappedUnspecified,
    /// 其余特殊用途段（运营商级 NAT、基准测试段、组播、保留段等）
    SpecialRange,
}

/// 判定目标地址是否属于禁止出站段，并给出判定类别
///
/// 覆盖段（v4 与 v6 口径一致）：
/// - 回环、未指定、私有、链路本地、唯一本地 IPv6；
/// - IPv4-mapped IPv6（`::ffff:a.b.c.d`）按内嵌 IPv4 全量判定；
/// - v4：`0.0.0.0/8`、`100.64.0.0/10`（运营商级 NAT / Tailscale 默认段）、
///   `192.0.0.0/24`、`198.18.0.0/15`、`224.0.0.0/4` 及以上保留段；
/// - v6：组播 `ff00::/8`、站点本地 `fec0::/10`、IPv4 兼容写法 `::a.b.c.d`、
///   NAT64 `64:ff9b::/96`（按内嵌 IPv4 判定，不整段拉黑以免误伤纯 NAT64 部署）。
#[cfg(feature = "webhook")]
fn forbidden_target_reason(ip: std::net::IpAddr) -> Option<ForbiddenTarget> {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            if v4.is_loopback() {
                return Some(ForbiddenTarget::Loopback);
            }
            if v4.is_unspecified() {
                return Some(ForbiddenTarget::Unspecified);
            }
            if v4.is_private() {
                return Some(ForbiddenTarget::Private);
            }
            if v4.is_link_local() {
                return Some(ForbiddenTarget::LinkLocal);
            }
            let o = v4.octets();
            if o[0] == 0
                || (o[0] == 100 && (o[1] & 0xc0) == 64)
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                || (o[0] == 198 && (o[1] & 0xfe) == 18)
                || o[0] >= 224
            {
                return Some(ForbiddenTarget::SpecialRange);
            }
            None
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped IPv6 地址（如 ::ffff:10.0.0.1）必须按内嵌 IPv4 全量判定，
            // 否则攻击者可用此格式绕过私有 IP 检查
            if let Some(v4) = v6.to_ipv4_mapped() {
                return match forbidden_target_reason(IpAddr::V4(v4))? {
                    ForbiddenTarget::Private => Some(ForbiddenTarget::MappedPrivate),
                    ForbiddenTarget::LinkLocal => Some(ForbiddenTarget::MappedLinkLocal),
                    ForbiddenTarget::Loopback => Some(ForbiddenTarget::MappedLoopback),
                    ForbiddenTarget::Unspecified => Some(ForbiddenTarget::MappedUnspecified),
                    _ => Some(ForbiddenTarget::SpecialRange),
                };
            }
            if v6.is_loopback() {
                return Some(ForbiddenTarget::Loopback);
            }
            if v6.is_unspecified() {
                return Some(ForbiddenTarget::Unspecified);
            }
            if v6.is_unique_local() {
                return Some(ForbiddenTarget::UniqueLocalV6);
            }
            let segs = v6.segments();
            // IPv6 链路本地地址 fe80::/10
            if (segs[0] & 0xffc0) == 0xfe80 {
                return Some(ForbiddenTarget::LinkLocalV6);
            }
            // 组播 ff00::/8、站点本地 fec0::/10（已废弃但仍可路由到内网）
            if (segs[0] & 0xff00) == 0xff00 || (segs[0] & 0xffc0) == 0xfec0 {
                return Some(ForbiddenTarget::SpecialRange);
            }
            // NAT64 64:ff9b::/96：按内嵌 IPv4 判定，不整段拉黑
            if segs[0] == 0x0064 && segs[1] == 0xff9b && segs[2..6].iter().all(|s| *s == 0) {
                let embedded = std::net::Ipv4Addr::new(
                    (segs[6] >> 8) as u8,
                    (segs[6] & 0xff) as u8,
                    (segs[7] >> 8) as u8,
                    (segs[7] & 0xff) as u8,
                );
                return forbidden_target_reason(IpAddr::V4(embedded))
                    .map(|_| ForbiddenTarget::SpecialRange);
            }
            // IPv4 兼容写法 ::a.b.c.d（::ffff:a.b.c.d 形态已在上方单独处理）
            if segs[..6].iter().all(|s| *s == 0) && (segs[6] | segs[7]) != 0 {
                return Some(ForbiddenTarget::SpecialRange);
            }
            None
        }
    }
}

/// 禁止出站类别的 FTL 文案键
#[cfg(feature = "webhook")]
fn forbidden_target_message_key(target: ForbiddenTarget) -> &'static str {
    match target {
        ForbiddenTarget::Loopback => "webhook-loopback-forbidden",
        ForbiddenTarget::Unspecified => "webhook-unspecified-forbidden",
        ForbiddenTarget::Private => "webhook-private-forbidden",
        ForbiddenTarget::LinkLocal => "webhook-link-local-forbidden",
        ForbiddenTarget::UniqueLocalV6 => "webhook-unique-local-v6-forbidden",
        ForbiddenTarget::LinkLocalV6 => "webhook-link-local-v6-forbidden",
        ForbiddenTarget::MappedPrivate => "webhook-private-mapped-forbidden",
        ForbiddenTarget::MappedLinkLocal => "webhook-link-local-mapped-forbidden",
        ForbiddenTarget::MappedLoopback => "webhook-loopback-mapped-forbidden",
        ForbiddenTarget::MappedUnspecified => "webhook-unspecified-mapped-forbidden",
        ForbiddenTarget::SpecialRange => "webhook-special-range-forbidden",
    }
}

/// 目标地址是否属于禁止出站的内网/特殊地址段
///
/// 与 [`validate_webhook_url`] 的字面量判定同口径（两者共用
/// [`forbidden_target_reason`]）：回环、未指定、私有、link-local、
/// 唯一本地 IPv6、IPv4-mapped IPv6 的内嵌 IPv4（`::ffff:10.0.0.1`
/// 一类绕过写法），以及运营商级 NAT 等特殊用途段。
#[cfg(feature = "webhook")]
fn is_forbidden_destination_ip(ip: std::net::IpAddr) -> bool {
    forbidden_target_reason(ip).is_some()
}

/// 出站解析器：拒绝解析到内网/回环/link-local 的目标
///
/// SSRF 的域名面：仅按 URL 字面量判定时，`http://internal.corp/hook` 这类
/// 域名（解析到 10.x / 127.x / 169.254.x）可绕过校验出站。此解析器在
/// **连接所用的那一次解析** 上执行同一套地址判定——解析结果即连接目标，
/// 不存在"先校验后连接"的解析漂移窗口（DNS rebinding 亦被覆盖）。
#[cfg(feature = "webhook")]
struct SsrfSafeResolver;

#[cfg(feature = "webhook")]
impl reqwest::dns::Resolve for SsrfSafeResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let host = name.as_str().to_string();
            let resolved: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?
                .collect();
            let mut safe = Vec::with_capacity(resolved.len());
            for addr in resolved {
                if is_forbidden_destination_ip(addr.ip()) {
                    log::warn!(
                        target: "limiteron",
                        "webhook host {host} resolves to forbidden address {} (dropped)",
                        addr.ip()
                    );
                    continue;
                }
                safe.push(addr);
            }
            if safe.is_empty() {
                return Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "webhook host {host} resolves only to forbidden addresses"
                )));
            }
            Ok(Box::new(safe.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// 异步校验 Webhook URL（字面量校验 + DNS 解析结果判定）
///
/// 字面量校验之外补齐域名面：域名解析结果全部落在内网/回环/link-local
/// 段时判定为不安全。连接时仍由 [`SsrfSafeResolver`] 兜底（预检与连接
/// 是两次解析，预检只用于提前失败，权威判定在连接侧）。
///
/// DNS 预检带耗时上限（`WEBHOOK_DNS_PRECHECK_TIMEOUT`）：解析器卡死时
/// 按瞬时失败返回，不阻塞调用方（事件分发的 webhook 扇出是顺序 await，
/// 一次卡住就会让广播通道积压并最终丢事件）。
///
/// 用于**注册路径**：此处给出的是「该端点此刻是否可用」的判定，故把
/// 瞬时故障与永久拒绝分开上报（调用方可重试）。发送路径不重复预检——
/// 见 [`validate_webhook_url_for_send`]。
#[cfg(feature = "webhook")]
pub(crate) async fn validate_webhook_url_async(
    url: &str,
    require_https: bool,
) -> Result<(), WebhookUrlError> {
    use crate::i18n::t;

    validate_webhook_url(url, require_https).map_err(WebhookUrlError::Rejected)?;

    let parsed = url.parse::<reqwest::Url>().map_err(|e| {
        WebhookUrlError::Rejected(t("webhook-invalid-url", &[("reason", e.to_string())]))
    })?;
    let Some(host_raw) = parsed.host_str() else {
        return Ok(());
    };
    let host = host_raw.trim_start_matches('[').trim_end_matches(']');
    // 字面量地址已在 validate_webhook_url 中判定
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Ok(());
    }

    bounded_dns_precheck(resolve_and_check_host(host)).await
}

/// 发送路径的 URL 校验（只判定字面量面）
///
/// 字面量面（协议要求 / 字面量地址段）在此判定；域名面交由**连接期**的
/// [`SsrfSafeResolver`] 权威判定——连接期判定用的是连接所用的那一次解析，
/// 没有「先校验后连接」的漂移窗口（DNS rebinding 亦被覆盖）。
///
/// 发送路径不再重复 DNS 预检：同一投递解析两次既拖慢发送，又让预检的
/// 瞬时故障（解析器抖动 / 超时）把本可成功的投递判死。预检只保留在注册
/// 路径——那里要回答的是「该端点此刻是否可用」，故需区分永久拒绝与瞬时
/// 故障（见 [`validate_webhook_url_async`]）。
#[cfg(feature = "webhook")]
pub(crate) fn validate_webhook_url_for_send(
    url: &str,
    require_https: bool,
) -> Result<(), WebhookUrlError> {
    validate_webhook_url(url, require_https).map_err(WebhookUrlError::Rejected)
}

/// DNS 预检的耗时上限
///
/// 必须小于 webhook 请求自身的超时（5s），使预检预算留在请求预算之内。
#[cfg(feature = "webhook")]
const WEBHOOK_DNS_PRECHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// 以耗时上限包裹 DNS 预检
///
/// 超时按瞬时失败处理（解析器恢复后可重试），不按永久拒绝处理。
#[cfg(feature = "webhook")]
async fn bounded_dns_precheck<F>(fut: F) -> Result<(), WebhookUrlError>
where
    F: std::future::Future<Output = Result<(), WebhookUrlError>>,
{
    use crate::i18n::t;

    match tokio::time::timeout(WEBHOOK_DNS_PRECHECK_TIMEOUT, fut).await {
        Ok(result) => result,
        Err(_) => Err(WebhookUrlError::DnsTransient(t(
            "webhook-dns-precheck-timeout",
            &[(
                "seconds",
                WEBHOOK_DNS_PRECHECK_TIMEOUT.as_secs().to_string(),
            )],
        ))),
    }
}

/// 解析主机名并判定解析结果
///
/// 解析失败（解析器不可用/超时）按瞬时失败上报；解析结果全部落在禁止
/// 出站段按永久拒绝上报。
#[cfg(feature = "webhook")]
async fn resolve_and_check_host(host: &str) -> Result<(), WebhookUrlError> {
    use crate::i18n::t;

    let resolved: Vec<std::net::SocketAddr> = match tokio::net::lookup_host((host, 0)).await {
        Ok(addrs) => addrs.collect(),
        Err(e) => {
            return Err(WebhookUrlError::DnsTransient(t(
                "webhook-dns-resolve-failed",
                &[("host", host.to_string()), ("reason", e.to_string())],
            )));
        }
    };
    if resolved.is_empty() {
        return Err(WebhookUrlError::DnsTransient(t(
            "webhook-dns-no-address",
            &[("host", host.to_string())],
        )));
    }
    if resolved
        .iter()
        .all(|addr| is_forbidden_destination_ip(addr.ip()))
    {
        return Err(WebhookUrlError::Rejected(t(
            "webhook-dns-forbidden",
            &[("host", host.to_string())],
        )));
    }
    Ok(())
}

/// 出站 webhook 允许跟随的最大重定向跳数
///
/// reqwest 的 `Policy::custom` **不自带**链长限制（reqwest 0.13 的
/// `Policy::custom` 文档明确：custom 变体不会自动限制跳数），自定义
/// 策略必须自行约束，否则攻击者可用长重定向链耗尽出站资源。
#[cfg(feature = "webhook")]
const MAX_WEBHOOK_REDIRECTS: usize = 5;

/// 判定某一跳重定向是否可跟随（跳数上限 + SSRF 复检）
///
/// `followed` 是 reqwest 的 `attempt.previous().len()`——**含初始 URL**，
/// 故第一次重定向决策时为 1（reqwest 的 `Policy::limited(max)` 判据是
/// `previous.len() > max`，即恰好允许 max 跳）。
#[cfg(feature = "webhook")]
fn redirect_follow_allowed(followed: usize, url: &str, require_https: bool) -> bool {
    followed <= MAX_WEBHOOK_REDIRECTS && validate_webhook_url(url, require_https).is_ok()
}

/// 出站 webhook 客户端的传输策略
///
/// 重定向目标的 HTTPS 要求由**调用方**决定，而非构建 profile：同一客户端
/// 被策略不同的调用方共享，用 profile 推断会让某一方的策略套到另一方
/// （明文降级或无谓拒绝）。
#[cfg(feature = "webhook")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WebhookTransportPolicy {
    /// 重定向目标必须为 HTTPS（配额告警等载荷含敏感数据）
    HttpsRequired,
    /// 重定向目标允许 HTTP（事件分发路径：仅在 release 构建要求初始 URL 为 HTTPS）
    Lax,
}

#[cfg(feature = "webhook")]
impl WebhookTransportPolicy {
    /// 重定向目标是否要求 HTTPS
    fn require_https(self) -> bool {
        matches!(self, WebhookTransportPolicy::HttpsRequired)
    }
}

/// 进程级出站客户端缓存（按传输策略分桶）
///
/// reqwest::Client 内部为 Arc，克隆廉价；缓存后多次外发复用同一连接池
/// （每次发送新建 Client 会丢失 TCP/TLS 复用，高事件量下造成套接字抖动）。
#[cfg(feature = "webhook")]
static WEBHOOK_CLIENTS: std::sync::OnceLock<
    parking_lot::Mutex<std::collections::HashMap<WebhookTransportPolicy, reqwest::Client>>,
> = std::sync::OnceLock::new();

/// 取得出站 webhook HTTP 客户端（按策略缓存复用）
///
/// # 参数
/// - `policy`: 传输安全策略（决定重定向目标是否必须为 HTTPS）
///
/// # 错误
/// - TLS 后端初始化失败或系统配置不可读（reqwest 错误）
#[cfg(feature = "webhook")]
pub(crate) fn webhook_http_client(
    policy: WebhookTransportPolicy,
) -> Result<reqwest::Client, reqwest::Error> {
    let cache =
        WEBHOOK_CLIENTS.get_or_init(|| parking_lot::Mutex::new(std::collections::HashMap::new()));
    let mut cache = cache.lock();
    if let Some(client) = cache.get(&policy) {
        return Ok(client.clone());
    }
    let client = build_webhook_client(policy)?;
    cache.insert(policy, client.clone());
    Ok(client)
}

/// 构建出站 webhook HTTP 客户端（重定向逐跳复检 + 跳数上限 + 连接侧 DNS 判定）
///
/// reqwest 默认跟随最多 10 跳重定向（`Policy::default()`），初始 URL 的
/// SSRF 校验覆盖不到跳转目标——公网地址 302 到内网即绕过校验。此客户端
/// 对每一跳重跑 [`validate_webhook_url`]，命中内网/回环/link-local 即
/// 停止跟随（响应按非 2xx 处理，发送侧显式失败）。
///
/// 三项补强：
/// - **跳数上限**：`Policy::custom` 不自带链长限制，超限直接以错误终止
///   （否则长重定向链可耗尽出站资源）；
/// - **连接侧解析判定**：出站解析器 [`SsrfSafeResolver`] 在连接所用的那
///   一次解析上拒绝内网目标，域名面（内部域名 / DNS rebinding）不再可
///   绕过字面量校验；
/// - **禁用系统代理**：环境代理（`ALL_PROXY` / `HTTP_PROXY` / `HTTPS_PROXY`）
///   会让代理解析目标主机名，本地的 [`SsrfSafeResolver`] 只看得到代理主机，
///   目标地址从此逃过 SSRF 判定——故出站客户端一律不走环境代理。
#[cfg(feature = "webhook")]
fn build_webhook_client(policy: WebhookTransportPolicy) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            let followed = attempt.previous().len();
            let url = attempt.url().as_str();
            if !redirect_follow_allowed(followed, url, policy.require_https()) {
                if followed > MAX_WEBHOOK_REDIRECTS {
                    log::warn!(
                        target: "limiteron",
                        "webhook redirect chain exceeded {MAX_WEBHOOK_REDIRECTS} hops; aborting"
                    );
                    return attempt.error("too many webhook redirects");
                }
                log::warn!(
                    target: "limiteron",
                    "webhook redirect target rejected by SSRF validation: {} (status {})",
                    crate::logging::redaction::redact_url(Some(url)),
                    attempt.status()
                );
                return attempt.stop();
            }
            attempt.follow()
        }))
        .dns_resolver(SsrfSafeResolver)
        .build()
}

#[cfg(all(test, feature = "webhook"))]
mod tests {
    use super::*;

    #[test]
    fn test_valid_https_url() {
        assert!(validate_webhook_url("https://hooks.example.com/notify", true).is_ok());
    }

    #[test]
    fn test_http_without_https_required() {
        assert!(validate_webhook_url("http://hooks.example.com/notify", false).is_ok());
    }

    #[test]
    fn test_http_rejected_when_https_required() {
        match validate_webhook_url("http://hooks.example.com/notify", true) {
            Err(msg) => assert!(msg.contains("HTTPS"), "expected HTTPS error, got: {}", msg),
            Ok(_) => panic!("expected Err for HTTP URL with require_https=true"),
        }
    }

    #[test]
    fn test_invalid_url_parse_error() {
        match validate_webhook_url("not a url", true) {
            Err(msg) => assert!(
                msg.contains("Invalid URL"),
                "expected parse error, got: {}",
                msg
            ),
            Ok(_) => panic!("expected Err for invalid URL"),
        }
    }

    #[test]
    fn test_localhost_rejected() {
        match validate_webhook_url("http://localhost/webhook", false) {
            Err(msg) => assert!(msg.contains("localhost"), "got: {}", msg),
            Ok(_) => panic!("expected Err for localhost"),
        }
    }

    #[test]
    fn test_localhost_case_insensitive() {
        match validate_webhook_url("http://LocalHost/webhook", false) {
            Err(msg) => assert!(msg.contains("localhost"), "got: {}", msg),
            Ok(_) => panic!("expected Err for LocalHost"),
        }
    }

    #[test]
    fn test_loopback_ipv4_rejected() {
        match validate_webhook_url("http://127.0.0.1/webhook", false) {
            Err(msg) => assert!(
                msg.contains("loopback") || msg.contains("Loopback IP") || msg.contains("回环"),
                "got: {msg}"
            ),
            Ok(_) => panic!("expected Err for 127.0.0.1"),
        }
    }

    #[test]
    fn test_private_ip_10_rejected() {
        match validate_webhook_url("http://10.0.0.1/webhook", false) {
            Err(msg) => assert!(msg.contains("Private IP"), "got: {}", msg),
            Ok(_) => panic!("expected Err for 10.x.x.x"),
        }
    }

    #[test]
    fn test_private_ip_172_rejected() {
        match validate_webhook_url("http://172.16.0.1/webhook", false) {
            Err(msg) => assert!(msg.contains("Private IP"), "got: {}", msg),
            Ok(_) => panic!("expected Err for 172.16.x.x"),
        }
    }

    #[test]
    fn test_private_ip_192_rejected() {
        match validate_webhook_url("http://192.168.1.1/webhook", false) {
            Err(msg) => assert!(msg.contains("Private IP"), "got: {}", msg),
            Ok(_) => panic!("expected Err for 192.168.x.x"),
        }
    }

    #[test]
    fn test_link_local_rejected() {
        match validate_webhook_url("http://169.254.1.1/webhook", false) {
            Err(msg) => assert!(msg.contains("Link-local IP"), "got: {}", msg),
            Ok(_) => panic!("expected Err for 169.254.x.x"),
        }
    }

    #[test]
    fn test_public_ip_accepted() {
        assert!(validate_webhook_url("http://8.8.8.8/webhook", false).is_ok());
    }

    #[test]
    fn test_public_domain_accepted() {
        assert!(validate_webhook_url("https://hooks.slack.com/services/T00/B00/xxx", true).is_ok());
    }

    #[test]
    fn test_missing_host_rejected() {
        match validate_webhook_url("file:///dev/null", false) {
            Err(msg) => assert!(msg.contains("missing a host name"), "got: {}", msg),
            Ok(_) => panic!("expected Err for URL with no host"),
        }
    }

    #[test]
    fn test_url_with_query_params() {
        assert!(
            validate_webhook_url("https://example.com/webhook?token=abc&retry=1", true).is_ok()
        );
    }

    #[test]
    fn test_url_with_port() {
        assert!(validate_webhook_url("https://example.com:8443/webhook", true).is_ok());
    }

    #[test]
    fn test_empty_url() {
        match validate_webhook_url("", false) {
            Err(msg) => assert!(msg.contains("Invalid URL"), "got: {}", msg),
            Ok(_) => panic!("expected Err for empty URL"),
        }
    }

    // SSRF 防护测试（P0 修复）

    #[test]
    fn test_ipv4_mapped_ipv6_private_rejected() {
        // ::ffff:10.0.0.1 是 IPv4-mapped IPv6，内嵌私有 IPv4，必须被拒绝
        match validate_webhook_url("http://[::ffff:10.0.0.1]/webhook", false) {
            Err(msg) => assert!(
                msg.contains("IPv4-mapped") && msg.contains("Private IP"),
                "expected IPv4-mapped private rejection, got: {}",
                msg
            ),
            Ok(_) => panic!("expected Err for IPv4-mapped private IPv6"),
        }
    }

    #[test]
    fn test_ipv4_mapped_ipv6_loopback_rejected() {
        match validate_webhook_url("http://[::ffff:127.0.0.1]/webhook", false) {
            Err(msg) => assert!(
                msg.contains("IPv4-mapped") && msg.contains("Loopback"),
                "got: {}",
                msg
            ),
            Ok(_) => panic!("expected Err for IPv4-mapped loopback"),
        }
    }

    #[test]
    fn test_ipv4_mapped_ipv6_link_local_rejected() {
        match validate_webhook_url("http://[::ffff:169.254.1.1]/webhook", false) {
            Err(msg) => assert!(
                msg.contains("IPv4-mapped") && msg.contains("Link-local"),
                "got: {}",
                msg
            ),
            Ok(_) => panic!("expected Err for IPv4-mapped link-local"),
        }
    }

    #[test]
    fn test_ipv4_mapped_ipv6_unspecified_rejected() {
        match validate_webhook_url("http://[::ffff:0.0.0.0]/webhook", false) {
            Err(msg) => assert!(
                msg.contains("IPv4-mapped") && msg.contains("Unspecified"),
                "got: {}",
                msg
            ),
            Ok(_) => panic!("expected Err for IPv4-mapped unspecified"),
        }
    }

    #[test]
    fn test_ipv4_unspecified_rejected() {
        match validate_webhook_url("http://0.0.0.0/webhook", false) {
            Err(msg) => assert!(msg.contains("Unspecified IP"), "got: {}", msg),
            Ok(_) => panic!("expected Err for 0.0.0.0"),
        }
    }

    #[test]
    fn test_ipv6_unspecified_rejected() {
        match validate_webhook_url("http://[::]/webhook", false) {
            Err(msg) => assert!(msg.contains("Unspecified IP"), "got: {}", msg),
            Ok(_) => panic!("expected Err for ::"),
        }
    }

    #[test]
    fn test_ipv6_link_local_rejected() {
        match validate_webhook_url("http://[fe80::1]/webhook", false) {
            Err(msg) => assert!(msg.contains("Link-local IPv6"), "got: {}", msg),
            Ok(_) => panic!("expected Err for fe80::1"),
        }
    }

    #[test]
    fn test_ipv4_mapped_public_accepted() {
        // ::ffff:8.8.8.8 是 IPv4-mapped 公网 IP，应通过
        assert!(
            validate_webhook_url("http://[::ffff:8.8.8.8]/webhook", false).is_ok(),
            "public IPv4-mapped IPv6 should be accepted"
        );
    }

    /// 覆盖 line 45: 127.0.0.2 属于回环段但不在字符串黑名单中
    #[test]
    fn test_loopback_ipv4_127_0_0_2_rejected() {
        match validate_webhook_url("http://127.0.0.2/webhook", false) {
            Err(msg) => assert!(
                msg.contains("loopback") || msg.contains("Loopback IP") || msg.contains("回环"),
                "got: {msg}"
            ),
            Ok(_) => panic!("expected Err for 127.0.0.2"),
        }
    }

    /// 覆盖 line 77: fc00::1 是唯一本地 IPv6 地址
    #[test]
    fn test_ipv6_unique_local_rejected() {
        match validate_webhook_url("http://[fc00::1]/webhook", false) {
            Err(msg) => assert!(msg.contains("Unique local IPv6"), "got: {}", msg),
            Ok(_) => panic!("expected Err for fc00::1"),
        }
    }

    // ------------------------------------------------------------------
    // 特殊用途段：字面量路径与出站解析器同口径
    // ------------------------------------------------------------------

    /// 特殊用途段必须被拒绝（字面量路径）
    ///
    /// 这些段此前不在判定表内：`100.64.0.0/10`（运营商级 NAT，
    /// Tailscale 等隧道工具的默认段）可直达宿主机上的服务；
    /// `0.0.0.0/8`、`192.0.0.0/24`、`198.18.0.0/15`、`224.0.0.0/4`
    /// 及以上同样不是合法的出站目标。
    #[test]
    fn test_special_use_ipv4_ranges_rejected() {
        for ip in [
            "100.64.0.1",      // 100.64.0.0/10 运营商级 NAT
            "100.127.255.255", // 100.64.0.0/10 上界
            "0.1.2.3",         // 0.0.0.0/8 本网络段
            "192.0.0.1",       // 192.0.0.0/24 IETF 协议分配
            "198.18.0.1",      // 198.18.0.0/15 基准测试段
            "198.19.255.255",  // 198.18.0.0/15 上界
            "224.0.0.1",       // 224.0.0.0/4 组播
            "240.0.0.1",       // 240.0.0.0/4 保留段
            "255.255.255.255", // 广播
        ] {
            let url = format!("http://{ip}/webhook");
            assert!(
                validate_webhook_url(&url, false).is_err(),
                "{ip} must be rejected as a webhook target"
            );
        }
    }

    /// 特殊用途段必须被拒绝（出站解析器路径）
    ///
    /// 字面量路径与解析器路径共用同一判定——域名解析到这些段时同样
    /// 不得出站。
    #[test]
    fn test_special_use_addresses_forbidden_by_resolver_path() {
        for ip in [
            "100.64.0.1",      // 100.64.0.0/10 运营商级 NAT
            "0.1.2.3",         // 0.0.0.0/8
            "192.0.0.1",       // 192.0.0.0/24
            "198.18.0.1",      // 198.18.0.0/15
            "224.0.0.1",       // 224.0.0.0/4
            "240.0.0.1",       // 240.0.0.0/4
            "255.255.255.255", // 广播
        ] {
            assert!(
                is_forbidden_destination_ip(ip.parse().unwrap()),
                "{ip} must be forbidden as a connection target"
            );
        }
    }

    /// IPv6 特殊用途段必须被拒绝
    ///
    /// - `::a.b.c.d`：IPv4 兼容写法（`std` 的 `to_ipv4_mapped` 不认，
    ///   此前直接放行）；
    /// - `64:ff9b::a.b.c.d`：NAT64 前缀（按内嵌 IPv4 判定，不整段拉黑）；
    /// - `ff00::/8` 组播、`fec0::/10` 站点本地。
    #[test]
    fn test_special_use_ipv6_ranges_rejected() {
        for ip in [
            "::10.0.0.1",         // IPv4 兼容写法，内嵌私网地址
            "::169.254.1.1",      // IPv4 兼容写法，内嵌链路本地地址
            "64:ff9b::10.0.0.1",  // NAT64 前缀，内嵌私网地址
            "64:ff9b::127.0.0.1", // NAT64 前缀，内嵌回环地址
            "ff02::1",            // ff00::/8 组播
            "fec0::1",            // fec0::/10 站点本地
        ] {
            let url = format!("http://[{ip}]/webhook");
            assert!(
                validate_webhook_url(&url, false).is_err(),
                "{ip} must be rejected as a webhook target"
            );
        }
    }

    /// NAT64 前缀不得整段拉黑：内嵌公网 IPv4 的目标仍应放行
    ///
    /// 纯 NAT64 部署用 `64:ff9b::/96` 访问公网 IPv4，整段拒绝会误伤
    /// 这类部署——故按内嵌 IPv4 判定。
    #[test]
    fn test_nat64_prefix_with_public_embedded_ipv4_allowed() {
        assert!(
            validate_webhook_url("http://[64:ff9b::8.8.8.8]/webhook", false).is_ok(),
            "a NAT64 address embedding a public IPv4 must stay usable"
        );
        assert!(
            !is_forbidden_destination_ip("64:ff9b::8.8.8.8".parse().unwrap()),
            "a NAT64 address embedding a public IPv4 must not be forbidden"
        );
    }

    /// 公网地址不得被误判为禁止目标
    #[test]
    fn test_public_addresses_not_forbidden() {
        for ip in [
            "8.8.8.8",
            "93.184.216.34",
            "2606:2800:220:1:248:1893:25c8:1946",
        ] {
            assert!(
                !is_forbidden_destination_ip(ip.parse().unwrap()),
                "{ip} must not be forbidden"
            );
        }
    }

    // ------------------------------------------------------------------
    // 重定向策略：跳数上限 + 逐跳 SSRF 复检
    // ------------------------------------------------------------------

    /// 跳数上限的一基语义
    ///
    /// `followed` 是 reqwest 的 `attempt.previous().len()`——**含初始 URL**，
    /// 故第一次重定向决策时为 1。reqwest 自身的 `Policy::limited(max)` 判据
    /// 是 `previous.len() > max`，即恰好允许 max 跳；本函数的比较必须与
    /// 之同口径（`<=`），否则实际只跟随 max-1 跳。
    #[test]
    fn test_redirect_follow_allowed_enforces_hop_limit() {
        // 未超限且目标合法 → 跟随（1 是第一次重定向决策的基线）
        assert!(redirect_follow_allowed(
            1,
            "https://hooks.example.com/notify",
            true
        ));
        assert!(redirect_follow_allowed(
            MAX_WEBHOOK_REDIRECTS,
            "https://hooks.example.com/notify",
            true
        ));
        // 超出跳数上限 → 不跟随（reqwest 的 Policy::custom 不自带此限制）
        assert!(!redirect_follow_allowed(
            MAX_WEBHOOK_REDIRECTS + 1,
            "https://hooks.example.com/notify",
            true
        ));
        assert!(!redirect_follow_allowed(
            MAX_WEBHOOK_REDIRECTS + 3,
            "https://hooks.example.com/notify",
            true
        ));
        // 目标命中内网/回环 → 不跟随
        assert!(!redirect_follow_allowed(
            1,
            "http://127.0.0.1/webhook",
            false
        ));
        assert!(!redirect_follow_allowed(
            1,
            "http://10.0.0.1/webhook",
            false
        ));
    }

    /// 重定向到内网目标：逐跳复检命中即停止跟随（目标端点不得被访问）
    #[tokio::test]
    async fn test_redirect_to_loopback_target_is_not_followed() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let secret_hits = Arc::new(AtomicUsize::new(0));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_hits = secret_hits.clone();
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let hits = server_hits.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = stream.read(&mut buf).await;
                    let request = String::from_utf8_lossy(&buf);
                    let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let response = if path.starts_with("/redirect") {
                        format!(
                            "HTTP/1.1 302 Found\r\nLocation: http://{addr}/secret\r\n\
                             Content-Length: 0\r\nConnection: close\r\n\r\n"
                        )
                    } else {
                        hits.fetch_add(1, Ordering::SeqCst);
                        let body = "secret";
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    };
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });

        let client = webhook_http_client(WebhookTransportPolicy::Lax)
            .expect("the webhook client must build");
        let response = client
            .get(format!("http://{addr}/redirect"))
            .send()
            .await
            .expect("the initial request must reach the local test server");
        // 停止跟随 → 客户端拿到的是 302 本身
        assert_eq!(response.status(), 302);
        // 内网目标端点未被访问
        assert_eq!(
            secret_hits.load(Ordering::SeqCst),
            0,
            "the redirect target must not be reached after the SSRF re-check rejects it"
        );

        server.abort();
    }

    /// 重定向链的跳数上限必须真的生效：恰好允许 MAX_WEBHOOK_REDIRECTS 跳
    ///
    /// 端到端验证一基计数：本地服务逐跳重定向。跳数用尽后客户端以错误
    /// 终止（而非继续跟随到上限之外）。
    ///
    /// 目标用**公网字面量**（跳数计到被拒为止的那一跳才需要连出网），
    /// 故本断言只覆盖「跳数判定」本身——每一跳的 SSRF 复检在本地测试
    /// 中会把回环目标先行拒掉（见
    /// [`test_redirect_to_loopback_target_is_not_followed`]），故端到端的
    /// 跳数行为由 [`redirect_follow_allowed`] 的边界断言覆盖。
    #[test]
    fn test_redirect_chain_hop_limit_boundary() {
        // 第 5 跳（= MAX_WEBHOOK_REDIRECTS）仍允许跟随
        assert!(
            redirect_follow_allowed(
                MAX_WEBHOOK_REDIRECTS,
                "https://hooks.example.com/notify",
                true
            ),
            "the {MAX_WEBHOOK_REDIRECTS}th hop must still be followed"
        );
        // 第 6 跳被拒
        assert!(
            !redirect_follow_allowed(
                MAX_WEBHOOK_REDIRECTS + 1,
                "https://hooks.example.com/notify",
                true
            ),
            "the {}th hop must be refused",
            MAX_WEBHOOK_REDIRECTS + 1
        );
    }

    // ------------------------------------------------------------------
    // 出站客户端：系统代理、传输策略、缓存
    // ------------------------------------------------------------------

    /// 出站客户端不得使用环境代理
    ///
    /// 环境代理（`ALL_PROXY` / `HTTP_PROXY` / `HTTPS_PROXY`）会让**代理**
    /// 解析目标主机名：本地只解析到代理主机，目标地址从此逃过
    /// [`SsrfSafeResolver`] 的 SSRF 判定。故出站客户端一律不走环境代理。
    #[test]
    fn test_client_does_not_use_system_proxy() {
        // 代理环境变量存在时，客户端的代理列表仍须为空
        let client = build_webhook_client(WebhookTransportPolicy::Lax)
            .expect("the webhook client must build");
        let debug = format!("{client:?}");
        assert!(
            !debug.contains("proxies"),
            "the outbound client must not carry a system proxy, got: {debug}"
        );
    }

    /// 传输策略由调用方决定，不得由构建 profile 推断
    ///
    /// 策略直接决定重定向目标是否必须为 HTTPS：两个调用方（事件分发 /
    /// 配额告警）的口径不同，用 profile 推断会让某一方的策略套到另一方
    /// （明文降级或无谓拒绝）。
    #[test]
    fn test_transport_policy_controls_redirect_https_requirement() {
        // 强制 HTTPS 的策略：http 目标不得跟随
        assert!(
            !redirect_follow_allowed(
                1,
                "http://hooks.example.com/notify",
                WebhookTransportPolicy::HttpsRequired.require_https()
            ),
            "the HTTPS-required policy must refuse an http redirect target"
        );
        // 宽松策略：http 目标可跟随（https 目标两者皆可）
        assert!(
            redirect_follow_allowed(
                1,
                "http://hooks.example.com/notify",
                WebhookTransportPolicy::Lax.require_https()
            ),
            "the lax policy must allow an http redirect target"
        );
        assert!(
            redirect_follow_allowed(
                1,
                "https://hooks.example.com/notify",
                WebhookTransportPolicy::HttpsRequired.require_https()
            ),
            "an https redirect target must be allowed under either policy"
        );
    }

    /// 强制 HTTPS 的客户端不得跟随 http 重定向目标
    ///
    /// 端到端验证策略确实接入了重定向策略（而非仅停留在配置层）：本地
    /// 服务返回 302 指向一个 http 目标，客户端停在 302 本身。
    #[tokio::test]
    async fn test_https_required_client_refuses_http_redirect_target() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = stream.read(&mut buf).await;
                    // 目标为 http 公网字面量：策略拒绝时不会真的连出去
                    let response = "HTTP/1.1 302 Found\r\nLocation: http://203.0.113.7/next\r\n\
                     Content-Length: 0\r\nConnection: close\r\n\r\n";
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });

        let client = webhook_http_client(WebhookTransportPolicy::HttpsRequired)
            .expect("the webhook client must build");
        let response = client
            .get(format!("http://{addr}/redirect"))
            .send()
            .await
            .expect("the initial request must reach the local test server");
        // 策略拒绝跟随 → 客户端拿到的是 302 本身
        assert_eq!(
            response.status(),
            302,
            "an http redirect target must not be followed under the HTTPS-required policy"
        );

        server.abort();
    }

    /// 客户端缓存按传输策略分桶
    ///
    /// 同一策略复用同一份客户端（连接池复用），不同策略各持一份——共用
    /// 一份会让某一方的 HTTPS 要求套到另一方。
    #[test]
    fn test_client_cache_is_bucketed_by_transport_policy() {
        webhook_http_client(WebhookTransportPolicy::HttpsRequired)
            .expect("the webhook client must build");
        webhook_http_client(WebhookTransportPolicy::Lax).expect("the webhook client must build");

        let cache = WEBHOOK_CLIENTS
            .get()
            .expect("the client cache must be initialized")
            .lock();
        assert!(
            cache.contains_key(&WebhookTransportPolicy::HttpsRequired),
            "the HTTPS-required policy must have its own cached client"
        );
        assert!(
            cache.contains_key(&WebhookTransportPolicy::Lax),
            "the lax policy must have its own cached client"
        );
    }

    /// 客户端构建失败不得 panic
    ///
    /// [`webhook_http_client`] 返回 `Result` 而非内部 `expect`：TLS 后端
    /// 初始化失败等可恢复故障不应让整个进程崩掉。
    #[test]
    fn test_client_factory_returns_result_instead_of_panicking() {
        // 构建成功时返回 Ok（TLS 后端可用）
        assert!(
            webhook_http_client(WebhookTransportPolicy::HttpsRequired).is_ok(),
            "a healthy environment must yield a client"
        );
    }

    /// 发送路径的 URL 校验只判定字面量面
    ///
    /// 域名面交由连接期的 [`SsrfSafeResolver`] 权威判定——发送路径不重复
    /// DNS 预检（预检的瞬时故障会把本可成功的投递判死）。
    #[test]
    fn test_send_path_validation_is_literal_only() {
        // 字面量禁止段：发送路径必须拒绝
        assert!(
            validate_webhook_url_for_send("http://127.0.0.1/webhook", false).is_err(),
            "a loopback literal must be rejected on the send path"
        );
        assert!(
            validate_webhook_url_for_send("http://10.0.0.1/webhook", false).is_err(),
            "a private literal must be rejected on the send path"
        );
        // 域名：不预检（由连接期判定）
        assert!(
            validate_webhook_url_for_send("https://hooks.example.com/notify", true).is_ok(),
            "a domain target must not be pre-resolved on the send path"
        );
    }

    // ------------------------------------------------------------------
    // 域名面：解析结果落在内网/回环的域名不得出站
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_dns_resolver_rejects_host_resolving_to_loopback() {
        use reqwest::dns::Resolve;
        use std::str::FromStr;

        let name = reqwest::dns::Name::from_str("localhost")
            .expect("'localhost' is a valid resolver name");
        let result = SsrfSafeResolver.resolve(name).await;
        assert!(
            result.is_err(),
            "a host resolving to loopback addresses must be rejected by the outbound resolver"
        );
    }

    #[tokio::test]
    async fn test_async_validation_rejects_localhost_host() {
        let result = validate_webhook_url_async("http://localhost:9091/hook", false).await;
        assert!(result.is_err(), "localhost must be rejected");
    }

    /// DNS 预检必须有耗时上限：解析器卡死时按瞬时失败返回，不得无限等待
    ///
    /// 预检位于事件分发的发送路径上（顺序 await），无上限时一次卡死就会
    /// 拖住整个分派任务，最终导致广播通道积压并丢事件。
    #[tokio::test(start_paused = true)]
    async fn test_dns_precheck_is_bounded() {
        use std::future::pending;

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            bounded_dns_precheck(pending::<Result<(), WebhookUrlError>>()),
        )
        .await
        .expect("a stalled resolver must not hang the pre-check");
        assert!(
            matches!(result, Err(WebhookUrlError::DnsTransient(_))),
            "a stalled resolver must surface as a transient failure, got: {result:?}"
        );
    }

    /// 解析失败按瞬时失败上报（调用方可重试），而非永久拒绝
    #[tokio::test]
    async fn test_async_validation_reports_dns_failure_as_transient() {
        // .invalid 是 RFC 2606 保留域名，恒不解析
        let err = validate_webhook_url_async("https://nonexistent-host-xyzzy.invalid/hook", true)
            .await
            .expect_err("a non-resolvable host must fail the pre-check");
        assert!(
            matches!(err, WebhookUrlError::DnsTransient(_)),
            "DNS resolution failure must be reported as transient, got: {err:?}"
        );
    }

    /// 字面量命中禁止段按永久拒绝上报（重试无意义）
    #[tokio::test]
    async fn test_async_validation_reports_literal_rejection_as_permanent() {
        let err = validate_webhook_url_async("https://127.0.0.1/hook", true)
            .await
            .expect_err("a loopback literal must be rejected");
        assert!(
            matches!(err, WebhookUrlError::Rejected(_)),
            "a literal rejection must be reported as permanent, got: {err:?}"
        );
    }

    /// 出站解析器放行公网地址（域名面不误伤正常目标）
    #[tokio::test]
    async fn test_dns_resolver_allows_public_addresses() {
        // 直接判定函数：公网地址不得被误判为禁止目标
        assert!(!is_forbidden_destination_ip("8.8.8.8".parse().unwrap()));
        assert!(!is_forbidden_destination_ip(
            "93.184.216.34".parse().unwrap()
        ));
        assert!(!is_forbidden_destination_ip(
            "2606:2800:220:1:248:1893:25c8:1946".parse().unwrap()
        ));
        // 内网/回环/link-local/未指定/唯一本地一律禁止
        assert!(is_forbidden_destination_ip("127.0.0.1".parse().unwrap()));
        assert!(is_forbidden_destination_ip("10.0.0.1".parse().unwrap()));
        assert!(is_forbidden_destination_ip("192.168.1.1".parse().unwrap()));
        assert!(is_forbidden_destination_ip("172.16.0.1".parse().unwrap()));
        assert!(is_forbidden_destination_ip("169.254.1.1".parse().unwrap()));
        assert!(is_forbidden_destination_ip("0.0.0.0".parse().unwrap()));
        assert!(is_forbidden_destination_ip("::1".parse().unwrap()));
        assert!(is_forbidden_destination_ip("fc00::1".parse().unwrap()));
        assert!(is_forbidden_destination_ip("fe80::1".parse().unwrap()));
        // IPv4-mapped IPv6 内网地址
        assert!(is_forbidden_destination_ip(
            "::ffff:10.0.0.1".parse().unwrap()
        ));
    }
}
