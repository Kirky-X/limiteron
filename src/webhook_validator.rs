// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Webhook URL 校验
//!
//! 防止 SSRF 攻击，确保 webhook URL 指向安全的公网地址。
//!
//! 错误文案经 FTL 目录 `webhook-*` 键构造（T025 MEDIUM-1 接线）：
//! `t(key, args)` 随 locale 渲染，默认/回退为英文规范串。

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
        // T025 MEDIUM-1：错误文案接线 FTL（webhook-invalid-url），zh 渲染可达
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

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if ip.is_loopback() {
            return Err(t("webhook-loopback-forbidden", &[]));
        }
        if ip.is_unspecified() {
            return Err(t("webhook-unspecified-forbidden", &[]));
        }
        match ip {
            std::net::IpAddr::V4(v4) => {
                if v4.is_private() {
                    return Err(t("webhook-private-forbidden", &[]));
                }
                if v4.is_link_local() {
                    return Err(t("webhook-link-local-forbidden", &[]));
                }
            }
            std::net::IpAddr::V6(v6) => {
                // IPv4-mapped IPv6 地址（如 ::ffff:10.0.0.1）必须检查内嵌 IPv4
                // 否则攻击者可用此格式绕过私有 IP 检查
                if let Some(v4) = v6.to_ipv4_mapped() {
                    if v4.is_private() {
                        return Err(t(
                            "webhook-private-mapped-forbidden",
                            &[],
                        )); // webhook-private-mapped-forbidden
                    }
                    if v4.is_link_local() {
                        return Err(t(
                            "webhook-link-local-mapped-forbidden",
                            &[],
                        )); // webhook-link-local-mapped-forbidden
                    }
                    if v4.is_loopback() {
                        return Err(t(
                            "webhook-loopback-mapped-forbidden",
                            &[],
                        )); // webhook-loopback-mapped-forbidden
                    }
                    if v4.is_unspecified() {
                        return Err(t(
                            "webhook-unspecified-mapped-forbidden",
                            &[],
                        )); // webhook-unspecified-mapped-forbidden
                    }
                }
                if v6.is_unique_local() {
                    return Err(t("webhook-unique-local-v6-forbidden", &[]));
                }
                // IPv6 链路本地地址 fe80::/10
                let segs = v6.segments();
                if (segs[0] & 0xffc0) == 0xfe80 {
                    return Err(t("webhook-link-local-v6-forbidden", &[]));
                }
            }
        }
    }

    Ok(())
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
            Err(msg) => assert!(msg.contains("Loopback IP"), "got: {}", msg),
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
            Err(msg) => assert!(msg.contains("Loopback IP"), "got: {}", msg),
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
}
