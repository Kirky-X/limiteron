// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Admin API configuration

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
}
