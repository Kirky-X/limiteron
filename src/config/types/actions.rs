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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
