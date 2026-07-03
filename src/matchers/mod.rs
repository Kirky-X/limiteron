//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 匹配器模块
//!
//! 实现标识符提取器和规则匹配引擎。
//!
//! # 标识符提取器
//!
//! 支持从请求中提取多种类型的标识符：
//! - 用户ID (UserId)
//! - IP地址 (Ip)
//! - MAC地址 (Mac)
//! - API密钥 (ApiKey)
//! - 设备ID (DeviceId)
//!
//! # 规则匹配引擎
//!
//! 支持复杂的规则匹配逻辑：
//! - 优先级排序
//! - 复合条件 (AND/OR/NOT)
//! - 高性能匹配 (< 200μs P99)
//! - 支持至少100条规则

// 子模块
#[cfg(feature = "geo-matching")]
pub mod geo;

#[cfg(feature = "device-matching")]
pub mod device;

pub mod custom;

// 新拆分的子模块
pub mod composite;
pub mod engine;
pub mod extractors;
pub mod traits;

// Re-export traits
pub use traits::{Identifier, IdentifierExtractor, RequestContext};

// Re-export extractors
pub use extractors::{
    ApiKeyExtractor, ApiKeyExtractorBuilder, CustomExtractor, DeviceIdExtractor,
    DeviceIdExtractorBuilder, IpExtractor, IpExtractorBuilder, MacExtractor, MacExtractorBuilder,
    UserIdExtractor, UserIdExtractorBuilder,
};

// Re-export composite
pub use composite::{CompositeExtractor, CompositeExtractorBuilder};

// Re-export engine
pub use engine::{
    CompositeCondition, ConditionEvaluator, IpRange, LogicalOperator, MatchCondition, MatcherStats,
    Rule, RuleMatcher, RuleMatcherBuilder,
};

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use std::net::IpAddr;
    use std::sync::Arc;

    // ==================== 标识符提取器测试 ====================

    #[test]
    fn test_user_id_extractor_from_header() {
        let extractor = UserIdExtractor::from_header("X-User-Id");
        let context = RequestContext::new().with_header("X-User-Id", "user123");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("user123".to_string()));
    }

    #[test]
    fn test_user_id_extractor_from_query_param() {
        let extractor = UserIdExtractor::from_query_param("user_id");
        let context = RequestContext::new().with_query_param("user_id", "user456");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("user456".to_string()));
    }

    #[test]
    fn test_user_id_extractor_with_default() {
        let extractor = UserIdExtractor::from_header("X-User-Id").with_default("default");
        let context = RequestContext::new();

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("default".to_string()));
    }

    #[test]
    fn test_user_id_extractor_priority() {
        let extractor = UserIdExtractor::new(
            Some("X-User-Id".to_string()),
            Some("user_id".to_string()),
            None,
        );
        let context = RequestContext::new()
            .with_header("X-User-Id", "header_user")
            .with_query_param("user_id", "query_user");

        let identifier = extractor.extract(&context).unwrap();
        // 应该优先从header提取
        assert_eq!(identifier, Identifier::UserId("header_user".to_string()));
    }

    #[test]
    fn test_ip_extractor_from_header() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        let context = RequestContext::new().with_header("X-Forwarded-For", "192.168.1.1");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Ip("192.168.1.1".to_string()));
    }

    #[test]
    fn test_ip_extractor_from_client_ip() {
        let extractor = IpExtractor::new_default();
        let context = RequestContext::new().with_client_ip("10.0.0.1");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Ip("10.0.0.1".to_string()));
    }

    #[test]
    fn test_ip_extractor_multiple_headers() {
        let extractor = IpExtractor::from_headers(vec!["X-Real-IP", "X-Forwarded-For"]);
        let context = RequestContext::new()
            .with_header("X-Forwarded-For", "192.168.1.1")
            .with_header("X-Real-IP", "10.0.0.1");

        let identifier = extractor.extract(&context).unwrap();
        // 应该优先从第一个header提取
        assert_eq!(identifier, Identifier::Ip("10.0.0.1".to_string()));
    }

    #[test]
    fn test_ip_extractor_parse_list() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        let context = RequestContext::new()
            .with_header("X-Forwarded-For", "192.168.1.1, 10.0.0.1, 172.16.0.1");

        let identifier = extractor.extract(&context).unwrap();
        // 应该提取第一个IP
        assert_eq!(identifier, Identifier::Ip("192.168.1.1".to_string()));
    }

    #[test]
    fn test_mac_extractor_from_header() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let context = RequestContext::new().with_header("X-Mac-Address", "00:1A:2B:3C:4D:5E");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Mac("00:1A:2B:3C:4D:5E".to_string()));
    }

    #[test]
    fn test_mac_extractor_validate_format() {
        let extractor = MacExtractor::from_header("X-Mac-Address");

        // 有效的MAC地址
        let context1 = RequestContext::new().with_header("X-Mac-Address", "00:1A:2B:3C:4D:5E");
        assert!(extractor.extract(&context1).is_some());

        // 无效的MAC地址
        let context2 = RequestContext::new().with_header("X-Mac-Address", "invalid");
        assert!(extractor.extract(&context2).is_none());
    }

    #[test]
    fn test_api_key_extractor_from_authorization() {
        let extractor = ApiKeyExtractor::from_authorization_header();
        let context = RequestContext::new().with_header("Authorization", "Bearer my-api-key");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::ApiKey("my-api-key".to_string()));
    }

    #[test]
    fn test_api_key_extractor_from_header() {
        let extractor = ApiKeyExtractor::from_header("X-API-Key");
        let context = RequestContext::new().with_header("X-API-Key", "my-api-key");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::ApiKey("my-api-key".to_string()));
    }

    #[test]
    fn test_device_id_extractor_from_header() {
        let extractor = DeviceIdExtractor::from_header("X-Device-Id");
        let context = RequestContext::new().with_header("X-Device-Id", "device-123");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::DeviceId("device-123".to_string()));
    }

    #[test]
    fn test_composite_extractor() {
        let extractor = CompositeExtractor::new(
            vec![
                Box::new(UserIdExtractor::from_header("X-User-Id")),
                Box::new(IpExtractor::new_default()),
            ],
            true,
        );

        // 应该从第一个提取器提取
        let context1 = RequestContext::new()
            .with_header("X-User-Id", "user123")
            .with_client_ip("10.0.0.1");
        let identifier1 = extractor.extract(&context1).unwrap();
        assert_eq!(identifier1, Identifier::UserId("user123".to_string()));

        // 应该从第二个提取器提取
        let context2 = RequestContext::new().with_client_ip("10.0.0.1");
        let identifier2 = extractor.extract(&context2).unwrap();
        assert_eq!(identifier2, Identifier::Ip("10.0.0.1".to_string()));
    }

    #[test]
    fn test_custom_extractor() {
        let extractor = CustomExtractor::new("MyExtractor", |context| {
            context
                .get_header("X-Custom")
                .map(|value| Identifier::UserId(value.clone()))
        });

        let context = RequestContext::new().with_header("X-Custom", "custom123");
        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("custom123".to_string()));
    }

    // ==================== IP范围测试 ====================

    #[test]
    fn test_ip_range_single() {
        let range: IpRange = "192.168.1.1".parse().unwrap();
        let ip: IpAddr = "192.168.1.1".parse().unwrap();
        assert!(range.contains(&ip));

        let ip2: IpAddr = "192.168.1.2".parse().unwrap();
        assert!(!range.contains(&ip2));
    }

    #[test]
    fn test_ip_range_ipv4_cidr() {
        let range: IpRange = "192.168.1.0/24".parse().unwrap();
        let ip1: IpAddr = "192.168.1.1".parse().unwrap();
        let ip2: IpAddr = "192.168.1.255".parse().unwrap();
        let ip3: IpAddr = "192.168.2.1".parse().unwrap();

        assert!(range.contains(&ip1));
        assert!(range.contains(&ip2));
        assert!(!range.contains(&ip3));
    }

    #[test]
    fn test_ip_range_ipv4_range() {
        let range: IpRange = "192.168.1.1-192.168.1.10".parse().unwrap();
        let ip1: IpAddr = "192.168.1.1".parse().unwrap();
        let ip2: IpAddr = "192.168.1.10".parse().unwrap();
        let ip3: IpAddr = "192.168.1.11".parse().unwrap();

        assert!(range.contains(&ip1));
        assert!(range.contains(&ip2));
        assert!(!range.contains(&ip3));
    }

    #[test]
    fn test_ip_range_invalid() {
        assert!("invalid".parse::<IpRange>().is_err());
        assert!("192.168.1.1/33".parse::<IpRange>().is_err());
        assert!("192.168.1.10-192.168.1.1".parse::<IpRange>().is_err());
    }

    // ==================== 规则匹配器测试 ====================

    #[test]
    fn test_rule_matcher_user_condition() {
        let rule = Rule {
            id: "rule1".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec![
                "user1".to_string(),
                "user2".to_string(),
            ])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        let context1 = RequestContext::new().with_header("X-User-Id", "user1");
        assert!(matcher.matches(&context1).is_some());

        let context2 = RequestContext::new().with_header("X-User-Id", "user3");
        assert!(matcher.matches(&context2).is_none());
    }

    #[test]
    fn test_rule_matcher_wildcard_user() {
        let rule = Rule {
            id: "rule1".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["*".to_string()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        let context = RequestContext::new();
        assert!(matcher.matches(&context).is_some());
    }

    #[test]
    fn test_rule_matcher_ip_condition() {
        let rule = Rule {
            id: "rule1".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::Ip(vec!["192.168.1.0/24".parse().unwrap()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        let context1 = RequestContext::new().with_client_ip("192.168.1.100");
        assert!(matcher.matches(&context1).is_some());

        let context2 = RequestContext::new().with_client_ip("10.0.0.1");
        assert!(matcher.matches(&context2).is_none());
    }

    #[test]
    fn test_rule_matcher_priority() {
        let rule1 = Rule {
            id: "rule1".to_string(),
            name: "Low Priority".to_string(),
            priority: 50,
            condition: Box::new(MatchCondition::User(vec!["*".to_string()])),
            enabled: true,
        };

        let rule2 = Rule {
            id: "rule2".to_string(),
            name: "High Priority".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule1, rule2]);

        let context = RequestContext::new().with_header("X-User-Id", "user1");
        let matched = matcher.matches(&context).unwrap();

        // 应该匹配高优先级的规则
        assert_eq!(matched.id, "rule2");
    }

    #[test]
    fn test_rule_matcher_disabled_rule() {
        let rule = Rule {
            id: "rule1".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: false,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        let context = RequestContext::new().with_header("X-User-Id", "user1");
        assert!(matcher.matches(&context).is_none());
    }

    #[test]
    fn test_rule_matcher_stats() {
        let rule = Rule {
            id: "rule1".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        let context1 = RequestContext::new().with_header("X-User-Id", "user1");
        matcher.matches(&context1);

        let context2 = RequestContext::new().with_header("X-User-Id", "user2");
        matcher.matches(&context2);

        let stats = matcher.stats();
        assert_eq!(stats.total_matches, 1);
        assert_eq!(stats.total_mismatches, 1);
    }

    #[test]
    fn test_rule_matcher_add_remove() {
        let rule1 = Rule {
            id: "rule1".to_string(),
            name: "Rule 1".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: true,
        };

        let mut matcher = RuleMatcher::new(vec![]);
        assert_eq!(matcher.rule_count(), 0);

        matcher.add_rule(rule1);
        assert_eq!(matcher.rule_count(), 1);

        matcher.remove_rule("rule1");
        assert_eq!(matcher.rule_count(), 0);
    }

    #[test]
    fn test_composite_condition_and() {
        let condition = CompositeCondition {
            conditions: vec![
                Box::new(MatchCondition::User(vec!["user1".to_string()])),
                Box::new(MatchCondition::Geo(vec!["US".to_string()])),
            ],
            operator: LogicalOperator::And,
        };

        let context1 = RequestContext::new()
            .with_header("X-User-Id", "user1")
            .with_header("X-Country", "US");
        assert!(condition.evaluate(&context1));

        let context2 = RequestContext::new()
            .with_header("X-User-Id", "user1")
            .with_header("X-Country", "CN");
        assert!(!condition.evaluate(&context2));
    }

    #[test]
    fn test_composite_condition_or() {
        let condition = CompositeCondition {
            conditions: vec![
                Box::new(MatchCondition::User(vec!["user1".to_string()])),
                Box::new(MatchCondition::User(vec!["user2".to_string()])),
            ],
            operator: LogicalOperator::Or,
        };

        let context1 = RequestContext::new().with_header("X-User-Id", "user1");
        assert!(condition.evaluate(&context1));

        let context2 = RequestContext::new().with_header("X-User-Id", "user2");
        assert!(condition.evaluate(&context2));

        let context3 = RequestContext::new().with_header("X-User-Id", "user3");
        assert!(!condition.evaluate(&context3));
    }

    #[test]
    fn test_composite_condition_not() {
        let condition = CompositeCondition {
            conditions: vec![Box::new(MatchCondition::User(vec!["user1".to_string()]))],
            operator: LogicalOperator::Not,
        };

        let context1 = RequestContext::new().with_header("X-User-Id", "user2");
        assert!(condition.evaluate(&context1));

        let context2 = RequestContext::new().with_header("X-User-Id", "user1");
        assert!(!condition.evaluate(&context2));
    }

    #[test]
    fn test_custom_condition() {
        let condition: Box<dyn ConditionEvaluator> = Box::new(MatchCondition::Custom(Arc::new(
            |context: &RequestContext| -> bool {
                context.get_header("X-Special").is_some_and(|v| v == "yes")
            },
        )));

        let context1 = RequestContext::new().with_header("X-Special", "yes");
        assert!(condition.evaluate(&context1));

        let context2 = RequestContext::new().with_header("X-Special", "no");
        assert!(!condition.evaluate(&context2));
    }

    #[test]
    fn test_identifier_key() {
        let user_id = Identifier::UserId("user123".to_string());
        assert_eq!(user_id.key(), "user_id:user123");

        let ip = Identifier::Ip("192.168.1.1".to_string());
        assert_eq!(ip.key(), "ip:192.168.1.1");
    }

    #[test]
    fn test_identifier_type_name() {
        assert_eq!(
            Identifier::UserId("test".to_string()).type_name(),
            "user_id"
        );
        assert_eq!(Identifier::Ip("test".to_string()).type_name(), "ip");
        assert_eq!(Identifier::Mac("test".to_string()).type_name(), "mac");
        assert_eq!(
            Identifier::ApiKey("test".to_string()).type_name(),
            "api_key"
        );
        assert_eq!(
            Identifier::DeviceId("test".to_string()).type_name(),
            "device_id"
        );
    }

    // ==================== 增强测试：标识符提取器边界条件 ====================

    #[test]
    fn test_user_id_extractor_empty_header() {
        let extractor = UserIdExtractor::from_header("X-User-Id");
        let context = RequestContext::new().with_header("X-User-Id", "");

        // 空字符串应该返回 None
        assert!(extractor.extract(&context).is_none());
    }

    #[test]
    fn test_user_id_extractor_empty_query_param() {
        let extractor = UserIdExtractor::from_query_param("user_id");
        let context = RequestContext::new().with_query_param("user_id", "");

        // 空字符串应该返回 None
        assert!(extractor.extract(&context).is_none());
    }

    #[test]
    fn test_user_id_extractor_case_insensitive_header() {
        let extractor = UserIdExtractor::from_header("X-User-Id");
        let context = RequestContext::new().with_header("x-user-id", "user123");

        // Header 名称应该不区分大小写
        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("user123".to_string()));
    }

    #[test]
    fn test_user_id_extractor_fallback_to_query_param() {
        let extractor = UserIdExtractor::new(
            Some("X-User-Id".to_string()),
            Some("user_id".to_string()),
            None,
        );
        let context = RequestContext::new().with_query_param("user_id", "query_user");

        // Header 不存在时应该从 query param 提取
        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("query_user".to_string()));
    }

    #[test]
    fn test_user_id_extractor_builder_pattern() {
        let extractor = UserIdExtractor::builder()
            .header_name("X-User-Id")
            .query_param_name("user_id")
            .default_user_id("guest")
            .build();

        let context = RequestContext::new();
        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("guest".to_string()));
    }

    #[test]
    fn test_user_id_extractor_with_dependencies() {
        let extractor = UserIdExtractor::with_dependencies(
            Some("X-User-Id".to_string()),
            Some("user_id".to_string()),
            Some("default_user".to_string()),
        );

        let context = RequestContext::new();
        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("default_user".to_string()));
    }

    #[test]
    fn test_user_id_extractor_special_characters() {
        let extractor = UserIdExtractor::from_header("X-User-Id");
        let context = RequestContext::new().with_header("X-User-Id", "user@example.com");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(
            identifier,
            Identifier::UserId("user@example.com".to_string())
        );
    }

    #[test]
    fn test_user_id_extractor_unicode() {
        let extractor = UserIdExtractor::from_header("X-User-Id");
        let context = RequestContext::new().with_header("X-User-Id", "用户123");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("用户123".to_string()));
    }

    // ==================== 增强测试：IP 提取器边界条件 ====================

    #[test]
    fn test_ip_extractor_ipv6() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        let context = RequestContext::new()
            .with_header("X-Forwarded-For", "2001:0db8:85a3:0000:0000:8a2e:0370:7334");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(
            identifier,
            Identifier::Ip("2001:0db8:85a3:0000:0000:8a2e:0370:7334".to_string())
        );
    }

    #[test]
    fn test_ip_extractor_ipv6_list() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        let context = RequestContext::new()
            .with_header("X-Forwarded-For", "2001:db8::1, 2001:db8::2, 2001:db8::3");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Ip("2001:db8::1".to_string()));
    }

    #[test]
    fn test_ip_extractor_invalid_ip_with_validation() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        let context = RequestContext::new().with_header("X-Forwarded-For", "invalid-ip");

        // 验证模式下无效 IP 应该返回 None
        assert!(extractor.extract(&context).is_none());
    }

    #[test]
    fn test_ip_extractor_invalid_ip_without_validation() {
        let extractor = IpExtractor::builder()
            .header_name("X-Forwarded-For")
            .validate(false)
            .build();
        let context = RequestContext::new().with_header("X-Forwarded-For", "invalid-ip");

        // 非验证模式下应该返回原始值
        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Ip("invalid-ip".to_string()));
    }

    #[test]
    fn test_ip_extractor_empty_header() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        let context = RequestContext::new().with_header("X-Forwarded-For", "");

        assert!(extractor.extract(&context).is_none());
    }

    #[test]
    fn test_ip_extractor_whitespace_handling() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        let context = RequestContext::new().with_header("X-Forwarded-For", "  192.168.1.1  ");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Ip("192.168.1.1".to_string()));
    }

    #[test]
    fn test_ip_extractor_list_with_spaces() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        let context = RequestContext::new()
            .with_header("X-Forwarded-For", "  192.168.1.1 , 10.0.0.1  , 172.16.0.1 ");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Ip("192.168.1.1".to_string()));
    }

    #[test]
    fn test_ip_extractor_header_priority() {
        let extractor = IpExtractor::from_headers(vec!["X-Real-IP", "X-Forwarded-For"]);
        let context = RequestContext::new()
            .with_header("X-Forwarded-For", "192.168.1.1")
            .with_header("X-Real-IP", "10.0.0.1");

        // 应该优先从第一个 header 提取
        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Ip("10.0.0.1".to_string()));
    }

    #[test]
    fn test_ip_extractor_fallback_to_client_ip() {
        let extractor = IpExtractor::new_default();
        let context = RequestContext::new().with_client_ip("10.0.0.1");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Ip("10.0.0.1".to_string()));
    }

    #[test]
    fn test_ip_extractor_builder_pattern() {
        let extractor = IpExtractor::builder()
            .header_name("X-Real-IP")
            .header_name("X-Forwarded-For")
            .validate(true)
            .build();

        let context = RequestContext::new().with_header("X-Forwarded-For", "192.168.1.1");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Ip("192.168.1.1".to_string()));
    }

    #[test]
    fn test_ip_extractor_with_dependencies() {
        let extractor = IpExtractor::with_dependencies(
            vec!["X-Real-IP".to_string(), "X-Forwarded-For".to_string()],
            true,
        );

        let context = RequestContext::new().with_header("X-Real-IP", "10.0.0.1");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Ip("10.0.0.1".to_string()));
    }

    #[test]
    fn test_ip_extractor_no_ip_available() {
        let extractor = IpExtractor::new_default();
        let context = RequestContext::new();

        assert!(extractor.extract(&context).is_none());
    }

    // ==================== 增强测试：MAC 地址格式验证 ====================

    #[test]
    fn test_mac_extractor_colon_format() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let context = RequestContext::new().with_header("X-Mac-Address", "00:1A:2B:3C:4D:5E");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Mac("00:1A:2B:3C:4D:5E".to_string()));
    }

    #[test]
    fn test_mac_extractor_hyphen_format() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let context = RequestContext::new().with_header("X-Mac-Address", "00-1A-2B-3C-4D-5E");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Mac("00-1A-2B-3C-4D-5E".to_string()));
    }

    #[test]
    fn test_mac_extractor_dot_format() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let context = RequestContext::new().with_header("X-Mac-Address", "001A.2B3C.4D5E");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Mac("001A.2B3C.4D5E".to_string()));
    }

    #[test]
    fn test_mac_extractor_no_separator_format() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let context = RequestContext::new().with_header("X-Mac-Address", "001A2B3C4D5E");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Mac("001A2B3C4D5E".to_string()));
    }

    #[test]
    fn test_mac_extractor_lowercase() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let context = RequestContext::new().with_header("X-Mac-Address", "00:1a:2b:3c:4d:5e");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Mac("00:1a:2b:3c:4d:5e".to_string()));
    }

    #[test]
    fn test_mac_extractor_invalid_too_short() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let context = RequestContext::new().with_header("X-Mac-Address", "00:1A:2B");

        assert!(extractor.extract(&context).is_none());
    }

    #[test]
    fn test_mac_extractor_invalid_too_long() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let context = RequestContext::new().with_header("X-Mac-Address", "00:1A:2B:3C:4D:5E:6F");

        assert!(extractor.extract(&context).is_none());
    }

    #[test]
    fn test_mac_extractor_invalid_characters() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let context = RequestContext::new().with_header("X-Mac-Address", "GG:1A:2B:3C:4D:5E");

        assert!(extractor.extract(&context).is_none());
    }

    #[test]
    fn test_mac_extractor_without_validation() {
        let extractor = MacExtractor::builder()
            .header_name("X-Mac-Address")
            .validate(false)
            .build();
        let context = RequestContext::new().with_header("X-Mac-Address", "invalid-mac");

        // 非验证模式下应该返回原始值
        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Mac("invalid-mac".to_string()));
    }

    #[test]
    fn test_mac_extractor_from_query_param() {
        let extractor = MacExtractor::from_query_param("mac");
        let context = RequestContext::new().with_query_param("mac", "00:1A:2B:3C:4D:5E");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Mac("00:1A:2B:3C:4D:5E".to_string()));
    }

    #[test]
    fn test_mac_extractor_builder_pattern() {
        let extractor = MacExtractor::builder()
            .header_name("X-Mac-Address")
            .query_param_name("mac")
            .validate(true)
            .build();

        let context = RequestContext::new().with_header("X-Mac-Address", "00:1A:2B:3C:4D:5E");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Mac("00:1A:2B:3C:4D:5E".to_string()));
    }

    #[test]
    fn test_mac_extractor_with_dependencies() {
        let extractor =
            MacExtractor::with_dependencies(Some("X-Mac-Address".to_string()), None, true);

        let context = RequestContext::new().with_header("X-Mac-Address", "00:1A:2B:3C:4D:5E");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Mac("00:1A:2B:3C:4D:5E".to_string()));
    }

    #[test]
    fn test_mac_extractor_empty_value() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let context = RequestContext::new().with_header("X-Mac-Address", "");

        assert!(extractor.extract(&context).is_none());
    }

    // ==================== 增强测试：API Key 提取器 ====================

    #[test]
    fn test_api_key_extractor_bearer_prefix() {
        let extractor = ApiKeyExtractor::from_authorization_header();
        let context = RequestContext::new().with_header("Authorization", "Bearer my-secret-key");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::ApiKey("my-secret-key".to_string()));
    }

    #[test]
    fn test_api_key_extractor_custom_prefix() {
        let extractor = ApiKeyExtractor::builder()
            .header_name("Authorization")
            .prefix("Token ")
            .build();
        let context = RequestContext::new().with_header("Authorization", "Token my-token-key");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::ApiKey("my-token-key".to_string()));
    }

    #[test]
    fn test_api_key_extractor_no_prefix() {
        let extractor = ApiKeyExtractor::from_header("X-API-Key");
        let context = RequestContext::new().with_header("X-API-Key", "raw-api-key");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::ApiKey("raw-api-key".to_string()));
    }

    #[test]
    fn test_api_key_extractor_with_whitespace() {
        let extractor = ApiKeyExtractor::from_authorization_header();
        let context =
            RequestContext::new().with_header("Authorization", "Bearer   my-key-with-spaces  ");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(
            identifier,
            Identifier::ApiKey("my-key-with-spaces".to_string())
        );
    }

    #[test]
    fn test_api_key_extractor_empty_after_prefix() {
        let extractor = ApiKeyExtractor::from_authorization_header();
        let context = RequestContext::new().with_header("Authorization", "Bearer ");

        assert!(extractor.extract(&context).is_none());
    }

    #[test]
    fn test_api_key_extractor_missing_prefix() {
        let extractor = ApiKeyExtractor::from_authorization_header();
        let context = RequestContext::new().with_header("Authorization", "my-key-without-bearer");

        // 没有 Bearer 前缀，应该返回 None
        assert!(extractor.extract(&context).is_none());
    }

    #[test]
    fn test_api_key_extractor_case_sensitive_prefix() {
        let extractor = ApiKeyExtractor::from_authorization_header();
        let context = RequestContext::new().with_header("Authorization", "bearer my-key");

        // 前缀区分大小写
        assert!(extractor.extract(&context).is_none());
    }

    #[test]
    fn test_api_key_extractor_builder_pattern() {
        let extractor = ApiKeyExtractor::builder()
            .header_name("X-API-Key")
            .prefix("Key ")
            .build();

        let context = RequestContext::new().with_header("X-API-Key", "Key my-custom-key");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::ApiKey("my-custom-key".to_string()));
    }

    #[test]
    fn test_api_key_extractor_with_dependencies() {
        let extractor =
            ApiKeyExtractor::with_dependencies(Some("X-API-Key".to_string()), None, None);

        let context = RequestContext::new().with_header("X-API-Key", "my-api-key");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::ApiKey("my-api-key".to_string()));
    }

    #[test]
    fn test_api_key_extractor_empty_header() {
        let extractor = ApiKeyExtractor::from_header("X-API-Key");
        let context = RequestContext::new().with_header("X-API-Key", "");

        assert!(extractor.extract(&context).is_none());
    }

    // ==================== 增强测试：DeviceId 提取器 ====================

    #[test]
    fn test_device_id_extractor_from_header_enhanced() {
        let extractor = DeviceIdExtractor::from_header("X-Device-Id");
        let context = RequestContext::new().with_header("X-Device-Id", "device-abc-123");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(
            identifier,
            Identifier::DeviceId("device-abc-123".to_string())
        );
    }

    #[test]
    fn test_device_id_extractor_from_query_param() {
        let extractor = DeviceIdExtractor::from_query_param("device_id");
        let context = RequestContext::new().with_query_param("device_id", "device-xyz-789");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(
            identifier,
            Identifier::DeviceId("device-xyz-789".to_string())
        );
    }

    #[test]
    fn test_device_id_extractor_header_priority() {
        let extractor = DeviceIdExtractor::new(
            Some("X-Device-Id".to_string()),
            Some("device_id".to_string()),
        );
        let context = RequestContext::new()
            .with_header("X-Device-Id", "header-device")
            .with_query_param("device_id", "query-device");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(
            identifier,
            Identifier::DeviceId("header-device".to_string())
        );
    }

    #[test]
    fn test_device_id_extractor_fallback_to_query_param() {
        let extractor = DeviceIdExtractor::new(
            Some("X-Device-Id".to_string()),
            Some("device_id".to_string()),
        );
        let context = RequestContext::new().with_query_param("device_id", "query-device");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::DeviceId("query-device".to_string()));
    }

    #[test]
    fn test_device_id_extractor_empty_value() {
        let extractor = DeviceIdExtractor::from_header("X-Device-Id");
        let context = RequestContext::new().with_header("X-Device-Id", "");

        assert!(extractor.extract(&context).is_none());
    }

    #[test]
    fn test_device_id_extractor_builder_pattern() {
        let extractor = DeviceIdExtractor::builder()
            .header_name("X-Device-Id")
            .query_param_name("device_id")
            .build();

        let context = RequestContext::new().with_header("X-Device-Id", "my-device");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::DeviceId("my-device".to_string()));
    }

    #[test]
    fn test_device_id_extractor_with_dependencies() {
        let extractor = DeviceIdExtractor::with_dependencies(Some("X-Device-Id".to_string()), None);

        let context = RequestContext::new().with_header("X-Device-Id", "my-device");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::DeviceId("my-device".to_string()));
    }

    #[test]
    fn test_device_id_extractor_no_device_id() {
        let extractor = DeviceIdExtractor::from_header("X-Device-Id");
        let context = RequestContext::new();

        assert!(extractor.extract(&context).is_none());
    }

    // ==================== 增强测试：组合提取器 ====================

    #[test]
    fn test_composite_extractor_priority_order() {
        let extractor = CompositeExtractor::new(
            vec![
                Box::new(UserIdExtractor::from_header("X-User-Id")),
                Box::new(ApiKeyExtractor::from_header("X-API-Key")),
                Box::new(IpExtractor::new_default()),
            ],
            false,
        );

        // 第一个提取器成功
        let context1 = RequestContext::new()
            .with_header("X-User-Id", "user123")
            .with_header("X-API-Key", "key456")
            .with_client_ip("10.0.0.1");
        let identifier1 = extractor.extract(&context1).unwrap();
        assert_eq!(identifier1, Identifier::UserId("user123".to_string()));

        // 第二个提取器成功
        let context2 = RequestContext::new()
            .with_header("X-API-Key", "key456")
            .with_client_ip("10.0.0.1");
        let identifier2 = extractor.extract(&context2).unwrap();
        assert_eq!(identifier2, Identifier::ApiKey("key456".to_string()));

        // 第三个提取器成功
        let context3 = RequestContext::new().with_client_ip("10.0.0.1");
        let identifier3 = extractor.extract(&context3).unwrap();
        assert_eq!(identifier3, Identifier::Ip("10.0.0.1".to_string()));
    }

    #[test]
    fn test_composite_extractor_fallback_enabled() {
        let extractor = CompositeExtractor::new(
            vec![Box::new(UserIdExtractor::from_header("X-User-Id"))],
            true,
        );

        // 没有匹配的提取器，但启用了 fallback，应该使用 client_ip
        let context = RequestContext::new().with_client_ip("10.0.0.1");
        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Ip("10.0.0.1".to_string()));
    }

    #[test]
    fn test_composite_extractor_fallback_disabled() {
        let extractor = CompositeExtractor::new(
            vec![Box::new(UserIdExtractor::from_header("X-User-Id"))],
            false,
        );

        // 没有匹配的提取器，禁用 fallback
        let context = RequestContext::new().with_client_ip("10.0.0.1");
        assert!(extractor.extract(&context).is_none());
    }

    #[test]
    fn test_composite_extractor_fallback_invalid_ip() {
        let extractor = CompositeExtractor::new(
            vec![Box::new(UserIdExtractor::from_header("X-User-Id"))],
            true,
        );

        // client_ip 无效
        let context = RequestContext::new().with_client_ip("invalid-ip");
        assert!(extractor.extract(&context).is_none());
    }

    #[test]
    fn test_composite_extractor_builder_pattern() {
        let extractor = CompositeExtractor::builder()
            .add_extractor(Box::new(UserIdExtractor::from_header("X-User-Id")))
            .add_extractor(Box::new(IpExtractor::new_default()))
            .with_fallback(true)
            .build();

        let context = RequestContext::new().with_client_ip("10.0.0.1");
        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Ip("10.0.0.1".to_string()));
    }

    #[test]
    fn test_composite_extractor_with_dependencies() {
        let extractor = CompositeExtractor::with_dependencies(
            vec![Box::new(UserIdExtractor::from_header("X-User-Id"))],
            false,
        );

        let context = RequestContext::new().with_header("X-User-Id", "user123");
        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("user123".to_string()));
    }

    #[test]
    fn test_composite_extractor_add_extractor() {
        let extractor = CompositeExtractor::new(vec![], false)
            .add_extractor(Box::new(UserIdExtractor::from_header("X-User-Id")));

        let context = RequestContext::new().with_header("X-User-Id", "user123");
        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("user123".to_string()));
    }

    #[test]
    fn test_composite_extractor_empty_extractors() {
        let extractor = CompositeExtractor::new(vec![], false);

        let context = RequestContext::new().with_client_ip("10.0.0.1");
        assert!(extractor.extract(&context).is_none());
    }

    // ==================== 增强测试：自定义提取器 ====================

    #[test]
    fn test_custom_extractor_with_closure() {
        let extractor = CustomExtractor::new("MyExtractor", |context| {
            context
                .get_header("X-Custom-Id")
                .map(|id| Identifier::UserId(id.clone()))
        });

        let context = RequestContext::new().with_header("X-Custom-Id", "custom-123");
        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("custom-123".to_string()));
    }

    #[test]
    fn test_custom_extractor_returns_none() {
        let extractor = CustomExtractor::new("MyExtractor", |context| {
            context
                .get_header("X-Custom-Id")
                .map(|id| Identifier::UserId(id.clone()))
        });

        let context = RequestContext::new();
        assert!(extractor.extract(&context).is_none());
    }

    #[test]
    fn test_custom_extractor_complex_logic() {
        let extractor = CustomExtractor::new("ComplexExtractor", |context| {
            // 组合多个条件
            let user_id = context.get_header("X-User-Id");
            let tenant = context.get_header("X-Tenant");

            match (user_id, tenant) {
                (Some(uid), Some(t)) => Some(Identifier::UserId(format!("{}:{}", t, uid))),
                (Some(uid), None) => Some(Identifier::UserId(uid.clone())),
                _ => None,
            }
        });

        let context = RequestContext::new()
            .with_header("X-User-Id", "user123")
            .with_header("X-Tenant", "tenant1");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(
            identifier,
            Identifier::UserId("tenant1:user123".to_string())
        );
    }

    #[test]
    fn test_custom_extractor_name() {
        let extractor = CustomExtractor::new("MyCustomExtractor", |_context| None);
        assert_eq!(extractor.name(), "MyCustomExtractor");
    }

    // ==================== 增强测试：IP 范围匹配 ====================

    #[test]
    fn test_ip_range_ipv4_cidr_boundary() {
        // /24 网络边界测试
        let range: IpRange = "192.168.1.0/24".parse().unwrap();

        // 网络地址
        let network: IpAddr = "192.168.1.0".parse().unwrap();
        assert!(range.contains(&network));

        // 广播地址
        let broadcast: IpAddr = "192.168.1.255".parse().unwrap();
        assert!(range.contains(&broadcast));

        // 网络外地址
        let outside1: IpAddr = "192.168.0.255".parse().unwrap();
        assert!(!range.contains(&outside1));

        let outside2: IpAddr = "192.168.2.0".parse().unwrap();
        assert!(!range.contains(&outside2));
    }

    #[test]
    fn test_ip_range_ipv4_cidr_32() {
        // /32 单主机
        let range: IpRange = "192.168.1.1/32".parse().unwrap();

        let ip: IpAddr = "192.168.1.1".parse().unwrap();
        assert!(range.contains(&ip));

        let ip2: IpAddr = "192.168.1.2".parse().unwrap();
        assert!(!range.contains(&ip2));
    }

    #[test]
    fn test_ip_range_ipv4_cidr_0() {
        // /0 所有 IPv4 地址
        let range: IpRange = "0.0.0.0/0".parse().unwrap();

        let ip1: IpAddr = "192.168.1.1".parse().unwrap();
        assert!(range.contains(&ip1));

        let ip2: IpAddr = "10.0.0.1".parse().unwrap();
        assert!(range.contains(&ip2));

        let ip3: IpAddr = "255.255.255.255".parse().unwrap();
        assert!(range.contains(&ip3));
    }

    #[test]
    fn test_ip_range_ipv6_cidr() {
        let range: IpRange = "2001:db8::/32".parse().unwrap();

        let ip1: IpAddr = "2001:db8::1".parse().unwrap();
        assert!(range.contains(&ip1));

        let ip2: IpAddr = "2001:db8:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap();
        assert!(range.contains(&ip2));

        let ip3: IpAddr = "2001:db9::1".parse().unwrap();
        assert!(!range.contains(&ip3));
    }

    #[test]
    fn test_ip_range_ipv6_cidr_128() {
        let range: IpRange = "2001:db8::1/128".parse().unwrap();

        let ip1: IpAddr = "2001:db8::1".parse().unwrap();
        assert!(range.contains(&ip1));

        let ip2: IpAddr = "2001:db8::2".parse().unwrap();
        assert!(!range.contains(&ip2));
    }

    #[test]
    fn test_ip_range_ipv4_range_boundary() {
        let range: IpRange = "192.168.1.1-192.168.1.10".parse().unwrap();

        // 起始地址
        let start: IpAddr = "192.168.1.1".parse().unwrap();
        assert!(range.contains(&start));

        // 结束地址
        let end: IpAddr = "192.168.1.10".parse().unwrap();
        assert!(range.contains(&end));

        // 中间地址
        let middle: IpAddr = "192.168.1.5".parse().unwrap();
        assert!(range.contains(&middle));

        // 范围外
        let outside1: IpAddr = "192.168.1.0".parse().unwrap();
        assert!(!range.contains(&outside1));

        let outside2: IpAddr = "192.168.1.11".parse().unwrap();
        assert!(!range.contains(&outside2));
    }

    #[test]
    fn test_ip_range_ipv6_not_supported_in_range_format() {
        // IPv6 范围格式不支持，应该返回错误
        let result = "2001:db8::1-2001:db8::10".parse::<IpRange>();
        assert!(result.is_err());
    }

    #[test]
    fn test_ip_range_ipv4_vs_ipv6() {
        let range: IpRange = "192.168.1.0/24".parse().unwrap();

        // IPv4 范围不应该匹配 IPv6 地址
        let ipv6: IpAddr = "::ffff:192.168.1.1".parse().unwrap();
        assert!(!range.contains(&ipv6));
    }

    #[test]
    fn test_ip_range_ipv6_vs_ipv4() {
        let range: IpRange = "2001:db8::/32".parse().unwrap();

        // IPv6 范围不应该匹配 IPv4 地址
        let ipv4: IpAddr = "192.168.1.1".parse().unwrap();
        assert!(!range.contains(&ipv4));
    }

    #[test]
    fn test_ip_range_parse_invalid_cidr_prefix() {
        // IPv4 前缀超过 32
        assert!("192.168.1.0/33".parse::<IpRange>().is_err());

        // IPv6 前缀超过 128
        assert!("2001:db8::/129".parse::<IpRange>().is_err());
    }

    #[test]
    fn test_ip_range_parse_invalid_ip() {
        assert!("invalid".parse::<IpRange>().is_err());
        assert!("256.256.256.256".parse::<IpRange>().is_err());
    }

    #[test]
    fn test_ip_range_parse_invalid_range() {
        // 起始 IP 大于结束 IP
        assert!("192.168.1.10-192.168.1.1".parse::<IpRange>().is_err());
    }

    #[test]
    fn test_ip_range_parse_invalid_format() {
        // 无效的 CIDR 格式
        assert!("192.168.1.0/".parse::<IpRange>().is_err());
        assert!("/24".parse::<IpRange>().is_err());

        // 无效的范围格式
        assert!("192.168.1.1-".parse::<IpRange>().is_err());
        assert!("-192.168.1.10".parse::<IpRange>().is_err());
    }

    // ==================== 增强测试：规则匹配引擎 ====================

    #[test]
    fn test_rule_matcher_geo_condition() {
        let rule = Rule {
            id: "geo_rule".to_string(),
            name: "Geo Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::Geo(vec![
                "US".to_string(),
                "CN".to_string(),
            ])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        // 匹配的国家
        let context1 = RequestContext::new().with_header("X-Country", "US");
        assert!(matcher.matches(&context1).is_some());

        let context2 = RequestContext::new().with_header("X-Country", "CN");
        assert!(matcher.matches(&context2).is_some());

        // 不匹配的国家
        let context3 = RequestContext::new().with_header("X-Country", "JP");
        assert!(matcher.matches(&context3).is_none());
    }

    #[test]
    fn test_rule_matcher_geo_wildcard() {
        let rule = Rule {
            id: "geo_wildcard".to_string(),
            name: "Geo Wildcard".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::Geo(vec!["*".to_string()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        // 通配符匹配任何国家（包括没有设置国家头）
        let context = RequestContext::new();
        assert!(matcher.matches(&context).is_some());
    }

    #[test]
    fn test_rule_matcher_device_condition() {
        let rule = Rule {
            id: "device_rule".to_string(),
            name: "Device Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::Device(vec![
                "mobile".to_string(),
                "tablet".to_string(),
            ])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        // 匹配的设备类型
        let context1 = RequestContext::new().with_header("X-Device-Type", "mobile");
        assert!(matcher.matches(&context1).is_some());

        let context2 = RequestContext::new().with_header("X-Device-Type", "tablet");
        assert!(matcher.matches(&context2).is_some());

        // 不匹配的设备类型
        let context3 = RequestContext::new().with_header("X-Device-Type", "desktop");
        assert!(matcher.matches(&context3).is_none());
    }

    #[test]
    fn test_rule_matcher_device_wildcard() {
        let rule = Rule {
            id: "device_wildcard".to_string(),
            name: "Device Wildcard".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::Device(vec!["*".to_string()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        let context = RequestContext::new();
        assert!(matcher.matches(&context).is_some());
    }

    #[test]
    fn test_rule_matcher_api_version_condition() {
        let rule = Rule {
            id: "api_version_rule".to_string(),
            name: "API Version Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::ApiVersion(vec![
                "v1".to_string(),
                "v2".to_string(),
            ])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        // 匹配的 API 版本
        let context1 = RequestContext::new().with_header("X-API-Version", "v1");
        assert!(matcher.matches(&context1).is_some());

        let context2 = RequestContext::new().with_header("X-API-Version", "v2");
        assert!(matcher.matches(&context2).is_some());

        // 不匹配的 API 版本
        let context3 = RequestContext::new().with_header("X-API-Version", "v3");
        assert!(matcher.matches(&context3).is_none());
    }

    #[test]
    fn test_rule_matcher_api_version_wildcard() {
        let rule = Rule {
            id: "api_version_wildcard".to_string(),
            name: "API Version Wildcard".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::ApiVersion(vec!["*".to_string()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        let context = RequestContext::new();
        assert!(matcher.matches(&context).is_some());
    }

    #[test]
    fn test_rule_matcher_ip_single() {
        let rule = Rule {
            id: "ip_single".to_string(),
            name: "IP Single".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::Ip(vec!["192.168.1.100".parse().unwrap()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        // 匹配的 IP
        let context1 = RequestContext::new().with_client_ip("192.168.1.100");
        assert!(matcher.matches(&context1).is_some());

        // 不匹配的 IP
        let context2 = RequestContext::new().with_client_ip("192.168.1.101");
        assert!(matcher.matches(&context2).is_none());
    }

    #[test]
    fn test_rule_matcher_ip_cidr() {
        let rule = Rule {
            id: "ip_cidr".to_string(),
            name: "IP CIDR".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::Ip(vec![
                "10.0.0.0/8".parse().unwrap(),
                "192.168.0.0/16".parse().unwrap(),
            ])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        // 匹配 10.0.0.0/8
        let context1 = RequestContext::new().with_client_ip("10.255.255.255");
        assert!(matcher.matches(&context1).is_some());

        // 匹配 192.168.0.0/16
        let context2 = RequestContext::new().with_client_ip("192.168.255.255");
        assert!(matcher.matches(&context2).is_some());

        // 不匹配
        let context3 = RequestContext::new().with_client_ip("172.16.0.1");
        assert!(matcher.matches(&context3).is_none());
    }

    #[test]
    fn test_rule_matcher_ip_range() {
        let rule = Rule {
            id: "ip_range".to_string(),
            name: "IP Range".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::Ip(vec!["172.16.0.1-172.16.0.100"
                .parse()
                .unwrap()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        // 范围内
        let context1 = RequestContext::new().with_client_ip("172.16.0.50");
        assert!(matcher.matches(&context1).is_some());

        // 范围外
        let context2 = RequestContext::new().with_client_ip("172.16.0.101");
        assert!(matcher.matches(&context2).is_none());
    }

    #[test]
    fn test_rule_matcher_ip_invalid_client_ip() {
        let rule = Rule {
            id: "ip_rule".to_string(),
            name: "IP Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::Ip(vec!["192.168.1.0/24".parse().unwrap()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        // 无效的 client_ip
        let context = RequestContext::new().with_client_ip("invalid-ip");
        assert!(matcher.matches(&context).is_none());
    }

    #[test]
    fn test_rule_matcher_no_client_ip() {
        let rule = Rule {
            id: "ip_rule".to_string(),
            name: "IP Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::Ip(vec!["192.168.1.0/24".parse().unwrap()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        // 没有 client_ip
        let context = RequestContext::new();
        assert!(matcher.matches(&context).is_none());
    }

    #[test]
    fn test_rule_matcher_multiple_rules_priority() {
        let rule1 = Rule {
            id: "low_priority".to_string(),
            name: "Low Priority".to_string(),
            priority: 10,
            condition: Box::new(MatchCondition::User(vec!["*".to_string()])),
            enabled: true,
        };

        let rule2 = Rule {
            id: "medium_priority".to_string(),
            name: "Medium Priority".to_string(),
            priority: 50,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: true,
        };

        let rule3 = Rule {
            id: "high_priority".to_string(),
            name: "High Priority".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule1, rule2, rule3]);

        let context = RequestContext::new().with_header("X-User-Id", "user1");
        let matched = matcher.matches(&context).unwrap();

        // 应该匹配最高优先级的规则
        assert_eq!(matched.id, "high_priority");
    }

    #[test]
    fn test_rule_matcher_match_all() {
        let rule1 = Rule {
            id: "rule1".to_string(),
            name: "Rule 1".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: true,
        };

        let rule2 = Rule {
            id: "rule2".to_string(),
            name: "Rule 2".to_string(),
            priority: 50,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: true,
        };

        let rule3 = Rule {
            id: "rule3".to_string(),
            name: "Rule 3".to_string(),
            priority: 25,
            condition: Box::new(MatchCondition::User(vec!["*".to_string()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule1, rule2, rule3]);

        let context = RequestContext::new().with_header("X-User-Id", "user1");
        let all_matches = matcher.match_all(&context);

        // 应该匹配所有三个规则
        assert_eq!(all_matches.len(), 3);
    }

    #[test]
    fn test_rule_matcher_match_all_with_disabled() {
        let rule1 = Rule {
            id: "rule1".to_string(),
            name: "Rule 1".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: true,
        };

        let rule2 = Rule {
            id: "rule2".to_string(),
            name: "Rule 2".to_string(),
            priority: 50,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: false, // 禁用
        };

        let matcher = RuleMatcher::new(vec![rule1, rule2]);

        let context = RequestContext::new().with_header("X-User-Id", "user1");
        let all_matches = matcher.match_all(&context);

        // 只应该匹配启用的规则
        assert_eq!(all_matches.len(), 1);
        assert_eq!(all_matches[0].id, "rule1");
    }

    #[test]
    fn test_rule_matcher_builder_pattern() {
        let rule1 = Rule {
            id: "rule1".to_string(),
            name: "Rule 1".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: true,
        };

        let rule2 = Rule {
            id: "rule2".to_string(),
            name: "Rule 2".to_string(),
            priority: 50,
            condition: Box::new(MatchCondition::User(vec!["user2".to_string()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule1, rule2]);

        let context = RequestContext::new().with_header("X-User-Id", "user1");
        assert!(matcher.matches(&context).is_some());
    }

    #[test]
    fn test_rule_matcher_with_dependencies() {
        let rule = Rule {
            id: "rule1".to_string(),
            name: "Rule 1".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: true,
        };

        let matcher = RuleMatcher::with_dependencies(vec![rule]);

        let context = RequestContext::new().with_header("X-User-Id", "user1");
        assert!(matcher.matches(&context).is_some());
    }

    #[test]
    fn test_rule_matcher_reset_stats() {
        let rule = Rule {
            id: "rule1".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        let context = RequestContext::new().with_header("X-User-Id", "user1");
        matcher.matches(&context);

        let stats = matcher.stats();
        assert_eq!(stats.total_matches, 1);

        matcher.reset_stats();

        let stats = matcher.stats();
        assert_eq!(stats.total_matches, 0);
    }

    // ==================== 增强测试：复合条件 ====================

    #[test]
    fn test_composite_condition_and_all_match() {
        let condition = CompositeCondition {
            conditions: vec![
                Box::new(MatchCondition::User(vec!["user1".to_string()])),
                Box::new(MatchCondition::Geo(vec!["US".to_string()])),
                Box::new(MatchCondition::Device(vec!["mobile".to_string()])),
            ],
            operator: LogicalOperator::And,
        };

        let context = RequestContext::new()
            .with_header("X-User-Id", "user1")
            .with_header("X-Country", "US")
            .with_header("X-Device-Type", "mobile");

        assert!(condition.evaluate(&context));
    }

    #[test]
    fn test_composite_condition_and_one_fails() {
        let condition = CompositeCondition {
            conditions: vec![
                Box::new(MatchCondition::User(vec!["user1".to_string()])),
                Box::new(MatchCondition::Geo(vec!["US".to_string()])),
            ],
            operator: LogicalOperator::And,
        };

        let context = RequestContext::new()
            .with_header("X-User-Id", "user1")
            .with_header("X-Country", "CN"); // 不匹配

        assert!(!condition.evaluate(&context));
    }

    #[test]
    fn test_composite_condition_and_empty_conditions() {
        let condition = CompositeCondition {
            conditions: vec![],
            operator: LogicalOperator::And,
        };

        let context = RequestContext::new();
        // 空条件的 AND 应该返回 true
        assert!(condition.evaluate(&context));
    }

    #[test]
    fn test_composite_condition_or_all_match() {
        let condition = CompositeCondition {
            conditions: vec![
                Box::new(MatchCondition::User(vec!["user1".to_string()])),
                Box::new(MatchCondition::User(vec!["user2".to_string()])),
            ],
            operator: LogicalOperator::Or,
        };

        let context = RequestContext::new().with_header("X-User-Id", "user1");
        assert!(condition.evaluate(&context));
    }

    #[test]
    fn test_composite_condition_or_one_matches() {
        let condition = CompositeCondition {
            conditions: vec![
                Box::new(MatchCondition::User(vec!["user1".to_string()])),
                Box::new(MatchCondition::User(vec!["user2".to_string()])),
            ],
            operator: LogicalOperator::Or,
        };

        let context = RequestContext::new().with_header("X-User-Id", "user2");
        assert!(condition.evaluate(&context));
    }

    #[test]
    fn test_composite_condition_or_none_match() {
        let condition = CompositeCondition {
            conditions: vec![
                Box::new(MatchCondition::User(vec!["user1".to_string()])),
                Box::new(MatchCondition::User(vec!["user2".to_string()])),
            ],
            operator: LogicalOperator::Or,
        };

        let context = RequestContext::new().with_header("X-User-Id", "user3");
        assert!(!condition.evaluate(&context));
    }

    #[test]
    fn test_composite_condition_or_empty_conditions() {
        let condition = CompositeCondition {
            conditions: vec![],
            operator: LogicalOperator::Or,
        };

        let context = RequestContext::new();
        // 空条件的 OR 应该返回 false
        assert!(!condition.evaluate(&context));
    }

    #[test]
    fn test_composite_condition_not_matches() {
        let condition = CompositeCondition {
            conditions: vec![Box::new(MatchCondition::User(vec!["user1".to_string()]))],
            operator: LogicalOperator::Not,
        };

        // 不匹配 user1，所以 NOT 返回 true
        let context = RequestContext::new().with_header("X-User-Id", "user2");
        assert!(condition.evaluate(&context));
    }

    #[test]
    fn test_composite_condition_not_fails() {
        let condition = CompositeCondition {
            conditions: vec![Box::new(MatchCondition::User(vec!["user1".to_string()]))],
            operator: LogicalOperator::Not,
        };

        // 匹配 user1，所以 NOT 返回 false
        let context = RequestContext::new().with_header("X-User-Id", "user1");
        assert!(!condition.evaluate(&context));
    }

    #[test]
    fn test_composite_condition_nested() {
        // (User == user1 AND Geo == US) OR (User == user2)
        let inner_and = CompositeCondition {
            conditions: vec![
                Box::new(MatchCondition::User(vec!["user1".to_string()])),
                Box::new(MatchCondition::Geo(vec!["US".to_string()])),
            ],
            operator: LogicalOperator::And,
        };

        let outer_or = CompositeCondition {
            conditions: vec![
                Box::new(inner_and),
                Box::new(MatchCondition::User(vec!["user2".to_string()])),
            ],
            operator: LogicalOperator::Or,
        };

        // user1 + US 应该匹配
        let context1 = RequestContext::new()
            .with_header("X-User-Id", "user1")
            .with_header("X-Country", "US");
        assert!(outer_or.evaluate(&context1));

        // user2 应该匹配
        let context2 = RequestContext::new().with_header("X-User-Id", "user2");
        assert!(outer_or.evaluate(&context2));

        // user1 + CN 不应该匹配
        let context3 = RequestContext::new()
            .with_header("X-User-Id", "user1")
            .with_header("X-Country", "CN");
        assert!(!outer_or.evaluate(&context3));

        // user3 不应该匹配
        let context4 = RequestContext::new().with_header("X-User-Id", "user3");
        assert!(!outer_or.evaluate(&context4));
    }

    #[test]
    fn test_composite_condition_description() {
        let condition = CompositeCondition {
            conditions: vec![
                Box::new(MatchCondition::User(vec!["user1".to_string()])),
                Box::new(MatchCondition::Geo(vec!["US".to_string()])),
            ],
            operator: LogicalOperator::And,
        };

        assert_eq!(condition.description(), "AND (2)");
    }

    // ==================== 增强测试：自定义条件 ====================

    #[test]
    fn test_custom_condition_with_context() {
        let condition: Box<dyn ConditionEvaluator> = Box::new(MatchCondition::Custom(Arc::new(
            |context: &RequestContext| -> bool {
                // 检查路径是否以 /api/ 开头
                context.path.starts_with("/api/")
            },
        )));

        let context1 = RequestContext::new().with_path("/api/users");
        assert!(condition.evaluate(&context1));

        let context2 = RequestContext::new().with_path("/web/page");
        assert!(!condition.evaluate(&context2));
    }

    #[test]
    fn test_custom_condition_with_method() {
        let condition: Box<dyn ConditionEvaluator> = Box::new(MatchCondition::Custom(Arc::new(
            |context: &RequestContext| -> bool {
                // 只匹配 POST 和 PUT 方法
                context.method == "POST" || context.method == "PUT"
            },
        )));

        let mut context1 = RequestContext::new();
        context1.method = "POST".to_string();
        assert!(condition.evaluate(&context1));

        let mut context2 = RequestContext::new();
        context2.method = "PUT".to_string();
        assert!(condition.evaluate(&context2));

        let mut context3 = RequestContext::new();
        context3.method = "GET".to_string();
        assert!(!condition.evaluate(&context3));
    }

    #[test]
    fn test_custom_condition_complex() {
        let condition: Box<dyn ConditionEvaluator> = Box::new(MatchCondition::Custom(Arc::new(
            |context: &RequestContext| -> bool {
                // 复杂条件：管理员用户或来自内网的请求
                let is_admin = context.get_header("X-Role").is_some_and(|r| r == "admin");

                let is_internal = context
                    .client_ip
                    .as_ref()
                    .is_some_and(|ip| ip.starts_with("10.") || ip.starts_with("192.168."));

                is_admin || is_internal
            },
        )));

        // 管理员用户
        let context1 = RequestContext::new().with_header("X-Role", "admin");
        assert!(condition.evaluate(&context1));

        // 内网 IP
        let context2 = RequestContext::new().with_client_ip("10.0.0.1");
        assert!(condition.evaluate(&context2));

        // 普通用户 + 外网 IP
        let context3 = RequestContext::new()
            .with_header("X-Role", "user")
            .with_client_ip("8.8.8.8");
        assert!(!condition.evaluate(&context3));
    }

    #[test]
    fn test_custom_condition_description() {
        let condition: Box<dyn ConditionEvaluator> =
            Box::new(MatchCondition::Custom(Arc::new(|_| true)));

        assert_eq!(condition.description(), "Custom condition");
    }

    // ==================== 增强测试：Identifier ====================

    #[test]
    fn test_identifier_as_str() {
        assert_eq!(
            Identifier::UserId("user123".to_string()).as_str(),
            "user123"
        );
        assert_eq!(
            Identifier::Ip("192.168.1.1".to_string()).as_str(),
            "192.168.1.1"
        );
        assert_eq!(
            Identifier::Mac("00:1A:2B:3C:4D:5E".to_string()).as_str(),
            "00:1A:2B:3C:4D:5E"
        );
        assert_eq!(Identifier::ApiKey("key123".to_string()).as_str(), "key123");
        assert_eq!(
            Identifier::DeviceId("device123".to_string()).as_str(),
            "device123"
        );
    }

    #[test]
    fn test_identifier_key_format() {
        assert_eq!(
            Identifier::UserId("user123".to_string()).key(),
            "user_id:user123"
        );
        assert_eq!(
            Identifier::Ip("192.168.1.1".to_string()).key(),
            "ip:192.168.1.1"
        );
        assert_eq!(
            Identifier::Mac("00:1A:2B:3C:4D:5E".to_string()).key(),
            "mac:00:1A:2B:3C:4D:5E"
        );
        assert_eq!(
            Identifier::ApiKey("key123".to_string()).key(),
            "api_key:key123"
        );
        assert_eq!(
            Identifier::DeviceId("device123".to_string()).key(),
            "device_id:device123"
        );
    }

    #[test]
    fn test_identifier_equality() {
        let id1 = Identifier::UserId("user123".to_string());
        let id2 = Identifier::UserId("user123".to_string());
        let id3 = Identifier::UserId("user456".to_string());
        let id4 = Identifier::Ip("user123".to_string());

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert_ne!(id1, id4);
    }

    #[test]
    fn test_identifier_hash() {
        use ahash::AHashSet;

        let mut set = AHashSet::new();
        set.insert(Identifier::UserId("user123".to_string()));
        set.insert(Identifier::UserId("user123".to_string())); // 重复
        set.insert(Identifier::UserId("user456".to_string()));

        assert_eq!(set.len(), 2);
    }

    // ==================== 增强测试：RequestContext ====================

    #[test]
    fn test_request_context_new() {
        let context = RequestContext::new();

        assert!(context.user_id.is_none());
        assert!(context.ip.is_none());
        assert!(context.mac.is_none());
        assert!(context.device_id.is_none());
        assert!(context.api_key.is_none());
        assert!(context.headers.is_empty());
        assert!(context.path.is_empty());
        assert!(context.method.is_empty());
        assert!(context.client_ip.is_none());
        assert!(context.query_params.is_empty());
    }

    #[test]
    fn test_request_context_default() {
        let context = RequestContext::default();

        assert!(context.user_id.is_none());
    }

    #[test]
    fn test_request_context_with_header() {
        let context = RequestContext::new()
            .with_header("X-User-Id", "user123")
            .with_header("X-Custom", "value");

        assert_eq!(
            context.get_header("X-User-Id"),
            Some(&"user123".to_string())
        );
        assert_eq!(
            context.get_header("x-user-id"),
            Some(&"user123".to_string())
        ); // 不区分大小写
        assert_eq!(context.get_header("X-Custom"), Some(&"value".to_string()));
    }

    #[test]
    fn test_request_context_with_client_ip() {
        let context = RequestContext::new().with_client_ip("192.168.1.1");

        assert_eq!(context.client_ip, Some("192.168.1.1".to_string()));
    }

    #[test]
    fn test_request_context_with_query_param() {
        let context = RequestContext::new()
            .with_query_param("user_id", "user123")
            .with_query_param("page", "1");

        assert_eq!(
            context.query_params.get("user_id"),
            Some(&"user123".to_string())
        );
        assert_eq!(context.query_params.get("page"), Some(&"1".to_string()));
    }

    #[test]
    fn test_request_context_with_path() {
        let context = RequestContext::new().with_path("/api/users");

        assert_eq!(context.path, "/api/users");
    }

    #[test]
    fn test_request_context_get_header_case_insensitive() {
        let context = RequestContext::new().with_header("X-User-Id", "user123");

        // 各种大小写组合
        assert_eq!(
            context.get_header("X-User-Id"),
            Some(&"user123".to_string())
        );
        assert_eq!(
            context.get_header("x-user-id"),
            Some(&"user123".to_string())
        );
        assert_eq!(
            context.get_header("X-USER-ID"),
            Some(&"user123".to_string())
        );
        assert_eq!(
            context.get_header("x-USER-id"),
            Some(&"user123".to_string())
        );
    }

    #[test]
    fn test_request_context_debug_sensitive_headers() {
        let context = RequestContext::new()
            .with_header("Authorization", "Bearer secret-token")
            .with_header("X-API-Key", "secret-key")
            .with_header("Cookie", "session=abc123")
            .with_header("X-Custom", "normal-value");

        let debug_str = format!("{:?}", context);

        // 敏感头应该被脱敏
        assert!(debug_str.contains("***"));
        assert!(!debug_str.contains("secret-token"));
        assert!(!debug_str.contains("secret-key"));
        assert!(!debug_str.contains("session=abc123"));

        // 非敏感头应该正常显示
        assert!(debug_str.contains("normal-value"));
    }

    #[test]
    fn test_request_context_debug_sensitive_query_params() {
        let context = RequestContext::new()
            .with_query_param("token", "secret-token")
            .with_query_param("api_key", "secret-key")
            .with_query_param("secret", "secret-value")
            .with_query_param("page", "1");

        let debug_str = format!("{:?}", context);

        // 敏感参数应该被脱敏
        assert!(debug_str.contains("***"));
        assert!(!debug_str.contains("secret-token"));
        assert!(!debug_str.contains("secret-key"));
        assert!(!debug_str.contains("secret-value"));
    }

    // ==================== 增强测试：MatchCondition description ====================

    #[test]
    fn test_match_condition_description_user() {
        let condition = MatchCondition::User(vec!["user1".to_string(), "user2".to_string()]);
        assert_eq!(condition.description(), "User in [\"user1\", \"user2\"]");
    }

    #[test]
    fn test_match_condition_description_ip() {
        let condition = MatchCondition::Ip(vec![
            "192.168.1.0/24".parse().unwrap(),
            "10.0.0.0/8".parse().unwrap(),
        ]);
        assert_eq!(condition.description(), "IP in 2 ranges");
    }

    #[test]
    fn test_match_condition_description_geo() {
        let condition = MatchCondition::Geo(vec!["US".to_string(), "CN".to_string()]);
        assert_eq!(condition.description(), "Country in [\"US\", \"CN\"]");
    }

    #[test]
    fn test_match_condition_description_api_version() {
        let condition = MatchCondition::ApiVersion(vec!["v1".to_string(), "v2".to_string()]);
        assert_eq!(condition.description(), "API version in [\"v1\", \"v2\"]");
    }

    #[test]
    fn test_match_condition_description_device() {
        let condition = MatchCondition::Device(vec!["mobile".to_string(), "tablet".to_string()]);
        assert_eq!(
            condition.description(),
            "Device type in [\"mobile\", \"tablet\"]"
        );
    }

    #[test]
    fn test_match_condition_description_custom() {
        let condition = MatchCondition::Custom(Arc::new(|_| true));
        assert_eq!(condition.description(), "Custom condition");
    }

    // ==================== 增强测试：LogicalOperator ====================

    #[test]
    fn test_logical_operator_equality() {
        assert_eq!(LogicalOperator::And, LogicalOperator::And);
        assert_eq!(LogicalOperator::Or, LogicalOperator::Or);
        assert_eq!(LogicalOperator::Not, LogicalOperator::Not);

        assert_ne!(LogicalOperator::And, LogicalOperator::Or);
        assert_ne!(LogicalOperator::Or, LogicalOperator::Not);
    }

    // ==================== 增强测试：Rule ====================

    #[test]
    fn test_rule_debug() {
        let rule = Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: true,
        };

        let debug_str = format!("{:?}", rule);

        assert!(debug_str.contains("test_rule"));
        assert!(debug_str.contains("Test Rule"));
        assert!(debug_str.contains("100"));
        assert!(debug_str.contains("enabled: true"));
    }

    // ==================== 增强测试：可信代理 IP 提取 ====================

    #[test]
    fn test_trusted_proxy_config_default() {
        use crate::config::TrustedProxyConfig;
        let config = TrustedProxyConfig::default();
        assert!(!config.enabled);
        assert!(config.proxies.is_empty());
    }

    #[test]
    fn test_trusted_proxy_is_trusted() {
        use crate::config::TrustedProxyConfig;
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".to_string(), "192.168.1.0/24".to_string()],
            max_hops: 10,
        };

        // 单个 IP 匹配
        assert!(config.is_trusted("10.0.0.1"));
        assert!(!config.is_trusted("10.0.0.2"));

        // CIDR 匹配
        assert!(config.is_trusted("192.168.1.100"));
        assert!(!config.is_trusted("192.168.2.1"));
    }

    #[test]
    fn test_trusted_proxy_cidr_ipv6() {
        use crate::config::TrustedProxyConfig;
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["2001:db8::/32".to_string()],
            max_hops: 10,
        };

        assert!(config.is_trusted("2001:db8::1"));
        assert!(config.is_trusted("2001:db8:abcd::1234"));
        assert!(!config.is_trusted("2001:db9::1"));
    }

    #[test]
    fn test_ip_extractor_with_trusted_proxies() {
        let config = crate::config::TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".to_string(), "172.16.0.1".to_string()],
            max_hops: 10,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".to_string()], true, config);

        // X-Forwarded-For: 客户端IP, 代理1, 代理2
        // 格式: client, proxy1, proxy2
        // 代理1 (10.0.0.1) 和代理2 (172.16.0.1) 都是可信的
        // 应该返回客户端 IP
        let context = RequestContext::new()
            .with_header("X-Forwarded-For", "192.168.1.100, 10.0.0.1, 172.16.0.1");
        let result = extractor.extract(&context);
        assert_eq!(result, Some(Identifier::Ip("192.168.1.100".to_string())));
    }

    #[test]
    fn test_ip_extractor_trusted_proxy_all_proxies_trusted() {
        let config = crate::config::TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".to_string()],
            max_hops: 10,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".to_string()], true, config);

        // 所有 IP 都是可信代理，返回最右边的
        let context = RequestContext::new().with_header("X-Forwarded-For", "10.0.0.1, 10.0.0.1");
        let result = extractor.extract(&context);
        assert_eq!(result, Some(Identifier::Ip("10.0.0.1".to_string())));
    }

    #[test]
    fn test_ip_extractor_trusted_proxy_disabled() {
        let config = crate::config::TrustedProxyConfig {
            enabled: false,
            proxies: vec!["10.0.0.1".to_string()],
            max_hops: 10,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".to_string()], true, config);

        // 禁用时，使用最左边的 IP
        let context = RequestContext::new().with_header("X-Forwarded-For", "10.0.0.1, 192.168.1.1");
        let result = extractor.extract(&context);
        assert_eq!(result, Some(Identifier::Ip("10.0.0.1".to_string())));
    }

    #[test]
    fn test_ip_extractor_builder_with_trusted_proxies() {
        let config = crate::config::TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".to_string()],
            max_hops: 10,
        };
        let extractor = IpExtractor::builder()
            .header_name("X-Forwarded-For")
            .validate(true)
            .trusted_proxy_config(config)
            .build();

        let context = RequestContext::new().with_header("X-Forwarded-For", "192.168.1.1, 10.0.0.1");
        let result = extractor.extract(&context);
        assert_eq!(result, Some(Identifier::Ip("192.168.1.1".to_string())));
    }

    #[test]
    fn test_ip_extractor_max_hops_exceeded() {
        use crate::config::TrustedProxyConfig;
        // 设置 max_hops = 3
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".to_string()],
            max_hops: 3,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".to_string()], true, config);

        // X-Forwarded-For 包含 5 个 IP,超过 max_hops = 3
        let context = RequestContext::new().with_header(
            "X-Forwarded-For",
            "192.168.1.1, 192.168.1.2, 192.168.1.3, 10.0.0.1, 10.0.0.2",
        );
        let result = extractor.extract(&context);
        // 应该返回 None,因为超过了 max_hops 限制
        assert_eq!(result, None);
    }

    #[test]
    fn test_ip_extractor_max_hops_within_limit() {
        use crate::config::TrustedProxyConfig;
        // 设置 max_hops = 5
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".to_string()],
            max_hops: 5,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".to_string()], true, config);

        // X-Forwarded-For 包含 3 个 IP,在 max_hops = 5 限制内
        let context = RequestContext::new()
            .with_header("X-Forwarded-For", "192.168.1.1, 192.168.1.2, 10.0.0.1");
        let result = extractor.extract(&context);
        // 应该正常返回客户端 IP
        assert_eq!(result, Some(Identifier::Ip("192.168.1.2".to_string())));
    }

    #[test]
    fn test_trusted_proxy_config_default_max_hops() {
        use crate::config::TrustedProxyConfig;
        let config = TrustedProxyConfig::default();
        assert_eq!(config.max_hops, 10);
    }
}

// ============================================================================
// 公共导出
// ============================================================================

// 地理位置匹配器
#[cfg(feature = "geo-matching")]
pub use geo::{GeoCacheStats, GeoCondition, GeoInfo, GeoMatcher};

// 设备类型匹配器
#[cfg(feature = "device-matching")]
pub use device::{DeviceCacheStats, DeviceCondition, DeviceInfo, DeviceMatcher, DeviceType};

// 自定义匹配器
pub use custom::{CustomMatcher, CustomMatcherRegistry, HeaderMatcher, TimeWindowMatcher};
