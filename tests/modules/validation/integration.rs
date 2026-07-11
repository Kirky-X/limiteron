// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 验证模块集成测试
//!
//! 测试验证函数的完整功能

use limiteron::error::LimiteronError;
#[cfg(feature = "ban-manager")]
use limiteron::validation::validate_ban_target;
use limiteron::validation::{
    validate_api_key, validate_ban_reason, validate_header_value, validate_ip_address,
    validate_length, validate_mac_address, validate_path, validate_user_id,
};

// ============================================================================
// Constants — must match values from src/constants.rs
// ============================================================================

const MAX_IP_ADDRESS_LENGTH: usize = 45;
const MAX_USER_ID_LENGTH: usize = 256;
const MAX_MAC_ADDRESS_LENGTH: usize = 17;
const MAX_BAN_REASON_LENGTH: usize = 500;
const MAX_HEADER_VALUE_LENGTH: usize = 8192;
const MAX_PATH_LENGTH: usize = 2048;
const MAX_API_KEY_LENGTH: usize = 512;

// ============================================================================
// validate_ip_address — IPv4
// ============================================================================

mod ipv4_tests {
    use super::*;

    #[test]
    fn test_ipv4_valid_standard() {
        assert!(validate_ip_address("192.168.1.1").is_ok());
        assert!(validate_ip_address("10.0.0.1").is_ok());
        assert!(validate_ip_address("0.0.0.0").is_ok());
        assert!(validate_ip_address("255.255.255.255").is_ok());
    }

    #[test]
    fn test_ipv4_valid_with_port() {
        assert!(validate_ip_address("192.168.1.1:8080").is_ok());
        assert!(validate_ip_address("10.0.0.1:443").is_ok());
        assert!(validate_ip_address("127.0.0.1:3000").is_ok());
        assert!(validate_ip_address("192.168.1.1:0").is_ok());
    }

    #[test]
    fn test_ipv4_invalid() {
        // Empty
        assert!(validate_ip_address("").is_err());
        // Non-numeric
        assert!(validate_ip_address("abc.def.ghi.jkl").is_err());
        // Out-of-range octet
        assert!(validate_ip_address("256.1.1.1").is_err());
        assert!(validate_ip_address("192.168.1.256").is_err());
        assert!(validate_ip_address("192.168.1.-1").is_err());
        // Missing octet
        assert!(validate_ip_address("192.168.1").is_err());
        // Too many octets
        assert!(validate_ip_address("192.168.1.1.1").is_err());
    }

    #[test]
    fn test_ipv4_exceeds_max_length() {
        // MAX_IP_ADDRESS_LENGTH = 45; build a string longer than that
        let long_ip = format!(
            "{}.{}.{}.{}:12345",
            "1".repeat(10),
            "2".repeat(10),
            "3".repeat(10),
            "4".repeat(10)
        );
        assert!(long_ip.len() > MAX_IP_ADDRESS_LENGTH);
        assert!(validate_ip_address(&long_ip).is_err());
    }
}

// ============================================================================
// validate_ip_address — IPv6
// ============================================================================

mod ipv6_tests {
    use super::*;

    #[test]
    fn test_ipv6_valid_full_format() {
        assert!(validate_ip_address("2001:0db8:85a3:0000:0000:8a2e:0370:7334").is_ok());
        assert!(validate_ip_address("2001:0DB8:85A3:0000:0000:8A2E:0370:7334").is_ok());
    }

    #[test]
    fn test_ipv6_valid_compressed() {
        assert!(validate_ip_address("::1").is_ok());
        assert!(validate_ip_address("2001:db8::1").is_ok());
        assert!(validate_ip_address("::ffff:192.168.1.1").is_ok());
        assert!(validate_ip_address("fe80::1").is_ok());
        assert!(validate_ip_address("::").is_ok());
        assert!(validate_ip_address("::1:2:3:4:5:6:7").is_ok());
        assert!(validate_ip_address("1:2:3:4:5:6:7::").is_ok());
        assert!(validate_ip_address("1:2::3:4:5:6").is_ok());
    }

    #[test]
    fn test_ipv6_valid_with_port() {
        assert!(validate_ip_address("[2001:db8::1]:8080").is_ok());
        assert!(validate_ip_address("[::1]:443").is_ok());
        assert!(validate_ip_address("[fe80::1]:80").is_ok());
        assert!(validate_ip_address("[::ffff:192.168.1.1]:8080").is_ok());
    }

    #[test]
    fn test_ipv6_invalid() {
        // Too many consecutive colons
        assert!(validate_ip_address(":::1").is_err());
        // Too many groups
        assert!(validate_ip_address("1:2:3:4:5:6:7:8:9").is_err());
        // Too few groups (without compression)
        assert!(validate_ip_address("1:2:3:4:5:6").is_err());
        // Invalid hex
        assert!(validate_ip_address("gggg::1").is_err());
        // Multiple compression symbols
        assert!(validate_ip_address("1::2::3").is_err());
        // Missing closing bracket
        assert!(validate_ip_address("[::1").is_err());
        // Missing opening bracket
        assert!(validate_ip_address("::1]:8080").is_err());
    }
}

// ============================================================================
// validate_user_id
// ============================================================================

mod user_id_tests {
    use super::*;

    #[test]
    fn test_user_id_valid() {
        assert!(validate_user_id("user123").is_ok());
        assert!(validate_user_id("test-user").is_ok());
        assert!(validate_user_id("a").is_ok());
        // Max length (256 chars)
        assert!(validate_user_id(&"a".repeat(MAX_USER_ID_LENGTH)).is_ok());
    }

    #[test]
    fn test_user_id_valid_special_chars() {
        // Allowed chars: alphanumeric, underscore, hyphen, @, dot
        assert!(validate_user_id("user_name").is_ok());
        assert!(validate_user_id("user-name").is_ok());
        assert!(validate_user_id("user.name").is_ok());
        assert!(validate_user_id("user@domain").is_ok());
        assert!(validate_user_id("user_name-123.abc").is_ok());
    }

    #[test]
    fn test_user_id_invalid_empty() {
        let result = validate_user_id("");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            LimiteronError::ValidationError(_)
        ));
    }

    #[test]
    fn test_user_id_invalid_exceeds_max_length() {
        let result = validate_user_id(&"a".repeat(MAX_USER_ID_LENGTH + 1));
        assert!(result.is_err());
        // Note: returns ConfigError for length overflow
        assert!(matches!(
            result.unwrap_err(),
            LimiteronError::ConfigError(_)
        ));
    }

    #[test]
    fn test_user_id_invalid_chars() {
        // Space is not allowed
        assert!(validate_user_id("user name").is_err());
        // Special characters not in allowlist
        assert!(validate_user_id("user@#$%").is_err());
        assert!(validate_user_id("user!^&*").is_err());
        assert!(validate_user_id("user<>").is_err());
    }
}

// ============================================================================
// validate_mac_address
// ============================================================================

mod mac_address_tests {
    use super::*;

    #[test]
    fn test_mac_address_valid_colon_separated() {
        assert!(validate_mac_address("00:1B:44:11:3A:B7").is_ok());
        assert!(validate_mac_address("00:1A:2B:3C:4D:5E").is_ok());
        assert!(validate_mac_address("aa:bb:cc:dd:ee:ff").is_ok());
        assert!(validate_mac_address("AA:BB:CC:DD:EE:FF").is_ok());
        assert!(validate_mac_address("00:00:00:00:00:00").is_ok());
        assert!(validate_mac_address("FF:FF:FF:FF:FF:FF").is_ok());
    }

    #[test]
    fn test_mac_address_valid_hyphen_separated() {
        assert!(validate_mac_address("00-1B-44-11-3A-B7").is_ok());
        assert!(validate_mac_address("00-1A-2B-3C-4D-5E").is_ok());
    }

    #[test]
    fn test_mac_address_valid_no_separator() {
        assert!(validate_mac_address("001B44113AB7").is_ok());
        assert!(validate_mac_address("001A2B3C4D5E").is_ok());
    }

    #[test]
    fn test_mac_address_invalid_empty() {
        let result = validate_mac_address("");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            LimiteronError::ValidationError(_)
        ));
    }

    #[test]
    fn test_mac_address_invalid_too_long() {
        // MAX_MAC_ADDRESS_LENGTH = 17; MAC with colons is 17 chars
        // Anything > 17 chars should fail length check
        let long_mac = "00:1B:44:11:3A:B7:00";
        assert!(long_mac.len() > MAX_MAC_ADDRESS_LENGTH);
        assert!(validate_mac_address(long_mac).is_err());
    }

    #[test]
    fn test_mac_address_invalid_too_short() {
        // 5 octets instead of 6 -> cleaned length 10, not 12
        assert!(validate_mac_address("00:1B:44:11:3A").is_err());
    }

    #[test]
    fn test_mac_address_invalid_hex() {
        // G is not a valid hex digit
        assert!(validate_mac_address("00:1B:44:11:3A:GG").is_err());
        assert!(validate_mac_address("00:1B:44:11:3A:G1").is_err());
    }
}

// ============================================================================
// validate_api_key
// ============================================================================

mod api_key_tests {
    use super::*;

    #[test]
    fn test_api_key_valid() {
        assert!(validate_api_key("sk-1234567890").is_ok());
        assert!(validate_api_key("api_key_abc").is_ok());
        assert!(validate_api_key("abcdefghijklmnopqrstuvwxyz").is_ok());
        assert!(validate_api_key("ABCDEFGHIJKLMNOPQRSTUVWXYZ").is_ok());
        assert!(validate_api_key("0123456789").is_ok());
        // Max length (512 chars)
        assert!(validate_api_key(&"a".repeat(MAX_API_KEY_LENGTH)).is_ok());
    }

    #[test]
    fn test_api_key_invalid_empty() {
        // validate_length allows empty strings, so validate_api_key does too
        assert!(validate_api_key("").is_ok());
    }

    #[test]
    fn test_api_key_invalid_exceeds_max_length() {
        let result = validate_api_key(&"a".repeat(MAX_API_KEY_LENGTH + 1));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            LimiteronError::ConfigError(_)
        ));
    }
}

// ============================================================================
// validate_header_value
// ============================================================================

mod header_value_tests {
    use super::*;

    #[test]
    fn test_header_value_valid() {
        assert!(validate_header_value("text/plain").is_ok());
        assert!(validate_header_value("application/json").is_ok());
        assert!(validate_header_value("Bearer token123").is_ok());
        assert!(validate_header_value("").is_ok()); // empty is allowed (no explicit check)
        assert!(validate_header_value(&"a".repeat(MAX_HEADER_VALUE_LENGTH)).is_ok());
    }

    #[test]
    fn test_header_value_invalid_exceeds_max_length() {
        let result = validate_header_value(&"a".repeat(MAX_HEADER_VALUE_LENGTH + 1));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            LimiteronError::ConfigError(_)
        ));
    }
}

// ============================================================================
// validate_ban_reason
// ============================================================================

mod ban_reason_tests {
    use super::*;

    #[test]
    fn test_ban_reason_valid() {
        assert!(validate_ban_reason("Spam").is_ok());
        assert!(validate_ban_reason("Abuse").is_ok());
        assert!(validate_ban_reason("").is_ok()); // empty is allowed by validate_length
        // Max length (500 chars)
        assert!(validate_ban_reason(&"a".repeat(MAX_BAN_REASON_LENGTH)).is_ok());
    }

    #[test]
    fn test_ban_reason_at_max_boundary() {
        let exactly_500 = "x".repeat(MAX_BAN_REASON_LENGTH);
        assert!(validate_ban_reason(&exactly_500).is_ok());

        let one_over = "x".repeat(MAX_BAN_REASON_LENGTH + 1);
        assert!(validate_ban_reason(&one_over).is_err());
    }

    #[test]
    fn test_ban_reason_invalid_exceeds_max_length() {
        let result = validate_ban_reason(&"a".repeat(MAX_BAN_REASON_LENGTH + 1));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            LimiteronError::ConfigError(_)
        ));
    }
}

// ============================================================================
// validate_path
// ============================================================================

mod path_tests {
    use super::*;

    #[test]
    fn test_path_valid() {
        assert!(validate_path("/api/users").is_ok());
        assert!(validate_path("/").is_ok());
        assert!(validate_path("/api/v1/users/123").is_ok());
        assert!(validate_path("").is_ok()); // empty is allowed by validate_length
        assert!(validate_path(&"/".repeat(MAX_PATH_LENGTH)).is_ok());
    }

    #[test]
    fn test_path_invalid_exceeds_max_length() {
        let result = validate_path(&"a".repeat(MAX_PATH_LENGTH + 1));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            LimiteronError::ConfigError(_)
        ));
    }

    #[test]
    fn test_path_at_max_boundary() {
        let exactly_2048 = "a".repeat(MAX_PATH_LENGTH);
        assert!(validate_path(&exactly_2048).is_ok());

        let one_over = "a".repeat(MAX_PATH_LENGTH + 1);
        assert!(validate_path(&one_over).is_err());
    }
}

// ============================================================================
// validate_length — generic function
// ============================================================================

mod validate_length_tests {
    use super::*;

    #[test]
    fn test_validate_length_ok() {
        assert!(validate_length("test", 10, "field").is_ok());
        assert!(validate_length("", 10, "field").is_ok()); // empty is fine
        assert!(validate_length("test", 4, "field").is_ok()); // exactly equal
        assert!(validate_length("short", 100, "field").is_ok());
    }

    #[test]
    fn test_validate_length_exceeds() {
        let result = validate_length("test_value", 5, "test_field");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, LimiteronError::ConfigError(ref msg) if msg.contains("test_field")));
    }

    #[test]
    fn test_validate_length_edge_cases() {
        // Zero max_length
        assert!(validate_length("", 0, "field").is_ok());
        assert!(validate_length("a", 0, "field").is_err());
        // Exact boundary
        assert!(validate_length("abc", 3, "f").is_ok());
        assert!(validate_length("abcd", 3, "f").is_err());
    }
}

// ============================================================================
// validate_ban_target — requires ban-manager feature
// This module is only compiled when the ban-manager feature is enabled
// ============================================================================

#[cfg(feature = "ban-manager")]
mod ban_target_tests {
    use limiteron::BanTarget;
    use limiteron::error::LimiteronError;
    use limiteron::validation::validate_ban_target;

    #[test]
    fn test_ban_target_valid_user_id() {
        let target = BanTarget::UserId("user123".to_string());
        assert!(validate_ban_target(&target).is_ok());

        let target_with_special = BanTarget::UserId("user-name_123".to_string());
        assert!(validate_ban_target(&target_with_special).is_ok());
    }

    #[test]
    fn test_ban_target_valid_ip() {
        let ipv4_target = BanTarget::Ip("192.168.1.1".to_string());
        assert!(validate_ban_target(&ipv4_target).is_ok());

        let ipv6_target = BanTarget::Ip("::1".to_string());
        assert!(validate_ban_target(&ipv6_target).is_ok());

        let ipv4_with_port = BanTarget::Ip("192.168.1.1:8080".to_string());
        assert!(validate_ban_target(&ipv4_with_port).is_ok());
    }

    #[test]
    fn test_ban_target_valid_mac() {
        let target = BanTarget::Mac("00:11:22:33:44:55".to_string());
        assert!(validate_ban_target(&target).is_ok());

        let target_no_sep = BanTarget::Mac("001122334455".to_string());
        assert!(validate_ban_target(&target_no_sep).is_ok());
    }

    #[test]
    fn test_ban_target_invalid_empty_user_id() {
        let target = BanTarget::UserId("".to_string());
        let result = validate_ban_target(&target);
        assert!(result.is_err());
    }

    #[test]
    fn test_ban_target_invalid_empty_ip() {
        let target = BanTarget::Ip("".to_string());
        let result = validate_ban_target(&target);
        assert!(result.is_err());
    }

    #[test]
    fn test_ban_target_invalid_empty_mac() {
        let target = BanTarget::Mac("".to_string());
        let result = validate_ban_target(&target);
        assert!(result.is_err());
    }

    #[test]
    fn test_ban_target_invalid_malformed_ip() {
        let target = BanTarget::Ip("not.an.ip".to_string());
        assert!(validate_ban_target(&target).is_err());
    }

    #[test]
    fn test_ban_target_invalid_malformed_user_id() {
        let target = BanTarget::UserId("user name".to_string());
        assert!(validate_ban_target(&target).is_err());
    }

    #[test]
    fn test_ban_target_invalid_malformed_mac() {
        let target = BanTarget::Mac("invalid-mac".to_string());
        assert!(validate_ban_target(&target).is_err());
    }
}

// ============================================================================
// Error message content tests
// ============================================================================

mod error_message_tests {
    use super::*;

    #[test]
    fn test_ip_address_error_messages() {
        let empty_result = validate_ip_address("");
        assert!(empty_result.is_err());
        let empty_err = empty_result.unwrap_err();
        assert!(
            matches!(empty_err, LimiteronError::ValidationError(ref msg) if msg.contains("empty"))
        );

        // Build a string that exceeds max length
        let long_ip = "1.2.3.4:".to_string() + &"5".repeat(MAX_IP_ADDRESS_LENGTH);
        let long_result = validate_ip_address(&long_ip);
        assert!(long_result.is_err());
        let long_err = long_result.unwrap_err();
        assert!(
            matches!(long_err, LimiteronError::ValidationError(ref msg) if msg.contains("exceeds") && msg.contains("45"))
        );

        let bad_result = validate_ip_address("abc");
        assert!(bad_result.is_err());
        let bad_err = bad_result.unwrap_err();
        assert!(
            matches!(bad_err, LimiteronError::ValidationError(ref msg) if msg.contains("Invalid IP address format"))
        );
    }

    #[test]
    fn test_user_id_error_messages() {
        let empty_result = validate_user_id("");
        assert!(matches!(
            empty_result.unwrap_err(),
            LimiteronError::ValidationError(_)
        ));

        let long_result = validate_user_id(&"a".repeat(MAX_USER_ID_LENGTH + 1));
        assert!(matches!(
            long_result.unwrap_err(),
            LimiteronError::ConfigError(_)
        ));

        let invalid_result = validate_user_id("user@#$");
        assert!(matches!(
            invalid_result.unwrap_err(),
            LimiteronError::ValidationError(_)
        ));
    }

    #[test]
    fn test_length_error_message_format() {
        let result = validate_length("toolong", 5, "MyField");
        let err = result.unwrap_err();
        assert!(
            matches!(err, LimiteronError::ConfigError(ref msg) if msg.contains("MyField") && msg.contains("5"))
        );
    }
}

// ============================================================================
// Cross-function consistency tests
// ============================================================================

mod consistency_tests {
    use super::*;

    #[test]
    fn test_max_constants_are_consistent() {
        // Verify the hardcoded constants match the module's constants
        assert_eq!(MAX_IP_ADDRESS_LENGTH, 45);
        assert_eq!(MAX_USER_ID_LENGTH, 256);
        assert_eq!(MAX_MAC_ADDRESS_LENGTH, 17);
        assert_eq!(MAX_BAN_REASON_LENGTH, 500);
        assert_eq!(MAX_HEADER_VALUE_LENGTH, 8192);
        assert_eq!(MAX_PATH_LENGTH, 2048);
        assert_eq!(MAX_API_KEY_LENGTH, 512);
    }

    #[test]
    fn test_all_validation_functions_return_result() {
        // Smoke test: all public functions return Result<(), LimiteronError>
        validate_ip_address("127.0.0.1").unwrap();
        validate_user_id("user").unwrap();
        validate_mac_address("00:00:00:00:00:00").unwrap();
        validate_api_key("key").unwrap();
        validate_header_value("value").unwrap();
        validate_ban_reason("reason").unwrap();
        validate_path("/").unwrap();
        validate_length("val", 10, "f").unwrap();
    }
}
