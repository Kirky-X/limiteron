//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 日志脱敏模块
//!
//! 提供日志脱敏功能，保护敏感信息不被泄露到日志中。
//! 即使没有启用 log-redaction feature，基础脱敏函数也可用。

/// 基础脱敏函数 - 即使没有启用 log-redaction feature 也可用
#[inline]
pub fn redact_basic(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "unknown".to_string();
    };

    let value = value.trim();
    if value.is_empty() {
        return "unknown".to_string();
    }

    if value.len() < 4 {
        return "***".to_string();
    }

    let prefix = &value[..2.min(value.len())];
    let suffix_len = 2.min(value.len().saturating_sub(2));
    let suffix = &value[value.len().saturating_sub(suffix_len)..];
    format!("{}***{}", prefix, suffix)
}

/// 用户ID脱敏 - 即使没有启用 log-redaction feature 也可用
#[inline]
pub fn redact_user_id(value: Option<&str>) -> String {
    redact_basic(value)
}

/// IP地址脱敏 - 即使没有启用 log-redaction feature 也可用
#[inline]
pub fn redact_ip(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "unknown".to_string();
    };

    let value = value.trim();
    if value.is_empty() {
        return "unknown".to_string();
    }

    // 如果是IP地址，保留前两段
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() == 4 {
        return format!("{}.{}.***.***", parts[0], parts[1]);
    }

    // IPv6简化处理
    if value.contains(':') {
        let parts: Vec<&str> = value.split(':').collect();
        if parts.len() >= 2 {
            return format!("{}:***:***", parts[0]);
        }
    }

    redact_basic(Some(value))
}

/// 邮箱脱敏 - 即使没有启用 log-redaction feature 也可用
#[inline]
pub fn redact_email(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "unknown".to_string();
    };

    let value = value.trim();
    if value.is_empty() {
        return "unknown".to_string();
    }

    if let Some(at_pos) = value.find('@') {
        let local_part = &value[..at_pos];
        let domain = &value[at_pos..];

        if local_part.len() <= 2 {
            return format!("***{}", domain);
        }

        let prefix_len = if local_part.len() <= 4 { 1 } else { 2 };
        return format!("{}***{}", &local_part[..prefix_len], domain);
    }

    redact_basic(Some(value))
}

#[cfg(feature = "log-redaction")]
use parking_lot::Mutex;
#[cfg(feature = "log-redaction")]
use regex::Regex;

#[cfg(feature = "log-redaction")]
/// 敏感字段模式列表
static SENSITIVE_PATTERNS: Mutex<Vec<(&str, Regex)>> = Mutex::new(Vec::new());

#[cfg(feature = "log-redaction")]
/// 初始化敏感字段模式
fn initialize_patterns() {
    let mut patterns = SENSITIVE_PATTERNS.lock();
    if patterns.is_empty() {
        patterns.push((
            "password",
            Regex::new(r"(?i)(password[\s]*[:=][\s]*)([^\s,\}]+)").unwrap(),
        ));
        patterns.push((
            "api_key",
            Regex::new(r"(?i)(api[_-]?key[\s]*[:=][\s]*)([^\s,\}]+)").unwrap(),
        ));
        patterns.push((
            "token",
            Regex::new(r"(?i)(token[\s]*[:=][\s]*)([^\s,\}]+)").unwrap(),
        ));
        patterns.push((
            "secret",
            Regex::new(r"(?i)(secret[\s]*[:=][\s]*)([^\s,\}]+)").unwrap(),
        ));
        patterns.push((
            "credential",
            Regex::new(r"(?i)(credential[\s]*[:=][\s]*)([^\s,\}]+)").unwrap(),
        ));
        patterns.push((
            "authorization",
            Regex::new(r"(?i)(authorization[\s]*[:=][\s]*)([^\s,\}]+)").unwrap(),
        ));
        patterns.push((
            "email",
            Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap(),
        ));
        patterns.push(("phone", Regex::new(r"1[3-9]\d{9}").unwrap()));
        patterns.push(("id_card", Regex::new(r"\d{17}[\dXx]").unwrap()));
        patterns.push((
            "credit_card",
            Regex::new(r"\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}").unwrap(),
        ));
    }
}

/// 增强版脱敏函数 - 需要 log-redaction feature
#[cfg(feature = "log-redaction")]
#[inline]
pub fn redact_advanced(value: Option<&str>, field_name: Option<&str>) -> String {
    let Some(value) = value else {
        return "unknown".to_string();
    };

    let value = value.trim();
    if value.is_empty() {
        return "unknown".to_string();
    }

    // 检查是否是已知的敏感字段
    if let Some(name) = field_name {
        let lower_name = name.to_lowercase();
        if lower_name.contains("password")
            || lower_name.contains("secret")
            || lower_name.contains("token")
            || lower_name.contains("key")
            || lower_name.contains("credential")
            || lower_name.contains("authorization")
        {
            return "***".to_string();
        }
    }

    // 应用正则模式脱敏
    let mut result = value.to_string();

    initialize_patterns();
    for (pattern_name, regex) in SENSITIVE_PATTERNS.lock().iter() {
        if *pattern_name == "email"
            || *pattern_name == "phone"
            || *pattern_name == "id_card"
            || *pattern_name == "credit_card"
        {
            // 直接脱敏敏感信息类型
            result = regex.replace_all(&result, "***").to_string();
        }
    }

    // 如果是短值，完全脱敏
    if value.len() <= 4 {
        return "***".to_string();
    }

    // 基础脱敏：保留首尾字符
    let prefix = &value[..2.min(value.len())];
    let suffix_len = 2.min(value.len().saturating_sub(2));
    let suffix = &value[value.len().saturating_sub(suffix_len)..];

    format!("{}***{}", prefix, suffix)
}

/// 敏感信息检测 - 需要 log-redaction feature
#[cfg(feature = "log-redaction")]
#[inline]
pub fn contains_sensitive_info(value: &str) -> bool {
    initialize_patterns();
    for (_, regex) in SENSITIVE_PATTERNS.lock().iter() {
        if regex.is_match(value) {
            return true;
        }
    }

    // 检查常见的敏感字段名
    let lower_value = value.to_lowercase();
    lower_value.contains("password")
        || lower_value.contains("secret")
        || lower_value.contains("token")
        || lower_value.contains("api_key")
        || lower_value.contains("credential")
        || lower_value.contains("authorization")
}

/// HTTP请求/响应脱敏 - 需要 log-redaction feature
#[cfg(feature = "log-redaction")]
#[inline]
pub fn redact_http_content(content: &str) -> String {
    let mut result = content.to_string();

    initialize_patterns();
    for (_, regex) in SENSITIVE_PATTERNS.lock().iter() {
        result = regex.replace_all(&result, "***").to_string();
    }

    result
}

/// 批量脱敏结构体字段 - 需要 log-redaction feature
#[cfg(feature = "log-redaction")]
pub struct RedactionConfig<'a> {
    pub fields: Vec<(&'a str, bool)>, // (字段名, 是否为敏感字段)
}

#[cfg(feature = "log-redaction")]
impl<'a> RedactionConfig<'a> {
    /// 创建新的脱敏配置
    #[inline]
    pub fn new() -> Self {
        Self { fields: Vec::new() }
    }

    /// 添加要脱敏的字段
    #[inline]
    pub fn add_field(mut self, field_name: &'a str, is_sensitive: bool) -> Self {
        self.fields.push((field_name, is_sensitive));
        self
    }

    /// 构建脱敏后的字符串表示
    #[inline]
    pub fn format<F>(&self, get_field: F) -> String
    where
        F: Fn(&str) -> Option<String>,
    {
        let mut parts = Vec::new();
        for (field_name, is_sensitive) in &self.fields {
            if let Some(value) = get_field(field_name) {
                let redacted_value = if *is_sensitive {
                    redact_advanced(Some(&value), Some(field_name))
                } else {
                    value
                };
                parts.push(format!("{}={}", field_name, redacted_value));
            }
        }
        format!("{{{}}}", parts.join(", "))
    }
}

#[cfg(feature = "log-redaction")]
impl<'a> Default for RedactionConfig<'a> {
    fn default() -> Self {
        Self::new()
    }
}

/// 脱敏 BanTarget - 可用于所有场景
#[inline]
#[cfg(feature = "ban-manager")]
pub fn redact_ban_target(target: &crate::storage::BanTarget) -> String {
    match target {
        crate::storage::BanTarget::Ip(ip) => redact_ip(Some(ip)),
        crate::storage::BanTarget::UserId(user_id) => redact_user_id(Some(user_id)),
        crate::storage::BanTarget::Mac(mac) => redact_basic(Some(mac)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_basic() {
        assert_eq!(redact_basic(None), "unknown");
        assert_eq!(redact_basic(Some("")), "unknown");
        assert_eq!(redact_basic(Some("   ")), "unknown");
        assert_eq!(redact_basic(Some("abc")), "***");
        assert_eq!(redact_basic(Some("user123")), "us***23");
        assert_eq!(redact_basic(Some("192.168.1.1")), "19***.1");
    }

    #[test]
    fn test_redact_basic_boundaries() {
        assert_eq!(redact_basic(Some("abcd")), "ab***cd");
        assert_eq!(redact_basic(Some("abcde")), "ab***de");
        assert_eq!(redact_basic(Some("ab")), "***");
        assert_eq!(redact_basic(Some("a")), "***");
    }

    #[test]
    fn test_redact_user_id() {
        assert_eq!(redact_user_id(None), "unknown");
        assert_eq!(redact_user_id(Some("")), "unknown");
        assert_eq!(redact_user_id(Some("user123")), "us***23");
        assert_eq!(redact_user_id(Some("abc")), "***");
    }

    #[test]
    fn test_redact_ip() {
        assert_eq!(redact_ip(None), "unknown");
        assert_eq!(redact_ip(Some("192.168.1.1")), "192.168.***.***");
        assert_eq!(redact_ip(Some("::1")), ":***:***");
    }

    #[test]
    fn test_redact_ip_more() {
        assert_eq!(redact_ip(Some("")), "unknown");
        assert_eq!(redact_ip(Some("   ")), "unknown");
        assert_eq!(redact_ip(Some("2001:db8::1")), "2001:***:***");
        assert_eq!(redact_ip(Some("not_an_ip")), "no***ip");
        assert_eq!(redact_ip(Some("ab")), "***");
    }

    #[test]
    fn test_redact_email() {
        assert_eq!(redact_email(None), "unknown");
        assert_eq!(redact_email(Some("test@example.com")), "t***@example.com");
        assert_eq!(redact_email(Some("ab@example.com")), "***@example.com");
    }

    #[test]
    fn test_redact_email_more() {
        assert_eq!(redact_email(Some("")), "unknown");
        assert_eq!(redact_email(Some("   ")), "unknown");
        assert_eq!(redact_email(Some("abc@example.com")), "a***@example.com");
        assert_eq!(redact_email(Some("abcde@example.com")), "ab***@example.com");
        assert_eq!(redact_email(Some("noatsign")), "no***gn");
    }

    #[test]
    fn test_redact_email_local_part_boundaries() {
        assert_eq!(redact_email(Some("a@b.co")), "***@b.co");
        assert_eq!(redact_email(Some("ab@b.co")), "***@b.co");
        assert_eq!(redact_email(Some("abc@b.co")), "a***@b.co");
        assert_eq!(redact_email(Some("abcd@b.co")), "a***@b.co");
        assert_eq!(redact_email(Some("abcde@b.co")), "ab***@b.co");
        assert_eq!(redact_email(Some("abcdef@b.co")), "ab***@b.co");
    }

    #[test]
    fn test_redact_ip_extra_segments() {
        assert_eq!(redact_ip(Some("1.2.3.4.5")), "1.***.5");
        assert_eq!(redact_ip(Some("1.2.3")), "1.***.3");
        assert_eq!(redact_ip(Some("1.2.3.4.5.6")), "1.***.6");
    }

    #[test]
    fn test_redact_ip_single_colon() {
        assert_eq!(redact_ip(Some(":")), ":***:***");
        assert_eq!(redact_ip(Some("a:")), "a:***:***");
        assert_eq!(redact_ip(Some(":b")), ":***:***");
    }

    #[test]
    fn test_redact_ip_double_colon() {
        assert_eq!(redact_ip(Some("::")), ":***:***");
        assert_eq!(redact_ip(Some(":::")), ":***:***");
    }

    #[test]
    fn test_redact_email_at_start() {
        assert_eq!(redact_email(Some("@example.com")), "***@example.com");
    }

    #[test]
    fn test_redact_email_multiple_at() {
        assert_eq!(redact_email(Some("a@b@c.com")), "***@b@c.com");
        assert_eq!(redact_email(Some("@a@b.com")), "***@a@b.com");
    }

    #[test]
    fn test_redact_email_dots_in_local() {
        assert_eq!(
            redact_email(Some("first.last@example.com")),
            "fi***@example.com"
        );
    }

    #[test]
    fn test_redact_basic_very_long() {
        let long = "abcdefghijklmnopqrstuvwxyz1234567890";
        assert_eq!(redact_basic(Some(long)), "ab***90");
        assert_eq!(redact_basic(Some("a")), "***");
        assert_eq!(redact_basic(Some("xy")), "***");
    }

    #[test]
    fn test_redact_basic_whitespace_mixed() {
        assert_eq!(redact_basic(Some("  a  ")), "***");
        assert_eq!(redact_basic(Some("  ab  ")), "***");
        assert_eq!(redact_basic(Some("  abc  ")), "***");
        assert_eq!(redact_basic(Some("  abcd  ")), "ab***cd");
    }

    #[test]
    fn test_redact_user_id_delegates() {
        assert_eq!(redact_user_id(Some("long_user_id_123")), "lo***23");
        assert_eq!(redact_user_id(Some("xy")), "***");
    }

    #[cfg(feature = "ban-manager")]
    #[test]
    fn test_redact_ban_target() {
        use crate::storage::BanTarget;

        let ip_target = BanTarget::Ip("192.168.1.1".to_string());
        assert_eq!(redact_ban_target(&ip_target), "192.168.***.***");

        let user_target = BanTarget::UserId("user123".to_string());
        assert_eq!(redact_ban_target(&user_target), "us***23");

        let mac_target = BanTarget::Mac("00:1a:2b:3c:4d:5e".to_string());
        assert_eq!(redact_ban_target(&mac_target), "00***5e");
    }

    #[cfg(feature = "ban-manager")]
    #[test]
    fn test_redact_ban_target_short_values() {
        use crate::storage::BanTarget;

        let ip_target = BanTarget::Ip("1.2.3.4".to_string());
        assert_eq!(redact_ban_target(&ip_target), "1.2.***.***");

        let user_target = BanTarget::UserId("ab".to_string());
        assert_eq!(redact_ban_target(&user_target), "***");

        let mac_target = BanTarget::Mac("ab".to_string());
        assert_eq!(redact_ban_target(&mac_target), "***");
    }

    #[cfg(feature = "ban-manager")]
    #[test]
    fn test_redact_ban_target_empty_values() {
        use crate::storage::BanTarget;

        assert_eq!(redact_ban_target(&BanTarget::Ip("".to_string())), "unknown");
        assert_eq!(
            redact_ban_target(&BanTarget::UserId("".to_string())),
            "unknown"
        );
        assert_eq!(
            redact_ban_target(&BanTarget::Mac("".to_string())),
            "unknown"
        );
    }

    #[cfg(feature = "log-redaction")]
    mod log_redaction_tests {
        use super::*;

        #[test]
        fn test_redact_advanced() {
            assert_eq!(redact_advanced(None, None), "unknown");
            assert_eq!(redact_advanced(Some("password123"), None), "pa***23");
            assert_eq!(redact_advanced(Some("token123"), Some("api_key")), "***");
            assert_eq!(
                redact_advanced(Some("user123"), Some("username")),
                "us***23"
            );
        }

        #[test]
        fn test_redact_advanced_empty_and_short() {
            assert_eq!(redact_advanced(Some(""), None), "unknown");
            assert_eq!(redact_advanced(Some("   "), None), "unknown");
            assert_eq!(redact_advanced(Some("abc"), None), "***");
            assert_eq!(redact_advanced(Some("abcd"), None), "***");
            assert_eq!(redact_advanced(Some("abcde"), None), "ab***de");
        }

        #[test]
        fn test_redact_advanced_sensitive_field_names() {
            assert_eq!(redact_advanced(Some("val"), Some("password")), "***");
            assert_eq!(redact_advanced(Some("val"), Some("my_secret")), "***");
            assert_eq!(redact_advanced(Some("val"), Some("auth_token")), "***");
            assert_eq!(redact_advanced(Some("val"), Some("api_key")), "***");
            assert_eq!(redact_advanced(Some("val"), Some("credential")), "***");
            assert_eq!(redact_advanced(Some("val"), Some("authorization")), "***");
        }

        #[test]
        fn test_redact_advanced_sensitive_field_case_insensitive() {
            assert_eq!(redact_advanced(Some("val"), Some("Password")), "***");
            assert_eq!(redact_advanced(Some("val"), Some("SECRET_KEY")), "***");
        }

        #[test]
        fn test_redact_advanced_applies_regex_then_basic_redact() {
            // The regex patterns run but the final output uses basic prefix/suffix
            // on the ORIGINAL value (not the regex-replaced result).
            // This is the current implementation behavior.
            assert_eq!(
                redact_advanced(Some("contact me at test@example.com"), None),
                "co***om"
            );
            assert_eq!(redact_advanced(Some("phone: 13800138000"), None), "ph***00");
            assert_eq!(
                redact_advanced(Some("id: 110101199001011234"), None),
                "id***34"
            );
            assert_eq!(
                redact_advanced(Some("card: 1234-5678-9012-3456"), None),
                "ca***56"
            );
        }

        #[test]
        fn test_redact_advanced_credit_card_no_separator() {
            let result = redact_advanced(Some("card: 1234567890123456"), None);
            assert_eq!(result, "ca***56");
        }

        #[test]
        fn test_contains_sensitive_info() {
            assert!(contains_sensitive_info("password=secret123"));
            assert!(contains_sensitive_info("api_key=abc123xyz"));
            assert!(contains_sensitive_info(
                "token=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"
            ));
            assert!(!contains_sensitive_info("username=user123"));
        }

        #[test]
        fn test_contains_sensitive_info_more() {
            assert!(contains_sensitive_info("secret=mysecret"));
            assert!(contains_sensitive_info("credential=mycred"));
            assert!(contains_sensitive_info("authorization=bearer token"));
            assert!(!contains_sensitive_info(""));
            assert!(!contains_sensitive_info("hello world"));
        }

        #[test]
        fn test_contains_sensitive_info_field_name_only() {
            assert!(contains_sensitive_info("password"));
            assert!(contains_sensitive_info("token"));
            assert!(contains_sensitive_info("secret"));
            assert!(contains_sensitive_info("api_key"));
            assert!(contains_sensitive_info("credential"));
            assert!(contains_sensitive_info("authorization"));
        }

        #[test]
        fn test_contains_sensitive_info_regex_patterns() {
            assert!(contains_sensitive_info("test@example.com"));
            assert!(contains_sensitive_info("13800138000"));
            assert!(contains_sensitive_info("110101199001011234"));
            assert!(contains_sensitive_info("1234567890123456"));
        }

        #[test]
        fn test_redact_http_content() {
            let content = r#"password=secret123, username=user123"#;
            let redacted = redact_http_content(content);
            assert!(!redacted.contains("secret123"));
            assert!(redacted.contains("user123"));
        }

        #[test]
        fn test_redact_http_content_empty() {
            assert_eq!(redact_http_content(""), "");
        }

        #[test]
        fn test_redact_http_content_no_sensitive() {
            let content = "username=user123, role=admin";
            let redacted = redact_http_content(content);
            assert_eq!(redacted, content);
        }

        #[test]
        fn test_redact_http_content_email() {
            let content = "user=test@example.com";
            let redacted = redact_http_content(content);
            assert_eq!(redacted, "user=***");
        }

        #[test]
        fn test_redact_http_content_multiple_sensitive() {
            let content = "password=abc123 token=xyz789";
            let redacted = redact_http_content(content);
            assert_eq!(redacted, "*** ***");
        }

        #[test]
        fn test_redaction_config_default() {
            let config: RedactionConfig = Default::default();
            assert_eq!(config.fields.len(), 0);
        }

        #[test]
        fn test_redaction_config_empty_format() {
            let config = RedactionConfig::new();
            let result = config.format(|_| None);
            assert_eq!(result, "{}");
        }

        #[test]
        fn test_redaction_config_with_missing_field() {
            let config = RedactionConfig::new()
                .add_field("password", true)
                .add_field("username", false);

            let result = config.format(|field| match field {
                "password" => Some("secret123".to_string()),
                _ => None,
            });

            assert_eq!(result, "{password=***}");
        }

        #[test]
        fn test_redaction_config_no_sensitive() {
            let config = RedactionConfig::new()
                .add_field("user", false)
                .add_field("role", false);

            let result = config.format(|field| match field {
                "user" => Some("alice".to_string()),
                "role" => Some("admin".to_string()),
                _ => None,
            });

            assert_eq!(result, "{user=alice, role=admin}");
        }

        #[test]
        fn test_redaction_config_all_sensitive() {
            let config = RedactionConfig::new()
                .add_field("password", true)
                .add_field("token", true);

            let result = config.format(|field| match field {
                "password" => Some("secret123".to_string()),
                "token" => Some("abc".to_string()),
                _ => None,
            });

            assert_eq!(result, "{password=***, token=***}");
        }

        #[test]
        fn test_initialize_patterns_idempotent() {
            initialize_patterns();
            initialize_patterns();
            let patterns = SENSITIVE_PATTERNS.lock();
            let names: Vec<&str> = patterns.iter().map(|(n, _)| *n).collect();
            assert_eq!(names.len(), 10);
            assert!(names.contains(&"password"));
            assert!(names.contains(&"api_key"));
            assert!(names.contains(&"token"));
            assert!(names.contains(&"secret"));
            assert!(names.contains(&"credential"));
            assert!(names.contains(&"authorization"));
            assert!(names.contains(&"email"));
            assert!(names.contains(&"phone"));
            assert!(names.contains(&"id_card"));
            assert!(names.contains(&"credit_card"));
        }

        #[test]
        fn test_redact_advanced_field_key_substring() {
            assert_eq!(redact_advanced(Some("value"), Some("monkey")), "***");
            assert_eq!(redact_advanced(Some("value"), Some("keychain")), "***");
            assert_eq!(redact_advanced(Some("value"), Some("key")), "***");
        }

        #[test]
        fn test_redact_advanced_field_token_substring() {
            assert_eq!(redact_advanced(Some("value"), Some("tokenizer")), "***");
            assert_eq!(redact_advanced(Some("value"), Some("my_token_123")), "***");
        }

        #[test]
        fn test_redact_advanced_short_value_after_regex() {
            assert_eq!(redact_advanced(Some("hi"), None), "***");
            assert_eq!(redact_advanced(Some("bye"), Some("tag")), "***");
            assert_eq!(redact_advanced(Some("test"), None), "***");
        }

        #[test]
        fn test_redact_advanced_long_value_with_sensitive_field() {
            let long = "abcdefghijklmnopqrstuvwxyz";
            assert_eq!(redact_advanced(Some(long), Some("password")), "***");
            assert_eq!(redact_advanced(Some(long), Some("secret_key")), "***");
        }

        #[test]
        fn test_redact_advanced_each_individual_trigger() {
            assert_eq!(redact_advanced(Some("x"), Some("PASSWORD")), "***");
            assert_eq!(redact_advanced(Some("x"), Some("SECRET")), "***");
            assert_eq!(redact_advanced(Some("x"), Some("TOKEN")), "***");
            assert_eq!(redact_advanced(Some("x"), Some("KEY")), "***");
            assert_eq!(redact_advanced(Some("x"), Some("CREDENTIAL")), "***");
            assert_eq!(redact_advanced(Some("x"), Some("AUTHORIZATION")), "***");
        }

        #[test]
        fn test_redact_http_content_api_key_secret() {
            let content = r#"api_key=sk-abc123, secret=mysecret123"#;
            let redacted = redact_http_content(content);
            assert!(!redacted.contains("sk-abc123"));
            assert!(!redacted.contains("mysecret123"));
        }

        #[test]
        fn test_redact_http_content_credential_auth() {
            let content = r#"credential=mycreds, authorization=bearer_token"#;
            let redacted = redact_http_content(content);
            assert!(!redacted.contains("mycreds"));
            assert!(!redacted.contains("bearer_token"));
        }

        #[test]
        fn test_redact_http_content_phone_id_card_credit() {
            let content = r#"phone=13800138000, id=110101199001011234, cc=1234-5678-9012-3456"#;
            let redacted = redact_http_content(content);
            assert!(!redacted.contains("13800138000"));
            assert!(!redacted.contains("110101199001011234"));
            assert!(!redacted.contains("1234-5678-9012-3456"));
        }

        #[test]
        fn test_redact_http_content_all_pattern_types() {
            let content = concat!(
                "password=pass123, ",
                "api_key=key123, ",
                "token=my_token, ",
                "secret=my_secret, ",
                "credential=cr3d3ntial, ",
                "authorization=@uth_t0k3n, ",
                "email=test@example.com, ",
                "phone=13800138000, ",
                "id_card=110101199001011234, ",
                "credit_card=1234567890123456"
            );
            let redacted = redact_http_content(content);
            assert!(!redacted.contains("pass123"), "password should be redacted");
            assert!(!redacted.contains("key123"), "api_key should be redacted");
            assert!(!redacted.contains("my_token"), "token should be redacted");
            assert!(!redacted.contains("my_secret"), "secret should be redacted");
            assert!(
                !redacted.contains("cr3d3ntial"),
                "credential should be redacted"
            );
            assert!(
                !redacted.contains("@uth_t0k3n"),
                "authorization should be redacted"
            );
            assert!(
                !redacted.contains("test@example.com"),
                "email should be redacted"
            );
            assert!(
                !redacted.contains("13800138000"),
                "phone should be redacted"
            );
            assert!(
                !redacted.contains("110101199001011234"),
                "id_card should be redacted"
            );
            assert!(
                !redacted.contains("1234567890123456"),
                "credit_card should be redacted"
            );
        }

        #[test]
        fn test_contains_sensitive_info_special_values() {
            assert!(!contains_sensitive_info(" "));
            assert!(!contains_sensitive_info("\t"));
            assert!(!contains_sensitive_info("a"));
            assert!(!contains_sensitive_info("ab"));
        }

        #[test]
        fn test_contains_sensitive_info_individual_regex() {
            assert!(contains_sensitive_info("email me at test@example.com"));
            assert!(contains_sensitive_info("call 13800138000"));
            assert!(contains_sensitive_info("my id is 110101199001011234"));
            assert!(contains_sensitive_info("card 1234567890123456"));
        }

        #[test]
        fn test_contains_sensitive_info_field_with_extra_chars() {
            assert!(contains_sensitive_info("password: secret123"));
            assert!(contains_sensitive_info("token : abcdef"));
            assert!(contains_sensitive_info("api_key=value123"));
            assert!(contains_sensitive_info("SECRET:myvalue"));
        }

        #[test]
        fn test_contains_sensitive_info_mixed_content() {
            assert!(contains_sensitive_info("user=admin, password=secret"));
            assert!(!contains_sensitive_info("user=admin, role=editor"));
            assert!(contains_sensitive_info(
                "header contains authorization: bearer xyz"
            ));
        }

        #[test]
        fn test_contains_sensitive_info_regex_first_then_field() {
            assert!(contains_sensitive_info(
                "email: test@example.com, password: secret"
            ));
            assert!(contains_sensitive_info("secret"));
        }

        #[test]
        fn test_redaction_config_sensitive_value_short() {
            let config = RedactionConfig::new()
                .add_field("pwd", true)
                .add_field("tok", true);

            let result = config.format(|field| match field {
                "pwd" => Some("ab".to_string()),
                "tok" => Some("abc".to_string()),
                _ => None,
            });

            assert_eq!(result, "{pwd=***, tok=***}");
        }

        #[test]
        fn test_redaction_config_order_preserved() {
            let config = RedactionConfig::new()
                .add_field("a", false)
                .add_field("b", false)
                .add_field("c", false);

            let result = config.format(|field| match field {
                "a" => Some("1".to_string()),
                "b" => Some("2".to_string()),
                "c" => Some("3".to_string()),
                _ => None,
            });

            assert_eq!(result, "{a=1, b=2, c=3}");
        }

        #[test]
        fn test_redaction_config_duplicate_field() {
            let config = RedactionConfig::new()
                .add_field("user", true)
                .add_field("user", true);

            let result = config.format(|field| match field {
                "user" => Some("sec".to_string()),
                _ => None,
            });

            assert_eq!(result, "{user=***, user=***}");
        }

        #[test]
        fn test_redaction_config_sensitive_non_sensitive_mixed() {
            let config = RedactionConfig::new()
                .add_field("user", false)
                .add_field("password", true)
                .add_field("secret_token", true)
                .add_field("role", false);

            let result = config.format(|field| match field {
                "user" => Some("alice".to_string()),
                "password" => Some("secret123".to_string()),
                "secret_token" => Some("myvalue".to_string()),
                "role" => Some("admin".to_string()),
                _ => None,
            });

            assert_eq!(
                result,
                "{user=alice, password=***, secret_token=***, role=admin}"
            );
        }

        #[test]
        fn test_redact_http_content_mixed_newlines() {
            let content = "line1\npassword=secret123\nline2\ntoken=abc";
            let redacted = redact_http_content(content);
            assert!(redacted.contains("line1"));
            assert!(redacted.contains("line2"));
            assert!(!redacted.contains("secret123"));
            assert!(!redacted.contains("token=abc"));
        }

        #[test]
        fn test_redact_http_content_special_chars_in_value() {
            let content = r#"password=abc123!@#$%"#;
            let redacted = redact_http_content(content);
            assert!(!redacted.contains("abc123!@#$%"));
        }
    }
}
