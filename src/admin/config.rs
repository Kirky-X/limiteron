// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Admin API configuration

use ahash::AHashMap as HashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Admin API configuration validation error
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("API key is required when admin API is enabled")]
    ApiKeyRequired,
    #[error("API key must be at least 16 characters, got {0}")]
    ApiKeyTooShort(usize),
}

/// Admin API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminApiConfig {
    /// Listening host
    #[serde(default = "default_host")]
    pub host: String,
    /// Listening port
    #[serde(default = "default_port")]
    pub port: u16,
    /// API Key for authentication (required when enabled)
    pub api_key: String,
    /// Whether to enable the admin API
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// API key → operator identity mapping (vuln-0001 修复)
    ///
    /// 当管理员通过不同 API key 鉴权时，绑定到对应的 operator 身份，
    /// 防止客户端在 JSON body 中伪造 `operator` 字段。mapping 为空时
    /// 回退到默认 `"admin-api"` 并记录 warn 日志（向后兼容）。
    #[serde(default)]
    pub api_key_operators: HashMap<String, String>,
    /// 按路径分组的速率限制配置 (vuln-0002 修复)
    ///
    /// key = 分组名（"ban" / "quota" / "default"），value = (max_requests, window_secs)。
    /// middleware 在鉴权之前检查速率限制，超限返回 429 + Retry-After。
    /// 默认：ban=(100,60), quota=(50,60), default=(200,60)。
    #[serde(default = "default_rate_limits")]
    pub rate_limits: HashMap<String, (u64, u64)>,
}

/// 默认速率限制配置：ban=100/min, quota=50/min, default=200/min
fn default_rate_limits() -> HashMap<String, (u64, u64)> {
    let mut m = HashMap::new();
    m.insert("ban".to_string(), (100, 60));
    m.insert("quota".to_string(), (50, 60));
    m.insert("default".to_string(), (200, 60));
    m
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    9090
}

fn default_enabled() -> bool {
    false
}

impl Default for AdminApiConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            api_key: String::new(),
            enabled: default_enabled(),
            api_key_operators: HashMap::new(),
            rate_limits: default_rate_limits(),
        }
    }
}

impl AdminApiConfig {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            api_key: api_key.into(),
            enabled: true,
            api_key_operators: HashMap::new(),
            rate_limits: default_rate_limits(),
        }
    }

    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = api_key.into();
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 添加 API key → operator 映射（vuln-0001 修复）
    pub fn with_api_key_operator(
        mut self,
        api_key: impl Into<String>,
        operator: impl Into<String>,
    ) -> Self {
        self.api_key_operators
            .insert(api_key.into(), operator.into());
        self
    }

    /// 批量设置 API key → operator 映射
    pub fn with_api_key_operators(mut self, mapping: HashMap<String, String>) -> Self {
        self.api_key_operators = mapping;
        self
    }

    /// 查找 API key 对应的 operator 身份
    ///
    /// 返回 `Some(operator)` 表示配置中存在显式映射；
    /// 返回 `None` 表示未配置映射，调用方应回退到默认 `"admin-api"` 并记录 warn。
    pub fn operator_for_api_key(&self, api_key: &str) -> Option<&str> {
        self.api_key_operators.get(api_key).map(|s| s.as_str())
    }

    /// 设置指定分组的速率限制 (vuln-0002 修复)
    ///
    /// # 参数
    /// - `group`: 分组名（"ban" / "quota" / "default"）
    /// - `max`: 窗口内最大请求数
    /// - `window_secs`: 窗口时长（秒）
    pub fn with_rate_limit(mut self, group: &str, max: u64, window_secs: u64) -> Self {
        self.rate_limits
            .insert(group.to_string(), (max, window_secs));
        self
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.enabled && self.api_key.is_empty() {
            return Err(ConfigError::ApiKeyRequired);
        }
        if !self.api_key.is_empty() && self.api_key.len() < 16 {
            return Err(ConfigError::ApiKeyTooShort(self.api_key.len()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = AdminApiConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 9090);
        assert!(config.api_key.is_empty());
        assert!(!config.enabled);
    }

    #[test]
    fn test_new_sets_api_key_and_enabled() {
        let config = AdminApiConfig::new("my-secure-api-key-32chars");
        assert_eq!(config.api_key, "my-secure-api-key-32chars");
        assert!(config.enabled);
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 9090);
    }

    #[test]
    fn test_builder_methods() {
        let config = AdminApiConfig::new("initial-api-key-32chars!!")
            .with_host("0.0.0.0")
            .with_port(8080)
            .with_api_key("replacement-api-key-32ch")
            .with_enabled(false);
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert_eq!(config.api_key, "replacement-api-key-32ch");
        assert!(!config.enabled);
    }

    #[test]
    fn test_address_format() {
        let config = AdminApiConfig::new("my-secure-api-key-32chars")
            .with_host("192.168.1.1")
            .with_port(3000);
        assert_eq!(config.address(), "192.168.1.1:3000");
    }

    #[test]
    fn test_validate_enabled_empty_key_returns_api_key_required() {
        let config = AdminApiConfig {
            api_key: String::new(),
            enabled: true,
            ..Default::default()
        };
        match config.validate() {
            Err(ConfigError::ApiKeyRequired) => {}
            other => panic!("expected ApiKeyRequired, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_short_key_returns_too_short_with_length() {
        let config = AdminApiConfig {
            api_key: "short".to_string(),
            enabled: true,
            ..Default::default()
        };
        match config.validate() {
            Err(ConfigError::ApiKeyTooShort(5)) => {}
            other => panic!("expected ApiKeyTooShort(5), got {:?}", other),
        }
    }

    #[test]
    fn test_validate_disabled_empty_key_ok() {
        let config = AdminApiConfig {
            api_key: String::new(),
            enabled: false,
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_valid_config_ok() {
        let config = AdminApiConfig::new("this-is-a-valid-api-key-32ch");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_error_display_messages() {
        assert_eq!(
            ConfigError::ApiKeyRequired.to_string(),
            "API key is required when admin API is enabled"
        );
        assert_eq!(
            ConfigError::ApiKeyTooShort(10).to_string(),
            "API key must be at least 16 characters, got 10"
        );
    }

    // ========================================================================
    // vuln-0001 修复测试：api_key_operators 映射
    // ========================================================================

    #[test]
    fn test_default_config_has_empty_api_key_operators() {
        let config = AdminApiConfig::default();
        assert!(config.api_key_operators.is_empty());
    }

    #[test]
    fn test_with_api_key_operator_adds_mapping() {
        let config = AdminApiConfig::new("my-secure-api-key-32chars")
            .with_api_key_operator("my-secure-api-key-32chars", "admin-alice");
        assert_eq!(
            config.operator_for_api_key("my-secure-api-key-32chars"),
            Some("admin-alice")
        );
    }

    #[test]
    fn test_with_api_key_operators_replaces_mapping() {
        let mut mapping = HashMap::new();
        mapping.insert("key-a-16chars-long".to_string(), "admin-a".to_string());
        mapping.insert("key-b-16chars-long".to_string(), "admin-b".to_string());
        let config = AdminApiConfig::new("primary-key-16chars!!!").with_api_key_operators(mapping);
        assert_eq!(
            config.operator_for_api_key("key-a-16chars-long"),
            Some("admin-a")
        );
        assert_eq!(
            config.operator_for_api_key("key-b-16chars-long"),
            Some("admin-b")
        );
        // primary key 未在 mapping 中 → None（回退到默认 "admin-api"）
        assert_eq!(config.operator_for_api_key("primary-key-16chars!!!"), None);
    }

    #[test]
    fn test_operator_for_api_key_returns_none_when_not_configured() {
        let config = AdminApiConfig::new("my-secure-api-key-32chars");
        // 未配置 mapping → 任何 key 都返回 None
        assert_eq!(
            config.operator_for_api_key("my-secure-api-key-32chars"),
            None
        );
    }

    #[test]
    fn test_operator_for_api_key_returns_none_for_unknown_key() {
        let config = AdminApiConfig::new("my-secure-api-key-32chars")
            .with_api_key_operator("my-secure-api-key-32chars", "admin-alice");
        // 未在 mapping 中的 key → None
        assert_eq!(config.operator_for_api_key("unknown-key"), None);
    }
}
