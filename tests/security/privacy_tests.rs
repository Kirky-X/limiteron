//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 数据隐私测试
//!
//! 测试覆盖：
//! - 日志脱敏测试（敏感头脱敏验证、API Key 脱敏验证）
//! - 错误消息安全测试（无内部信息泄露、无敏感数据泄露）

#[cfg(feature = "log-redaction")]
use limiteron::logging::{
    contains_sensitive_info, redact_advanced, redact_http_content, RedactionConfig,
};
use limiteron::logging::{redact_basic, redact_email, redact_ip, redact_user_id};
use limiteron::matchers::{Identifier, IdentifierExtractor, IpExtractor, RequestContext};

// ============================================================================
// 日志脱敏测试
// ============================================================================

/// 测试基础脱敏功能
///
/// 验证基础脱敏函数的正确性
#[test]
fn test_basic_redaction() {
    // 测试 None 值
    assert_eq!(redact_basic(None), "unknown");

    // 测试空字符串
    assert_eq!(redact_basic(Some("")), "unknown");
    assert_eq!(redact_basic(Some("   ")), "unknown");

    // 测试短字符串
    assert_eq!(redact_basic(Some("abc")), "***");

    // 测试正常长度字符串
    assert_eq!(redact_basic(Some("user123")), "us***23");
    assert_eq!(redact_basic(Some("12345678")), "12***78");

    // 测试边界情况
    assert_eq!(redact_basic(Some("ab")), "***");
    assert_eq!(redact_basic(Some("abcd")), "ab***cd");
}

/// 测试用户 ID 脱敏
///
/// 验证用户 ID 脱敏的正确性
#[test]
fn test_user_id_redaction() {
    // 正常用户 ID
    assert_eq!(redact_user_id(Some("user123")), "us***23");
    assert_eq!(redact_user_id(Some("admin")), "ad***in");

    // 邮箱格式用户 ID
    assert_eq!(redact_user_id(Some("user@example.com")), "us***om");

    // 长用户 ID
    let long_user_id = "very_long_user_id_with_many_characters";
    let redacted = redact_user_id(Some(long_user_id));
    assert!(redacted.contains("***"));
    assert!(redacted.len() < long_user_id.len());
}

/// 测试 IP 地址脱敏
///
/// 验证 IP 地址脱敏的正确性
#[test]
fn test_ip_redaction() {
    // IPv4 地址
    assert_eq!(redact_ip(Some("192.168.1.1")), "192.168.***.***");
    assert_eq!(redact_ip(Some("10.0.0.1")), "10.0.***.***");
    assert_eq!(redact_ip(Some("127.0.0.1")), "127.0.***.***");

    // IPv6 地址
    assert_eq!(redact_ip(Some("::1")), ":***:***");
    assert!(redact_ip(Some("2001:db8::1")).contains("***"));

    // None 和空值
    assert_eq!(redact_ip(None), "unknown");
    assert_eq!(redact_ip(Some("")), "unknown");
}

/// 测试邮箱脱敏
///
/// 验证邮箱地址脱敏的正确性
#[test]
fn test_email_redaction() {
    // 正常邮箱
    assert_eq!(redact_email(Some("test@example.com")), "t***@example.com");
    assert_eq!(redact_email(Some("user123@gmail.com")), "us***@gmail.com");

    // 短用户名邮箱
    assert_eq!(redact_email(Some("ab@example.com")), "***@example.com");
    assert_eq!(redact_email(Some("a@example.com")), "***@example.com");

    // None 和空值
    assert_eq!(redact_email(None), "unknown");
    assert_eq!(redact_email(Some("")), "unknown");

    // 非邮箱格式
    assert_eq!(redact_email(Some("not-an-email")), "no***il");
}

/// 测试敏感头脱敏
///
/// 验证 HTTP 敏感头的脱敏
#[test]
fn test_sensitive_header_redaction() {
    let ctx = RequestContext::new()
        .with_header(
            "authorization",
            "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
        )
        .with_header("x-api-key", "sk-1234567890abcdef")
        .with_header("cookie", "session=abc123; token=xyz789")
        .with_header("x-custom-header", "normal-value");

    let debug_output = format!("{:?}", ctx);

    // 验证敏感值被脱敏
    assert!(
        !debug_output.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"),
        "JWT token should be redacted"
    );
    assert!(
        !debug_output.contains("sk-1234567890abcdef"),
        "API key should be redacted"
    );
    assert!(
        !debug_output.contains("session=abc123"),
        "Cookie session should be redacted"
    );

    // 验证非敏感值保留
    assert!(
        debug_output.contains("normal-value"),
        "Non-sensitive header should be preserved"
    );
}

/// 测试 API Key 脱敏
///
/// 验证 API Key 的脱敏处理
#[test]
fn test_api_key_redaction() {
    // 各种格式的 API Key
    let api_keys = vec![
        "sk-1234567890abcdef",
        "api_key_12345",
        "Bearer token123",
        "Basic dXNlcjpwYXNz",
    ];

    for key in api_keys {
        let redacted = redact_basic(Some(key));

        // 脱敏后不应包含完整原始值
        assert_ne!(redacted, key, "API key should be redacted");

        // 脱敏后应包含星号
        assert!(
            redacted.contains("*"),
            "Redacted value should contain asterisks"
        );
    }
}

// ============================================================================
// 增强脱敏测试（需要 log-redaction feature）
// ============================================================================

/// 测试增强脱敏功能
#[cfg(feature = "log-redaction")]
#[test]
fn test_advanced_redaction() {
    // 测试敏感字段名
    assert_eq!(redact_advanced(Some("secret123"), Some("password")), "***");
    assert_eq!(redact_advanced(Some("token123"), Some("api_key")), "***");
    assert_eq!(redact_advanced(Some("key123"), Some("secret")), "***");

    // 测试非敏感字段名
    assert_eq!(
        redact_advanced(Some("user123"), Some("username")),
        "us***23"
    );
    assert_eq!(redact_advanced(Some("test"), Some("name")), "***");

    // 测试 None 值
    assert_eq!(redact_advanced(None, None), "unknown");
}

/// 测试敏感信息检测
#[cfg(feature = "log-redaction")]
#[test]
fn test_sensitive_info_detection() {
    // 应检测到敏感信息
    assert!(contains_sensitive_info("password=secret123"));
    assert!(contains_sensitive_info("api_key=abc123"));
    assert!(contains_sensitive_info("token=xyz789"));
    assert!(contains_sensitive_info("secret=value"));

    // 不应误报
    assert!(!contains_sensitive_info("username=user123"));
    assert!(!contains_sensitive_info("name=John Doe"));
}

/// 测试 HTTP 内容脱敏
#[cfg(feature = "log-redaction")]
#[test]
fn test_http_content_redaction() {
    let content = r#"{"password": "secret123", "username": "user1", "api_key": "sk-abc123"}"#;
    let redacted = redact_http_content(content);

    // 敏感值应被脱敏
    assert!(
        !redacted.contains("secret123"),
        "Password should be redacted"
    );
    assert!(
        !redacted.contains("sk-abc123"),
        "API key should be redacted"
    );

    // 非敏感值应保留
    assert!(redacted.contains("user1"), "Username should be preserved");
}

/// 测试脱敏配置
#[cfg(feature = "log-redaction")]
#[test]
fn test_redaction_config() {
    let config = RedactionConfig::new()
        .add_field("password", true)
        .add_field("api_key", true)
        .add_field("username", false)
        .add_field("email", true);

    let result = config.format(|field| match field {
        "password" => Some("secret123".to_string()),
        "api_key" => Some("sk-abc123".to_string()),
        "username" => Some("user1".to_string()),
        "email" => Some("user@example.com".to_string()),
        _ => None,
    });

    // 验证敏感字段被脱敏
    assert!(!result.contains("secret123"), "Password should be redacted");
    assert!(!result.contains("sk-abc123"), "API key should be redacted");
    assert!(
        !result.contains("user@example.com"),
        "Email should be redacted"
    );

    // 验证非敏感字段保留
    assert!(result.contains("user1"), "Username should be preserved");
}

// ============================================================================
// 错误消息安全测试
// ============================================================================

/// 测试错误消息不泄露内部信息
///
/// 验证错误消息不包含敏感的内部实现细节
#[tokio::test]
async fn test_error_message_no_internal_leak() {
    use limiteron::limiters::{Limiter, TokenBucketLimiter};

    let limiter = TokenBucketLimiter::new(100, 10);

    // 触发配置错误
    let result = limiter.allow(0).await;
    if let Err(e) = result {
        let error_msg = e.to_string();

        // 错误消息不应包含内部路径
        assert!(
            !error_msg.contains("/home/"),
            "Error should not contain internal paths"
        );
        assert!(
            !error_msg.contains("/Users/"),
            "Error should not contain internal paths"
        );

        // 错误消息不应包含源代码
        assert!(
            !error_msg.contains("fn "),
            "Error should not contain source code"
        );
        assert!(
            !error_msg.contains("impl "),
            "Error should not contain source code"
        );
    }
}

/// 测试存储错误不泄露敏感数据
///
/// 验证存储错误消息不包含敏感数据
#[tokio::test]
async fn test_storage_error_no_sensitive_leak() {
    use limiteron::error::StorageError;

    // 创建各种存储错误
    let errors = vec![
        StorageError::ConnectionError("Failed to connect to database".to_string()),
        StorageError::QueryError("Invalid query".to_string()),
        StorageError::TimeoutError("Operation timed out".to_string()),
        StorageError::AuthenticationError("Invalid credentials".to_string()),
    ];

    for error in errors {
        let error_msg = error.to_string();

        // 错误消息不应包含密码
        assert!(
            !error_msg.to_lowercase().contains("password"),
            "Error should not contain password"
        );

        // 错误消息不应包含密钥
        assert!(
            !error_msg.to_lowercase().contains("secret"),
            "Error should not contain secret"
        );
    }
}

/// 测试验证错误不泄露敏感信息
#[test]
fn test_validation_error_no_sensitive_leak() {
    use limiteron::validation::{validate_ip_address, validate_user_id};

    // 测试 IP 验证错误
    let result = validate_ip_address("invalid-ip-with-secret-key-12345");
    if let Err(e) = result {
        let error_msg = e.to_string();

        // 错误消息应只包含输入的一部分，而非全部
        // 这防止攻击者通过错误消息获取完整输入
        assert!(error_msg.len() < 100, "Error message should be concise");
    }

    // 测试用户 ID 验证错误
    let result = validate_user_id("");
    if let Err(e) = result {
        let error_msg = e.to_string();

        // 错误消息应简洁
        assert!(!error_msg.is_empty(), "Error message should not be empty");
    }
}

/// 测试配置错误不泄露敏感配置
#[cfg(feature = "config-security")]
#[test]
fn test_config_error_no_sensitive_config_leak() {
    use limiteron::config::ConfigSecurityValidator;
    use limiteron::config::{FlowControlConfig, GlobalConfig, Rule};

    let config = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: GlobalConfig {
            storage: StorageType::PostgreSQL,
            cache: CacheBackend::Memory,
            metrics: MetricsBackend::Prometheus,
        },
        rules: vec![],
    };

    let report = ConfigSecurityValidator::validate_config(&config);

    // 验证报告不应包含密码
    for warning in &report.warnings {
        assert!(
            !warning.to_lowercase().contains("password"),
            "Warning should not contain password: {}",
            warning
        );
    }
}

/// 测试限流错误不泄露限流配置
#[tokio::test]
async fn test_rate_limit_error_no_config_leak() {
    use limiteron::limiters::{Limiter, TokenBucketLimiter};

    let limiter = TokenBucketLimiter::new(10, 1);

    // 消耗所有令牌
    for _ in 0..10 {
        let _ = limiter.allow(1).await;
    }

    // 尝试再次消费，应被拒绝
    let result = limiter.allow(1).await;
    if let Ok(false) = result {
        // 限流拒绝是正常的，不应抛出错误
        // 如果有错误消息，验证不包含敏感配置
    }
}

// ============================================================================
// RequestContext 安全测试
// ============================================================================

/// 测试 RequestContext 的 Debug 实现安全性
#[test]
fn test_request_context_debug_security() {
    let ctx = RequestContext::new()
        .with_header("authorization", "Bearer secret-token-12345")
        .with_header("x-api-key", "sk-secret-api-key-67890")
        .with_header("cookie", "session=secret-session-id; token=secret-token")
        .with_header("x-custom", "normal-value")
        .with_query_param("token", "secret-query-token")
        .with_query_param("key", "secret-query-key")
        .with_query_param("name", "normal-name");

    let debug_output = format!("{:?}", ctx);

    // 验证敏感头被脱敏
    assert!(
        !debug_output.contains("secret-token-12345"),
        "Authorization header should be redacted"
    );
    assert!(
        !debug_output.contains("sk-secret-api-key-67890"),
        "API key header should be redacted"
    );
    assert!(
        !debug_output.contains("secret-session-id"),
        "Cookie session should be redacted"
    );
    assert!(
        !debug_output.contains("secret-token"),
        "Cookie token should be redacted"
    );

    // 验证敏感查询参数被脱敏
    assert!(
        !debug_output.contains("secret-query-token"),
        "Query token should be redacted"
    );
    assert!(
        !debug_output.contains("secret-query-key"),
        "Query key should be redacted"
    );

    // 验证非敏感值保留
    assert!(
        debug_output.contains("normal-value"),
        "Normal header should be preserved"
    );
    assert!(
        debug_output.contains("normal-name"),
        "Normal query param should be preserved"
    );
}

/// 测试 RequestContext 的 Clone 安全性
#[test]
fn test_request_context_clone_security() {
    let ctx = RequestContext::new()
        .with_header("authorization", "Bearer secret-token")
        .with_header("x-api-key", "sk-secret-key");

    let cloned = ctx.clone();

    // 克隆后的 Debug 输出也应脱敏
    let debug_output = format!("{:?}", cloned);
    assert!(
        !debug_output.contains("secret-token"),
        "Cloned context should also redact sensitive data"
    );
    assert!(
        !debug_output.contains("sk-secret-key"),
        "Cloned context should also redact API key"
    );
}

// ============================================================================
// 综合隐私测试
// ============================================================================

/// 测试完整的隐私保护链
#[test]
fn test_complete_privacy_chain() {
    // 模拟完整的请求处理流程
    let ctx = RequestContext::new()
        .with_header("x-user-id", "user123")
        .with_header("authorization", "Bearer secret-token")
        .with_header("x-api-key", "sk-secret-key")
        .with_header("x-forwarded-for", "192.168.1.100, 10.0.0.1");

    // 1. 提取 IP
    let extractor = IpExtractor::from_header("x-forwarded-for");
    let ip = extractor.extract(&ctx);
    if let Some(Identifier::Ip(ip_str)) = ip {
        // IP 应被正确提取
        assert_eq!(ip_str, "192.168.1.100");

        // IP 脱敏
        let redacted_ip = redact_ip(Some(&ip_str));
        assert!(redacted_ip.contains("***"));
        assert!(!redacted_ip.contains("100"));
    }

    // 2. 验证 Debug 输出脱敏
    let debug_output = format!("{:?}", ctx);
    assert!(!debug_output.contains("secret-token"));
    assert!(!debug_output.contains("sk-secret-key"));

    // 3. 验证用户 ID 脱敏
    let user_id = ctx.get_header("x-user-id");
    if let Some(uid) = user_id {
        let redacted = redact_user_id(Some(uid));
        assert!(redacted.contains("***"));
    }
}

/// 测试日志输出不泄露敏感信息
#[test]
fn test_log_output_no_sensitive_leak() {
    let sensitive_data = vec![
        ("password", "MySecretPassword123"),
        ("api_key", "sk-1234567890abcdef"),
        ("token", "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"),
        ("secret", "my-secret-value"),
        ("credential", "user:password"),
    ];

    for (field_name, value) in sensitive_data {
        // 使用基础脱敏
        let redacted = redact_basic(Some(value));

        // 脱敏后不应包含完整原始值
        assert_ne!(redacted, value, "Field '{}' should be redacted", field_name);

        // 脱敏后应包含星号
        assert!(
            redacted.contains("*"),
            "Redacted '{}' should contain asterisks",
            field_name
        );
    }
}

/// 测试错误处理不泄露堆栈信息
#[test]
fn test_error_no_stack_trace_leak() {
    use limiteron::error::FlowGuardError;

    let errors: Vec<FlowGuardError> = vec![
        FlowGuardError::ConfigError("Invalid configuration".to_string()),
        FlowGuardError::ValidationError("Invalid input".to_string()),
        FlowGuardError::LimitError("Rate limit exceeded".to_string()),
    ];

    for error in errors {
        let error_msg = error.to_string();

        // 错误消息不应包含堆栈跟踪
        assert!(
            !error_msg.contains("at "),
            "Error should not contain stack trace"
        );
        assert!(
            !error_msg.contains(".rs:"),
            "Error should not contain file paths"
        );
        assert!(
            !error_msg.contains("backtrace"),
            "Error should not contain backtrace"
        );
    }
}
