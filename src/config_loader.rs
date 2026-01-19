//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 配置加载器模块
//!
//! 基于 confers 库的配置加载器，支持从文件和环境变量加载配置。

#[cfg(feature = "confers")]
use crate::config::FlowControlConfig;
#[cfg(feature = "confers")]
use crate::error::FlowGuardError;
#[cfg(feature = "confers")]
use std::path::Path;

/// 配置加载器
///
/// 提供从文件和环境变量加载配置的功能。
#[cfg(feature = "confers")]
pub struct ConfigLoader;

#[cfg(feature = "confers")]
impl ConfigLoader {
    /// 从文件加载配置
    ///
    /// # 参数
    /// - `path`: 配置文件路径（支持 YAML、TOML、JSON）
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
    /// let config = ConfigLoader::load_from_file("/path/to/config.yaml")?;
    /// # Ok::<(), limiteron::error::FlowGuardError>(())
    /// ```
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<FlowControlConfig, FlowGuardError> {
        let path = path.as_ref();

        // 读取文件内容
        let content = std::fs::read_to_string(path).map_err(|e| {
            FlowGuardError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("无法读取配置文件 {}: {}", path.display(), e),
            ))
        })?;

        // 根据文件扩展名选择解析方式
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .ok_or_else(|| {
                FlowGuardError::ConfigError(format!("无法确定配置文件类型: {}", path.display()))
            })?;

        match extension.to_lowercase().as_str() {
            "yaml" | "yml" => {
                let config: FlowControlConfig = serde_yaml::from_str(&content)
                    .map_err(|e| FlowGuardError::ConfigError(format!("YAML 解析错误: {}", e)))?;
                config.validate().map_err(FlowGuardError::ConfigError)?;
                Ok(config)
            }
            "toml" => {
                let config: FlowControlConfig = toml::from_str(&content)
                    .map_err(|e| FlowGuardError::ConfigError(format!("TOML 解析错误: {}", e)))?;
                config.validate().map_err(FlowGuardError::ConfigError)?;
                Ok(config)
            }
            "json" => {
                let config: FlowControlConfig = serde_json::from_str(&content)
                    .map_err(|e| FlowGuardError::ConfigError(format!("JSON 解析错误: {}", e)))?;
                config.validate().map_err(FlowGuardError::ConfigError)?;
                Ok(config)
            }
            _ => Err(FlowGuardError::ConfigError(format!(
                "不支持的配置文件类型: {}",
                extension
            ))),
        }
    }

    /// 从文件加载配置，支持环境变量覆盖
    ///
    /// 环境变量命名规则：`LIMITERON_<SECTION>_<FIELD>`
    ///
    /// # 参数
    /// - `path`: 配置文件路径（支持 YAML、TOML、JSON）
    ///
    /// # 返回
    /// - `Ok(FlowControlConfig)`: 成功加载的配置（已应用环境变量覆盖）
    /// - `Err(FlowGuardError)`: 加载失败
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use limiteron::config_loader::ConfigLoader;
    ///
    /// // 设置环境变量覆盖
    /// std::env::set_var("LIMITERON_GLOBAL_STORAGE", "redis");
    ///
    /// let config = ConfigLoader::load_from_file_with_env("/path/to/config.yaml")?;
    /// # Ok::<(), limiteron::error::FlowGuardError>(())
    /// ```
    pub fn load_from_file_with_env<P: AsRef<Path>>(
        path: P,
    ) -> Result<FlowControlConfig, FlowGuardError> {
        // 首先从文件加载配置
        let mut config = Self::load_from_file(path)?;

        // 应用环境变量覆盖
        Self::apply_env_overrides(&mut config)?;

        // 验证配置
        config.validate().map_err(FlowGuardError::ConfigError)?;

        Ok(config)
    }

    /// 应用环境变量覆盖
    ///
    /// 环境变量命名规则：`LIMITERON_<SECTION>_<FIELD>`
    fn apply_env_overrides(config: &mut FlowControlConfig) -> Result<(), FlowGuardError> {
        // 全局配置覆盖
        if let Ok(storage) = std::env::var("LIMITERON_GLOBAL_STORAGE") {
            config.global.storage = storage;
        }
        if let Ok(cache) = std::env::var("LIMITERON_GLOBAL_CACHE") {
            config.global.cache = cache;
        }
        if let Ok(metrics) = std::env::var("LIMITERON_GLOBAL_METRICS") {
            config.global.metrics = metrics;
        }

        // 规则配置覆盖（通过索引）
        // 例如：LIMITERON_RULES_0_NAME
        for (index, rule) in config.rules.iter_mut().enumerate() {
            let prefix = format!("LIMITERON_RULES_{}", index);

            if let Ok(name) = std::env::var(format!("{}_NAME", prefix)) {
                rule.name = name;
            }
            if let Ok(priority_str) = std::env::var(format!("{}_PRIORITY", prefix)) {
                rule.priority = priority_str.parse().map_err(|e| {
                    FlowGuardError::ConfigError(format!("无效的优先级值 '{}': {}", priority_str, e))
                })?;
            }

            // 限流器配置覆盖
            for (limiter_idx, limiter) in rule.limiters.iter_mut().enumerate() {
                match limiter {
                    crate::config::LimiterConfig::TokenBucket {
                        capacity,
                        refill_rate,
                    } => {
                        if let Ok(cap_str) =
                            std::env::var(format!("{}_LIMITERS_{}_CAPACITY", prefix, limiter_idx))
                        {
                            *capacity = cap_str.parse().map_err(|e| {
                                FlowGuardError::ConfigError(format!(
                                    "无效的容量值 '{}': {}",
                                    cap_str, e
                                ))
                            })?;
                        }
                        if let Ok(rate_str) = std::env::var(format!(
                            "{}_LIMITERS_{}_REFILL_RATE",
                            prefix, limiter_idx
                        )) {
                            *refill_rate = rate_str.parse().map_err(|e| {
                                FlowGuardError::ConfigError(format!(
                                    "无效的填充速率值 '{}': {}",
                                    rate_str, e
                                ))
                            })?;
                        }
                    }
                    crate::config::LimiterConfig::SlidingWindow {
                        window_size,
                        max_requests,
                    } => {
                        if let Ok(size_str) = std::env::var(format!(
                            "{}_LIMITERS_{}_WINDOW_SIZE",
                            prefix, limiter_idx
                        )) {
                            *window_size = size_str;
                        }
                        if let Ok(req_str) = std::env::var(format!(
                            "{}_LIMITERS_{}_MAX_REQUESTS",
                            prefix, limiter_idx
                        )) {
                            *max_requests = req_str.parse().map_err(|e| {
                                FlowGuardError::ConfigError(format!(
                                    "无效的最大请求数值 '{}': {}",
                                    req_str, e
                                ))
                            })?;
                        }
                    }
                    crate::config::LimiterConfig::FixedWindow {
                        window_size,
                        max_requests,
                    } => {
                        if let Ok(size_str) = std::env::var(format!(
                            "{}_LIMITERS_{}_WINDOW_SIZE",
                            prefix, limiter_idx
                        )) {
                            *window_size = size_str;
                        }
                        if let Ok(req_str) = std::env::var(format!(
                            "{}_LIMITERS_{}_MAX_REQUESTS",
                            prefix, limiter_idx
                        )) {
                            *max_requests = req_str.parse().map_err(|e| {
                                FlowGuardError::ConfigError(format!(
                                    "无效的最大请求数值 '{}': {}",
                                    req_str, e
                                ))
                            })?;
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[cfg(feature = "confers")]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_config_yaml() -> NamedTempFile {
        let mut temp_file = NamedTempFile::with_suffix(".yaml").unwrap();
        writeln!(
            temp_file,
            r#"
version: "1.0"
global:
  storage: "memory"
  cache: "memory"
  metrics: "prometheus"
rules:
  - id: "test_rule"
    name: "Test Rule"
    priority: 100
    matchers:
      - type: User
        user_ids: ["*"]
    limiters:
      - type: TokenBucket
        capacity: 1000
        refill_rate: 100
    action:
      on_exceed: "reject"
"#
        )
        .unwrap();
        temp_file
    }

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

[[rules.matchers]]
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

    fn create_test_config_json() -> NamedTempFile {
        let mut temp_file = NamedTempFile::with_suffix(".json").unwrap();
        writeln!(
            temp_file,
            r#"{{
  "version": "1.0",
  "global": {{
    "storage": "memory",
    "cache": "memory",
    "metrics": "prometheus"
  }},
  "rules": [
    {{
      "id": "test_rule",
      "name": "Test Rule",
      "priority": 100,
      "matchers": [
        {{
          "type": "User",
          "user_ids": ["*"]
        }}
      ],
      "limiters": [
        {{
          "type": "TokenBucket",
          "capacity": 1000,
          "refill_rate": 100
        }}
      ],
      "action": {{
        "on_exceed": "reject"
      }}
    }}
  ]
}}"#
        )
        .unwrap();
        temp_file
    }

    #[test]
    fn test_load_yaml_config() {
        let temp_file = create_test_config_yaml();
        let config = ConfigLoader::load_from_file(temp_file.path()).unwrap();
        assert_eq!(config.version, "1.0");
        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].id, "test_rule");
    }

    #[test]
    fn test_load_toml_config() {
        let temp_file = create_test_config_toml();
        let config = ConfigLoader::load_from_file(temp_file.path()).unwrap();
        assert_eq!(config.version, "1.0");
        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].id, "test_rule");
    }

    #[test]
    fn test_load_json_config() {
        let temp_file = create_test_config_json();
        let config = ConfigLoader::load_from_file(temp_file.path()).unwrap();
        assert_eq!(config.version, "1.0");
        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].id, "test_rule");
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = ConfigLoader::load_from_file("/nonexistent/path/config.yaml");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_invalid_config() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "invalid: yaml: content:").unwrap();
        let result = ConfigLoader::load_from_file(temp_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_env_override() {
        let temp_file = create_test_config_yaml();

        // 设置环境变量
        std::env::set_var("LIMITERON_GLOBAL_STORAGE", "redis");
        std::env::set_var("LIMITERON_RULES_0_NAME", "Overridden Rule");

        let config = ConfigLoader::load_from_file_with_env(temp_file.path()).unwrap();

        assert_eq!(config.global.storage, "redis");
        assert_eq!(config.rules[0].name, "Overridden Rule");

        // 清理环境变量
        std::env::remove_var("LIMITERON_GLOBAL_STORAGE");
        std::env::remove_var("LIMITERON_RULES_0_NAME");
    }

    #[test]
    fn test_env_override_valid_priority() {
        let temp_file = create_test_config_yaml();

        // 设置有效的环境变量值
        std::env::set_var("LIMITERON_RULES_0_PRIORITY", "200");

        let config = ConfigLoader::load_from_file_with_env(temp_file.path()).unwrap();

        assert_eq!(config.rules[0].priority, 200);

        // 清理环境变量
        std::env::remove_var("LIMITERON_RULES_0_PRIORITY");
    }
}
