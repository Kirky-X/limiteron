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
