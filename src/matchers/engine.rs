//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 规则匹配引擎
//!
//! 包含规则匹配相关的类型：MatchCondition, IpRange, LogicalOperator,
//! CompositeCondition, Rule, RuleMatcher, ConditionEvaluator

use super::traits::RequestContext;
use crate::config::ConfigMatcher;
use crate::error::FlowGuardError;
use parking_lot::RwLock;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// 匹配条件
// ============================================================================

/// 匹配条件
///
/// 定义单个匹配条件。
#[derive(Clone)]
pub enum MatchCondition {
    /// 用户ID匹配
    User(Vec<String>),
    /// IP范围匹配
    Ip(Vec<IpRange>),
    /// 地理位置匹配
    Geo(Vec<String>),
    /// API版本匹配
    ApiVersion(Vec<String>),
    /// 设备类型匹配
    Device(Vec<String>),
    /// 自定义匹配
    Custom(Arc<dyn Fn(&RequestContext) -> bool + Send + Sync>),
}

impl std::fmt::Debug for MatchCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatchCondition::User(ids) => f.debug_tuple("User").field(ids).finish(),
            MatchCondition::Ip(ranges) => f.debug_tuple("Ip").field(&ranges.len()).finish(),
            MatchCondition::Geo(countries) => f.debug_tuple("Geo").field(countries).finish(),
            MatchCondition::ApiVersion(versions) => {
                f.debug_tuple("ApiVersion").field(versions).finish()
            }
            MatchCondition::Device(device_types) => {
                f.debug_tuple("Device").field(device_types).finish()
            }
            MatchCondition::Custom(_) => f.debug_tuple("Custom").field(&"<closure>").finish(),
        }
    }
}

// ============================================================================
// IP范围
// ============================================================================

/// IP范围
#[derive(Debug, Clone)]
pub enum IpRange {
    /// 单个IP
    Single(IpAddr),
    /// IPv4 CIDR
    Ipv4Cidr { addr: Ipv4Addr, prefix: u8 },
    /// IPv6 CIDR
    Ipv6Cidr { addr: Ipv6Addr, prefix: u8 },
    /// IPv4范围
    Ipv4Range { start: Ipv4Addr, end: Ipv4Addr },
}

impl IpRange {
    /// 检查IP是否在范围内
    pub fn contains(&self, ip: &IpAddr) -> bool {
        match self {
            IpRange::Single(addr) => addr == ip,
            IpRange::Ipv4Cidr { addr, prefix } => {
                if let IpAddr::V4(ipv4) = ip {
                    self.ipv4_in_cidr(ipv4, addr, *prefix)
                } else {
                    false
                }
            }
            IpRange::Ipv6Cidr { addr, prefix } => {
                if let IpAddr::V6(ipv6) = ip {
                    self.ipv6_in_cidr(ipv6, addr, *prefix)
                } else {
                    false
                }
            }
            IpRange::Ipv4Range { start, end } => {
                if let IpAddr::V4(ipv4) = ip {
                    ipv4 >= start && ipv4 <= end
                } else {
                    false
                }
            }
        }
    }

    /// 检查IPv4是否在CIDR范围内
    fn ipv4_in_cidr(&self, ip: &Ipv4Addr, network: &Ipv4Addr, prefix: u8) -> bool {
        let ip_u32 = u32::from(*ip);
        let network_u32 = u32::from(*network);
        let mask = if prefix == 0 {
            0
        } else {
            0xFFFFFFFF << (32 - prefix)
        };

        (ip_u32 & mask) == (network_u32 & mask)
    }

    /// 检查IPv6是否在CIDR范围内
    fn ipv6_in_cidr(&self, ip: &Ipv6Addr, network: &Ipv6Addr, prefix: u8) -> bool {
        let ip_segments = ip.segments();
        let network_segments = network.segments();

        let full_segments = (prefix / 16) as usize;
        let remaining_bits = prefix % 16;

        // 检查完整的段
        for i in 0..full_segments {
            if ip_segments[i] != network_segments[i] {
                return false;
            }
        }

        // 检查剩余的位
        if remaining_bits > 0 && full_segments < 8 {
            let mask = 0xFFFFu16 << (16 - remaining_bits);
            if (ip_segments[full_segments] & mask) != (network_segments[full_segments] & mask) {
                return false;
            }
        }

        true
    }
}

impl FromStr for IpRange {
    type Err = FlowGuardError;

    /// 从字符串解析IP范围
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains('/') {
            // CIDR格式
            let parts: Vec<&str> = s.split('/').collect();
            if parts.len() != 2 {
                return Err(FlowGuardError::ConfigError(format!(
                    "无效的CIDR格式: {}",
                    s
                )));
            }

            let addr: IpAddr = parts[0]
                .parse()
                .map_err(|_| FlowGuardError::ConfigError(format!("无效的IP地址: {}", parts[0])))?;
            let prefix: u8 = parts[1]
                .parse()
                .map_err(|_| FlowGuardError::ConfigError(format!("无效的前缀: {}", parts[1])))?;

            match addr {
                IpAddr::V4(ipv4) => {
                    if prefix > 32 {
                        return Err(FlowGuardError::ConfigError(format!(
                            "IPv4前缀不能超过32: {}",
                            prefix
                        )));
                    }
                    Ok(IpRange::Ipv4Cidr { addr: ipv4, prefix })
                }
                IpAddr::V6(ipv6) => {
                    if prefix > 128 {
                        return Err(FlowGuardError::ConfigError(format!(
                            "IPv6前缀不能超过128: {}",
                            prefix
                        )));
                    }
                    Ok(IpRange::Ipv6Cidr { addr: ipv6, prefix })
                }
            }
        } else if s.contains('-') {
            // 范围格式
            let parts: Vec<&str> = s.split('-').collect();
            if parts.len() != 2 {
                return Err(FlowGuardError::ConfigError(format!(
                    "无效的IP范围格式: {}",
                    s
                )));
            }

            let start: Ipv4Addr = parts[0]
                .parse()
                .map_err(|_| FlowGuardError::ConfigError(format!("无效的起始IP: {}", parts[0])))?;
            let end: Ipv4Addr = parts[1]
                .parse()
                .map_err(|_| FlowGuardError::ConfigError(format!("无效的结束IP: {}", parts[1])))?;

            if start > end {
                return Err(FlowGuardError::ConfigError(format!(
                    "起始IP不能大于结束IP: {} - {}",
                    parts[0], parts[1]
                )));
            }

            Ok(IpRange::Ipv4Range { start, end })
        } else {
            // 单个IP
            let addr: IpAddr = s
                .parse()
                .map_err(|_| FlowGuardError::ConfigError(format!("无效的IP地址: {}", s)))?;
            Ok(IpRange::Single(addr))
        }
    }
}

// ============================================================================
// 逻辑操作符
// ============================================================================

/// 逻辑操作符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOperator {
    /// 逻辑与
    And,
    /// 逻辑或
    Or,
    /// 逻辑非
    Not,
}

// ============================================================================
// 复合条件
// ============================================================================

/// 复合条件
///
/// 支持AND/OR/NOT逻辑操作。
pub struct CompositeCondition {
    /// 子条件列表
    pub conditions: Vec<Box<dyn ConditionEvaluator>>,
    /// 逻辑操作符
    pub operator: LogicalOperator,
}

impl std::fmt::Debug for CompositeCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeCondition")
            .field("conditions", &self.conditions.len())
            .field("operator", &self.operator)
            .finish()
    }
}

impl Clone for CompositeCondition {
    fn clone(&self) -> Self {
        // 由于 ConditionEvaluator 不能 Clone，我们创建一个新的 CompositeCondition
        // 实际使用时，用户需要重新构建条件
        Self {
            conditions: self
                .conditions
                .iter()
                .map(|_| {
                    // 由于无法克隆 trait 对象，这里返回一个占位符
                    // 实际使用中，需要重新构建条件
                    Box::new(MatchCondition::User(vec![])) as Box<dyn ConditionEvaluator>
                })
                .collect(),
            operator: self.operator,
        }
    }
}

// ============================================================================
// 条件评估器
// ============================================================================

/// 条件评估器 trait
///
/// 所有条件都需要实现此trait。
pub trait ConditionEvaluator: Send + Sync {
    /// 评估条件
    fn evaluate(&self, context: &RequestContext) -> bool;

    /// 获取条件描述
    fn description(&self) -> String;
}

impl ConditionEvaluator for MatchCondition {
    fn evaluate(&self, context: &RequestContext) -> bool {
        match self {
            MatchCondition::User(user_ids) => {
                if let Some(user_id) = context.get_header("X-User-Id") {
                    user_ids.contains(&user_id.to_string()) || user_ids.contains(&"*".to_string())
                } else {
                    user_ids.contains(&"*".to_string())
                }
            }
            MatchCondition::Ip(ip_ranges) => {
                if let Some(client_ip) = &context.client_ip {
                    if let Ok(ip) = client_ip.parse::<IpAddr>() {
                        return ip_ranges.iter().any(|range| range.contains(&ip));
                    }
                }
                false
            }
            MatchCondition::Geo(countries) => {
                if let Some(country) = context.get_header("X-Country") {
                    countries.contains(&country.to_string()) || countries.contains(&"*".to_string())
                } else {
                    countries.contains(&"*".to_string())
                }
            }
            MatchCondition::ApiVersion(versions) => {
                if let Some(version) = context.get_header("X-API-Version") {
                    versions.contains(&version.to_string()) || versions.contains(&"*".to_string())
                } else {
                    versions.contains(&"*".to_string())
                }
            }
            MatchCondition::Device(device_types) => {
                if let Some(device_type) = context.get_header("X-Device-Type") {
                    device_types.contains(&device_type.to_string())
                        || device_types.contains(&"*".to_string())
                } else {
                    device_types.contains(&"*".to_string())
                }
            }
            MatchCondition::Custom(eval_fn) => eval_fn(context),
        }
    }

    fn description(&self) -> String {
        match self {
            MatchCondition::User(ids) => format!("User in {:?}", ids),
            MatchCondition::Ip(ranges) => format!("IP in {} ranges", ranges.len()),
            MatchCondition::Geo(countries) => format!("Country in {:?}", countries),
            MatchCondition::ApiVersion(versions) => format!("API version in {:?}", versions),
            MatchCondition::Device(device_types) => format!("Device type in {:?}", device_types),
            MatchCondition::Custom(_) => "Custom condition".to_string(),
        }
    }
}

impl ConditionEvaluator for CompositeCondition {
    fn evaluate(&self, context: &RequestContext) -> bool {
        match self.operator {
            LogicalOperator::And => self.conditions.iter().all(|c| c.evaluate(context)),
            LogicalOperator::Or => self.conditions.iter().any(|c| c.evaluate(context)),
            LogicalOperator::Not => {
                // NOT操作符只应该有一个子条件
                self.conditions
                    .first()
                    .is_some_and(|c| !c.evaluate(context))
            }
        }
    }

    fn description(&self) -> String {
        let op_str = match self.operator {
            LogicalOperator::And => "AND",
            LogicalOperator::Or => "OR",
            LogicalOperator::Not => "NOT",
        };
        format!("{} ({})", op_str, self.conditions.len())
    }
}

// ============================================================================
// 规则
// ============================================================================

/// 规则
pub struct Rule {
    /// 规则ID
    pub id: String,
    /// 规则名称
    pub name: String,
    /// 优先级（数值越大优先级越高）
    pub priority: u16,
    /// 匹配条件
    pub condition: Box<dyn ConditionEvaluator>,
    /// 是否启用
    pub enabled: bool,
}

impl std::fmt::Debug for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rule")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("priority", &self.priority)
            .field("enabled", &self.enabled)
            .field("condition", &"<condition>")
            .finish()
    }
}

impl Clone for Rule {
    fn clone(&self) -> Self {
        // 由于 ConditionEvaluator 不能 Clone，我们创建一个新的 Rule
        // 实际使用时，用户需要重新构建规则
        Self {
            id: self.id.clone(),
            name: self.name.clone(),
            priority: self.priority,
            condition: Box::new(MatchCondition::User(vec![])) as Box<dyn ConditionEvaluator>,
            enabled: self.enabled,
        }
    }
}

// ============================================================================
// 匹配器统计
// ============================================================================

/// 匹配器统计信息
#[derive(Debug, Clone, Default)]
pub struct MatcherStats {
    /// 总匹配次数
    pub total_matches: u64,
    /// 总不匹配次数
    pub total_mismatches: u64,
    /// 最后匹配时间
    pub last_match_time: Option<Instant>,
    /// 平均匹配时间（纳秒）
    pub avg_match_time_ns: u64,
}

// ============================================================================
// 规则匹配器
// ============================================================================

/// 规则匹配器
///
/// 高性能规则匹配引擎，支持优先级排序和复合条件。
pub struct RuleMatcher {
    /// 规则列表（按优先级排序）
    rules: Vec<Rule>,
    /// 匹配统计
    stats: RwLock<MatcherStats>,
}

/// RuleMatcher 构建器
///
/// 用于链式配置 RuleMatcher 实例。
#[derive(Debug, Default)]
pub struct RuleMatcherBuilder {
    rules: Vec<Rule>,
}

impl RuleMatcherBuilder {
    /// 创建新的 RuleMatcherBuilder
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// 添加规则
    pub fn add_rule(mut self, rule: Rule) -> Self {
        self.rules.push(rule);
        self
    }

    /// 批量添加规则
    pub fn add_rules(mut self, rules: Vec<Rule>) -> Self {
        self.rules.extend(rules);
        self
    }

    /// 构建 RuleMatcher 实例
    pub fn build(self) -> RuleMatcher {
        RuleMatcher::with_dependencies(self.rules)
    }
}

impl RuleMatcher {
    /// 创建新的规则匹配器
    ///
    /// # 参数
    /// - `rules`: 规则列表
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::{RuleMatcher, Rule, MatchCondition};
    ///
    /// let matcher = RuleMatcher::new(vec![
    ///     Rule {
    ///         id: "rule1".to_string(),
    ///         name: "Test Rule".to_string(),
    ///         priority: 100,
    ///         condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
    ///         enabled: true,
    ///     },
    /// ]);
    /// ```
    pub fn new(rules: Vec<Rule>) -> Self {
        let mut matcher = Self {
            rules: Vec::new(),
            stats: RwLock::new(MatcherStats::default()),
        };

        for rule in rules {
            matcher.add_rule(rule);
        }

        matcher
    }

    /// 使用依赖注入创建 RuleMatcher（用于应用容器集成）
    pub fn with_dependencies(rules: Vec<Rule>) -> Self {
        let mut matcher = Self {
            rules: Vec::new(),
            stats: RwLock::new(MatcherStats::default()),
        };

        for rule in rules {
            matcher.add_rule(rule);
        }

        matcher
    }

    /// 添加规则
    ///
    /// # 参数
    /// - `rule`: 规则
    pub fn add_rule(&mut self, rule: Rule) {
        // 按优先级排序（降序）
        let pos = self
            .rules
            .binary_search_by(|r| r.priority.cmp(&rule.priority).reverse())
            .unwrap_or_else(|pos| pos);

        self.rules.insert(pos, rule);
    }

    /// 移除规则
    ///
    /// # 参数
    /// - `rule_id`: 规则ID
    pub fn remove_rule(&mut self, rule_id: &str) -> Option<Rule> {
        if let Some(pos) = self.rules.iter().position(|r| r.id == rule_id) {
            Some(self.rules.remove(pos))
        } else {
            None
        }
    }

    /// 检查请求是否匹配任何规则
    ///
    /// # 参数
    /// - `context`: 请求上下文
    ///
    /// # 返回
    /// - `Some(rule)`: 匹配的规则
    /// - `None`: 没有匹配的规则
    ///
    /// # 性能
    /// - P99延迟 < 200μs
    /// - 支持至少100条规则
    pub fn matches(&self, context: &RequestContext) -> Option<&Rule> {
        let start = Instant::now();

        // 按优先级顺序检查规则
        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }

            if rule.condition.evaluate(context) {
                // 更新统计信息
                let elapsed = start.elapsed().as_nanos() as u64;
                {
                    let mut stats = self.stats.write();
                    stats.total_matches += 1;
                    stats.last_match_time = Some(Instant::now());

                    // 更新平均匹配时间（使用指数移动平均）
                    if stats.total_matches == 1 {
                        stats.avg_match_time_ns = elapsed;
                    } else {
                        stats.avg_match_time_ns = (stats.avg_match_time_ns * 9 + elapsed) / 10;
                    }
                }

                return Some(rule);
            }
        }

        {
            let mut stats = self.stats.write();
            stats.total_mismatches += 1;
        }
        None
    }

    /// 获取所有匹配的规则
    ///
    /// # 参数
    /// - `context`: 请求上下文
    ///
    /// # 返回
    /// - 匹配的规则列表（按优先级排序）
    pub fn match_all(&self, context: &RequestContext) -> Vec<&Rule> {
        self.rules
            .iter()
            .filter(|rule| rule.enabled && rule.condition.evaluate(context))
            .collect()
    }

    /// 获取统计信息
    pub fn stats(&self) -> MatcherStats {
        self.stats.read().clone()
    }

    /// 重置统计信息
    pub fn reset_stats(&self) {
        let mut stats = self.stats.write();
        *stats = MatcherStats::default();
    }

    /// 获取规则数量
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 从配置创建规则匹配器
    ///
    /// # 参数
    /// - `config_matchers`: 配置中的匹配器列表
    pub fn from_config(config_matchers: &[ConfigMatcher]) -> Result<Self, FlowGuardError> {
        let mut rules = Vec::new();

        for (index, matcher) in config_matchers.iter().enumerate() {
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
                    // 自定义匹配器需要在运行时通过CustomMatcherRegistry处理
                    // 这里返回一个占位符，实际匹配逻辑由CustomMatcherRegistry处理
                    let name = name.clone();
                    Box::new(MatchCondition::Custom(Arc::new(move |_context| {
                        // 自定义匹配器的实际匹配逻辑在CustomMatcherRegistry中实现
                        // 这里只是占位符，返回false表示不匹配
                        log::warn!("自定义匹配器 '{}' 需要通过CustomMatcherRegistry处理", name);
                        false
                    })))
                }
            };

            rules.push(Rule {
                id: format!("rule_{}", index),
                name: format!("Rule {}", index),
                priority: 100,
                condition,
                enabled: true,
            });
        }

        Ok(Self::new(rules))
    }
}
