//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 配置加载器 - 使用 serde 从文件加载配置

use crate::config::FlowControlConfig;
use crate::FlowGuardError;
use std::fs;
use std::path::Path;

/// 配置加载器
pub struct ConfigLoader;

impl ConfigLoader {
    /// 从配置文件加载配置
    ///
    /// 支持 YAML、TOML、JSON 格式,根据文件扩展名自动检测
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<FlowControlConfig, FlowGuardError> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(FlowGuardError::ConfigError(format!(
                "Config file not found: {}",
                path.display()
            )));
        }

        let content = fs::read_to_string(path).map_err(|e| {
            FlowGuardError::ConfigError(format!("Failed to read config file: {}", e))
        })?;

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let config: FlowControlConfig = match ext.as_str() {
            "yaml" | "yml" => serde_yaml::from_str(&content).map_err(|e| {
                FlowGuardError::ConfigError(format!("Failed to parse YAML config: {}", e))
            })?,
            "toml" => toml::from_str(&content).map_err(|e| {
                FlowGuardError::ConfigError(format!("Failed to parse TOML config: {}", e))
            })?,
            "json" => serde_json::from_str(&content).map_err(|e| {
                FlowGuardError::ConfigError(format!("Failed to parse JSON config: {}", e))
            })?,
            _ => {
                return Err(FlowGuardError::ConfigError(format!(
                    "Unsupported config file format: {}. Supported: yaml, yml, toml, json",
                    ext
                )));
            }
        };

        Ok(config)
    }

    /// 从配置文件加载配置并应用环境变量覆盖
    ///
    /// 先调用 `load_from_file` 加载基础配置，然后检查以下环境变量并覆盖对应字段：
    /// - `LIMITERON_GLOBAL_STORAGE`: 覆盖 `global.storage`（memory/postgresql/redis）
    /// - `LIMITERON_GLOBAL_CACHE`: 覆盖 `global.cache`
    /// - `LIMITERON_GLOBAL_METRICS`: 覆盖 `global.metrics`
    ///
    /// 环境变量值为空或未设置时不覆盖。无效值会返回 ConfigError。
    pub fn load_from_file_with_env<P: AsRef<Path>>(
        path: P,
    ) -> Result<FlowControlConfig, FlowGuardError> {
        let mut config = Self::load_from_file(path)?;

        if let Ok(storage_str) = std::env::var("LIMITERON_GLOBAL_STORAGE") {
            if !storage_str.trim().is_empty() {
                let storage_type = crate::config::types::StorageType::parse(&storage_str)
                    .ok_or_else(|| {
                        FlowGuardError::ConfigError(format!(
                            "Invalid LIMITERON_GLOBAL_STORAGE value: {}. Valid: memory, postgresql, redis",
                            storage_str
                        ))
                    })?;
                config.global.storage = storage_type;
            }
        }

        if let Ok(cache_str) = std::env::var("LIMITERON_GLOBAL_CACHE") {
            if !cache_str.trim().is_empty() {
                let cache_backend = crate::config::types::CacheBackend::parse(cache_str.trim())
                    .ok_or_else(|| {
                        FlowGuardError::ConfigError(format!(
                            "Invalid LIMITERON_GLOBAL_CACHE value: {}. Valid: memory, redis, none",
                            cache_str
                        ))
                    })?;
                config.global.cache = cache_backend;
            }
        }

        if let Ok(metrics_str) = std::env::var("LIMITERON_GLOBAL_METRICS") {
            if !metrics_str.trim().is_empty() {
                let metrics_backend =
                    crate::config::types::MetricsBackend::parse(metrics_str.trim()).ok_or_else(|| {
                        FlowGuardError::ConfigError(format!(
                            "Invalid LIMITERON_GLOBAL_METRICS value: {}. Valid: prometheus, statsd, none",
                            metrics_str
                        ))
                    })?;
                config.global.metrics = metrics_backend;
            }
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn valid_yaml() -> String {
        r#"
version: "0.1.0"
global:
  storage: memory
  cache: memory
  metrics: none
rules:
  - id: "rule1"
    name: "Test"
    priority: 100
    matchers:
      - type: User
        user_ids: ["u1"]
    limiters:
      - type: TokenBucket
        capacity: 100
        refill_rate: 10
    action:
      on_exceed: degrade
"#
        .into()
    }

    #[test]
    fn test_load_yaml() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(valid_yaml().as_bytes()).unwrap();
        let path = file.path().with_extension("yaml");
        std::fs::copy(file.path(), &path).unwrap();
        let result = ConfigLoader::load_from_file(&path);
        std::fs::remove_file(&path).ok();
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_toml() {
        let toml = r#"
version = "0.1.0"
[global]
storage = "memory"
cache = "memory"
metrics = "none"

[[rules]]
id = "rule1"
name = "Test"
priority = 100

[[rules.matchers]]
type = "User"
user_ids = ["u1"]

[[rules.limiters]]
type = "TokenBucket"
capacity = 100
refill_rate = 10

[rules.action]
on_exceed = "degrade"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml.as_bytes()).unwrap();
        let path = file.path().with_extension("toml");
        std::fs::copy(file.path(), &path).unwrap();
        let result = ConfigLoader::load_from_file(&path);
        std::fs::remove_file(&path).ok();
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_json() {
        let json = r#"{
  "version": "0.1.0",
  "global": {
    "storage": "memory",
    "cache": "memory",
    "metrics": "none"
  },
  "rules": [
    {
      "id": "rule1",
      "name": "Test",
      "priority": 100,
      "matchers": [{"type": "User", "user_ids": ["u1"]}],
      "limiters": [{"type": "TokenBucket", "capacity": 100, "refill_rate": 10}],
      "action": {"on_exceed": "degrade"}
    }
  ]
}"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(json.as_bytes()).unwrap();
        let path = file.path().with_extension("json");
        std::fs::copy(file.path(), &path).unwrap();
        let result = ConfigLoader::load_from_file(&path);
        std::fs::remove_file(&path).ok();
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_file_not_found() {
        let result = ConfigLoader::load_from_file("/tmp/limiteron_nonexistent_config.yaml");
        assert!(result.is_err());
        match result {
            Err(FlowGuardError::ConfigError(msg)) => assert!(msg.contains("not found")),
            _ => panic!("expected ConfigError"),
        }
    }

    #[test]
    fn test_load_invalid_yaml() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"invalid: yaml: content: :").unwrap();
        let path = file.path().with_extension("yaml");
        std::fs::copy(file.path(), &path).unwrap();
        let result = ConfigLoader::load_from_file(&path);
        std::fs::remove_file(&path).ok();
        assert!(result.is_err());
    }

    #[test]
    fn test_load_unsupported_format() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"content").unwrap();
        let path = file.path().with_extension("txt");
        std::fs::copy(file.path(), &path).unwrap();
        let result = ConfigLoader::load_from_file(&path);
        std::fs::remove_file(&path).ok();
        assert!(result.is_err());
        match result {
            Err(FlowGuardError::ConfigError(msg)) => assert!(msg.contains("Unsupported")),
            _ => panic!("expected ConfigError"),
        }
    }

    #[test]
    fn test_load_invalid_toml() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"this is = = invalid toml [[[").unwrap();
        let path = file.path().with_extension("toml");
        std::fs::copy(file.path(), &path).unwrap();
        let result = ConfigLoader::load_from_file(&path);
        std::fs::remove_file(&path).ok();
        assert!(result.is_err());
        match result {
            Err(FlowGuardError::ConfigError(msg)) => assert!(msg.contains("TOML")),
            _ => panic!("expected ConfigError for invalid TOML"),
        }
    }

    #[test]
    fn test_load_invalid_json() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"{ this is not valid json }").unwrap();
        let path = file.path().with_extension("json");
        std::fs::copy(file.path(), &path).unwrap();
        let result = ConfigLoader::load_from_file(&path);
        std::fs::remove_file(&path).ok();
        assert!(result.is_err());
        match result {
            Err(FlowGuardError::ConfigError(msg)) => assert!(msg.contains("JSON")),
            _ => panic!("expected ConfigError for invalid JSON"),
        }
    }

    #[test]
    fn test_load_no_extension() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"content").unwrap();
        let path = file.path().with_extension("");
        std::fs::copy(file.path(), &path).unwrap();
        let result = ConfigLoader::load_from_file(&path);
        std::fs::remove_file(&path).ok();
        assert!(result.is_err());
        match result {
            Err(FlowGuardError::ConfigError(msg)) => assert!(msg.contains("Unsupported")),
            _ => panic!("expected ConfigError for no extension"),
        }
    }

    #[test]
    fn test_load_yaml_with_yml_extension() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(valid_yaml().as_bytes()).unwrap();
        let path = file.path().with_extension("yml");
        std::fs::copy(file.path(), &path).unwrap();
        let result = ConfigLoader::load_from_file(&path);
        std::fs::remove_file(&path).ok();
        assert!(result.is_ok());
    }

    // 覆盖 line 30：path 存在但 read_to_string 失败（如目录）
    #[test]
    fn test_load_directory_as_file() {
        let dir = tempfile::tempdir().unwrap();
        let result = ConfigLoader::load_from_file(dir.path());
        assert!(result.is_err());
        match result {
            Err(FlowGuardError::ConfigError(msg)) => {
                assert!(msg.contains("Failed to read"));
            }
            _ => panic!("expected ConfigError for directory read failure"),
        }
    }

    // ===== load_from_file_with_env 测试 =====
    // 注意：环境变量是进程级全局状态，并行测试会互相污染。
    // 使用 mutex 序列化所有 env 相关测试，确保它们不会同时运行。

    use std::sync::{Mutex, OnceLock};

    fn env_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn write_temp_config(content: &str, ext: &str) -> std::path::PathBuf {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        let path = file.path().with_extension(ext);
        std::fs::copy(file.path(), &path).unwrap();
        path
    }

    #[test]
    fn test_load_from_file_with_env_no_override() {
        let _guard = env_test_lock().lock().unwrap();
        let path = write_temp_config(&valid_yaml(), "yaml");
        // 确保环境变量未设置
        std::env::remove_var("LIMITERON_GLOBAL_STORAGE");
        std::env::remove_var("LIMITERON_GLOBAL_CACHE");
        std::env::remove_var("LIMITERON_GLOBAL_METRICS");

        let result = ConfigLoader::load_from_file_with_env(&path);
        std::fs::remove_file(&path).ok();
        assert!(result.is_ok());
        let config = result.unwrap();
        // 默认值应保持不变（valid_yaml 中 storage=memory）
        assert_eq!(
            config.global.storage,
            crate::config::types::StorageType::Memory
        );
    }

    #[test]
    fn test_load_from_file_with_env_storage_override() {
        let _guard = env_test_lock().lock().unwrap();
        let path = write_temp_config(&valid_yaml(), "yaml");
        std::env::set_var("LIMITERON_GLOBAL_STORAGE", "redis");

        let result = ConfigLoader::load_from_file_with_env(&path);
        std::env::remove_var("LIMITERON_GLOBAL_STORAGE");
        std::fs::remove_file(&path).ok();

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(
            config.global.storage,
            crate::config::types::StorageType::Redis
        );
    }

    #[test]
    fn test_load_from_file_with_env_invalid_storage() {
        let _guard = env_test_lock().lock().unwrap();
        let path = write_temp_config(&valid_yaml(), "yaml");
        std::env::set_var("LIMITERON_GLOBAL_STORAGE", "invalid_storage");

        let result = ConfigLoader::load_from_file_with_env(&path);
        std::env::remove_var("LIMITERON_GLOBAL_STORAGE");
        std::fs::remove_file(&path).ok();

        assert!(result.is_err());
        match result {
            Err(FlowGuardError::ConfigError(msg)) => {
                assert!(msg.contains("Invalid LIMITERON_GLOBAL_STORAGE"));
            }
            _ => panic!("expected ConfigError for invalid storage env"),
        }
    }

    #[test]
    fn test_load_from_file_with_env_empty_value_no_override() {
        let _guard = env_test_lock().lock().unwrap();
        let path = write_temp_config(&valid_yaml(), "yaml");
        std::env::set_var("LIMITERON_GLOBAL_STORAGE", "");

        let result = ConfigLoader::load_from_file_with_env(&path);
        std::env::remove_var("LIMITERON_GLOBAL_STORAGE");
        std::fs::remove_file(&path).ok();

        assert!(result.is_ok());
        let config = result.unwrap();
        // 空值不应覆盖，保持原值 memory
        assert_eq!(
            config.global.storage,
            crate::config::types::StorageType::Memory
        );
    }

    #[test]
    fn test_load_from_file_with_env_cache_override() {
        let _guard = env_test_lock().lock().unwrap();
        let path = write_temp_config(&valid_yaml(), "yaml");
        std::env::set_var("LIMITERON_GLOBAL_CACHE", "redis");

        let result = ConfigLoader::load_from_file_with_env(&path);
        std::env::remove_var("LIMITERON_GLOBAL_CACHE");
        std::fs::remove_file(&path).ok();

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(
            config.global.cache,
            crate::config::types::CacheBackend::Redis
        );
    }

    #[test]
    fn test_load_from_file_with_env_metrics_override() {
        let _guard = env_test_lock().lock().unwrap();
        let path = write_temp_config(&valid_yaml(), "yaml");
        std::env::set_var("LIMITERON_GLOBAL_METRICS", "statsd");

        let result = ConfigLoader::load_from_file_with_env(&path);
        std::env::remove_var("LIMITERON_GLOBAL_METRICS");
        std::fs::remove_file(&path).ok();

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(
            config.global.metrics,
            crate::config::types::MetricsBackend::Statsd
        );
    }

    #[test]
    fn test_load_from_file_with_env_invalid_cache() {
        let _guard = env_test_lock().lock().unwrap();
        let path = write_temp_config(&valid_yaml(), "yaml");
        std::env::set_var("LIMITERON_GLOBAL_CACHE", "invalid_cache");

        let result = ConfigLoader::load_from_file_with_env(&path);
        std::env::remove_var("LIMITERON_GLOBAL_CACHE");
        std::fs::remove_file(&path).ok();

        assert!(result.is_err());
        match result {
            Err(FlowGuardError::ConfigError(msg)) => {
                assert!(msg.contains("Invalid LIMITERON_GLOBAL_CACHE"));
            }
            _ => panic!("expected ConfigError for invalid cache env"),
        }
    }

    #[test]
    fn test_load_from_file_with_env_invalid_metrics() {
        let _guard = env_test_lock().lock().unwrap();
        let path = write_temp_config(&valid_yaml(), "yaml");
        std::env::set_var("LIMITERON_GLOBAL_METRICS", "invalid_metrics");

        let result = ConfigLoader::load_from_file_with_env(&path);
        std::env::remove_var("LIMITERON_GLOBAL_METRICS");
        std::fs::remove_file(&path).ok();

        assert!(result.is_err());
        match result {
            Err(FlowGuardError::ConfigError(msg)) => {
                assert!(msg.contains("Invalid LIMITERON_GLOBAL_METRICS"));
            }
            _ => panic!("expected ConfigError for invalid metrics env"),
        }
    }
}
