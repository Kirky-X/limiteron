//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 配置加载器 - 使用 confers 从文件加载配置

use crate::config::FlowControlConfig;
use crate::FlowGuardError;
use confers::ConfigBuilder;
use std::path::Path;

/// 配置加载器
pub struct ConfigLoader;

impl ConfigLoader {
    /// 从配置文件加载配置
    ///
    /// 支持 YAML、TOML、JSON 格式,根据文件扩展名自动检测
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<FlowControlConfig, FlowGuardError> {
        let path = path.as_ref();

        let config: FlowControlConfig = ConfigBuilder::new()
            .file(path)
            .build()
            .map_err(|e| FlowGuardError::ConfigError(format!("Failed to load config: {}", e)))?;

        Ok(config)
    }
}
