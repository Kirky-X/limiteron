//! 管理API配置

use serde::{Deserialize, Serialize};

/// 管理API配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminApiConfig {
    /// 监听地址
    #[serde(default = "default_host")]
    pub host: String,
    /// 监听端口
    #[serde(default = "default_port")]
    pub port: u16,
    /// API Key认证(可选)
    pub api_key: Option<String>,
    /// 是否启用
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
    true
}

impl Default for AdminApiConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            api_key: None,
            enabled: default_enabled(),
        }
    }
}

impl AdminApiConfig {
    /// 创建新配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置监听地址
    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// 设置监听端口
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// 设置API Key
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// 设置是否启用
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 获取完整地址
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
