//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 统一验证模块
//!
//! 提供集中化的验证函数，消除跨模块的重复验证逻辑。

use crate::constants::{
    MAX_API_KEY_LENGTH, MAX_BAN_REASON_LENGTH, MAX_HEADER_VALUE_LENGTH, MAX_IP_ADDRESS_LENGTH,
    MAX_MAC_ADDRESS_LENGTH, MAX_PATH_LENGTH, MAX_USER_ID_LENGTH,
};
use crate::error::FlowGuardError;

/// Validates an IP address (IPv4 or IPv6).
///
/// # Arguments
/// * `ip` - The IP address string to validate
///
/// # Returns
/// * `Ok(())` - Valid IP address
/// * `Err(FlowGuardError)` - Validation failed
pub fn validate_ip_address(ip: &str) -> Result<(), FlowGuardError> {
    if ip.is_empty() {
        return Err(FlowGuardError::ConfigError(
            "IP address cannot be empty".to_string(),
        ));
    }

    if ip.len() > MAX_IP_ADDRESS_LENGTH {
        return Err(FlowGuardError::ConfigError(format!(
            "IP address exceeds maximum length ({})",
            MAX_IP_ADDRESS_LENGTH
        )));
    }

    // First, handle IP with port (e.g., "192.168.1.1:8080")
    let ip_part = if let Some(pos) = ip.rfind(':') {
        if pos > 0 && ip.as_bytes().get(pos - 1) != Some(&b'[') {
            &ip[..pos]
        } else {
            ip
        }
    } else {
        ip
    };

    // IPv4 validation
    if ip_part.contains('.') {
        let parts: Vec<&str> = ip_part.split('.').collect();
        if parts.len() == 4
            && parts
                .iter()
                .all(|p| p.parse::<u8>().map(|_| true).unwrap_or(false))
        {
            return Ok(());
        }
        return Err(FlowGuardError::ConfigError(
            "Invalid IPv4 address format".to_string(),
        ));
    }

    // IPv6 validation (basic check)
    if ip_part.contains(':') {
        let parts: Vec<&str> = ip_part.split(':').collect();
        // IPv6 should have at least 2 parts and each part should be valid hex
        if parts.len() >= 2
            && parts
                .iter()
                .all(|p| p.is_empty() || p.chars().all(|c| c.is_ascii_hexdigit()))
        {
            return Ok(());
        }
        return Err(FlowGuardError::ConfigError(
            "Invalid IPv6 address format".to_string(),
        ));
    }

    Err(FlowGuardError::ConfigError(
        "Invalid IP address format".to_string(),
    ))
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
        return Err(FlowGuardError::ConfigError(
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
        return Err(FlowGuardError::ConfigError(
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
        return Err(FlowGuardError::ConfigError(
            "MAC address cannot be empty".to_string(),
        ));
    }

    if mac.len() > MAX_MAC_ADDRESS_LENGTH {
        return Err(FlowGuardError::ConfigError(format!(
            "MAC address exceeds maximum length ({})",
            MAX_MAC_ADDRESS_LENGTH
        )));
    }

    // Standard MAC format: XX:XX:XX:XX:XX:XX
    let cleaned = mac.replace(':', "");
    if cleaned.len() != 12 {
        return Err(FlowGuardError::ConfigError(
            "MAC address must be 12 hexadecimal characters".to_string(),
        ));
    }

    if !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(FlowGuardError::ConfigError(
            "MAC address must contain only hexadecimal characters".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_ip_address_ipv4() {
        assert!(validate_ip_address("192.168.1.1").is_ok());
        assert!(validate_ip_address("10.0.0.1").is_ok());
        assert!(validate_ip_address("255.255.255.255").is_ok());
        assert!(validate_ip_address("0.0.0.0").is_ok());
        assert!(validate_ip_address("192.168.1.1:8080").is_ok()); // With port
    }

    #[test]
    fn test_validate_ip_address_invalid() {
        assert!(validate_ip_address("").is_err());
        assert!(validate_ip_address("abc").is_err());
        assert!(validate_ip_address("256.1.1.1").is_err()); // Invalid octet
        assert!(validate_ip_address("192.168.1").is_err()); // Missing octet
    }

    #[test]
    fn test_validate_ip_address_ipv6() {
        assert!(validate_ip_address("::1").is_ok());
        assert!(validate_ip_address("2001:0db8:85a3:0000:0000:8a2e:0370:7334").is_ok());
        assert!(validate_ip_address("fe80::1").is_ok());
    }

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
        assert!(validate_user_id("user name").is_err()); // Space not allowed
        assert!(validate_user_id("user@#$%").is_err()); // Special chars not allowed
    }

    #[test]
    fn test_validate_mac_address() {
        assert!(validate_mac_address("00:1A:2B:3C:4D:5E").is_ok());
        assert!(validate_mac_address("001A2B3C4D5E").is_ok()); // Without colons
        assert!(validate_mac_address("aa:bb:cc:dd:ee:ff").is_ok());
    }

    #[test]
    fn test_validate_mac_address_invalid() {
        assert!(validate_mac_address("").is_err());
        assert!(validate_mac_address("00:1A:2B:3C:4D").is_err()); // Too short
        assert!(validate_mac_address("00:1A:2B:3C:4D:5E:6F").is_err()); // Too long
        assert!(validate_mac_address("00:1A:2B:3C:4D:GG").is_err()); // Invalid hex
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
        assert!(validate_ban_reason(&"x".repeat(500)).is_ok()); // Max length
        assert!(validate_ban_reason(&"x".repeat(501)).is_err()); // Over max
    }

    #[test]
    fn test_validate_api_key() {
        assert!(validate_api_key("sk-abc123xyz").is_ok());
        assert!(validate_api_key(&"a".repeat(512)).is_ok()); // Max length
        assert!(validate_api_key(&"a".repeat(513)).is_err()); // Over max
    }
}
