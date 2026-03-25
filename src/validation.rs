//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 统一验证模块
//!
//! 提供集中化的验证函数，消除跨模块的重复验证逻辑。
//!
//! # IP 地址验证
//!
//! 本模块使用 Rust 标准库 `std::net::IpAddr` 进行 IP 地址验证，支持以下格式：
//!
//! ## IPv4 格式
//! - 标准格式：`192.168.1.1`
//! - 带端口：`192.168.1.1:8080`
//!
//! ## IPv6 格式
//! - 完整格式：`2001:0db8:85a3:0000:0000:8a2e:0370:7334`
//! - 压缩格式：`2001:db8:85a3::8a2e:370:7334`
//! - 混合格式：`::ffff:192.168.1.1`（IPv4 映射地址）
//! - 本地地址：`::1`（回环地址）
//! - 全零：`::`（未指定地址）
//! - 带端口：`[2001:db8::1]:8080`（IPv6 地址需用方括号包裹）

use crate::constants::{
    MAX_API_KEY_LENGTH, MAX_BAN_REASON_LENGTH, MAX_HEADER_VALUE_LENGTH, MAX_IP_ADDRESS_LENGTH,
    MAX_MAC_ADDRESS_LENGTH, MAX_PATH_LENGTH, MAX_USER_ID_LENGTH,
};
use crate::error::FlowGuardError;
#[cfg(feature = "ban-manager")]
use crate::storage::BanTarget;

/// Validates an IP address (IPv4 or IPv6).
///
/// 使用 Rust 标准库 `std::net::IpAddr` 进行验证，支持完整的 IPv4 和 IPv6 格式。
///
/// # Arguments
/// * `ip` - The IP address string to validate
///
/// # Returns
/// * `Ok(())` - Valid IP address
/// * `Err(FlowGuardError)` - Validation failed
///
/// # Supported Formats
///
/// ## IPv4
/// - `192.168.1.1` - 标准 IPv4 地址
/// - `192.168.1.1:8080` - 带端口的 IPv4 地址
///
/// ## IPv6
/// - `2001:0db8:85a3:0000:0000:8a2e:0370:7334` - 完整格式
/// - `2001:db8:85a3::8a2e:370:7334` - 压缩格式（连续零块用 :: 表示）
/// - `::ffff:192.168.1.1` - IPv4 映射地址
/// - `::1` - 本地回环地址
/// - `::` - 未指定地址（全零）
/// - `[2001:db8::1]:8080` - 带端口的 IPv6 地址
///
/// # Examples
///
/// ```
/// use limiteron::validation::validate_ip_address;
///
/// // IPv4 验证
/// assert!(validate_ip_address("192.168.1.1").is_ok());
/// assert!(validate_ip_address("192.168.1.1:8080").is_ok());
///
/// // IPv6 验证
/// assert!(validate_ip_address("::1").is_ok());
/// assert!(validate_ip_address("2001:db8::1").is_ok());
/// assert!(validate_ip_address("::ffff:192.168.1.1").is_ok());
/// ```
pub fn validate_ip_address(ip: &str) -> Result<(), FlowGuardError> {
    if ip.is_empty() {
        return Err(FlowGuardError::ValidationError(
            "IP address cannot be empty".to_string(),
        ));
    }

    if ip.len() > MAX_IP_ADDRESS_LENGTH {
        return Err(FlowGuardError::ValidationError(format!(
            "IP address exceeds maximum length (max: {}, actual: {})",
            MAX_IP_ADDRESS_LENGTH,
            ip.len()
        )));
    }

    // 提取 IP 地址部分（处理带端口的情况）
    let ip_part = extract_ip_part(ip)?;

    // 使用标准库解析 IP 地址
    ip_part.parse::<std::net::IpAddr>().map_err(|_| {
        FlowGuardError::ValidationError(format!("Invalid IP address format: {}", ip))
    })?;

    Ok(())
}

/// 从可能包含端口的字符串中提取 IP 地址部分。
///
/// 支持以下格式：
/// - IPv4: `192.168.1.1` 或 `192.168.1.1:8080`
/// - IPv6: `::1` 或 `[::1]:8080`
fn extract_ip_part(ip: &str) -> Result<&str, FlowGuardError> {
    // IPv6 地址带端口格式：[IPv6]:port
    if ip.starts_with('[') {
        if let Some(close_bracket) = ip.find(']') {
            return Ok(&ip[1..close_bracket]);
        }
        return Err(FlowGuardError::ValidationError(
            "Invalid IPv6 address format: missing closing bracket".to_string(),
        ));
    }

    // IPv4 地址带端口格式：IPv4:port
    // 需要区分 IPv4:port 和 IPv6 地址中的冒号
    // IPv6 地址包含多个冒号，而 IPv4:port 只有一个冒号
    if let Some(last_colon) = ip.rfind(':') {
        // 检查是否是 IPv6 地址（包含多个冒号或以冒号开头/结尾）
        let colon_count = ip.chars().filter(|&c| c == ':').count();
        if colon_count == 1 {
            // 只有一个冒号，可能是 IPv4:port
            let potential_ip = &ip[..last_colon];
            // 验证是否为有效的 IPv4 格式
            if potential_ip.contains('.') {
                return Ok(potential_ip);
            }
        }
        // 多个冒号或以冒号开头，是纯 IPv6 地址
    }

    // 无端口或纯 IPv6 地址
    Ok(ip)
}

/// Validates a user ID.
///
/// # Arguments
/// * `user_id` - The user ID string to validate
///
/// # Returns
/// * `Ok(())` - Valid user ID
/// * `Err(FlowGuardError)` - Validation failed
pub fn validate_user_id(user_id: &str) -> Result<(), FlowGuardError> {
    if user_id.is_empty() {
        return Err(FlowGuardError::ValidationError(
            "User ID cannot be empty".to_string(),
        ));
    }

    if user_id.len() > MAX_USER_ID_LENGTH {
        return Err(FlowGuardError::ConfigError(format!(
            "User ID exceeds maximum length ({})",
            MAX_USER_ID_LENGTH
        )));
    }

    // Allow alphanumeric, underscore, hyphen, and @ symbol
    if !user_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '@' || c == '.')
    {
        return Err(FlowGuardError::ValidationError(
            "User ID contains invalid characters".to_string(),
        ));
    }

    Ok(())
}

/// Validates a MAC address.
///
/// # Arguments
/// * `mac` - The MAC address string to validate
///
/// # Returns
/// * `Ok(())` - Valid MAC address
/// * `Err(FlowGuardError)` - Validation failed
pub fn validate_mac_address(mac: &str) -> Result<(), FlowGuardError> {
    if mac.is_empty() {
        return Err(FlowGuardError::ValidationError(
            "MAC address cannot be empty".to_string(),
        ));
    }

    if mac.len() > MAX_MAC_ADDRESS_LENGTH {
        return Err(FlowGuardError::ConfigError(format!(
            "MAC address exceeds maximum length ({})",
            MAX_MAC_ADDRESS_LENGTH
        )));
    }

    // Standard MAC format: XX:XX:XX:XX:XX:XX or with hyphens/periods
    let cleaned = mac.replace([':', '-', '.'], "");
    if cleaned.len() != 12 {
        return Err(FlowGuardError::ValidationError(
            "Invalid MAC address format".to_string(),
        ));
    }

    if !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(FlowGuardError::ValidationError(
            "MAC address contains invalid characters".to_string(),
        ));
    }

    Ok(())
}

/// Generic length validation.
///
/// # Arguments
/// * `value` - The value to validate
/// * `max_length` - Maximum allowed length
/// * `field_name` - Name of the field for error messages
///
/// # Returns
/// * `Ok(())` - Valid length
/// * `Err(FlowGuardError)` - Validation failed
pub fn validate_length(
    value: &str,
    max_length: usize,
    field_name: &str,
) -> Result<(), FlowGuardError> {
    if value.len() > max_length {
        return Err(FlowGuardError::ConfigError(format!(
            "{} exceeds maximum length ({})",
            field_name, max_length
        )));
    }
    Ok(())
}

/// Validates a ban reason.
///
/// # Arguments
/// * `reason` - The ban reason to validate
///
/// # Returns
/// * `Ok(())` - Valid reason
/// * `Err(FlowGuardError)` - Validation failed
pub fn validate_ban_reason(reason: &str) -> Result<(), FlowGuardError> {
    validate_length(reason, MAX_BAN_REASON_LENGTH, "Ban reason")
}

/// Validates an API key.
///
/// # Arguments
/// * `api_key` - The API key to validate
///
/// # Returns
/// * `Ok(())` - Valid API key
/// * `Err(FlowGuardError)` - Validation failed
pub fn validate_api_key(api_key: &str) -> Result<(), FlowGuardError> {
    validate_length(api_key, MAX_API_KEY_LENGTH, "API key")
}

/// Validates a header value.
///
/// # Arguments
/// * `value` - The header value to validate
///
/// # Returns
/// * `Ok(())` - Valid header value
/// * `Err(FlowGuardError)` - Validation failed
pub fn validate_header_value(value: &str) -> Result<(), FlowGuardError> {
    validate_length(value, MAX_HEADER_VALUE_LENGTH, "Header value")
}

/// Validates a path.
///
/// # Arguments
/// * `path` - The path to validate
///
/// # Returns
/// * `Ok(())` - Valid path
/// * `Err(FlowGuardError)` - Validation failed
pub fn validate_path(path: &str) -> Result<(), FlowGuardError> {
    validate_length(path, MAX_PATH_LENGTH, "Path")
}

/// Validates a ban target.
///
/// 根据封禁目标类型调用相应的验证函数。
///
/// # Arguments
/// * `target` - The ban target to validate
///
/// # Returns
/// * `Ok(())` - Valid ban target
/// * `Err(FlowGuardError)` - Validation failed
///
/// # Examples
///
/// ```
/// use limiteron::validation::validate_ban_target;
/// use limiteron::BanTarget;
///
/// // IP 地址验证
/// let ip_target = BanTarget::Ip("192.168.1.1".to_string());
/// assert!(validate_ban_target(&ip_target).is_ok());
///
/// // 用户 ID 验证
/// let user_target = BanTarget::UserId("user123".to_string());
/// assert!(validate_ban_target(&user_target).is_ok());
///
/// // MAC 地址验证
/// let mac_target = BanTarget::Mac("00:1A:2B:3C:4D:5E".to_string());
/// assert!(validate_ban_target(&mac_target).is_ok());
/// ```
#[cfg(feature = "ban-manager")]
pub fn validate_ban_target(target: &BanTarget) -> Result<(), FlowGuardError> {
    match target {
        BanTarget::Ip(ip) => validate_ip_address(ip),
        BanTarget::UserId(user_id) => validate_user_id(user_id),
        BanTarget::Mac(mac) => validate_mac_address(mac),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== IPv4 测试 ====================

    #[test]
    fn test_validate_ip_address_ipv4() {
        // 标准 IPv4 格式
        assert!(validate_ip_address("192.168.1.1").is_ok());
        assert!(validate_ip_address("10.0.0.1").is_ok());
        assert!(validate_ip_address("255.255.255.255").is_ok());
        assert!(validate_ip_address("0.0.0.0").is_ok());
        // 带端口的 IPv4
        assert!(validate_ip_address("192.168.1.1:8080").is_ok());
        assert!(validate_ip_address("127.0.0.1:443").is_ok());
    }

    #[test]
    fn test_validate_ip_address_ipv4_invalid() {
        // 无效 IPv4
        assert!(validate_ip_address("").is_err());
        assert!(validate_ip_address("abc").is_err());
        assert!(validate_ip_address("256.1.1.1").is_err()); // 超出范围的八位组
        assert!(validate_ip_address("192.168.1").is_err()); // 缺少八位组
        assert!(validate_ip_address("192.168.1.1.1").is_err()); // 过多八位组
        assert!(validate_ip_address("192.168.1.256").is_err()); // 超出范围
        assert!(validate_ip_address("192.168.1.-1").is_err()); // 负数
    }

    // ==================== IPv6 测试 ====================

    #[test]
    fn test_ipv6_full_format() {
        // 完整格式 IPv6 地址
        assert!(validate_ip_address("2001:0db8:85a3:0000:0000:8a2e:0370:7334").is_ok());
        assert!(validate_ip_address("2001:0db8:85a3:0000:0000:8a2e:0370:7334").is_ok());
        // 全小写
        assert!(validate_ip_address("2001:0db8:85a3:0000:0000:8a2e:0370:7334").is_ok());
        // 全大写
        assert!(validate_ip_address("2001:0DB8:85A3:0000:0000:8A2E:0370:7334").is_ok());
    }

    #[test]
    fn test_ipv6_compressed_format() {
        // 压缩格式（使用 :: 表示连续的零块）
        assert!(validate_ip_address("2001:db8:85a3::8a2e:370:7334").is_ok());
        assert!(validate_ip_address("::1").is_ok()); // 本地回环地址
        assert!(validate_ip_address("::").is_ok()); // 未指定地址（全零）
        assert!(validate_ip_address("fe80::1").is_ok()); // 链路本地地址
        assert!(validate_ip_address("::ffff:0:0").is_ok()); // IPv4 映射地址基础
                                                            // 开头压缩
        assert!(validate_ip_address("::1:2:3:4:5:6:7").is_ok());
        // 结尾压缩
        assert!(validate_ip_address("1:2:3:4:5:6:7::").is_ok());
        // 中间压缩
        assert!(validate_ip_address("1:2::3:4:5:6").is_ok());
    }

    #[test]
    fn test_ipv6_mixed_format() {
        // IPv4 映射地址（混合格式）
        assert!(validate_ip_address("::ffff:192.168.1.1").is_ok());
        assert!(validate_ip_address("::ffff:10.0.0.1").is_ok());
        assert!(validate_ip_address("::ffff:127.0.0.1").is_ok());
        // IPv4 兼容地址（已弃用但仍有效）
        assert!(validate_ip_address("::192.168.1.1").is_ok());
    }

    #[test]
    fn test_ipv6_with_port() {
        // 带端口的 IPv6 地址（使用方括号）
        assert!(validate_ip_address("[::1]:8080").is_ok());
        assert!(validate_ip_address("[2001:db8::1]:443").is_ok());
        assert!(validate_ip_address("[fe80::1]:80").is_ok());
        assert!(validate_ip_address("[::ffff:192.168.1.1]:8080").is_ok());
    }

    #[test]
    fn test_ipv6_special_addresses() {
        // 本地回环地址
        assert!(validate_ip_address("::1").is_ok());
        // 未指定地址
        assert!(validate_ip_address("::").is_ok());
        // 链路本地地址
        assert!(validate_ip_address("fe80::1").is_ok());
        // 唯一本地地址
        assert!(validate_ip_address("fc00::1").is_ok());
        assert!(validate_ip_address("fd00::1").is_ok());
        // 多播地址
        assert!(validate_ip_address("ff00::1").is_ok());
        // 文档用途地址
        assert!(validate_ip_address("2001:db8::1").is_ok());
    }

    #[test]
    fn test_ipv6_invalid() {
        // 无效的 IPv6 地址
        assert!(validate_ip_address(":::1").is_err()); // 过多的连续冒号
        assert!(validate_ip_address("1:2:3:4:5:6:7:8:9").is_err()); // 过多的块
        assert!(validate_ip_address("1:2:3:4:5:6").is_err()); // 过少的块（无压缩）
        assert!(validate_ip_address("gggg::1").is_err()); // 无效的十六进制字符
        assert!(validate_ip_address("1::2::3").is_err()); // 多个压缩符号
                                                          // 无效的方括号格式
        assert!(validate_ip_address("[::1").is_err()); // 缺少闭合方括号
        assert!(validate_ip_address("::1]:8080").is_err()); // 缺少开放方括号
    }

    // ==================== 其他验证测试 ====================

    #[test]
    fn test_validate_user_id() {
        assert!(validate_user_id("user123").is_ok());
        assert!(validate_user_id("user-name_123").is_ok());
        assert!(validate_user_id("user@example.com").is_ok());
        assert!(validate_user_id("a").is_ok());
    }

    #[test]
    fn test_validate_user_id_invalid() {
        assert!(validate_user_id("").is_err());
        assert!(validate_user_id("user name").is_err()); // 空格不允许
        assert!(validate_user_id("user@#$%").is_err()); // 特殊字符不允许
    }

    #[test]
    fn test_validate_mac_address() {
        assert!(validate_mac_address("00:1A:2B:3C:4D:5E").is_ok());
        assert!(validate_mac_address("001A2B3C4D5E").is_ok()); // 无冒号
        assert!(validate_mac_address("aa:bb:cc:dd:ee:ff").is_ok());
    }

    #[test]
    fn test_validate_mac_address_invalid() {
        assert!(validate_mac_address("").is_err());
        assert!(validate_mac_address("00:1A:2B:3C:4D").is_err()); // 过短
        assert!(validate_mac_address("00:1A:2B:3C:4D:5E:6F").is_err()); // 过长
        assert!(validate_mac_address("00:1A:2B:3C:4D:GG").is_err()); // 无效十六进制
    }

    #[test]
    fn test_validate_length() {
        assert!(validate_length("test", 10, "test").is_ok());
        assert!(validate_length("", 10, "test").is_ok());
        assert!(validate_length("test", 4, "test").is_ok());
    }

    #[test]
    fn test_validate_length_exceeds() {
        assert!(validate_length("test_value", 5, "test").is_err());
    }

    #[test]
    fn test_validate_ban_reason() {
        assert!(validate_ban_reason("Spam behavior").is_ok());
        assert!(validate_ban_reason(&"x".repeat(500)).is_ok()); // 最大长度
        assert!(validate_ban_reason(&"x".repeat(501)).is_err()); // 超过最大长度
    }

    #[test]
    fn test_validate_api_key() {
        assert!(validate_api_key("sk-abc123xyz").is_ok());
        assert!(validate_api_key(&"a".repeat(512)).is_ok()); // 最大长度
        assert!(validate_api_key(&"a".repeat(513)).is_err()); // 超过最大长度
    }

    // ==================== BanTarget 测试 ====================

    #[test]
    #[cfg(feature = "ban-manager")]
    fn test_validate_ban_target_ip() {
        use crate::storage::BanTarget;

        // 有效的 IP 地址
        let ip_target = BanTarget::Ip("192.168.1.1".to_string());
        assert!(validate_ban_target(&ip_target).is_ok());

        let ipv6_target = BanTarget::Ip("::1".to_string());
        assert!(validate_ban_target(&ipv6_target).is_ok());

        // 无效的 IP 地址
        let invalid_ip = BanTarget::Ip("invalid".to_string());
        assert!(validate_ban_target(&invalid_ip).is_err());
    }

    #[test]
    #[cfg(feature = "ban-manager")]
    fn test_validate_ban_target_user_id() {
        use crate::storage::BanTarget;

        // 有效的用户 ID
        let user_target = BanTarget::UserId("user123".to_string());
        assert!(validate_ban_target(&user_target).is_ok());

        // 无效的用户 ID
        let invalid_user = BanTarget::UserId("".to_string());
        assert!(validate_ban_target(&invalid_user).is_err());
    }

    #[test]
    #[cfg(feature = "ban-manager")]
    fn test_validate_ban_target_mac() {
        use crate::storage::BanTarget;

        // 有效的 MAC 地址
        let mac_target = BanTarget::Mac("00:1A:2B:3C:4D:5E".to_string());
        assert!(validate_ban_target(&mac_target).is_ok());

        // 无效的 MAC 地址
        let invalid_mac = BanTarget::Mac("invalid".to_string());
        assert!(validate_ban_target(&invalid_mac).is_err());
    }
}
