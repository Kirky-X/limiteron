//! Webhook URL 校验
//!
//! 防止 SSRF 攻击，确保 webhook URL 指向安全的公网地址。

/// 校验 Webhook URL 是否安全
///
/// # 安全检查项
/// - 必须为 HTTPS 协议（当 `require_https` 为 true 时）
/// - 禁止 localhost / 127.0.0.1 / ::1
/// - 禁止私有 IP 段（10.x, 172.16.x, 192.168.x）
/// - 禁止 link-local 地址
/// - 禁止唯一本地 IPv6 地址（fc00::/7）
///
/// # 参数
/// - `url`: 待校验的 URL
/// - `require_https`: 是否强制要求 HTTPS
///
/// # 返回
/// - `Ok(())`: URL 安全
/// - `Err(String)`: 不安全的原因
#[cfg(feature = "webhook")]
pub(crate) fn validate_webhook_url(url: &str, require_https: bool) -> Result<(), String> {
    let parsed = url
        .parse::<reqwest::Url>()
        .map_err(|e| format!("无效的 URL: {}", e))?;

    if require_https && parsed.scheme() != "https" {
        return Err("Webhook URL 必须使用 HTTPS 协议".to_string());
    }

    let host = parsed.host_str().ok_or("URL 缺少主机名".to_string())?;
    let lower_host = host.to_lowercase();

    if lower_host == "localhost" || lower_host == "127.0.0.1" || lower_host == "::1" {
        return Err("禁止使用 localhost 或回环地址".to_string());
    }

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if ip.is_loopback() {
            return Err("禁止使用回环 IP 地址".to_string());
        }
        match ip {
            std::net::IpAddr::V4(v4) => {
                if v4.is_private() {
                    return Err("禁止使用私有 IP 地址".to_string());
                }
                if v4.is_link_local() {
                    return Err("禁止使用链路本地地址".to_string());
                }
            }
            std::net::IpAddr::V6(v6) => {
                if v6.is_unique_local() {
                    return Err("禁止使用唯一本地 IPv6 地址".to_string());
                }
            }
        }
    }

    Ok(())
}
