//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 规则构建器模块
//!
//! 提供从配置构建规则和决策链的功能，将规则构建逻辑从 Governor 中分离出来。
//!
//! # 功能
//!
//! - 从 FlowControlConfig 构建规则列表
//! - 从 FlowControlConfig 构建决策链映射
//! - 时长字符串解析

use crate::config::{FlowControlConfig, LimiterConfig, Matcher as ConfigMatcher};
use crate::constants::{SECONDS_PER_HOUR, SECONDS_PER_MINUTE};
use crate::decision_chain::{DecisionChain, DecisionNode};
use crate::error::FlowGuardError;
use crate::limiters::{
    ConcurrencyLimiter, FixedWindowLimiter, Limiter, SlidingWindowLimiter, TokenBucketLimiter,
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
/// use limiteron::rule_builder::RuleBuilder;
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
    /// - `Err(FlowGuardError)`: 解析失败
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rule_builder::RuleBuilder;
    /// use std::time::Duration;
    ///
    /// assert_eq!(RuleBuilder::parse_duration("100ms").unwrap(), Duration::from_millis(100));
    /// assert_eq!(RuleBuilder::parse_duration("10s").unwrap(), Duration::from_secs(10));
    /// assert_eq!(RuleBuilder::parse_duration("5m").unwrap(), Duration::from_secs(300));
    /// assert_eq!(RuleBuilder::parse_duration("2h").unwrap(), Duration::from_secs(7200));
    /// ```
    pub fn parse_duration(s: &str) -> Result<Duration, FlowGuardError> {
        let s = s.trim();
        let (num, unit) = if s.ends_with("ms") {
            (s.trim_end_matches("ms"), "ms")
        } else if s.ends_with('s') {
            (s.trim_end_matches('s'), "s")
        } else if s.ends_with('m') {
            (s.trim_end_matches('m'), "m")
        } else if s.ends_with('h') {
            (s.trim_end_matches('h'), "h")
        } else {
            return Err(FlowGuardError::ConfigError(format!(
                "Invalid duration format: {}",
                s
            )));
        };

        let val: u64 = num.parse().map_err(|_| {
            FlowGuardError::ConfigError(format!("Invalid duration number: {}", num))
        })?;

        match unit {
            "ms" => Ok(Duration::from_millis(val)),
            "s" => Ok(Duration::from_secs(val)),
            "m" => Ok(Duration::from_secs(val * SECONDS_PER_MINUTE)),
            "h" => Ok(Duration::from_secs(val * SECONDS_PER_HOUR)),
            _ => Err(FlowGuardError::ConfigError(format!(
                "Invalid duration unit '{}'. Valid units: ms, s, m, h",
                unit
            ))),
        }
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
    /// - `Err(FlowGuardError)`: 构建失败
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rule_builder::RuleBuilder;
    /// use limiteron::config::FlowControlConfig;
    ///
    /// let config = FlowControlConfig::default();
    /// let chains = RuleBuilder::build_rule_chains(&config).unwrap();
    /// ```
    pub fn build_rule_chains(
        config: &FlowControlConfig,
    ) -> Result<DashMap<String, DecisionChain>, FlowGuardError> {
        let chains = DashMap::new();

        for rule in &config.rules {
            let mut nodes: Vec<DecisionNode> = Vec::new();

            for (index, limiter_config) in rule.limiters.iter().enumerate() {
                let (limiter, type_name): (Arc<dyn Limiter>, &str) = match limiter_config {
                    LimiterConfig::TokenBucket {
                        capacity,
                        refill_rate,
                    } => (
                        Arc::new(TokenBucketLimiter::new(*capacity, *refill_rate)),
                        "TokenBucket",
                    ),
                    LimiterConfig::SlidingWindow {
                        window_size,
                        max_requests,
                    } => {
                        let duration = Self::parse_duration(window_size)?;
                        (
                            Arc::new(SlidingWindowLimiter::new(duration, *max_requests)),
                            "SlidingWindow",
                        )
                    }
                    LimiterConfig::FixedWindow {
                        window_size,
                        max_requests,
                    } => {
                        let duration = Self::parse_duration(window_size)?;
                        (
                            Arc::new(FixedWindowLimiter::new(duration, *max_requests)),
                            "FixedWindow",
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
                        "Concurrency",
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
    /// - `Err(FlowGuardError)`: 构建失败
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rule_builder::RuleBuilder;
    /// use limiteron::config::FlowControlConfig;
    ///
    /// let config = FlowControlConfig::default();
    /// let rules = RuleBuilder::build_rules(&config).unwrap();
    /// ```
    pub fn build_rules(config: &FlowControlConfig) -> Result<Vec<MatcherRule>, FlowGuardError> {
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
}
