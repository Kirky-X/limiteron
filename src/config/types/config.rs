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
    /// X-Forwarded-For 中允许的最大 IP 跳数（防止 IP 列表过长攻击）
    /// 默认值: 10
    #[serde(default = "default_max_hops")]
    pub max_hops: usize,
}

/// 默认的 max_hops 值
fn default_max_hops() -> usize {
    10
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
            max_hops: default_max_hops(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_type_parse() {
        assert_eq!(StorageType::parse("memory"), Some(StorageType::Memory));
        assert_eq!(
            StorageType::parse("postgresql"),
            Some(StorageType::PostgreSQL)
        );
        assert_eq!(
            StorageType::parse("postgres"),
            Some(StorageType::PostgreSQL)
        );
        assert_eq!(StorageType::parse("redis"), Some(StorageType::Redis));
        assert_eq!(StorageType::parse("invalid"), None);
    }

    #[test]
    fn test_storage_type_from_str() {
        assert_eq!(StorageType::from("memory"), StorageType::Memory);
        assert_eq!(StorageType::from("invalid"), StorageType::Memory);
    }

    #[test]
    fn test_storage_type_display() {
        assert_eq!(format!("{}", StorageType::Memory), "memory");
        assert_eq!(format!("{}", StorageType::PostgreSQL), "postgresql");
        assert_eq!(format!("{}", StorageType::Redis), "redis");
    }

    #[test]
    fn test_storage_type_default() {
        assert_eq!(StorageType::default(), StorageType::Memory);
    }

    #[test]
    fn test_trusted_proxy_config_default() {
        let config = TrustedProxyConfig::default();
        assert!(!config.enabled);
        assert!(config.proxies.is_empty());
        assert_eq!(config.max_hops, 10);
    }

    #[test]
    fn test_trusted_proxy_config_validate_disabled() {
        let config = TrustedProxyConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_trusted_proxy_config_validate_valid() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["192.168.1.1".to_string(), "10.0.0.0/8".to_string()],
            max_hops: 10,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_trusted_proxy_config_validate_invalid() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["not-an-ip".to_string()],
            max_hops: 10,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_trusted_proxy_is_trusted() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["192.168.1.1".to_string(), "10.0.0.0/8".to_string()],
            max_hops: 10,
        };
        assert!(config.is_trusted("192.168.1.1"));
        assert!(config.is_trusted("10.0.0.1"));
        assert!(!config.is_trusted("1.2.3.4"));
        assert!(!config.is_trusted("not-an-ip"));
    }

    #[test]
    fn test_trusted_proxy_is_trusted_disabled() {
        let config = TrustedProxyConfig::default();
        assert!(!config.is_trusted("192.168.1.1"));
    }

    #[test]
    fn test_global_config_default() {
        let config = GlobalConfig::default();
        assert_eq!(config.storage, StorageType::Memory);
        assert_eq!(config.cache, CacheBackend::default());
        assert_eq!(config.metrics, MetricsBackend::default());
        assert!(!config.trusted_proxies.enabled);
    }

    #[test]
    fn test_global_config_validate() {
        let config = GlobalConfig::default();
        assert!(config.validate().is_ok());
    }
}
