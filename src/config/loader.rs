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
}
