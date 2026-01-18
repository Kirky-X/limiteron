//! 配置安全验证模块
//!
//! 提供增强的配置安全验证功能，防止恶意配置注入和配置滥用。

use crate::config::{FlowControlConfig, GlobalConfig, LimiterConfig, Matcher, Rule};

/// 配置安全验证结果
#[derive(Debug, Clone)]
pub struct ConfigSecurityReport {
    pub is_safe: bool,
    pub warnings: Vec<String>,
    pub suggestions: Vec<String>,
}

impl ConfigSecurityReport {
    /// 创建新的安全报告
    pub fn new() -> Self {
        Self {
            is_safe: true,
            warnings: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    /// 添加警告
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
        self.is_safe = false;
    }

    /// 添加建议
    pub fn add_suggestion(&mut self, suggestion: String) {
        self.suggestions.push(suggestion);
    }

    /// 检查是否包含敏感信息
    pub fn contains_sensitive_info(&self) -> bool {
        !self.warnings.is_empty()
    }
}

impl Default for ConfigSecurityReport {
    fn default() -> Self {
        Self::new()
    }
}

/// 配置安全验证器
pub struct ConfigSecurityValidator;

impl ConfigSecurityValidator {
    /// 验证完整配置的安全性
    pub fn validate_config(config: &FlowControlConfig) -> ConfigSecurityReport {
        let mut report = ConfigSecurityReport::new();

        // 验证版本号安全性
        Self::validate_version(&config.version, &mut report);

        // 验证全局配置
        Self::validate_global_config(&config.global, &mut report);

        // 验证规则配置
        for (index, rule) in config.rules.iter().enumerate() {
            Self::validate_rule(rule, index, &mut report);
        }

        report
    }

    /// 验证版本号
    fn validate_version(version: &str, report: &mut ConfigSecurityReport) {
        // 检查版本号是否为空
        if version.is_empty() {
            report.add_warning("版本号为空，可能导致配置管理问题".to_string());
        }

        // 检查版本号长度（防止过长的版本号）
        if version.len() > 50 {
            report.add_warning("版本号过长，可能影响性能".to_string());
        }

        // 检查版本号是否包含特殊字符
        if version.contains('<')
            || version.contains('>')
            || version.contains('|')
            || version.contains('&')
        {
            report.add_warning("版本号包含特殊字符，可能存在注入风险".to_string());
        }
    }

    /// 验证全局配置
    fn validate_global_config(global: &GlobalConfig, report: &mut ConfigSecurityReport) {
        // 验证存储类型
        let valid_storages = ["memory", "redis", "postgresql"];
        if !valid_storages.contains(&global.storage.as_str()) {
            report.add_warning(format!(
                "无效的存储类型: {}，仅支持 {:?}",
                global.storage, valid_storages
            ));
        }

        // 验证缓存类型
        let valid_caches = ["memory", "redis"];
        if !valid_caches.contains(&global.cache.as_str()) {
            report.add_warning(format!(
                "无效的缓存类型: {}，仅支持 {:?}",
                global.cache, valid_caches
            ));
        }

        // 验证指标类型
        let valid_metrics = ["prometheus", "opentelemetry"];
        if !valid_metrics.contains(&global.metrics.as_str()) {
            report.add_warning(format!(
                "无效的指标类型: {}，仅支持 {:?}",
                global.metrics, valid_metrics
            ));
        }
    }

    /// 验证规则配置
    fn validate_rule(rule: &Rule, index: usize, report: &mut ConfigSecurityReport) {
        // 验证规则ID
        if rule.id.is_empty() {
            report.add_warning(format!("规则[{}]的ID为空", index));
        } else if rule.id.len() > 100 {
            report.add_warning(format!("规则[{}]的ID过长，可能影响性能", index));
        }

        // 检查规则ID中的特殊字符
        if rule.id.contains('<')
            || rule.id.contains('>')
            || rule.id.contains('|')
            || rule.id.contains('&')
            || rule.id.contains('\'')
            || rule.id.contains('"')
        {
            report.add_warning(format!(
                "规则[{}]的ID包含特殊字符，可能存在注入风险: {}",
                index, rule.id
            ));
        }

        // 验证规则名称
        if rule.name.is_empty() {
            report.add_warning(format!("规则[{}]的名称为空", index));
        }

        // 验证优先级范围
        if rule.priority > 10000 {
            report.add_warning(format!(
                "规则[{}]的优先级过高({})，可能导致其他规则被忽略",
                index, rule.priority
            ));
        }

        // 验证匹配器
        for (matcher_index, matcher) in rule.matchers.iter().enumerate() {
            Self::validate_matcher(matcher, index, matcher_index, report);
        }

        // 验证限流器
        for (limiter_index, limiter) in rule.limiters.iter().enumerate() {
            Self::validate_limiter(limiter, index, limiter_index, report);
        }
    }

    /// 验证匹配器配置
    fn validate_matcher(
        matcher: &Matcher,
        rule_index: usize,
        matcher_index: usize,
        report: &mut ConfigSecurityReport,
    ) {
        match matcher {
            Matcher::User { user_ids } => {
                for user_id in user_ids {
                    Self::validate_user_id(user_id, rule_index, matcher_index, report);
                }
            }
            Matcher::Ip { ip_ranges } => {
                for ip_range in ip_ranges {
                    Self::validate_ip_range(ip_range, rule_index, matcher_index, report);
                }
            }
            Matcher::Geo { countries } => {
                if countries.is_empty() {
                    report.add_warning(format!(
                        "规则[{}]匹配器[{}]的国家列表为空",
                        rule_index, matcher_index
                    ));
                }
            }
            Matcher::ApiVersion { versions } => {
                for version in versions {
                    Self::validate_api_version(version, rule_index, matcher_index, report);
                }
            }
            Matcher::Device { device_types } => {
                if device_types.is_empty() {
                    report.add_warning(format!(
                        "规则[{}]匹配器[{}]的设备类型列表为空",
                        rule_index, matcher_index
                    ));
                }
            }
            Matcher::Custom { name, .. } => {
                if name.is_empty() {
                    report.add_warning(format!(
                        "规则[{}]匹配器[{}]的自定义匹配器名称为空",
                        rule_index, matcher_index
                    ));
                }
            }
        }
    }

    /// 验证用户ID
    fn validate_user_id(
        user_id: &str,
        rule_index: usize,
        matcher_index: usize,
        report: &mut ConfigSecurityReport,
    ) {
        if user_id.is_empty() {
            report.add_warning(format!(
                "规则[{}]匹配器[{}]的用户ID为空",
                rule_index, matcher_index
            ));
            return;
        }

        if user_id.len() > 256 {
            report.add_warning(format!(
                "规则[{}]匹配器[{}]的用户ID过长: {}",
                rule_index, matcher_index, user_id
            ));
        }

        // 检查用户ID中的特殊字符
        if user_id.contains('<')
            || user_id.contains('>')
            || user_id.contains('|')
            || user_id.contains('&')
        {
            report.add_warning(format!(
                "规则[{}]匹配器[{}]的用户ID包含特殊字符: {}",
                rule_index, matcher_index, user_id
            ));
        }
    }

    /// 验证IP范围
    fn validate_ip_range(
        ip_range: &str,
        rule_index: usize,
        matcher_index: usize,
        report: &mut ConfigSecurityReport,
    ) {
        if ip_range.is_empty() {
            report.add_warning(format!(
                "规则[{}]匹配器[{}]的IP范围为空",
                rule_index, matcher_index
            ));
            return;
        }

        if ip_range.len() > 100 {
            report.add_warning(format!(
                "规则[{}]匹配器[{}]的IP范围过长: {}",
                rule_index, matcher_index, ip_range
            ));
        }

        // 简单的IP范围格式检查
        if ip_range.contains('<')
            || ip_range.contains('>')
            || ip_range.contains('|')
            || ip_range.contains('&')
        {
            report.add_warning(format!(
                "规则[{}]匹配器[{}]的IP范围包含特殊字符: {}",
                rule_index, matcher_index, ip_range
            ));
        }
    }

    /// 验证API版本
    fn validate_api_version(
        version: &str,
        rule_index: usize,
        matcher_index: usize,
        report: &mut ConfigSecurityReport,
    ) {
        if version.is_empty() {
            report.add_warning(format!(
                "规则[{}]匹配器[{}]的API版本为空",
                rule_index, matcher_index
            ));
            return;
        }

        if version.len() > 50 {
            report.add_warning(format!(
                "规则[{}]匹配器[{}]的API版本过长: {}",
                rule_index, matcher_index, version
            ));
        }

        // 检查版本格式
        if version.contains(' ') || version.contains(';') || version.contains('$') {
            report.add_warning(format!(
                "规则[{}]匹配器[{}]的API版本格式可能无效: {}",
                rule_index, matcher_index, version
            ));
        }
    }

    /// 验证限流器配置
    fn validate_limiter(
        limiter: &LimiterConfig,
        rule_index: usize,
        limiter_index: usize,
        report: &mut ConfigSecurityReport,
    ) {
        match limiter {
            LimiterConfig::TokenBucket {
                capacity,
                refill_rate,
            } => {
                if *capacity == 0 {
                    report.add_warning(format!(
                        "规则[{}]限流器[{}]的令牌桶容量为0",
                        rule_index, limiter_index
                    ));
                }
                if *capacity > 1_000_000 {
                    report.add_warning(format!(
                        "规则[{}]限流器[{}]的令牌桶容量过大: {}",
                        rule_index, limiter_index, capacity
                    ));
                }
                if *refill_rate == 0 {
                    report.add_warning(format!(
                        "规则[{}]限流器[{}]的填充速率为0",
                        rule_index, limiter_index
                    ));
                }
                if *refill_rate > 1_000_000 {
                    report.add_warning(format!(
                        "规则[{}]限流器[{}]的填充速率过大: {}",
                        rule_index, limiter_index, refill_rate
                    ));
                }
            }
            LimiterConfig::SlidingWindow {
                window_size,
                max_requests,
            } => {
                Self::validate_window_size(window_size, rule_index, limiter_index, report);
                if *max_requests == 0 {
                    report.add_warning(format!(
                        "规则[{}]限流器[{}]的最大请求数为0",
                        rule_index, limiter_index
                    ));
                }
                if *max_requests > 1_000_000 {
                    report.add_warning(format!(
                        "规则[{}]限流器[{}]的最大请求数过大: {}",
                        rule_index, limiter_index, max_requests
                    ));
                }
            }
            LimiterConfig::FixedWindow {
                window_size,
                max_requests,
            } => {
                Self::validate_window_size(window_size, rule_index, limiter_index, report);
                if *max_requests == 0 {
                    report.add_warning(format!(
                        "规则[{}]限流器[{}]的最大请求数为0",
                        rule_index, limiter_index
                    ));
                }
            }
            LimiterConfig::Quota {
                quota_type,
                limit,
                window,
                overdraft: overdraft_limit,
            } => {
                if quota_type.is_empty() {
                    report.add_warning(format!(
                        "规则[{}]限流器[{}]的配额类型为空",
                        rule_index, limiter_index
                    ));
                }
                if *limit == 0 {
                    report.add_warning(format!(
                        "规则[{}]限流器[{}]的配额限制为0",
                        rule_index, limiter_index
                    ));
                }
                if *limit > 1_000_000_000 {
                    report.add_warning(format!(
                        "规则[{}]限流器[{}]的配额限制过大: {}",
                        rule_index, limiter_index, limit
                    ));
                }
                if let Some(overdraft) = overdraft_limit {
                    if overdraft.max_overdraft > *limit {
                        report.add_warning(format!(
                            "规则[{}]限流器[{}]的透支配额({})超过配额限制({})",
                            rule_index, limiter_index, overdraft.max_overdraft, limit
                        ));
                    }
                }
                Self::validate_window_size(window, rule_index, limiter_index, report);
            }
            LimiterConfig::Concurrency { max_concurrent } => {
                if *max_concurrent == 0 {
                    report.add_warning(format!(
                        "规则[{}]限流器[{}]的最大并发数为0",
                        rule_index, limiter_index
                    ));
                }
                if *max_concurrent > 100000 {
                    report.add_warning(format!(
                        "规则[{}]限流器[{}]的最大并发数过大: {}",
                        rule_index, limiter_index, max_concurrent
                    ));
                }
            }
            LimiterConfig::Custom { name, config: _ } => {
                if name.is_empty() {
                    report.add_warning(format!(
                        "规则[{}]限流器[{}]的自定义限流器名称为空",
                        rule_index, limiter_index
                    ));
                }
            }
        }
    }

    /// 验证窗口大小
    fn validate_window_size(
        window_size: &str,
        rule_index: usize,
        limiter_index: usize,
        report: &mut ConfigSecurityReport,
    ) {
        if window_size.is_empty() {
            report.add_warning(format!(
                "规则[{}]限流器[{}]的窗口大小为空",
                rule_index, limiter_index
            ));
            return;
        }

        if window_size.len() > 50 {
            report.add_warning(format!(
                "规则[{}]限流器[{}]的窗口大小格式过长: {}",
                rule_index, limiter_index, window_size
            ));
        }

        // 检查格式
        if !window_size.ends_with("ms")
            && !window_size.ends_with('s')
            && !window_size.ends_with('m')
            && !window_size.ends_with('h')
        {
            report.add_warning(format!(
                "规则[{}]限流器[{}]的窗口大小格式无效: {}",
                rule_index, limiter_index, window_size
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GlobalConfig, LimiterConfig, Matcher, Rule};

    #[test]
    fn test_valid_config_security() {
        let config = FlowControlConfig {
            version: "1.0.0".to_string(),
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
                    user_ids: vec!["user1".to_string(), "user2".to_string()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 100,
                    refill_rate: 10,
                }],
                action: Default::default(),
            }],
        };

        let report = ConfigSecurityValidator::validate_config(&config);
        assert!(report.is_safe);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn test_invalid_storage_type() {
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
        assert!(!report.is_safe);
        assert!(!report.warnings.is_empty());
    }

    #[test]
    fn test_suspicious_user_id() {
        let config = FlowControlConfig {
            version: "1.0.0".to_string(),
            global: GlobalConfig::default(),
            rules: vec![Rule {
                id: "test_rule".to_string(),
                name: "Test Rule".to_string(),
                priority: 100,
                matchers: vec![Matcher::User {
                    user_ids: vec!["user<script>alert(1)</script>".to_string()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 100,
                    refill_rate: 10,
                }],
                action: Default::default(),
            }],
        };

        let report = ConfigSecurityValidator::validate_config(&config);
        assert!(!report.is_safe);
        assert!(report.warnings.iter().any(|w| w.contains("特殊字符")));
    }

    #[test]
    fn test_empty_version() {
        let config = FlowControlConfig {
            version: "".to_string(),
            global: GlobalConfig::default(),
            rules: vec![],
        };

        let report = ConfigSecurityValidator::validate_config(&config);
        assert!(!report.is_safe);
        assert!(report.warnings.iter().any(|w| w.contains("版本号")));
    }
}
