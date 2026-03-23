// Copyright (c) 2026, Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! 配置模块
//!
//! 定义流量控制的配置结构。

use crate::constants::{VALID_CACHE_TYPES, VALID_METRICS_TYPES, VALID_STORAGE_TYPES};
use ahash::AHashSet as HashSet;
use chrono::{DateTime, Utc};
use ipnet::IpNet;
use serde::{Deserialize, Serialize};

/// 超限动作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// 拒绝请求
    Reject,
    /// 允许请求通过（仅记录）
    Allow,
    /// 降级处理
    Degrade,
}

impl Default for Action {
    fn default() -> Self {
        Self::Reject
    }
}

impl Action {
    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "reject" => Some(Self::Reject),
            "allow" => Some(Self::Allow),
            "degrade" => Some(Self::Degrade),
            _ => None,
        }
    }

    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Reject => "reject",
            Self::Allow => "allow",
            Self::Degrade => "degrade",
        }
    }
}

/// 封禁范围
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BanScope {
    /// 按 IP 封禁
    Ip,
    /// 按用户封禁
    User,
    /// 按 MAC 地址封禁
    Mac,
}

impl Default for BanScope {
    fn default() -> Self {
        Self::Ip
    }
}

impl BanScope {
    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "ip" => Some(Self::Ip),
            "user" => Some(Self::User),
            "mac" => Some(Self::Mac),
            _ => None,
        }
    }

    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ip => "ip",
            Self::User => "user",
            Self::Mac => "mac",
        }
    }
}

/// 缓存后端类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheBackend {
    /// 内存缓存
    Memory,
    /// Redis 缓存
    Redis,
    /// 无缓存
    None,
}

impl Default for CacheBackend {
    fn default() -> Self {
        Self::Memory
    }
}

impl CacheBackend {
    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "memory" => Some(Self::Memory),
            "redis" => Some(Self::Redis),
            "none" => Some(Self::None),
            _ => None,
        }
    }

    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Redis => "redis",
            Self::None => "none",
        }
    }
}

/// 指标后端类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricsBackend {
    /// Prometheus
    Prometheus,
    /// StatsD
    Statsd,
    /// 无指标
    None,
}

impl Default for MetricsBackend {
    fn default() -> Self {
        Self::Prometheus
    }
}

impl MetricsBackend {
    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "prometheus" => Some(Self::Prometheus),
            "statsd" => Some(Self::Statsd),
            "none" => Some(Self::None),
            _ => None,
        }
    }

    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Prometheus => "prometheus",
            Self::Statsd => "statsd",
            Self::None => "none",
        }
    }
}

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
    /// 重新加载
    Reload,
    /// 回滚操作
    Rollback { target_version: String },
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
    /// 可信代理配置（用于安全提取客户端 IP）
    #[serde(default)]
    pub trusted_proxies: TrustedProxyConfig,
}

/// 可信代理配置
///
/// 用于从 X-Forwarded-For 头中安全提取真实客户端 IP 地址。
/// 配置可信代理列表后，系统会从右向左查找第一个非可信代理的 IP 作为客户端 IP。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedProxyConfig {
    /// 是否启用可信代理模式
    #[serde(default)]
    pub enabled: bool,
    /// 可信代理 IP 列表（支持 CIDR 表示法）
    #[serde(default)]
    pub proxies: Vec<String>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
            trusted_proxies: TrustedProxyConfig::default(),
        }
    }
}

impl Default for TrustedProxyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            proxies: Vec::new(),
        }
    }
}

impl TrustedProxyConfig {
    /// 校验可信代理配置
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        for proxy in &self.proxies {
            if let Err(e) = Self::parse_cidr_or_ip(proxy) {
                return Err(format!("无效的代理地址 '{}': {}", proxy, e));
            }
        }
        Ok(())
    }

    /// 解析 CIDR 或单个 IP 地址
    fn parse_cidr_or_ip(s: &str) -> Result<(), String> {
        if s.contains('/') {
            s.parse::<IpNet>().map(|_| ()).map_err(|e| e.to_string())
        } else {
            s.parse::<std::net::IpAddr>()
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
    }

    /// 检查 IP 是否在可信代理列表中
    pub fn is_trusted(&self, ip: &str) -> bool {
        if !self.enabled || self.proxies.is_empty() {
            return false;
        }
        let Ok(ip_addr) = ip.parse::<std::net::IpAddr>() else {
            return false;
        };
        for proxy in &self.proxies {
            if proxy.contains('/') {
                if let Ok(network) = proxy.parse::<IpNet>() {
                    if network.contains(&ip_addr) {
                        return true;
                    }
                }
            } else if proxy == ip {
                return true;
            }
        }
        false
    }
}

impl GlobalConfig {
    /// 校验全局配置
    pub fn validate(&self) -> Result<(), String> {
        if !VALID_STORAGE_TYPES.contains(&self.storage.as_str()) {
            return Err(format!(
                "无效的存储类型: {}, 有效值: {:?}",
                self.storage, VALID_STORAGE_TYPES
            ));
        }

        if !VALID_CACHE_TYPES.contains(&self.cache.as_str()) {
            return Err(format!(
                "无效的缓存类型: {}, 有效值: {:?}",
                self.cache, VALID_CACHE_TYPES
            ));
        }

        if !VALID_METRICS_TYPES.contains(&self.metrics.as_str()) {
            return Err(format!(
                "无效的指标类型: {}, 有效值: {:?}",
                self.metrics, VALID_METRICS_TYPES
            ));
        }

        // 校验可信代理配置
        self.trusted_proxies.validate()?;

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
        /// 告警触发阈值（使用百分比 0-100），超过此比例时触发告警
        /// 默认值：80，即使用率达到 80% 时触发告警
        alert_threshold: Option<u8>,
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
                alert_threshold,
                overdraft,
            } => {
                if quota_type.is_empty() {
                    return Err("配额类型不能为空".to_string());
                }
                if *limit == 0 {
                    return Err("配额限制不能为0".to_string());
                }
                if let Some(threshold) = alert_threshold {
                    if *threshold > 100 {
                        return Err("告警阈值不能超过100%".to_string());
                    }
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
        parse_window_size(window_size).map(|_| ())
    }
}

pub(crate) fn parse_window_size(window_size: &str) -> Result<std::time::Duration, String> {
    let trimmed = window_size.trim();
    if trimmed.is_empty() {
        return Err("窗口大小不能为空".to_string());
    }

    let split_index = trimmed
        .find(|c: char| c.is_alphabetic())
        .unwrap_or(trimmed.len());
    let (num_part, unit_part) = trimmed.split_at(split_index);
    let num_str = num_part.trim();
    let unit = unit_part.trim().to_lowercase();

    if num_str.is_empty() {
        return Err("窗口大小格式错误：缺少数字部分".to_string());
    }

    if unit.is_empty() {
        return Err("窗口大小格式错误：缺少单位".to_string());
    }

    let num: u64 = num_str
        .parse()
        .map_err(|_| format!("无效的数字格式: {}", num_str))?;

    if num == 0 {
        return Err("窗口大小必须大于0".to_string());
    }

    match unit.as_str() {
        "ms" | "millisecond" | "milliseconds" => Ok(std::time::Duration::from_millis(num)),
        "s" | "sec" | "second" | "seconds" => Ok(std::time::Duration::from_secs(num)),
        "m" | "min" | "minute" | "minutes" => Ok(std::time::Duration::from_secs(num * 60)),
        "h" | "hr" | "hour" | "hours" => Ok(std::time::Duration::from_secs(num * 3600)),
        "d" | "day" | "days" => Ok(std::time::Duration::from_secs(num * 86400)),
        _ => Err(format!(
            "不支持的单位: {}。支持的单位: ms, s, m, h, d",
            unit
        )),
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
    pub on_exceed: Action,
    pub ban: Option<BanConfig>,
}

impl Default for ActionConfig {
    fn default() -> Self {
        Self {
            on_exceed: Action::default(),
            ban: None,
        }
    }
}

impl ActionConfig {
    /// 校验动作配置
    pub fn validate(&self) -> Result<(), String> {
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
    pub scope: BanScope,
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
                storage: "memory".to_string(),
                cache: "memory".to_string(),
                metrics: "prometheus".to_string(),
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
    fn test_invalid_storage() {
        let config = FlowControlConfig {
            version: "1.0".to_string(),
            global: GlobalConfig {
                storage: "invalid".to_string(),
                cache: "memory".to_string(),
                metrics: "prometheus".to_string(),
                trusted_proxies: TrustedProxyConfig::default(),
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
                on_exceed: Action::Reject,
                ban: None,
            },
        };

        let config = FlowControlConfig {
            version: "1.0".to_string(),
            global: GlobalConfig {
                storage: "memory".to_string(),
                cache: "memory".to_string(),
                metrics: "prometheus".to_string(),
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
        assert_eq!(config.global.storage, "memory");
        assert_eq!(config.global.cache, "memory");
        assert_eq!(config.global.metrics, "prometheus");
        assert!(config.rules.is_empty());
    }

    #[test]
    fn test_global_config_default() {
        let global = GlobalConfig::default();
        assert_eq!(global.storage, "memory");
        assert_eq!(global.cache, "memory");
        assert_eq!(global.metrics, "prometheus");
    }

    #[test]
    fn test_global_config_validate_success() {
        let global = GlobalConfig {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
            trusted_proxies: TrustedProxyConfig::default(),
        };
        assert!(global.validate().is_ok());
    }

    #[test]
    fn test_global_config_validate_invalid_storage() {
        let global = GlobalConfig {
            storage: "invalid".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
            trusted_proxies: TrustedProxyConfig::default(),
        };
        let result = global.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("无效的存储类型"));
    }

    #[test]
    fn test_global_config_validate_invalid_cache() {
        let global = GlobalConfig {
            storage: "memory".to_string(),
            cache: "invalid".to_string(),
            metrics: "prometheus".to_string(),
            trusted_proxies: TrustedProxyConfig::default(),
        };
        let result = global.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("无效的缓存类型"));
    }

    #[test]
    fn test_global_config_validate_invalid_metrics() {
        let global = GlobalConfig {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "invalid".to_string(),
            trusted_proxies: TrustedProxyConfig::default(),
        };
        let result = global.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("无效的指标类型"));
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
    fn test_config_builder_new() {
        let builder = ConfigBuilder::new();
        assert_eq!(builder.storage, "memory");
        assert_eq!(builder.cache, "memory");
        assert_eq!(builder.metrics, "prometheus");
        assert!(builder.rules.is_empty());
    }

    #[test]
    fn test_config_builder_default() {
        let builder = ConfigBuilder::default();
        assert_eq!(builder.storage, "memory");
    }

    #[test]
    fn test_config_builder_with_storage() {
        let builder = ConfigBuilder::new().with_storage("postgres");
        assert_eq!(builder.storage, "postgres");
    }

    #[test]
    fn test_config_builder_with_cache() {
        let builder = ConfigBuilder::new().with_cache("redis");
        assert_eq!(builder.cache, "redis");
    }

    #[test]
    fn test_config_builder_with_metrics() {
        let builder = ConfigBuilder::new().with_metrics("none");
        assert_eq!(builder.metrics, "none");
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
        assert_eq!(history.max_records, 50);
        assert!(history.get_records().is_empty());
    }

    #[test]
    fn test_config_history_default() {
        let history = ConfigHistory::default();
        assert_eq!(history.max_records, 100);
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
/// 提供流式API构建FlowControlConfig配置，不依赖confers库。
///
/// # 示例
///
/// ```rust
/// use limiteron::config::ConfigBuilder;
///
/// let config = ConfigBuilder::new()
///     .with_storage("memory")
///     .with_cache("memory")
///     .with_metrics("prometheus")
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
    storage: String,
    cache: String,
    metrics: String,
    /// 可信代理配置
    trusted_proxies: TrustedProxyConfig,
    /// 规则列表
    rules: Vec<RuleBuilder>,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
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
    pub fn with_storage(mut self, storage: impl Into<String>) -> Self {
        self.storage = storage.into();
        self
    }

    /// 设置缓存类型
    pub fn with_cache(mut self, cache: impl Into<String>) -> Self {
        self.cache = cache.into();
        self
    }

    /// 设置可信代理配置
    pub fn with_trusted_proxies(mut self, config: TrustedProxyConfig) -> Self {
        self.trusted_proxies = config;
        self
    }

    /// 设置指标类型
    pub fn with_metrics(mut self, metrics: impl Into<String>) -> Self {
        self.metrics = metrics.into();
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
