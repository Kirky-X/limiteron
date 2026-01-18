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

    if value.len() <= 4 {
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

        return format!("{}***{}", &local_part[..1], domain);
    }

    redact_basic(Some(value))
}

#[cfg(feature = "log-redaction")]
use regex::Regex;
#[cfg(feature = "log-redaction")]
use std::sync::Mutex as SyncMutex;

#[cfg(feature = "log-redaction")]
/// 敏感字段模式列表
static SENSITIVE_PATTERNS: SyncMutex<Vec<(&str, Regex)>> = SyncMutex::new(Vec::new());

#[cfg(feature = "log-redaction")]
/// 初始化敏感字段模式
fn initialize_patterns() {
    let mut patterns = SENSITIVE_PATTERNS.lock().unwrap();
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
pub fn redact_enhanced(value: Option<&str>, field_name: Option<&str>) -> String {
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
    for (pattern_name, regex) in SENSITIVE_PATTERNS.lock().unwrap().iter() {
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
    for (_, regex) in SENSITIVE_PATTERNS.lock().unwrap().iter() {
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
    for (_, regex) in SENSITIVE_PATTERNS.lock().unwrap().iter() {
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
                    redact_enhanced(Some(&value), Some(field_name))
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
impl Default for RedactionConfig<'_> {
    #[inline]
    fn default() -> Self {
        Self::new()
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
        // Implementation: prefix (first 2) + *** + suffix (last 2)
        assert_eq!(redact_basic(Some("user123")), "us***23");
        assert_eq!(redact_basic(Some("192.168.1.1")), "19***.1");
    }

    #[test]
    fn test_redact_ip() {
        assert_eq!(redact_ip(None), "unknown");
        assert_eq!(redact_ip(Some("192.168.1.1")), "192.168.***.***");
        // IPv6: implementation takes first segment
        assert_eq!(redact_ip(Some("::1")), ":***:***");
    }

    #[test]
    fn test_redact_email() {
        assert_eq!(redact_email(None), "unknown");
        assert_eq!(redact_email(Some("test@example.com")), "t***@example.com");
        // Implementation uses basic redaction for short usernames
        assert_eq!(redact_email(Some("ab@example.com")), "***@example.com");
    }

    #[cfg(feature = "log-redaction")]
    mod log_redaction_tests {
        use super::*;

        #[test]
        fn test_redact_enhanced() {
            // 敏感字段应该完全脱敏
            assert_eq!(redact_enhanced(Some("secret123"), Some("password")), "***");
            assert_eq!(redact_enhanced(Some("token123"), Some("api_key")), "***");

            // 普通字段使用基础脱敏
            assert_eq!(
                redact_enhanced(Some("user123"), Some("username")),
                "us***23"
            );
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
        fn test_redact_http_content() {
            // Use format matching the regex pattern (key=value without quotes around value)
            let content = r#"password=secret123, username=user123"#;
            let redacted = redact_http_content(content);
            assert!(!redacted.contains("secret123"));
            assert!(redacted.contains("user123"));
        }

        #[test]
        fn test_redaction_config() {
            let config = RedactionConfig::new()
                .add_field("password", true)
                .add_field("username", false);

            let result = config.format(|field| match field {
                "password" => Some("secret123".to_string()),
                "username" => Some("user123".to_string()),
                _ => None,
            });

            assert_eq!(result, "{password=***, username=user123}");
        }
    }
}
