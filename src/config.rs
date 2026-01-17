//! 配置模块
//!
//! 定义流量控制的配置结构。

use ahash::AHashSet as HashSet;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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

/// 配置变更来源
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChangeSource {
    /// 手动触发
    Manual { operator: String },
    /// 自动检测（轮询）
    Poll,
    /// 自动检测（Watch）
    Watch,
    /// API触发
    Api,
}

/// 配置变更记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigChangeRecord {
    pub timestamp: DateTime<Utc>,
    pub old_version: Option<String>,
    pub new_version: String,
    pub old_hash: Option<String>,
    pub new_hash: String,
    pub source: ChangeSource,
    pub changes: Vec<String>,
}

/// 配置变更历史
#[derive(Debug, Clone)]
pub struct ConfigHistory {
    records: Vec<ConfigChangeRecord>,
    max_records: usize,
}

impl ConfigHistory {
    pub fn new(max_records: usize) -> Self {
        Self {
            records: Vec::with_capacity(max_records),
            max_records,
        }
    }

    pub fn add_record(&mut self, record: ConfigChangeRecord) {
        self.records.push(record);
        if self.records.len() > self.max_records {
            self.records.remove(0);
        }
    }

    pub fn get_records(&self) -> &[ConfigChangeRecord] {
        &self.records
    }

    pub fn get_latest(&self) -> Option<&ConfigChangeRecord> {
        self.records.last()
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }
}

impl Default for ConfigHistory {
    fn default() -> Self {
        Self::new(100)
    }
}

/// 全局配置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlobalConfig {
    pub storage: String,
    pub cache: String,
    pub metrics: String,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
        }
    }
}

impl GlobalConfig {
    /// 校验全局配置
    pub fn validate(&self) -> Result<(), String> {
        let valid_storages = ["memory", "redis", "postgresql"];
        if !valid_storages.contains(&self.storage.as_str()) {
            return Err(format!(
                "无效的存储类型: {}, 有效值: {:?}",
                self.storage, valid_storages
            ));
        }

        let valid_caches = ["memory", "redis"];
        if !valid_caches.contains(&self.cache.as_str()) {
            return Err(format!(
                "无效的缓存类型: {}, 有效值: {:?}",
                self.cache, valid_caches
            ));
        }

        let valid_metrics = ["prometheus", "opentelemetry"];
        if !valid_metrics.contains(&self.metrics.as_str()) {
            return Err(format!(
                "无效的指标类型: {}, 有效值: {:?}",
                self.metrics, valid_metrics
            ));
        }

        Ok(())
    }
}

/// 规则配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub priority: u16,
    pub matchers: Vec<Matcher>,
    pub limiters: Vec<LimiterConfig>,
    pub action: ActionConfig,
}

impl Rule {
    /// 校验规则
    pub fn validate(&self) -> Result<(), String> {
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

        // 校验匹配器
        for (index, matcher) in self.matchers.iter().enumerate() {
            matcher
                .validate()
                .map_err(|e| format!("匹配器[{}]: {}", index, e))?;
        }

        // 校验限流器
        for (index, limiter) in self.limiters.iter().enumerate() {
            limiter
                .validate()
                .map_err(|e| format!("限流器[{}]: {}", index, e))?;
        }

        // 校验动作
        self.action.validate()?;

        Ok(())
    }
}

/// 匹配器
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Matcher {
    User {
        user_ids: Vec<String>,
    },
    Ip {
        ip_ranges: Vec<String>,
    },
    Geo {
        countries: Vec<String>,
    },
    ApiVersion {
        versions: Vec<String>,
    },
    Device {
        device_types: Vec<String>,
    },
    /// 自定义匹配器
    Custom {
        /// 匹配器名称
        name: String,
        /// 匹配器配置（JSON格式）
        config: serde_json::Value,
    },
}

impl Matcher {
    /// 校验匹配器
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Matcher::User { user_ids } => {
                if user_ids.is_empty() {
                    return Err("用户ID列表不能为空".to_string());
                }
            }
            Matcher::Ip { ip_ranges } => {
                if ip_ranges.is_empty() {
                    return Err("IP范围列表不能为空".to_string());
                }
            }
            Matcher::Geo { countries } => {
                if countries.is_empty() {
                    return Err("国家列表不能为空".to_string());
                }
            }
            Matcher::ApiVersion { versions } => {
                if versions.is_empty() {
                    return Err("API版本列表不能为空".to_string());
                }
            }
            Matcher::Device { device_types } => {
                if device_types.is_empty() {
                    return Err("设备类型列表不能为空".to_string());
                }
            }
            Matcher::Custom { name, config } => {
                if name.is_empty() {
                    return Err("自定义匹配器名称不能为空".to_string());
                }
                if config.is_null() {
                    return Err("自定义匹配器配置不能为空".to_string());
                }
            }
        }
        Ok(())
    }
}

/// 限流器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LimiterConfig {
    TokenBucket {
        capacity: u64,
        refill_rate: u64,
    },
    SlidingWindow {
        window_size: String,
        max_requests: u64,
    },
    FixedWindow {
        window_size: String,
        max_requests: u64,
    },
    Quota {
        quota_type: String,
        limit: u64,
        window: String,
        overdraft: Option<OverdraftConfig>,
    },
    Concurrency {
        max_concurrent: u64,
    },
    /// 自定义限流器
    Custom {
        /// 限流器名称
        name: String,
        /// 限流器配置（JSON格式）
        config: serde_json::Value,
    },
}

impl LimiterConfig {
    /// 校验限流器
    pub fn validate(&self) -> Result<(), String> {
        match self {
            LimiterConfig::TokenBucket {
                capacity,
                refill_rate,
            } => {
                if *capacity == 0 {
                    return Err("令牌桶容量不能为0".to_string());
                }
                if *refill_rate == 0 {
                    return Err("填充速率不能为0".to_string());
                }
            }
            LimiterConfig::SlidingWindow {
                window_size,
                max_requests,
            } => {
                if *max_requests == 0 {
                    return Err("最大请求数不能为0".to_string());
                }
                Self::validate_window_size(window_size)?;
            }
            LimiterConfig::FixedWindow {
                window_size,
                max_requests,
            } => {
                if *max_requests == 0 {
                    return Err("最大请求数不能为0".to_string());
                }
                Self::validate_window_size(window_size)?;
            }
            LimiterConfig::Quota {
                quota_type,
                limit,
                window,
                overdraft,
            } => {
                if quota_type.is_empty() {
                    return Err("配额类型不能为空".to_string());
                }
                if *limit == 0 {
                    return Err("配额限制不能为0".to_string());
                }
                Self::validate_window_size(window)?;
                if let Some(overdraft) = overdraft {
                    overdraft.validate()?;
                }
            }
            LimiterConfig::Concurrency { max_concurrent } => {
                if *max_concurrent == 0 {
                    return Err("最大并发数不能为0".to_string());
                }
            }
            LimiterConfig::Custom { name, config } => {
                if name.is_empty() {
                    return Err("自定义限流器名称不能为空".to_string());
                }
                if config.is_null() {
                    return Err("自定义限流器配置不能为空".to_string());
                }
            }
        }
        Ok(())
    }

    /// 校验窗口大小
    fn validate_window_size(window_size: &str) -> Result<(), String> {
        // 简单校验窗口大小格式
        if window_size.is_empty() {
            return Err("窗口大小不能为空".to_string());
        }
        // TODO: 添加更详细的格式校验
        Ok(())
    }
}

/// 透支配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverdraftConfig {
    pub enabled: bool,
    pub max_overdraft: u64,
}

impl OverdraftConfig {
    /// 校验透支配置
    pub fn validate(&self) -> Result<(), String> {
        if self.enabled && self.max_overdraft == 0 {
            return Err("透支启用时，最大透支量不能为0".to_string());
        }
        Ok(())
    }
}

/// 动作配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionConfig {
    pub on_exceed: String,
    pub ban: Option<BanConfig>,
}

impl Default for ActionConfig {
    fn default() -> Self {
        Self {
            on_exceed: "reject".to_string(),
            ban: None,
        }
    }
}

impl ActionConfig {
    /// 校验动作配置
    pub fn validate(&self) -> Result<(), String> {
        let valid_actions = ["reject", "allow", "degrade"];
        if !valid_actions.contains(&self.on_exceed.as_str()) {
            return Err(format!(
                "无效的动作: {}, 有效值: {:?}",
                self.on_exceed, valid_actions
            ));
        }

        if let Some(ban) = &self.ban {
            ban.validate()?;
        }

        Ok(())
    }
}

/// 封禁配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanConfig {
    pub threshold: u32,
    pub initial_duration: String,
    pub backoff_multiplier: f64,
    pub max_duration: String,
    pub scope: String,
}

impl BanConfig {
    /// 校验封禁配置
    pub fn validate(&self) -> Result<(), String> {
        if self.threshold == 0 {
            return Err("封禁阈值不能为0".to_string());
        }

        if self.backoff_multiplier <= 0.0 {
            return Err("退避倍数必须大于0".to_string());
        }

        let valid_scopes = ["ip", "user", "mac"];
        if !valid_scopes.contains(&self.scope.as_str()) {
            return Err(format!(
                "无效的封禁范围: {}, 有效值: {:?}",
                self.scope, valid_scopes
            ));
        }

        Ok(())
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
                storage: "memory".to_string(),
                cache: "memory".to_string(),
                metrics: "prometheus".to_string(),
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
                storage: "memory".to_string(),
                cache: "memory".to_string(),
                metrics: "prometheus".to_string(),
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
                    on_exceed: "reject".to_string(),
                    ban: None,
                },
            }],
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_storage() {
        let config = FlowControlConfig {
            version: "1.0".to_string(),
            global: GlobalConfig {
                storage: "invalid".to_string(),
                cache: "memory".to_string(),
                metrics: "prometheus".to_string(),
            },
            rules: vec![],
        };

        assert!(config.validate().is_err());
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
                on_exceed: "reject".to_string(),
                ban: None,
            },
        };

        let config = FlowControlConfig {
            version: "1.0".to_string(),
            global: GlobalConfig {
                storage: "memory".to_string(),
                cache: "memory".to_string(),
                metrics: "prometheus".to_string(),
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
}
