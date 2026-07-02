// Copyright (c) 2026, Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! 配置模块
//!
//! 定义流量控制的配置结构。

use ahash::AHashSet as HashSet;
use chrono::Utc;
use serde::{Deserialize, Serialize};

// 子模块
mod actions;
mod config;
mod history;
mod limiter;
mod limiter_type;
mod quota_type;
mod rule;

pub use actions::{Action, ActionConfig, BanConfig, BanScope, CacheBackend, MetricsBackend};
pub use config::{GlobalConfig, StorageType, TrustedProxyConfig};
pub use history::{ChangeSource, ConfigChangeRecord, ConfigHistory};
pub(crate) use limiter::parse_window_size;
pub use limiter::{LimiterConfig, OverdraftConfig};
pub use limiter_type::LimiterTypeName;
pub use quota_type::QuotaType;
pub use rule::Matcher;
pub use rule::Rule;

/// 流量控制配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowControlConfig {
    pub version: String,
    pub global: GlobalConfig,
    pub rules: Vec<Rule>,
}

impl Default for FlowControlConfig {
    fn default() -> Self {
        Self {
            version: "0.1.0".to_string(),
            global: GlobalConfig::default(),
            rules: Vec::new(),
        }
    }
}

impl FlowControlConfig {
    /// 校验配置
    pub fn validate(&self) -> Result<(), String> {
        // 校验版本
        if self.version.is_empty() {
            return Err("版本号不能为空".to_string());
        }

        // 校验全局配置
        self.global.validate()?;

        // 校验规则
        let mut rule_ids = HashSet::new();
        for (index, rule) in self.rules.iter().enumerate() {
            // 检查规则ID是否唯一
            if !rule_ids.insert(&rule.id) {
                return Err(format!("规则ID重复: {}", rule.id));
            }

            // 校验规则
            rule.validate()
                .map_err(|e| format!("规则[{}]校验失败: {}", index, e))?;
        }

        if self.rules.is_empty() {
            return Err("至少需要一个规则".to_string());
        }

        Ok(())
    }

    /// 计算配置哈希值
    pub fn compute_hash(&self) -> String {
        let config_str = serde_json::to_string(self).unwrap_or_default();
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        config_str.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// 比较配置是否相同（基于哈希值）
    pub fn is_same_as(&self, other: &FlowControlConfig) -> bool {
        self.compute_hash() == other.compute_hash()
    }

    /// 比较版本号
    pub fn compare_version(&self, other: &FlowControlConfig) -> std::cmp::Ordering {
        self.version.cmp(&other.version)
    }

    /// 创建配置变更记录
    pub fn create_change_record(
        &self,
        old_config: Option<&FlowControlConfig>,
        source: ChangeSource,
    ) -> ConfigChangeRecord {
        ConfigChangeRecord {
            timestamp: Utc::now(),
            old_version: old_config.map(|c| c.version.clone()),
            new_version: self.version.clone(),
            old_hash: old_config.map(|c| c.compute_hash()),
            new_hash: self.compute_hash(),
            source,
            changes: if let Some(old) = old_config {
                self.diff_changes(old)
            } else {
                vec!["初始配置".to_string()]
            },
        }
    }

    /// 比较配置差异
    fn diff_changes(&self, old: &FlowControlConfig) -> Vec<String> {
        let mut changes = Vec::new();

        // 比较版本
        if self.version != old.version {
            changes.push(format!("版本变更: {} -> {}", old.version, self.version));
        }

        // 比较全局配置
        if self.global != old.global {
            changes.push("全局配置已变更".to_string());
        }

        // 比较规则数量
        if self.rules.len() != old.rules.len() {
            changes.push(format!(
                "规则数量变更: {} -> {}",
                old.rules.len(),
                self.rules.len()
            ));
        }

        // 比较规则ID
        let old_rule_ids: HashSet<_> = old.rules.iter().map(|r| &r.id).collect();
        let new_rule_ids: HashSet<_> = self.rules.iter().map(|r| &r.id).collect();

        let added_rules: Vec<_> = new_rule_ids.difference(&old_rule_ids).collect();
        let removed_rules: Vec<_> = old_rule_ids.difference(&new_rule_ids).collect();

        if !added_rules.is_empty() {
            changes.push(format!("新增规则: {:?}", added_rules));
        }

        if !removed_rules.is_empty() {
            changes.push(format!("移除规则: {:?}", removed_rules));
        }

        if changes.is_empty() {
            changes.push("配置内容无变化".to_string());
        }

        changes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_config() {
        let config = FlowControlConfig {
            version: "1.0".to_string(),
            global: GlobalConfig {
                storage: StorageType::Memory,
                cache: CacheBackend::Memory,
                metrics: MetricsBackend::Prometheus,
                trusted_proxies: TrustedProxyConfig::default(),
            },
            rules: vec![],
        };

        // 测试校验应该失败，因为rules为空
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_with_rule() {
        let config = FlowControlConfig {
            version: "1.0".to_string(),
            global: GlobalConfig {
                storage: StorageType::Memory,
                cache: CacheBackend::Memory,
                metrics: MetricsBackend::Prometheus,
                trusted_proxies: TrustedProxyConfig::default(),
            },
            rules: vec![Rule {
                id: "test_rule".to_string(),
                name: "Test Rule".to_string(),
                priority: 100,
                matchers: vec![Matcher::User {
                    user_ids: vec!["*".to_string()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 1000,
                    refill_rate: 100,
                }],
                action: ActionConfig {
                    on_exceed: Action::Reject,
                    ban: None,
                },
            }],
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_duplicate_rule_ids() {
        let rule = Rule {
            id: "duplicate".to_string(),
            name: "Rule 1".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 1000,
                refill_rate: 100,
            }],
            action: ActionConfig {
                on_exceed: Action::Reject,
                ban: None,
            },
        };

        let config = FlowControlConfig {
            version: "1.0".to_string(),
            global: GlobalConfig {
                storage: StorageType::Memory,
                cache: CacheBackend::Memory,
                metrics: MetricsBackend::Prometheus,
                trusted_proxies: TrustedProxyConfig::default(),
            },
            rules: vec![rule.clone(), rule],
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_yaml_parsing() {
        let yaml = r#"
version: "1.0"
global:
  storage: "memory"
  cache: "memory"
  metrics: "prometheus"
rules:
  - id: "test_rule"
    name: "Test Rule"
    priority: 100
    matchers:
      - type: User
        user_ids: ["*"]
    limiters:
      - type: TokenBucket
        capacity: 1000
        refill_rate: 100
    action:
      on_exceed: "reject"
"#;

        let config: FlowControlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.version, "1.0");
        assert_eq!(config.rules.len(), 1);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_toml_parsing() {
        let toml = r#"
version = "1.0"

[global]
storage = "memory"
cache = "memory"
metrics = "prometheus"

[[rules]]
id = "test_rule"
name = "Test Rule"
priority = 100

[[rules.matchers]]
type = "User"
user_ids = ["*"]

[[rules.limiters]]
type = "TokenBucket"
capacity = 1000
refill_rate = 100

[rules.action]
on_exceed = "reject"
"#;

        let config: FlowControlConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.version, "1.0");
        assert_eq!(config.rules.len(), 1);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_flow_control_config_default() {
        let config = FlowControlConfig::default();
        assert_eq!(config.version, "0.1.0");
        assert_eq!(config.global.storage, StorageType::Memory);
        assert_eq!(config.global.cache, CacheBackend::Memory);
        assert_eq!(config.global.metrics, MetricsBackend::Prometheus);
        assert!(config.rules.is_empty());
    }

    #[test]
    fn test_global_config_default() {
        let global = GlobalConfig::default();
        assert_eq!(global.storage, StorageType::Memory);
        assert_eq!(global.cache, CacheBackend::Memory);
        assert_eq!(global.metrics, MetricsBackend::Prometheus);
    }

    #[test]
    fn test_global_config_validate_success() {
        let global = GlobalConfig {
            storage: StorageType::Memory,
            cache: CacheBackend::Memory,
            metrics: MetricsBackend::Prometheus,
            trusted_proxies: TrustedProxyConfig::default(),
        };
        assert!(global.validate().is_ok());
    }

    #[test]
    fn test_config_validate_empty_version() {
        let config = FlowControlConfig {
            version: "".to_string(),
            global: GlobalConfig::default(),
            rules: vec![Rule {
                id: "test".to_string(),
                name: "Test".to_string(),
                priority: 100,
                matchers: vec![Matcher::User {
                    user_ids: vec!["*".to_string()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 1000,
                    refill_rate: 100,
                }],
                action: ActionConfig::default(),
            }],
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("版本号不能为空"));
    }

    #[test]
    fn test_config_compute_hash() {
        let config1 = FlowControlConfig::default();
        let config2 = FlowControlConfig::default();
        assert_eq!(config1.compute_hash(), config2.compute_hash());
    }

    #[test]
    fn test_config_is_same_as() {
        let config1 = FlowControlConfig::default();
        let config2 = FlowControlConfig::default();
        assert!(config1.is_same_as(&config2));
    }

    #[test]
    fn test_config_compare_version() {
        let config1 = FlowControlConfig {
            version: "1.0".to_string(),
            ..Default::default()
        };
        let config2 = FlowControlConfig {
            version: "2.0".to_string(),
            ..Default::default()
        };
        assert_eq!(config1.compare_version(&config2), std::cmp::Ordering::Less);
        assert_eq!(
            config2.compare_version(&config1),
            std::cmp::Ordering::Greater
        );
        assert_eq!(config1.compare_version(&config1), std::cmp::Ordering::Equal);
    }

    #[test]
    fn test_config_create_change_record() {
        let old_config = FlowControlConfig {
            version: "1.0".to_string(),
            ..Default::default()
        };
        let new_config = FlowControlConfig {
            version: "2.0".to_string(),
            ..Default::default()
        };

        let record = new_config.create_change_record(
            Some(&old_config),
            ChangeSource::Manual {
                operator: "test".to_string(),
            },
        );

        assert_eq!(record.old_version, Some("1.0".to_string()));
        assert_eq!(record.new_version, "2.0");
        assert!(!record.changes.is_empty());
    }

    #[test]
    fn test_config_create_change_record_no_old() {
        let config = FlowControlConfig::default();
        let record = config.create_change_record(None, ChangeSource::Poll);
        assert!(record.old_version.is_none());
        assert_eq!(record.changes, vec!["初始配置"]);
    }

    #[test]
    fn test_rule_validate_empty_id() {
        let rule = Rule {
            id: "".to_string(),
            name: "Test".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 1000,
                refill_rate: 100,
            }],
            action: ActionConfig::default(),
        };
        let result = rule.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("规则ID不能为空"));
    }

    #[test]
    fn test_rule_validate_empty_name() {
        let rule = Rule {
            id: "test".to_string(),
            name: "".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 1000,
                refill_rate: 100,
            }],
            action: ActionConfig::default(),
        };
        let result = rule.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("规则名称不能为空"));
    }

    #[test]
    fn test_rule_validate_empty_matchers() {
        let rule = Rule {
            id: "test".to_string(),
            name: "Test".to_string(),
            priority: 100,
            matchers: vec![],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 1000,
                refill_rate: 100,
            }],
            action: ActionConfig::default(),
        };
        let result = rule.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("规则至少需要一个匹配器"));
    }

    #[test]
    fn test_rule_validate_empty_limiters() {
        let rule = Rule {
            id: "test".to_string(),
            name: "Test".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![],
            action: ActionConfig::default(),
        };
        let result = rule.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("规则至少需要一个限流器"));
    }

    #[test]
    fn test_matcher_validate_user_empty() {
        let matcher = Matcher::User { user_ids: vec![] };
        let result = matcher.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("用户ID列表不能为空"));
    }

    #[test]
    fn test_matcher_validate_ip_empty() {
        let matcher = Matcher::Ip { ip_ranges: vec![] };
        let result = matcher.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("IP范围列表不能为空"));
    }

    #[test]
    fn test_matcher_validate_geo_empty() {
        let matcher = Matcher::Geo { countries: vec![] };
        let result = matcher.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("国家列表不能为空"));
    }

    #[test]
    fn test_matcher_validate_api_version_empty() {
        let matcher = Matcher::ApiVersion { versions: vec![] };
        let result = matcher.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("API版本列表不能为空"));
    }

    #[test]
    fn test_matcher_validate_device_empty() {
        let matcher = Matcher::Device {
            device_types: vec![],
        };
        let result = matcher.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("设备类型列表不能为空"));
    }

    #[test]
    fn test_matcher_validate_custom_empty_name() {
        let matcher = Matcher::Custom {
            name: "".to_string(),
            config: serde_json::json!({"key": "value"}),
        };
        let result = matcher.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("自定义匹配器名称不能为空"));
    }

    #[test]
    fn test_matcher_validate_custom_null_config() {
        let matcher = Matcher::Custom {
            name: "test".to_string(),
            config: serde_json::Value::Null,
        };
        let result = matcher.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("自定义匹配器配置不能为空"));
    }

    #[test]
    fn test_limiter_config_validate_token_bucket_zero_capacity() {
        let config = LimiterConfig::TokenBucket {
            capacity: 0,
            refill_rate: 100,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("令牌桶容量不能为0"));
    }

    #[test]
    fn test_limiter_config_validate_token_bucket_zero_refill() {
        let config = LimiterConfig::TokenBucket {
            capacity: 1000,
            refill_rate: 0,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("填充速率不能为0"));
    }

    #[test]
    fn test_limiter_config_validate_sliding_window_zero_requests() {
        let config = LimiterConfig::SlidingWindow {
            window_size: "1s".to_string(),
            max_requests: 0,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("最大请求数不能为0"));
    }

    #[test]
    fn test_limiter_config_validate_fixed_window_zero_requests() {
        let config = LimiterConfig::FixedWindow {
            window_size: "1s".to_string(),
            max_requests: 0,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("最大请求数不能为0"));
    }

    #[test]
    fn test_limiter_config_validate_quota_valid() {
        // QuotaType is an enum, so it's always valid at compile time
        let config = LimiterConfig::Quota {
            quota_type: QuotaType::Count,
            limit: 1000,
            window: "1h".to_string(),
            alert_threshold: None,
            overdraft: None,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_limiter_config_validate_quota_zero_limit() {
        let config = LimiterConfig::Quota {
            quota_type: QuotaType::Token,
            limit: 0,
            window: "1h".to_string(),
            alert_threshold: None,
            overdraft: None,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("配额限制不能为0"));
    }

    #[test]
    fn test_limiter_config_validate_quota_threshold_over_100() {
        let config = LimiterConfig::Quota {
            quota_type: QuotaType::Token,
            limit: 1000,
            window: "1h".to_string(),
            alert_threshold: Some(101),
            overdraft: None,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("告警阈值不能超过100%"));
    }

    #[test]
    fn test_limiter_config_validate_concurrency_zero() {
        let config = LimiterConfig::Concurrency { max_concurrent: 0 };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("最大并发数不能为0"));
    }

    #[test]
    fn test_limiter_config_validate_custom_empty_name() {
        let config = LimiterConfig::Custom {
            name: "".to_string(),
            config: serde_json::json!({"key": "value"}),
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("自定义限流器名称不能为空"));
    }

    #[test]
    fn test_limiter_config_validate_custom_null_config() {
        let config = LimiterConfig::Custom {
            name: "test".to_string(),
            config: serde_json::Value::Null,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("自定义限流器配置不能为空"));
    }

    #[test]
    fn test_parse_window_size_seconds() {
        let duration = parse_window_size("10s").unwrap();
        assert_eq!(duration, std::time::Duration::from_secs(10));
    }

    #[test]
    fn test_parse_window_size_minutes() {
        let duration = parse_window_size("5m").unwrap();
        assert_eq!(duration, std::time::Duration::from_secs(300));
    }

    #[test]
    fn test_parse_window_size_hours() {
        let duration = parse_window_size("2h").unwrap();
        assert_eq!(duration, std::time::Duration::from_secs(7200));
    }

    #[test]
    fn test_parse_window_size_days() {
        let duration = parse_window_size("1d").unwrap();
        assert_eq!(duration, std::time::Duration::from_secs(86400));
    }

    #[test]
    fn test_parse_window_size_milliseconds() {
        let duration = parse_window_size("500ms").unwrap();
        assert_eq!(duration, std::time::Duration::from_millis(500));
    }

    #[test]
    fn test_parse_window_size_empty() {
        let result = parse_window_size("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("窗口大小不能为空"));
    }

    #[test]
    fn test_parse_window_size_no_unit() {
        let result = parse_window_size("10");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("缺少单位"));
    }

    #[test]
    fn test_parse_window_size_invalid_unit() {
        let result = parse_window_size("10x");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("不支持的单位"));
    }

    #[test]
    fn test_parse_window_size_zero() {
        let result = parse_window_size("0s");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("窗口大小必须大于0"));
    }

    #[test]
    fn test_overdraft_config_validate_enabled_zero() {
        let config = OverdraftConfig {
            enabled: true,
            max_overdraft: 0,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("最大透支量不能为0"));
    }

    #[test]
    fn test_overdraft_config_validate_disabled() {
        let config = OverdraftConfig {
            enabled: false,
            max_overdraft: 0,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_action_config_default() {
        let action = ActionConfig::default();
        assert_eq!(action.on_exceed, Action::Reject);
        assert!(action.ban.is_none());
    }

    #[test]
    fn test_action_config_validate_success() {
        let action = ActionConfig {
            on_exceed: Action::Reject,
            ban: None,
        };
        assert!(action.validate().is_ok());
    }

    #[test]
    fn test_ban_config_validate_threshold_zero() {
        let config = BanConfig {
            threshold: 0,
            initial_duration: "1m".to_string(),
            backoff_multiplier: 2.0,
            max_duration: "1h".to_string(),
            scope: BanScope::Ip,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("封禁阈值不能为0"));
    }

    #[test]
    fn test_ban_config_validate_backoff_zero() {
        let config = BanConfig {
            threshold: 10,
            initial_duration: "1m".to_string(),
            backoff_multiplier: 0.0,
            max_duration: "1h".to_string(),
            scope: BanScope::Ip,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("退避倍数必须大于0"));
    }

    #[test]
    fn test_config_builder_new() {
        let builder = ConfigBuilder::new();
        assert_eq!(builder.storage, StorageType::Memory);
        assert_eq!(builder.cache, CacheBackend::Memory);
        assert_eq!(builder.metrics, MetricsBackend::Prometheus);
        assert!(builder.rules.is_empty());
    }

    #[test]
    fn test_config_builder_default() {
        let builder = ConfigBuilder::default();
        assert_eq!(builder.storage, StorageType::Memory);
    }

    #[test]
    fn test_config_builder_with_storage() {
        let builder = ConfigBuilder::new().with_storage(StorageType::PostgreSQL);
        assert_eq!(builder.storage, StorageType::PostgreSQL);
    }

    #[test]
    fn test_config_builder_with_cache() {
        let builder = ConfigBuilder::new().with_cache(CacheBackend::Redis);
        assert_eq!(builder.cache, CacheBackend::Redis);
    }

    #[test]
    fn test_config_builder_with_metrics() {
        let builder = ConfigBuilder::new().with_metrics(MetricsBackend::None);
        assert_eq!(builder.metrics, MetricsBackend::None);
    }

    #[test]
    fn test_config_builder_build_success() {
        let config = ConfigBuilder::new()
            .with_rule(|rule| {
                rule.id("test")
                    .name("Test Rule")
                    .user_matcher(vec!["*".to_string()])
                    .token_bucket(1000, 100)
            })
            .build();

        assert!(config.is_ok());
        let config = config.unwrap();
        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].id, "test");
    }

    #[test]
    fn test_config_builder_build_empty_rules() {
        let result = ConfigBuilder::new().build();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("至少需要一个规则"));
    }

    #[test]
    fn test_rule_builder_new() {
        let builder = RuleBuilder::new();
        assert!(builder.id.is_empty());
        assert!(builder.name.is_empty());
        assert_eq!(builder.priority, 100);
        assert!(builder.matchers.is_empty());
        assert!(builder.limiters.is_empty());
    }

    #[test]
    fn test_rule_builder_default() {
        let builder = RuleBuilder::default();
        assert!(builder.id.is_empty());
    }

    #[test]
    fn test_rule_builder_id() {
        let builder = RuleBuilder::new().id("my-rule");
        assert_eq!(builder.id, "my-rule");
    }

    #[test]
    fn test_rule_builder_name() {
        let builder = RuleBuilder::new().name("My Rule");
        assert_eq!(builder.name, "My Rule");
    }

    #[test]
    fn test_rule_builder_priority() {
        let builder = RuleBuilder::new().priority(200);
        assert_eq!(builder.priority, 200);
    }

    #[test]
    fn test_rule_builder_user_matcher() {
        let builder = RuleBuilder::new().user_matcher(vec!["user1".to_string()]);
        assert_eq!(builder.matchers.len(), 1);
    }

    #[test]
    fn test_rule_builder_ip_matcher() {
        let builder = RuleBuilder::new().ip_matcher(vec!["192.168.1.0/24".to_string()]);
        assert_eq!(builder.matchers.len(), 1);
    }

    #[test]
    fn test_rule_builder_token_bucket() {
        let builder = RuleBuilder::new().token_bucket(1000, 100);
        assert_eq!(builder.limiters.len(), 1);
    }

    #[test]
    fn test_rule_builder_fixed_window() {
        let builder = RuleBuilder::new().fixed_window("1s", 100);
        assert_eq!(builder.limiters.len(), 1);
    }

    #[test]
    fn test_rule_builder_sliding_window() {
        let builder = RuleBuilder::new().sliding_window("1s", 100);
        assert_eq!(builder.limiters.len(), 1);
    }

    #[test]
    fn test_rule_builder_concurrency_limit() {
        let builder = RuleBuilder::new().concurrency_limit(50);
        assert_eq!(builder.limiters.len(), 1);
    }

    #[test]
    fn test_rule_builder_on_reject() {
        let builder = RuleBuilder::new().on_reject();
        assert_eq!(builder.action.on_exceed, Action::Reject);
    }

    #[test]
    fn test_rule_builder_on_allow() {
        let builder = RuleBuilder::new().on_allow();
        assert_eq!(builder.action.on_exceed, Action::Allow);
    }

    #[test]
    fn test_rule_builder_on_degrade() {
        let builder = RuleBuilder::new().on_degrade();
        assert_eq!(builder.action.on_exceed, Action::Degrade);
    }

    #[test]
    fn test_rule_builder_build_success() {
        let rule = RuleBuilder::new()
            .id("test")
            .name("Test")
            .user_matcher(vec!["*".to_string()])
            .token_bucket(1000, 100)
            .build();

        assert!(rule.is_ok());
        let rule = rule.unwrap();
        assert_eq!(rule.id, "test");
    }

    #[test]
    fn test_rule_builder_build_empty_id() {
        let result = RuleBuilder::new()
            .name("Test")
            .user_matcher(vec!["*".to_string()])
            .token_bucket(1000, 100)
            .build();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("规则ID不能为空"));
    }

    #[test]
    fn test_rule_builder_build_empty_name() {
        let result = RuleBuilder::new()
            .id("test")
            .user_matcher(vec!["*".to_string()])
            .token_bucket(1000, 100)
            .build();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("规则名称不能为空"));
    }

    #[test]
    fn test_rule_builder_build_empty_matchers() {
        let result = RuleBuilder::new()
            .id("test")
            .name("Test")
            .token_bucket(1000, 100)
            .build();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("规则至少需要一个匹配器"));
    }

    #[test]
    fn test_rule_builder_build_empty_limiters() {
        let result = RuleBuilder::new()
            .id("test")
            .name("Test")
            .user_matcher(vec!["*".to_string()])
            .build();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("规则至少需要一个限流器"));
    }

    #[test]
    fn test_config_history_new() {
        let history = ConfigHistory::new(50);
        assert_eq!(history.max_records(), 50);
        assert!(history.get_records().is_empty());
    }

    #[test]
    fn test_config_history_default() {
        let history = ConfigHistory::default();
        assert_eq!(history.max_records(), 100);
    }

    #[test]
    fn test_config_history_add_record() {
        let mut history = ConfigHistory::new(10);
        let record = ConfigChangeRecord {
            timestamp: chrono::Utc::now(),
            old_version: None,
            new_version: "1.0".to_string(),
            old_hash: None,
            new_hash: "abc".to_string(),
            source: ChangeSource::Poll,
            changes: vec!["初始配置".to_string()],
        };
        history.add_record(record);
        assert_eq!(history.get_records().len(), 1);
    }

    #[test]
    fn test_config_history_max_records() {
        let mut history = ConfigHistory::new(3);
        for i in 0..5 {
            let record = ConfigChangeRecord {
                timestamp: chrono::Utc::now(),
                old_version: None,
                new_version: format!("{}", i),
                old_hash: None,
                new_hash: format!("hash{}", i),
                source: ChangeSource::Poll,
                changes: vec![],
            };
            history.add_record(record);
        }
        assert_eq!(history.get_records().len(), 3);
        assert_eq!(history.get_latest().unwrap().new_version, "4");
    }

    #[test]
    fn test_config_history_clear() {
        let mut history = ConfigHistory::new(10);
        let record = ConfigChangeRecord {
            timestamp: chrono::Utc::now(),
            old_version: None,
            new_version: "1.0".to_string(),
            old_hash: None,
            new_hash: "abc".to_string(),
            source: ChangeSource::Poll,
            changes: vec![],
        };
        history.add_record(record);
        history.clear();
        assert!(history.get_records().is_empty());
    }

    #[test]
    fn test_change_source_variants() {
        let sources = vec![
            ChangeSource::Manual {
                operator: "admin".to_string(),
            },
            ChangeSource::Poll,
            ChangeSource::Watch,
            ChangeSource::Api,
            ChangeSource::Reload,
            ChangeSource::Rollback {
                target_version: "1.0".to_string(),
            },
        ];
        assert_eq!(sources.len(), 6);
    }

    #[test]
    fn test_config_builder_with_trusted_proxies() {
        let trusted = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".to_string()],
            max_hops: 5,
        };
        let builder = ConfigBuilder::new().with_trusted_proxies(trusted.clone());
        assert!(builder.trusted_proxies.enabled);
        assert_eq!(
            builder.trusted_proxies.proxies,
            vec!["10.0.0.1".to_string()]
        );
        assert_eq!(builder.trusted_proxies.max_hops, 5);
    }

    fn make_rule(id: &str) -> Rule {
        Rule {
            id: id.to_string(),
            name: format!("Rule {}", id),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 1000,
                refill_rate: 100,
            }],
            action: ActionConfig {
                on_exceed: Action::Reject,
                ban: None,
            },
        }
    }

    #[test]
    fn test_diff_changes_global_config_changed() {
        let old_config = FlowControlConfig {
            version: "1.0".to_string(),
            global: GlobalConfig {
                storage: StorageType::Memory,
                cache: CacheBackend::Memory,
                metrics: MetricsBackend::Prometheus,
                trusted_proxies: TrustedProxyConfig::default(),
            },
            rules: vec![make_rule("r1")],
        };
        let new_config = FlowControlConfig {
            version: "1.0".to_string(),
            global: GlobalConfig {
                storage: StorageType::Memory,
                cache: CacheBackend::Redis, // 改变缓存类型
                metrics: MetricsBackend::Prometheus,
                trusted_proxies: TrustedProxyConfig::default(),
            },
            rules: vec![make_rule("r1")],
        };

        let record = new_config.create_change_record(
            Some(&old_config),
            ChangeSource::Manual {
                operator: "test".to_string(),
            },
        );
        assert!(record.changes.iter().any(|c| c.contains("全局配置已变更")));
    }

    #[test]
    fn test_diff_changes_rule_count_changed() {
        let old_config = FlowControlConfig {
            version: "1.0".to_string(),
            rules: vec![make_rule("r1"), make_rule("r2")],
            ..Default::default()
        };
        let new_config = FlowControlConfig {
            version: "1.0".to_string(),
            rules: vec![make_rule("r1")],
            ..Default::default()
        };

        let record = new_config.create_change_record(Some(&old_config), ChangeSource::Poll);
        assert!(record.changes.iter().any(|c| c.contains("规则数量变更")));
    }

    #[test]
    fn test_diff_changes_rules_added() {
        let old_config = FlowControlConfig {
            version: "1.0".to_string(),
            rules: vec![make_rule("r1")],
            ..Default::default()
        };
        let new_config = FlowControlConfig {
            version: "1.0".to_string(),
            rules: vec![make_rule("r1"), make_rule("r2")],
            ..Default::default()
        };

        let record = new_config.create_change_record(Some(&old_config), ChangeSource::Api);
        assert!(record.changes.iter().any(|c| c.contains("新增规则")));
    }

    #[test]
    fn test_diff_changes_rules_removed() {
        let old_config = FlowControlConfig {
            version: "1.0".to_string(),
            rules: vec![make_rule("r1"), make_rule("r2")],
            ..Default::default()
        };
        let new_config = FlowControlConfig {
            version: "1.0".to_string(),
            rules: vec![make_rule("r1")],
            ..Default::default()
        };

        let record = new_config.create_change_record(Some(&old_config), ChangeSource::Watch);
        assert!(record.changes.iter().any(|c| c.contains("移除规则")));
    }

    #[test]
    fn test_diff_changes_no_changes() {
        let old_config = FlowControlConfig {
            version: "1.0".to_string(),
            rules: vec![make_rule("r1")],
            ..Default::default()
        };
        let new_config = old_config.clone();

        let record = new_config.create_change_record(Some(&old_config), ChangeSource::Reload);
        assert!(record.changes.iter().any(|c| c.contains("配置内容无变化")));
    }
}

// ============================================================================
// Confers Integration (可选特性)
// ============================================================================
// 注意: confers API 不提供 ConfigMap, Validate, Sanitize traits
// 如需使用 confers 的完整功能，请为 FlowControlConfig derive confers::Config
// 当前实现保持 confers feature 可编译，但不提供额外的 trait 实现

// ============================================================================
// ConfigBuilder - 程序化配置构建（始终可用，不依赖confers）
// ============================================================================

/// 配置构建器
///
/// 提供流式API构建FlowControlConfig配置，不依赖confers库.
///
/// # 示例
///
/// ```rust
/// use limiteron::config::ConfigBuilder;
///
/// let config = ConfigBuilder::new()
///     .with_storage("memory".into())
///     .with_cache("memory".into())
///     .with_metrics("prometheus".into())
///     .with_rule(|rule| {
///         rule.id("default")
///             .name("Default Rule")
///             .priority(100)
///             .token_bucket(1000, 100)
///     })
///     .build();
/// ```
#[derive(Clone, Debug)]
pub struct ConfigBuilder {
    /// 全局配置
    storage: StorageType,
    cache: CacheBackend,
    metrics: MetricsBackend,
    /// 可信代理配置
    trusted_proxies: TrustedProxyConfig,
    /// 规则列表
    rules: Vec<RuleBuilder>,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self {
            storage: StorageType::Memory,
            cache: CacheBackend::Memory,
            metrics: MetricsBackend::Prometheus,
            trusted_proxies: TrustedProxyConfig::default(),
            rules: Vec::new(),
        }
    }
}

impl ConfigBuilder {
    /// 创建新的配置构建器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置存储类型
    pub fn with_storage(mut self, storage: StorageType) -> Self {
        self.storage = storage;
        self
    }

    /// 设置缓存类型
    pub fn with_cache(mut self, cache: CacheBackend) -> Self {
        self.cache = cache;
        self
    }

    /// 设置可信代理配置
    pub fn with_trusted_proxies(mut self, config: TrustedProxyConfig) -> Self {
        self.trusted_proxies = config;
        self
    }

    /// 设置指标类型
    pub fn with_metrics(mut self, metrics: MetricsBackend) -> Self {
        self.metrics = metrics;
        self
    }

    /// 添加规则
    pub fn with_rule<F>(mut self, f: F) -> Self
    where
        F: FnOnce(RuleBuilder) -> RuleBuilder,
    {
        let rule = f(RuleBuilder::new());
        self.rules.push(rule);
        self
    }

    /// 构建配置
    pub fn build(self) -> Result<FlowControlConfig, String> {
        let rules: Result<Vec<_>, _> = self.rules.into_iter().map(|r| r.build()).collect();
        let rules = rules?;

        if rules.is_empty() {
            return Err("至少需要一个规则".to_string());
        }

        let config = FlowControlConfig {
            version: "0.1.0".to_string(),
            global: GlobalConfig {
                storage: self.storage,
                cache: self.cache,
                metrics: self.metrics,
                trusted_proxies: self.trusted_proxies,
            },
            rules,
        };

        // 验证配置，验证失败时返回错误
        config.validate()?;
        Ok(config)
    }
}

/// 规则构建器
#[derive(Clone, Debug)]
pub struct RuleBuilder {
    id: String,
    name: String,
    priority: u16,
    matchers: Vec<Matcher>,
    limiters: Vec<LimiterConfig>,
    action: ActionConfig,
}

impl RuleBuilder {
    pub fn new() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            priority: 100,
            matchers: Vec::new(),
            limiters: Vec::new(),
            action: ActionConfig {
                on_exceed: Action::Reject,
                ban: None,
            },
        }
    }
}

impl Default for RuleBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn priority(mut self, priority: u16) -> Self {
        self.priority = priority;
        self
    }

    pub fn user_matcher(mut self, user_ids: Vec<String>) -> Self {
        self.matchers.push(Matcher::User { user_ids });
        self
    }

    pub fn ip_matcher(mut self, ip_ranges: Vec<String>) -> Self {
        self.matchers.push(Matcher::Ip { ip_ranges });
        self
    }

    pub fn token_bucket(mut self, capacity: u64, refill_rate: u64) -> Self {
        self.limiters.push(LimiterConfig::TokenBucket {
            capacity,
            refill_rate,
        });
        self
    }

    pub fn fixed_window(mut self, window_size: impl Into<String>, max_requests: u64) -> Self {
        self.limiters.push(LimiterConfig::FixedWindow {
            window_size: window_size.into(),
            max_requests,
        });
        self
    }

    pub fn sliding_window(mut self, window_size: impl Into<String>, max_requests: u64) -> Self {
        self.limiters.push(LimiterConfig::SlidingWindow {
            window_size: window_size.into(),
            max_requests,
        });
        self
    }

    pub fn concurrency_limit(mut self, max_concurrent: u64) -> Self {
        self.limiters
            .push(LimiterConfig::Concurrency { max_concurrent });
        self
    }

    pub fn on_reject(mut self) -> Self {
        self.action.on_exceed = Action::Reject;
        self
    }

    pub fn on_allow(mut self) -> Self {
        self.action.on_exceed = Action::Allow;
        self
    }

    pub fn on_degrade(mut self) -> Self {
        self.action.on_exceed = Action::Degrade;
        self
    }

    pub fn build(self) -> Result<Rule, String> {
        if self.id.is_empty() {
            return Err("规则ID不能为空".to_string());
        }
        if self.name.is_empty() {
            return Err("规则名称不能为空".to_string());
        }
        if self.matchers.is_empty() {
            return Err("规则至少需要一个匹配器".to_string());
        }
        if self.limiters.is_empty() {
            return Err("规则至少需要一个限流器".to_string());
        }

        Ok(Rule {
            id: self.id,
            name: self.name,
            priority: self.priority,
            matchers: self.matchers,
            limiters: self.limiters,
            action: self.action,
        })
    }
}
