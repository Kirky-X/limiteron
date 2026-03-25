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
mod rule;

pub use actions::{Action, ActionConfig, BanConfig, BanScope, CacheBackend, MetricsBackend};
pub use config::{GlobalConfig, StorageType, TrustedProxyConfig};
pub use history::{ChangeSource, ConfigChangeRecord, ConfigHistory};
pub use limiter::{parse_window_size, LimiterConfig, OverdraftConfig};
pub use rule::Matcher;
pub use rule::Rule;

// Re-export ConfigMatcher for backward compatibility
pub use Matcher as ConfigMatcher;

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
    fn test_limiter_config_validate_quota_empty_type() {
        let config = LimiterConfig::Quota {
            quota_type: "".to_string(),
            limit: 1000,
            window: "1h".to_string(),
            alert_threshold: None,
            overdraft: None,
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("配额类型不能为空"));
    }

    #[test]
    fn test_limiter_config_validate_quota_zero_limit() {
        let config = LimiterConfig::Quota {
            quota_type: "token".to_string(),
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
            quota_type: "token".to_string(),
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
}

// ============================================================================
// Confers Integration
// ============================================================================
// 使用 confers 库进行配置加载和验证：
//
// ```rust,ignore
// use confers::ConfigBuilder;
// use limiteron::config::FlowControlConfig;
//
// let config: FlowControlConfig = ConfigBuilder::new()
//     .file("config.toml")
//     .env_prefix("LIMITERON")
//     .build()?;
// ```
//
// 注意: ConfigBuilder 和 RuleBuilder 已移除，请使用 confers::ConfigBuilder
// 或直接构造配置结构体。
