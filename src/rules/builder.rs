// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 规则构建器模块
//!
//! 提供从配置构建规则和决策链的功能，将规则构建逻辑从 Governor 中分离出来。
//!
//! # 功能
//!
//! - 从 FlowControlConfig 构建规则列表
//! - 从 FlowControlConfig 构建决策链映射
//! - 时长字符串解析

use crate::config::{FlowControlConfig, LimiterConfig, LimiterTypeName, Matcher as ConfigMatcher};
use crate::decision_chain::{DecisionChain, DecisionNode};
use crate::error::LimiteronError;
use crate::limiters::{
    ConcurrencyLimiter, FixedWindowLimiter, Limiter, ShardedSlidingWindowLimiter,
    TokenBucketLimiter,
};
use crate::matchers::{
    CompositeCondition, ConditionEvaluator, IpRange, LogicalOperator, MatchCondition,
    Rule as MatcherRule,
};
use dashmap::DashMap;
use log::warn;
use std::sync::Arc;
use std::time::Duration;

/// 规则构建器
///
/// 负责从配置构建规则列表和决策链。
///
/// # 示例
///
/// ```rust
/// use limiteron::RuleBuilder;
/// use limiteron::config::FlowControlConfig;
///
/// let config = FlowControlConfig::default();
/// let rules = RuleBuilder::build_rules(&config).unwrap();
/// let chains = RuleBuilder::build_rule_chains(&config).unwrap();
/// ```
pub struct RuleBuilder;

impl RuleBuilder {
    /// 解析时长字符串
    ///
    /// 支持以下格式：
    /// - `ms` 后缀：毫秒
    /// - `s` 后缀：秒
    /// - `m` 后缀：分钟
    /// - `h` 后缀：小时
    ///
    /// # 参数
    ///
    /// - `s`: 时长字符串
    ///
    /// # 返回
    ///
    /// - `Ok(Duration)`: 解析成功
    /// - `Err(LimiteronError)`: 解析失败
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::RuleBuilder;
    /// use std::time::Duration;
    ///
    /// assert_eq!(RuleBuilder::parse_duration("100ms").unwrap(), Duration::from_millis(100));
    /// assert_eq!(RuleBuilder::parse_duration("10s").unwrap(), Duration::from_secs(10));
    /// assert_eq!(RuleBuilder::parse_duration("5m").unwrap(), Duration::from_secs(300));
    /// assert_eq!(RuleBuilder::parse_duration("2h").unwrap(), Duration::from_secs(7200));
    /// ```
    pub fn parse_duration(s: &str) -> Result<Duration, LimiteronError> {
        crate::config::parse_window_size(s).map_err(LimiteronError::ConfigError)
    }

    /// 从配置构建规则对应的决策链
    ///
    /// # 参数
    ///
    /// - `config`: 流量控制配置
    ///
    /// # 返回
    ///
    /// - `Ok(DashMap<String, DecisionChain>)`: 决策链映射（规则ID -> 决策链）
    /// - `Err(LimiteronError)`: 构建失败
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::RuleBuilder;
    /// use limiteron::config::FlowControlConfig;
    ///
    /// let config = FlowControlConfig::default();
    /// let chains = RuleBuilder::build_rule_chains(&config).unwrap();
    /// ```
    pub fn build_rule_chains(
        config: &FlowControlConfig,
    ) -> Result<DashMap<String, DecisionChain>, LimiteronError> {
        let chains = DashMap::new();

        for rule in &config.rules {
            let mut nodes: Vec<DecisionNode> = Vec::new();

            for (index, limiter_config) in rule.limiters.iter().enumerate() {
                let (limiter, type_name): (Arc<dyn Limiter>, LimiterTypeName) = match limiter_config
                {
                    LimiterConfig::TokenBucket {
                        capacity,
                        refill_rate,
                    } => (
                        Arc::new(TokenBucketLimiter::new(*capacity, *refill_rate)),
                        LimiterTypeName::TokenBucket,
                    ),
                    LimiterConfig::SlidingWindow {
                        window_size,
                        max_requests,
                    } => {
                        let duration = Self::parse_duration(window_size)?;
                        (
                            Arc::new(ShardedSlidingWindowLimiter::new(duration, *max_requests)),
                            LimiterTypeName::SlidingWindow,
                        )
                    }
                    LimiterConfig::FixedWindow {
                        window_size,
                        max_requests,
                    } => {
                        let duration = Self::parse_duration(window_size)?;
                        (
                            Arc::new(FixedWindowLimiter::new(duration, *max_requests)),
                            LimiterTypeName::FixedWindow,
                        )
                    }
                    LimiterConfig::Quota {
                        quota_type: _,
                        limit: _,
                        window: _,
                        alert_threshold: _,
                        overdraft: _,
                    } => {
                        // Quota limiter requires quota-control feature
                        warn!(
                            "QuotaLimiter requires 'quota-control' feature to be enabled, \
                             skipping Quota configuration"
                        );
                        continue;
                    }
                    LimiterConfig::Concurrency { max_concurrent } => (
                        Arc::new(ConcurrencyLimiter::new(*max_concurrent)),
                        LimiterTypeName::Concurrency,
                    ),
                    LimiterConfig::Custom { name, config: _ } => {
                        // CustomLimiter integration requires custom-limiter feature and
                        // manual registration via CustomLimiterRegistry
                        #[cfg(feature = "custom-limiter")]
                        warn!(
                            "CustomLimiter '{}' requires registration via CustomLimiterRegistry, skipping",
                            name
                        );
                        #[cfg(not(feature = "custom-limiter"))]
                        warn!(
                            "CustomLimiter '{}' skipped - custom-limiter feature not enabled",
                            name
                        );
                        continue;
                    }
                };

                let node = DecisionNode::with_dependencies(
                    format!("{}_limiter_{}", rule.id, index),
                    format!("{} - {}", rule.name, type_name),
                    limiter,
                    100u16.saturating_sub(index as u16), // Priority: earlier limiters have higher priority
                );
                nodes.push(node);
            }

            chains.insert(rule.id.clone(), DecisionChain::with_dependencies(nodes));
        }

        Ok(chains)
    }

    /// 从配置构建规则列表
    ///
    /// # 参数
    ///
    /// - `config`: 流量控制配置
    ///
    /// # 返回
    ///
    /// - `Ok(Vec<MatcherRule>)`: 规则列表
    /// - `Err(LimiteronError)`: 构建失败
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::RuleBuilder;
    /// use limiteron::config::FlowControlConfig;
    ///
    /// let config = FlowControlConfig::default();
    /// let rules = RuleBuilder::build_rules(&config).unwrap();
    /// ```
    pub fn build_rules(config: &FlowControlConfig) -> Result<Vec<MatcherRule>, LimiteronError> {
        let mut rules = Vec::new();

        for rule_config in &config.rules {
            let mut conditions: Vec<Box<dyn ConditionEvaluator>> = Vec::new();

            for matcher in &rule_config.matchers {
                let condition: Box<dyn ConditionEvaluator> = match matcher {
                    ConfigMatcher::User { user_ids } => {
                        Box::new(MatchCondition::User(user_ids.clone()))
                    }
                    ConfigMatcher::Ip { ip_ranges } => {
                        let ranges: Result<Vec<IpRange>, _> =
                            ip_ranges.iter().map(|s| s.parse()).collect();
                        Box::new(MatchCondition::Ip(ranges?))
                    }
                    ConfigMatcher::Geo { countries } => {
                        Box::new(MatchCondition::Geo(countries.clone()))
                    }
                    ConfigMatcher::ApiVersion { versions } => {
                        Box::new(MatchCondition::ApiVersion(versions.clone()))
                    }
                    ConfigMatcher::Device { device_types } => {
                        Box::new(MatchCondition::Device(device_types.clone()))
                    }
                    ConfigMatcher::Custom { name, config: _ } => {
                        let name = name.clone();
                        Box::new(MatchCondition::Custom(Arc::new(move |_context| {
                            log::warn!("自定义匹配器 '{}' 需要通过CustomMatcherRegistry处理", name);
                            false
                        })))
                    }
                };
                conditions.push(condition);
            }

            let final_condition: Box<dyn ConditionEvaluator> = if conditions.len() == 1 {
                conditions.pop().unwrap()
            } else if conditions.is_empty() {
                continue;
            } else {
                Box::new(CompositeCondition {
                    conditions,
                    operator: LogicalOperator::And,
                })
            };

            rules.push(MatcherRule {
                id: rule_config.id.clone(),
                name: rule_config.name.clone(),
                priority: rule_config.priority,
                condition: final_condition,
                enabled: true,
            });
        }

        Ok(rules)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ActionConfig, Matcher, QuotaType, Rule};

    #[test]
    fn test_parse_duration_milliseconds() {
        assert_eq!(
            RuleBuilder::parse_duration("100ms").unwrap(),
            Duration::from_millis(100)
        );
        assert_eq!(
            RuleBuilder::parse_duration("1ms").unwrap(),
            Duration::from_millis(1)
        );
    }

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(
            RuleBuilder::parse_duration("10s").unwrap(),
            Duration::from_secs(10)
        );
        assert_eq!(
            RuleBuilder::parse_duration("1s").unwrap(),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(
            RuleBuilder::parse_duration("5m").unwrap(),
            Duration::from_secs(300)
        );
        assert_eq!(
            RuleBuilder::parse_duration("1m").unwrap(),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(
            RuleBuilder::parse_duration("2h").unwrap(),
            Duration::from_secs(7200)
        );
        assert_eq!(
            RuleBuilder::parse_duration("1h").unwrap(),
            Duration::from_secs(3600)
        );
    }

    #[test]
    fn test_parse_duration_with_whitespace() {
        assert_eq!(
            RuleBuilder::parse_duration(" 10s ").unwrap(),
            Duration::from_secs(10)
        );
    }

    #[test]
    fn test_parse_duration_invalid_format() {
        assert!(RuleBuilder::parse_duration("10").is_err());
        assert!(RuleBuilder::parse_duration("10x").is_err());
        assert!(RuleBuilder::parse_duration("").is_err());
    }

    #[test]
    fn test_parse_duration_invalid_number() {
        assert!(RuleBuilder::parse_duration("abs").is_err());
        assert!(RuleBuilder::parse_duration("abcms").is_err());
    }

    #[test]
    fn test_build_rules_empty_config() {
        let config = FlowControlConfig::default();
        let rules = RuleBuilder::build_rules(&config).unwrap();
        assert!(rules.is_empty());
    }

    #[test]
    fn test_build_rule_chains_empty_config() {
        let config = FlowControlConfig::default();
        let chains = RuleBuilder::build_rule_chains(&config).unwrap();
        assert!(chains.is_empty());
    }

    #[test]
    fn test_parse_duration_valid() {
        use std::time::Duration;
        assert_eq!(
            RuleBuilder::parse_duration("100ms").unwrap(),
            Duration::from_millis(100)
        );
        assert_eq!(
            RuleBuilder::parse_duration("10s").unwrap(),
            Duration::from_secs(10)
        );
        assert_eq!(
            RuleBuilder::parse_duration("5m").unwrap(),
            Duration::from_secs(300)
        );
        assert_eq!(
            RuleBuilder::parse_duration("2h").unwrap(),
            Duration::from_secs(7200)
        );
        assert_eq!(
            RuleBuilder::parse_duration("1d").unwrap(),
            Duration::from_secs(86400)
        );
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert!(RuleBuilder::parse_duration("").is_err());
        assert!(RuleBuilder::parse_duration("abc").is_err());
        assert!(RuleBuilder::parse_duration("10x").is_err());
    }

    #[test]
    fn test_build_rule_chains_with_single_rule() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "test-rule".to_string(),
                name: "Test Rule".to_string(),
                priority: 1,
                matchers: vec![Matcher::User {
                    user_ids: vec!["user1".to_string()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 100,
                    refill_rate: 10,
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let chains = RuleBuilder::build_rule_chains(&config).unwrap();
        assert_eq!(chains.len(), 1);
        assert!(chains.contains_key("test-rule"));
    }

    #[test]
    fn test_build_rule_chains_with_multiple_rules() {
        let config = FlowControlConfig {
            rules: vec![
                Rule {
                    id: "rule1".to_string(),
                    name: "Rule 1".to_string(),
                    priority: 1,
                    matchers: vec![Matcher::User {
                        user_ids: vec!["user1".to_string()],
                    }],
                    limiters: vec![LimiterConfig::TokenBucket {
                        capacity: 100,
                        refill_rate: 10,
                    }],
                    action: ActionConfig::default(),
                },
                Rule {
                    id: "rule2".to_string(),
                    name: "Rule 2".to_string(),
                    priority: 2,
                    matchers: vec![Matcher::Ip {
                        ip_ranges: vec!["192.168.1.0/24".to_string()],
                    }],
                    limiters: vec![LimiterConfig::FixedWindow {
                        window_size: "1m".to_string(),
                        max_requests: 100,
                    }],
                    action: ActionConfig::default(),
                },
            ],
            ..Default::default()
        };
        let chains = RuleBuilder::build_rule_chains(&config).unwrap();
        assert_eq!(chains.len(), 2);
        assert!(chains.contains_key("rule1"));
        assert!(chains.contains_key("rule2"));
    }

    #[test]
    fn test_build_rule_chains_empty_rules() {
        let config = FlowControlConfig {
            rules: vec![],
            ..Default::default()
        };
        let chains = RuleBuilder::build_rule_chains(&config).unwrap();
        assert!(chains.is_empty());
    }

    // ==================== build_rules coverage expansion ====================

    #[test]
    fn test_build_rules_single_user_matcher() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "user-rule".into(),
                name: "User Rule".into(),
                priority: 10,
                matchers: vec![Matcher::User {
                    user_ids: vec!["user1".into(), "user2".into()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 100,
                    refill_rate: 10,
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let rules = RuleBuilder::build_rules(&config).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "user-rule");
        assert_eq!(rules[0].name, "User Rule");
        assert_eq!(rules[0].priority, 10);
        assert!(rules[0].enabled);
    }

    #[test]
    fn test_build_rules_single_ip_matcher() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "ip-rule".into(),
                name: "IP Rule".into(),
                priority: 20,
                matchers: vec![Matcher::Ip {
                    ip_ranges: vec!["10.0.0.0/8".into()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 100,
                    refill_rate: 10,
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let rules = RuleBuilder::build_rules(&config).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "ip-rule");
    }

    #[test]
    fn test_build_rules_invalid_ip_range() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "bad-ip".into(),
                name: "Bad IP".into(),
                priority: 1,
                matchers: vec![Matcher::Ip {
                    ip_ranges: vec!["not-an-ip".into()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 100,
                    refill_rate: 10,
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        assert!(RuleBuilder::build_rules(&config).is_err());
    }

    #[test]
    fn test_build_rules_single_geo_matcher() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "geo-rule".into(),
                name: "Geo Rule".into(),
                priority: 30,
                matchers: vec![Matcher::Geo {
                    countries: vec!["US".into(), "CN".into()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 200,
                    refill_rate: 20,
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let rules = RuleBuilder::build_rules(&config).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "geo-rule");
    }

    #[test]
    fn test_build_rules_single_api_version_matcher() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "api-rule".into(),
                name: "API Rule".into(),
                priority: 40,
                matchers: vec![Matcher::ApiVersion {
                    versions: vec!["v1".into(), "v2".into()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 300,
                    refill_rate: 30,
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let rules = RuleBuilder::build_rules(&config).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "api-rule");
    }

    #[test]
    fn test_build_rules_single_device_matcher() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "device-rule".into(),
                name: "Device Rule".into(),
                priority: 50,
                matchers: vec![Matcher::Device {
                    device_types: vec!["mobile".into(), "desktop".into()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 400,
                    refill_rate: 40,
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let rules = RuleBuilder::build_rules(&config).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "device-rule");
    }

    #[test]
    fn test_build_rules_custom_matcher() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "custom-rule".into(),
                name: "Custom Rule".into(),
                priority: 60,
                matchers: vec![Matcher::Custom {
                    name: "my-custom".into(),
                    config: serde_json::json!({"key": "value"}),
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 500,
                    refill_rate: 50,
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let rules = RuleBuilder::build_rules(&config).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "custom-rule");
    }

    #[test]
    fn test_build_rules_multiple_matchers_composite() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "composite-rule".into(),
                name: "Composite Rule".into(),
                priority: 70,
                matchers: vec![
                    Matcher::User {
                        user_ids: vec!["user1".into()],
                    },
                    Matcher::Geo {
                        countries: vec!["US".into()],
                    },
                ],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 600,
                    refill_rate: 60,
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let rules = RuleBuilder::build_rules(&config).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "composite-rule");
    }

    #[test]
    fn test_build_rules_no_matchers_skipped() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "skip-rule".into(),
                name: "Skip Rule".into(),
                priority: 1,
                matchers: vec![],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 100,
                    refill_rate: 10,
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let rules = RuleBuilder::build_rules(&config).unwrap();
        assert!(rules.is_empty());
    }

    #[test]
    fn test_build_rules_multiple_rules_various_matchers() {
        let config = FlowControlConfig {
            rules: vec![
                Rule {
                    id: "rule-a".into(),
                    name: "Rule A".into(),
                    priority: 1,
                    matchers: vec![Matcher::User {
                        user_ids: vec!["user1".into()],
                    }],
                    limiters: vec![LimiterConfig::TokenBucket {
                        capacity: 100,
                        refill_rate: 10,
                    }],
                    action: ActionConfig::default(),
                },
                Rule {
                    id: "rule-b".into(),
                    name: "Rule B".into(),
                    priority: 2,
                    matchers: vec![Matcher::Device {
                        device_types: vec!["mobile".into()],
                    }],
                    limiters: vec![LimiterConfig::FixedWindow {
                        window_size: "1m".into(),
                        max_requests: 100,
                    }],
                    action: ActionConfig::default(),
                },
            ],
            ..Default::default()
        };
        let rules = RuleBuilder::build_rules(&config).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].id, "rule-a");
        assert_eq!(rules[1].id, "rule-b");
        assert_eq!(rules[0].priority, 1);
        assert_eq!(rules[1].priority, 2);
    }

    // ==================== build_rule_chains coverage expansion ====================

    #[test]
    fn test_build_rule_chains_sliding_window() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "sliding-rule".into(),
                name: "Sliding Window Rule".into(),
                priority: 1,
                matchers: vec![Matcher::User {
                    user_ids: vec!["user1".into()],
                }],
                limiters: vec![LimiterConfig::SlidingWindow {
                    window_size: "30s".into(),
                    max_requests: 100,
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let chains = RuleBuilder::build_rule_chains(&config).unwrap();
        assert_eq!(chains.len(), 1);
        assert!(chains.contains_key("sliding-rule"));
        let chain = chains.get("sliding-rule").unwrap();
        assert_eq!(chain.node_count(), 1);
    }

    #[test]
    fn test_build_rule_chains_concurrency() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "concurrency-rule".into(),
                name: "Concurrency Rule".into(),
                priority: 1,
                matchers: vec![Matcher::User {
                    user_ids: vec!["user1".into()],
                }],
                limiters: vec![LimiterConfig::Concurrency { max_concurrent: 50 }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let chains = RuleBuilder::build_rule_chains(&config).unwrap();
        assert_eq!(chains.len(), 1);
        assert!(chains.contains_key("concurrency-rule"));
        let chain = chains.get("concurrency-rule").unwrap();
        assert_eq!(chain.node_count(), 1);
    }

    #[test]
    fn test_build_rule_chains_quota_skipped() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "quota-rule".into(),
                name: "Quota Rule".into(),
                priority: 1,
                matchers: vec![Matcher::User {
                    user_ids: vec!["user1".into()],
                }],
                limiters: vec![LimiterConfig::Quota {
                    quota_type: QuotaType::Count,
                    limit: 1000,
                    window: "1d".into(),
                    alert_threshold: Some(80),
                    overdraft: None,
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let chains = RuleBuilder::build_rule_chains(&config).unwrap();
        assert_eq!(chains.len(), 1);
        assert!(chains.contains_key("quota-rule"));
        let chain = chains.get("quota-rule").unwrap();
        assert_eq!(chain.node_count(), 0);
    }

    #[test]
    fn test_build_rule_chains_custom_skipped() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "custom-rule".into(),
                name: "Custom Limiter Rule".into(),
                priority: 1,
                matchers: vec![Matcher::User {
                    user_ids: vec!["user1".into()],
                }],
                limiters: vec![LimiterConfig::Custom {
                    name: "my-custom-limiter".into(),
                    config: serde_json::json!({"key": "value"}),
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let chains = RuleBuilder::build_rule_chains(&config).unwrap();
        assert_eq!(chains.len(), 1);
        assert!(chains.contains_key("custom-rule"));
        let chain = chains.get("custom-rule").unwrap();
        assert_eq!(chain.node_count(), 0);
    }

    #[test]
    fn test_build_rule_chains_multiple_limiters_per_rule() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "multi-limiter".into(),
                name: "Multi Limiter".into(),
                priority: 1,
                matchers: vec![Matcher::User {
                    user_ids: vec!["user1".into()],
                }],
                limiters: vec![
                    LimiterConfig::TokenBucket {
                        capacity: 100,
                        refill_rate: 10,
                    },
                    LimiterConfig::FixedWindow {
                        window_size: "1m".into(),
                        max_requests: 50,
                    },
                ],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let chains = RuleBuilder::build_rule_chains(&config).unwrap();
        assert_eq!(chains.len(), 1);
        assert!(chains.contains_key("multi-limiter"));
        let chain = chains.get("multi-limiter").unwrap();
        assert_eq!(chain.node_count(), 2);
    }

    #[test]
    fn test_build_rule_chains_invalid_window_size() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "bad-window".into(),
                name: "Bad Window".into(),
                priority: 1,
                matchers: vec![Matcher::User {
                    user_ids: vec!["user1".into()],
                }],
                limiters: vec![LimiterConfig::SlidingWindow {
                    window_size: "invalid".into(),
                    max_requests: 100,
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        assert!(RuleBuilder::build_rule_chains(&config).is_err());
    }

    #[test]
    fn test_build_rule_chains_all_limiter_types() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "all-types".into(),
                name: "All Types".into(),
                priority: 100,
                matchers: vec![Matcher::User {
                    user_ids: vec!["user1".into()],
                }],
                limiters: vec![
                    LimiterConfig::TokenBucket {
                        capacity: 100,
                        refill_rate: 10,
                    },
                    LimiterConfig::SlidingWindow {
                        window_size: "30s".into(),
                        max_requests: 200,
                    },
                    LimiterConfig::FixedWindow {
                        window_size: "1m".into(),
                        max_requests: 50,
                    },
                    LimiterConfig::Concurrency { max_concurrent: 25 },
                    LimiterConfig::Quota {
                        quota_type: QuotaType::Count,
                        limit: 1000,
                        window: "1d".into(),
                        alert_threshold: None,
                        overdraft: None,
                    },
                    LimiterConfig::Custom {
                        name: "test".into(),
                        config: serde_json::json!({"key": "val"}),
                    },
                ],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let chains = RuleBuilder::build_rule_chains(&config).unwrap();
        assert_eq!(chains.len(), 1);
        let chain = chains.get("all-types").unwrap();
        assert_eq!(chain.node_count(), 4);
    }

    #[test]
    fn test_build_rule_chains_with_fixed_window() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "fixed-rule".into(),
                name: "Fixed Window Rule".into(),
                priority: 1,
                matchers: vec![Matcher::User {
                    user_ids: vec!["user1".into()],
                }],
                limiters: vec![LimiterConfig::FixedWindow {
                    window_size: "5m".into(),
                    max_requests: 200,
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let chains = RuleBuilder::build_rule_chains(&config).unwrap();
        assert_eq!(chains.len(), 1);
        assert!(chains.contains_key("fixed-rule"));
        let chain = chains.get("fixed-rule").unwrap();
        assert_eq!(chain.node_count(), 1);
    }

    #[test]
    fn test_build_rule_chains_token_bucket_boundary_values() {
        let config = FlowControlConfig {
            rules: vec![Rule {
                id: "boundary".into(),
                name: "Boundary".into(),
                priority: 1,
                matchers: vec![Matcher::User {
                    user_ids: vec!["user1".into()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 1,
                    refill_rate: 1,
                }],
                action: ActionConfig::default(),
            }],
            ..Default::default()
        };
        let chains = RuleBuilder::build_rule_chains(&config).unwrap();
        assert_eq!(chains.len(), 1);
        assert!(chains.contains_key("boundary"));
        let chain = chains.get("boundary").unwrap();
        assert_eq!(chain.node_count(), 1);
    }
}
