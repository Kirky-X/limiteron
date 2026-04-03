// Copyright (c) 2026, Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! 配置相关类型

use super::actions::{CacheBackend, MetricsBackend};
use ipnet::IpNet;
use serde::{Deserialize, Serialize};

/// 全局配置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlobalConfig {
    pub storage: StorageType,
    pub cache: CacheBackend,
    pub metrics: MetricsBackend,
    /// 可信代理配置（用于安全提取客户端 IP）
    #[serde(default)]
    pub trusted_proxies: TrustedProxyConfig,
}

/// 存储类型枚举
/// 兼容字符串和枚举类型的配置方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageType {
    /// 内存存储
    #[default]
    Memory,
    /// PostgreSQL 存储
    PostgreSQL,
    /// Redis 存储
    Redis,
}

impl StorageType {
    /// 从字符串解析
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "memory" => Some(Self::Memory),
            "postgresql" | "postgres" => Some(Self::PostgreSQL),
            "redis" => Some(Self::Redis),
            _ => None,
        }
    }
}

impl From<&str> for StorageType {
    fn from(s: &str) -> Self {
        Self::parse(s).unwrap_or_default()
    }
}

impl std::fmt::Display for StorageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Memory => write!(f, "memory"),
            Self::PostgreSQL => write!(f, "postgresql"),
            Self::Redis => write!(f, "redis"),
        }
    }
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
            storage: StorageType::Memory,
            cache: CacheBackend::default(),
            metrics: MetricsBackend::default(),
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
        // 枚举类型在编译时就保证类型安全，这里只校验可信代理配置
        self.trusted_proxies.validate()?;
        Ok(())
    }
}
