// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 配置加载器 - 使用 serde 从文件加载配置

use crate::LimiteronError;
use crate::config::FlowControlConfig;
use confers::loader::{LoaderConfig, load_file};
use std::path::Path;

/// 配置加载器
pub struct ConfigLoader;

impl ConfigLoader {
    /// 从配置文件加载配置
    ///
    /// 支持 YAML、TOML、JSON 格式，根据文件扩展名自动检测。解析委托
    /// confers `loader::load_file`：路径穿越防护、大小上限、TOCTOU
    /// 安全打开与特殊文件拒绝随 confers 免费获得（design D3）。
    /// 扩展名白名单严格限于 yaml|yml|toml|json——不用 confers 内容
    /// 嗅探兜底，防止 .txt 等任意内容被误判为合法配置。
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<FlowControlConfig, LimiteronError> {
        let path = path.as_ref();

        // ① 严格扩展名白名单预检：维持 "Unsupported config file format"
        // 公开契约（不经 confers 内容嗅探，见 design D3 Alternatives）
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "yaml" | "yml" | "toml" | "json" => {}
            _ => {
                return Err(LimiteronError::ConfigError(format!(
                    "Unsupported config file format: {}. Supported: yaml, yml, toml, json",
                    ext
                )));
            }
        }

        // ② confers 加载：路径校验（穿越防护）/大小上限/TOCTOU 安全
        // 打开/特殊文件拒绝 + 三元格式解析。放开绝对路径与既有公开
        // 契约一致（测试与调用方均传绝对路径）；穿越组件、URL 编码
        // 穿越等 confers 防护全部保留。
        let annotated =
            load_file(path, &LoaderConfig::default().allow_absolute()).map_err(map_config_error)?;

        // ③ AnnotatedValue → JSON → FlowControlConfig typed 反序列化
        serde_json::from_value::<FlowControlConfig>(annotated.to_json())
            .map_err(|e| map_parse_error(&ext, &e))
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
    ) -> Result<FlowControlConfig, LimiteronError> {
        let mut config = Self::load_from_file(path)?;

        if let Ok(storage_str) = std::env::var("LIMITERON_GLOBAL_STORAGE")
            && !storage_str.trim().is_empty()
        {
            // 与 cache/metrics 保持一致：先 trim 再解析（`" redis "` 不应报错）
            let storage_type = crate::config::StorageType::parse(storage_str.trim())
                    .ok_or_else(|| {
                        LimiteronError::ConfigError(format!(
                            "Invalid LIMITERON_GLOBAL_STORAGE value: {}. Valid: memory, postgresql, redis",
                            storage_str
                        ))
                    })?;
            config.global.storage = storage_type;
        }

        if let Ok(cache_str) = std::env::var("LIMITERON_GLOBAL_CACHE")
            && !cache_str.trim().is_empty()
        {
            let cache_backend =
                crate::config::CacheBackend::parse(cache_str.trim()).ok_or_else(|| {
                    LimiteronError::ConfigError(format!(
                        "Invalid LIMITERON_GLOBAL_CACHE value: {}. Valid: memory, redis, none",
                        cache_str
                    ))
                })?;
            config.global.cache = cache_backend;
        }

        if let Ok(metrics_str) = std::env::var("LIMITERON_GLOBAL_METRICS")
            && !metrics_str.trim().is_empty()
        {
            let metrics_backend =
                    crate::config::MetricsBackend::parse(metrics_str.trim()).ok_or_else(|| {
                        LimiteronError::ConfigError(format!(
                            "Invalid LIMITERON_GLOBAL_METRICS value: {}. Valid: prometheus, statsd, none",
                            metrics_str
                        ))
                    })?;
            config.global.metrics = metrics_backend;
        }

        Ok(config)
    }
}

/// confers `ConfigError` → limiteron 公开错误契约映射
///
/// - `FileNotFound` → "Config file not found: {}"；
/// - `ParseError` → "Failed to parse {FORMAT} config: {}"（FORMAT 为
///   confers 侧大写 YAML/TOML/JSON，与旧契约一致）；
/// - 其余（`InvalidValue` 覆盖目录/特殊文件/路径穿越等 confers 防护
///   命中、`SizeLimitExceeded`、`IoError`）→ "Failed to load config: {}"。
fn map_config_error(e: confers::ConfigError) -> LimiteronError {
    match e {
        confers::ConfigError::FileNotFound { filename, .. } => {
            LimiteronError::ConfigError(format!("Config file not found: {}", filename.display()))
        }
        confers::ConfigError::ParseError {
            format, message, ..
        } => LimiteronError::ConfigError(format!("Failed to parse {} config: {}", format, message)),
        other => LimiteronError::ConfigError(format!("Failed to load config: {}", other)),
    }
}

/// typed 反序列化失败的格式名标注：`Failed to parse {YAML|TOML|JSON} config`
fn map_parse_error(ext: &str, e: &serde_json::Error) -> LimiteronError {
    let format = match ext {
        "toml" => "TOML",
        "json" => "JSON",
        _ => "YAML",
    };
    LimiteronError::ConfigError(format!("Failed to parse {} config: {}", format, e))
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
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(
                    msg.contains("Config file not found"),
                    "the not-found contract must contain 'Config file not found': {msg}"
                )
            }
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
        match result {
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(
                    msg.contains("YAML"),
                    "the invalid YAML error must contain the format name YAML: {msg}"
                )
            }
            _ => panic!("expected ConfigError for invalid YAML"),
        }
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
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(
                    msg.contains("Unsupported"),
                    "must contain Unsupported: {msg}"
                );
                assert!(
                    msg.contains("yaml, yml, toml, json"),
                    "should list the supported extensions: {msg}"
                );
            }
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
            Err(LimiteronError::ConfigError(msg)) => assert!(msg.contains("TOML")),
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
            Err(LimiteronError::ConfigError(msg)) => assert!(msg.contains("JSON")),
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
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(
                    msg.contains("Unsupported"),
                    "must contain Unsupported: {msg}"
                );
                assert!(
                    msg.contains("yaml, yml, toml, json"),
                    "should list the supported extensions: {msg}"
                );
            }
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

    // 目录路径：confers 侧拒绝非普通文件，映射后仅断言 Err 存在性
    // （消息原文由 confers InvalidValue 承载，不作为公开契约钉死）
    #[test]
    fn test_load_directory_as_file() {
        let dir = tempfile::tempdir().unwrap();
        let result = ConfigLoader::load_from_file(dir.path());
        assert!(result.is_err());
        assert!(matches!(result, Err(LimiteronError::ConfigError(_))));
    }

    // load_from_file_with_env 测试
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
        unsafe { std::env::remove_var("LIMITERON_GLOBAL_STORAGE") };
        unsafe { std::env::remove_var("LIMITERON_GLOBAL_CACHE") };
        unsafe { std::env::remove_var("LIMITERON_GLOBAL_METRICS") };

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
        unsafe { std::env::set_var("LIMITERON_GLOBAL_STORAGE", "redis") };

        let result = ConfigLoader::load_from_file_with_env(&path);
        unsafe { std::env::remove_var("LIMITERON_GLOBAL_STORAGE") };
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
        unsafe { std::env::set_var("LIMITERON_GLOBAL_STORAGE", "invalid_storage") };

        let result = ConfigLoader::load_from_file_with_env(&path);
        unsafe { std::env::remove_var("LIMITERON_GLOBAL_STORAGE") };
        std::fs::remove_file(&path).ok();

        assert!(result.is_err());
        match result {
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(msg.contains("Invalid LIMITERON_GLOBAL_STORAGE"));
            }
            _ => panic!("expected ConfigError for invalid storage env"),
        }
    }

    #[test]
    fn test_load_from_file_with_env_empty_value_no_override() {
        let _guard = env_test_lock().lock().unwrap();
        let path = write_temp_config(&valid_yaml(), "yaml");
        unsafe { std::env::set_var("LIMITERON_GLOBAL_STORAGE", "") };

        let result = ConfigLoader::load_from_file_with_env(&path);
        unsafe { std::env::remove_var("LIMITERON_GLOBAL_STORAGE") };
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
        unsafe { std::env::set_var("LIMITERON_GLOBAL_CACHE", "redis") };

        let result = ConfigLoader::load_from_file_with_env(&path);
        unsafe { std::env::remove_var("LIMITERON_GLOBAL_CACHE") };
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
        unsafe { std::env::set_var("LIMITERON_GLOBAL_METRICS", "statsd") };

        let result = ConfigLoader::load_from_file_with_env(&path);
        unsafe { std::env::remove_var("LIMITERON_GLOBAL_METRICS") };
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
        unsafe { std::env::set_var("LIMITERON_GLOBAL_CACHE", "invalid_cache") };

        let result = ConfigLoader::load_from_file_with_env(&path);
        unsafe { std::env::remove_var("LIMITERON_GLOBAL_CACHE") };
        std::fs::remove_file(&path).ok();

        assert!(result.is_err());
        match result {
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(msg.contains("Invalid LIMITERON_GLOBAL_CACHE"));
            }
            _ => panic!("expected ConfigError for invalid cache env"),
        }
    }

    #[test]
    fn test_load_from_file_with_env_invalid_metrics() {
        let _guard = env_test_lock().lock().unwrap();
        let path = write_temp_config(&valid_yaml(), "yaml");
        unsafe { std::env::set_var("LIMITERON_GLOBAL_METRICS", "invalid_metrics") };

        let result = ConfigLoader::load_from_file_with_env(&path);
        unsafe { std::env::remove_var("LIMITERON_GLOBAL_METRICS") };
        std::fs::remove_file(&path).ok();

        assert!(result.is_err());
        match result {
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(msg.contains("Invalid LIMITERON_GLOBAL_METRICS"));
            }
            _ => panic!("expected ConfigError for invalid metrics env"),
        }
    }
}
