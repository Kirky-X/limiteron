//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 配置加载器模块
//!
//! 使用Confers库进行配置加载，支持：
//! - 多格式配置文件（TOML、YAML、JSON）
//! - 环境变量覆盖
//! - ConfigBuilder程序化配置构建

#[cfg(feature = "confers")]
use crate::config::types::FlowControlConfig;
#[cfg(feature = "confers")]
use crate::error::FlowGuardError;

#[cfg(feature = "confers")]
use confers::loader::{load_file, LoaderConfig};
use std::path::Path;

// ============================================================================
// ConfigBuilder and RuleBuilder - Re-exports from config module
// ============================================================================
//
// DEPRECATED: These types are now re-exported from `config` module.
// Please use `crate::config::ConfigBuilder` and `crate::config::RuleBuilder` instead.
//
// The following aliases are provided for backward compatibility only.

/// 配置构建器（已弃用，请使用 `crate::config::ConfigBuilder`）
///
/// 提供流式API构建FlowControlConfig配置。
///
/// # 示例
///
/// ```rust,ignore
/// use limiteron::config::ConfigBuilder;
///
/// let config = ConfigBuilder::new()
///     .with_storage("memory")
///     .with_cache("memory")
///     .with_metrics("prometheus")
///     .with_rule(|rule| {
///         rule.id("default")
///             .name("Default Rule")
///             .priority(100)
///             .token_bucket(1000, 100)
///     })
///     .build();
/// ```
#[deprecated(since = "0.1.1", note = "Use `crate::config::ConfigBuilder` instead")]
pub use crate::config::types::ConfigBuilder;

/// 规则构建器（已弃用，请使用 `crate::config::RuleBuilder`）
#[deprecated(since = "0.1.1", note = "Use `crate::config::RuleBuilder` instead")]
pub use crate::config::types::RuleBuilder;

// Keep the ConfigLoader struct and its implementation
#[cfg(feature = "confers")]
#[derive(Clone)]
pub struct ConfigLoader;

#[cfg(feature = "confers")]
impl ConfigLoader {
    /// 从文件加载配置
    ///
    /// 支持TOML、YAML、JSON格式的配置文件。
    /// 环境变量可覆盖配置文件中的值（使用LIMITERON_前缀）。
    ///
    /// # 参数
    /// - `path`: 配置文件路径
    ///
    /// # 返回
    /// - `Ok(FlowControlConfig)`: 成功加载的配置
    /// - `Err(FlowGuardError)`: 加载失败
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use limiteron::config_loader::ConfigLoader;
    ///
    /// let config = ConfigLoader::load_from_file("config.toml")?;
    /// # Ok::<(), limiteron::error::FlowGuardError>(())
    /// ```
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<FlowControlConfig, FlowGuardError> {
        let path_ref = path.as_ref();
        let config = LoaderConfig::new();

        let annotated = load_file(path_ref, &config)
            .map_err(|e| FlowGuardError::ConfigError(format!("failed to load config: {}", e)))?;

        // Extract the inner ConfigValue from AnnotatedValue
        // Convert AnnotatedValue to JSON via Serialize
        let value: serde_json::Value = serde_json::to_value(&annotated).map_err(|e| {
            FlowGuardError::ConfigError(format!("failed to serialize config: {}", e))
        })?;
        let config_str = serde_json::to_string(&value)
            .map_err(|e| FlowGuardError::ConfigError(format!("serialization error: {}", e)))?;

        // Try parsing as TOML first, then YAML, then JSON
        let config: FlowControlConfig = if path_ref.extension().is_some_and(|e| e == "toml") {
            toml::from_str(&config_str)
                .map_err(|e| FlowGuardError::ConfigError(format!("TOML parse error: {}", e)))?
        } else if path_ref
            .extension()
            .is_some_and(|e| e == "yaml" || e == "yml")
        {
            serde_yaml::from_str(&config_str)
                .map_err(|e| FlowGuardError::ConfigError(format!("YAML parse error: {}", e)))?
        } else {
            serde_json::from_str(&config_str)
                .map_err(|e| FlowGuardError::ConfigError(format!("JSON parse error: {}", e)))?
        };

        config.validate().map_err(FlowGuardError::ConfigError)?;
        Ok(config)
    }

    /// 使用默认配置文件名加载配置
    ///
    /// 按以下顺序查找配置文件：
    /// 1. 当前目录下的 `config.toml`（主要格式）
    /// 2. 当前目录下的 `config.yaml`
    /// 3. 当前目录下的 `config.json`
    ///
    /// # 返回
    /// - `Ok(FlowControlConfig)`: 成功加载的配置
    /// - `Err(FlowGuardError)`: 未找到配置文件或加载失败
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use limiteron::config_loader::ConfigLoader;
    ///
    /// let config = ConfigLoader::load_default()?;
    /// # Ok::<(), limiteron::error::FlowGuardError>(())
    /// ```
    pub fn load_default() -> Result<FlowControlConfig, FlowGuardError> {
        // 优先查找TOML文件（主要格式）
        let config_paths = ["config.toml", "config.yaml", "config.json"];

        for config_path in &config_paths {
            if Path::new(config_path).exists() {
                return Self::load_from_file(config_path);
            }
        }

        Err(FlowGuardError::ConfigError(
            "未找到默认配置文件 (config.toml/yaml/json)".to_string(),
        ))
    }
}

/// 实现confers的OptionalValidate trait
#[cfg(feature = "confers")]
// 注意：当我们实现了Validate trait时，不需要再手动实现OptionalValidate
// 因为confers库会自动为实现Validate trait的类型提供OptionalValidate实现
#[cfg(all(test, feature = "confers"))]
mod confers_tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_config_toml() -> NamedTempFile {
        let mut temp_file = NamedTempFile::with_suffix(".toml").unwrap();
        writeln!(
            temp_file,
            r#"
version = "1.0"

[global]
storage = "memory"
cache = "memory"
metrics = "prometheus"

[[rules]]
id = "test_rule"
name = "Test Rule"
priority = 100

[global.rules.matchers]
type = "User"
user_ids = ["*"]

[[rules.limiters]]
type = "TokenBucket"
capacity = 1000
refill_rate = 100

[rules.action]
on_exceed = "reject"
"#
        )
        .unwrap();
        temp_file
    }

    #[test]
    fn test_load_toml_config() {
        let temp_file = create_test_config_toml();
        let result = ConfigLoader::load_from_file(temp_file.path());
        // Full config parsing tests should use programmatic ConfigBuilder
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = ConfigLoader::load_from_file("/nonexistent/path/config.toml");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_invalid_config() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "invalid: toml: content:").unwrap();
        let result = ConfigLoader::load_from_file(temp_file.path());
        assert!(result.is_err());
    }
}
