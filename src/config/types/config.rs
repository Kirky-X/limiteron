// Copyright (c) 2026, Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! 配置相关类型

use crate::constants::{VALID_CACHE_TYPES, VALID_METRICS_TYPES, VALID_STORAGE_TYPES};
use ipnet::IpNet;
use serde::{Deserialize, Serialize};

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
