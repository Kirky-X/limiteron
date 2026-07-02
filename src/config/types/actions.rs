// Copyright (c) 2026, Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! 动作相关类型

use serde::{Deserialize, Serialize};

/// 超限动作类型
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// 拒绝请求
    #[default]
    Reject,
    /// 允许请求通过（仅记录）
    Allow,
    /// 降级处理
    Degrade,
}

impl Action {
    /// 从字符串解析
    pub fn parse(s: &str) -> Option<Self> {
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

impl From<&str> for Action {
    fn from(s: &str) -> Self {
        Self::parse(s).unwrap_or_default()
    }
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 封禁范围
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BanScope {
    /// 按 IP 封禁
    #[default]
    Ip,
    /// 按用户封禁
    User,
    /// 按 MAC 地址封禁
    Mac,
}

impl BanScope {
    /// 从字符串解析
    pub fn parse(s: &str) -> Option<Self> {
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

impl From<&str> for BanScope {
    fn from(s: &str) -> Self {
        Self::parse(s).unwrap_or_default()
    }
}

impl std::fmt::Display for BanScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 缓存后端类型
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheBackend {
    /// 内存缓存
    #[default]
    Memory,
    /// Redis 缓存
    Redis,
    /// 无缓存
    None,
}

impl CacheBackend {
    /// 从字符串解析
    pub fn parse(s: &str) -> Option<Self> {
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

impl From<&str> for CacheBackend {
    fn from(s: &str) -> Self {
        Self::parse(s).unwrap_or_default()
    }
}

impl std::fmt::Display for CacheBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 指标后端类型
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricsBackend {
    /// Prometheus
    #[default]
    Prometheus,
    /// StatsD
    Statsd,
    /// 无指标
    None,
}

impl MetricsBackend {
    /// 从字符串解析
    pub fn parse(s: &str) -> Option<Self> {
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

impl From<&str> for MetricsBackend {
    fn from(s: &str) -> Self {
        Self::parse(s).unwrap_or_default()
    }
}

impl std::fmt::Display for MetricsBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 动作配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionConfig {
    pub on_exceed: Action,
    pub ban: Option<BanConfig>,
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
    fn test_action_parse() {
        assert_eq!(Action::parse("reject"), Some(Action::Reject));
        assert_eq!(Action::parse("REJECT"), Some(Action::Reject));
        assert_eq!(Action::parse("allow"), Some(Action::Allow));
        assert_eq!(Action::parse("degrade"), Some(Action::Degrade));
        assert_eq!(Action::parse("invalid"), None);
        assert_eq!(Action::parse(""), None);
    }

    #[test]
    fn test_action_as_str() {
        assert_eq!(Action::Reject.as_str(), "reject");
        assert_eq!(Action::Allow.as_str(), "allow");
        assert_eq!(Action::Degrade.as_str(), "degrade");
    }

    #[test]
    fn test_action_display() {
        assert_eq!(format!("{}", Action::Reject), "reject");
        assert_eq!(format!("{}", Action::Allow), "allow");
        assert_eq!(format!("{}", Action::Degrade), "degrade");
    }

    #[test]
    fn test_action_from_str() {
        assert_eq!(Action::from("reject"), Action::Reject);
        assert_eq!(Action::from("invalid"), Action::Reject);
    }

    #[test]
    fn test_action_default() {
        assert_eq!(Action::default(), Action::Reject);
    }

    #[test]
    fn test_ban_scope_parse() {
        assert_eq!(BanScope::parse("ip"), Some(BanScope::Ip));
        assert_eq!(BanScope::parse("IP"), Some(BanScope::Ip));
        assert_eq!(BanScope::parse("user"), Some(BanScope::User));
        assert_eq!(BanScope::parse("mac"), Some(BanScope::Mac));
        assert_eq!(BanScope::parse("invalid"), None);
    }

    #[test]
    fn test_ban_scope_as_str() {
        assert_eq!(BanScope::Ip.as_str(), "ip");
        assert_eq!(BanScope::User.as_str(), "user");
        assert_eq!(BanScope::Mac.as_str(), "mac");
    }

    #[test]
    fn test_ban_scope_default() {
        assert_eq!(BanScope::default(), BanScope::Ip);
    }

    #[test]
    fn test_cache_backend_parse() {
        assert_eq!(CacheBackend::parse("memory"), Some(CacheBackend::Memory));
        assert_eq!(CacheBackend::parse("redis"), Some(CacheBackend::Redis));
        assert_eq!(CacheBackend::parse("none"), Some(CacheBackend::None));
        assert_eq!(CacheBackend::parse("invalid"), None);
    }

    #[test]
    fn test_cache_backend_as_str() {
        assert_eq!(CacheBackend::Memory.as_str(), "memory");
        assert_eq!(CacheBackend::Redis.as_str(), "redis");
        assert_eq!(CacheBackend::None.as_str(), "none");
    }

    #[test]
    fn test_metrics_backend_parse() {
        assert_eq!(
            MetricsBackend::parse("prometheus"),
            Some(MetricsBackend::Prometheus)
        );
        assert_eq!(
            MetricsBackend::parse("statsd"),
            Some(MetricsBackend::Statsd)
        );
        assert_eq!(MetricsBackend::parse("none"), Some(MetricsBackend::None));
        assert_eq!(MetricsBackend::parse("invalid"), None);
    }

    #[test]
    fn test_metrics_backend_as_str() {
        assert_eq!(MetricsBackend::Prometheus.as_str(), "prometheus");
        assert_eq!(MetricsBackend::Statsd.as_str(), "statsd");
        assert_eq!(MetricsBackend::None.as_str(), "none");
    }

    #[test]
    fn test_action_config_default() {
        let config = ActionConfig::default();
        assert_eq!(config.on_exceed, Action::Reject);
        assert!(config.ban.is_none());
    }

    #[test]
    fn test_action_config_validate_no_ban() {
        let config = ActionConfig {
            on_exceed: Action::Reject,
            ban: None,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_action_config_validate_with_valid_ban() {
        let config = ActionConfig {
            on_exceed: Action::Reject,
            ban: Some(BanConfig {
                threshold: 5,
                initial_duration: "1h".to_string(),
                backoff_multiplier: 2.0,
                max_duration: "24h".to_string(),
                scope: BanScope::Ip,
            }),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_action_config_validate_with_invalid_ban() {
        let config = ActionConfig {
            on_exceed: Action::Reject,
            ban: Some(BanConfig {
                threshold: 0,
                initial_duration: "1h".to_string(),
                backoff_multiplier: 2.0,
                max_duration: "24h".to_string(),
                scope: BanScope::Ip,
            }),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_ban_config_validate() {
        let valid = BanConfig {
            threshold: 5,
            initial_duration: "1h".to_string(),
            backoff_multiplier: 2.0,
            max_duration: "24h".to_string(),
            scope: BanScope::Ip,
        };
        assert!(valid.validate().is_ok());

        let invalid_threshold = BanConfig {
            threshold: 0,
            initial_duration: "1h".to_string(),
            backoff_multiplier: 2.0,
            max_duration: "24h".to_string(),
            scope: BanScope::Ip,
        };
        assert!(invalid_threshold.validate().is_err());

        let invalid_backoff = BanConfig {
            threshold: 5,
            initial_duration: "1h".to_string(),
            backoff_multiplier: 0.0,
            max_duration: "24h".to_string(),
            scope: BanScope::Ip,
        };
        assert!(invalid_backoff.validate().is_err());
    }

    #[test]
    fn test_ban_scope_from_str_and_display() {
        assert_eq!(BanScope::from("ip"), BanScope::Ip);
        assert_eq!(BanScope::from("user"), BanScope::User);
        assert_eq!(BanScope::from("mac"), BanScope::Mac);
        assert_eq!(BanScope::from("invalid"), BanScope::Ip);
        assert_eq!(format!("{}", BanScope::Ip), "ip");
        assert_eq!(format!("{}", BanScope::User), "user");
        assert_eq!(format!("{}", BanScope::Mac), "mac");
    }

    #[test]
    fn test_cache_backend_from_str_display_default() {
        assert_eq!(CacheBackend::from("memory"), CacheBackend::Memory);
        assert_eq!(CacheBackend::from("redis"), CacheBackend::Redis);
        assert_eq!(CacheBackend::from("none"), CacheBackend::None);
        assert_eq!(CacheBackend::from("invalid"), CacheBackend::Memory);
        assert_eq!(format!("{}", CacheBackend::Memory), "memory");
        assert_eq!(format!("{}", CacheBackend::Redis), "redis");
        assert_eq!(format!("{}", CacheBackend::None), "none");
        assert_eq!(CacheBackend::default(), CacheBackend::Memory);
    }

    #[test]
    fn test_metrics_backend_from_str_display_default() {
        assert_eq!(
            MetricsBackend::from("prometheus"),
            MetricsBackend::Prometheus
        );
        assert_eq!(MetricsBackend::from("statsd"), MetricsBackend::Statsd);
        assert_eq!(MetricsBackend::from("none"), MetricsBackend::None);
        assert_eq!(MetricsBackend::from("invalid"), MetricsBackend::Prometheus);
        assert_eq!(format!("{}", MetricsBackend::Prometheus), "prometheus");
        assert_eq!(format!("{}", MetricsBackend::Statsd), "statsd");
        assert_eq!(format!("{}", MetricsBackend::None), "none");
        assert_eq!(MetricsBackend::default(), MetricsBackend::Prometheus);
    }

    #[test]
    fn test_ban_scope_equality() {
        assert_eq!(BanScope::Ip, BanScope::Ip);
        assert_ne!(BanScope::Ip, BanScope::User);
        assert_ne!(BanScope::User, BanScope::Mac);
    }

    #[test]
    fn test_action_config_validate_with_invalid_backoff() {
        let config = ActionConfig {
            on_exceed: Action::Reject,
            ban: Some(BanConfig {
                threshold: 5,
                initial_duration: "1h".to_string(),
                backoff_multiplier: -1.0,
                max_duration: "24h".to_string(),
                scope: BanScope::Ip,
            }),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_action_equality() {
        assert_eq!(Action::Reject, Action::Reject);
        assert_ne!(Action::Reject, Action::Allow);
        assert_ne!(Action::Allow, Action::Degrade);
    }

    #[test]
    fn test_action_serde_roundtrip() {
        let json = serde_json::to_string(&Action::Allow).unwrap();
        assert_eq!(json, "\"allow\"");
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Action::Allow);
    }

    #[test]
    fn test_ban_scope_serde_roundtrip() {
        let json = serde_json::to_string(&BanScope::Mac).unwrap();
        assert_eq!(json, "\"mac\"");
        let back: BanScope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, BanScope::Mac);
    }
}
