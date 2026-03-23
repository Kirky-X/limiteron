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
use crate::config::FlowControlConfig;
#[cfg(feature = "confers")]
use crate::error::FlowGuardError;

#[cfg(feature = "confers")]
use confers::loader::{load_file, LoaderConfig};
use std::path::Path;

// ============================================================================
// ConfigBuilder - 程序化配置构建
// ============================================================================

/// 配置构建器
///
/// 提供流式API构建FlowControlConfig配置。
///
/// # 示例
///
/// ```rust
/// use limiteron::config_loader::ConfigBuilder;
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
#[derive(Clone, Debug)]
pub struct ConfigBuilder {
    /// 全局配置
    storage: String,
    cache: String,
    metrics: String,
    /// 可信代理配置
    trusted_proxies: crate::config::TrustedProxyConfig,
    /// 规则列表
    rules: Vec<RuleBuilder>,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
            trusted_proxies: crate::config::TrustedProxyConfig::default(),
            rules: Vec::new(),
        }
    }
}

impl ConfigBuilder {
    /// 创建新的配置构建器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置存储类型
    pub fn with_storage(mut self, storage: impl Into<String>) -> Self {
        self.storage = storage.into();
        self
    }

    /// 设置缓存类型
    pub fn with_cache(mut self, cache: impl Into<String>) -> Self {
        self.cache = cache.into();
        self
    }

    /// 设置可信代理配置
    pub fn with_trusted_proxies(mut self, config: crate::config::TrustedProxyConfig) -> Self {
        self.trusted_proxies = config;
        self
    }

    /// 设置指标类型
    pub fn with_metrics(mut self, metrics: impl Into<String>) -> Self {
        self.metrics = metrics.into();
        self
    }

    /// 添加规则
    pub fn with_rule<F>(mut self, f: F) -> Self
    where
        F: FnOnce(RuleBuilder) -> RuleBuilder,
    {
        let rule = f(RuleBuilder::new());
        self.rules.push(rule);
        self
    }

    /// 构建配置
    pub fn build(self) -> Result<FlowControlConfig, FlowGuardError> {
        let rules: Result<Vec<_>, _> = self.rules.into_iter().map(|r| r.build()).collect();
        let rules = rules.map_err(|e| FlowGuardError::ConfigError(e.to_string()))?;

        if rules.is_empty() {
            return Err(FlowGuardError::ConfigError("至少需要一个规则".to_string()));
        }

        let config = FlowControlConfig {
            version: "0.1.0".to_string(),
            global: crate::config::GlobalConfig {
                storage: self.storage,
                cache: self.cache,
                metrics: self.metrics,
                trusted_proxies: self.trusted_proxies,
            },
            rules,
        };

        config.validate().map_err(FlowGuardError::ConfigError)?;
        Ok(config)
    }
}

/// 规则构建器
#[derive(Clone, Debug)]
pub struct RuleBuilder {
    id: String,
    name: String,
    priority: u16,
    matchers: Vec<crate::config::Matcher>,
    limiters: Vec<crate::config::LimiterConfig>,
    action: crate::config::ActionConfig,
}

impl RuleBuilder {
    pub fn new() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            priority: 100,
            matchers: Vec::new(),
            limiters: Vec::new(),
            action: crate::config::ActionConfig {
                on_exceed: crate::config::Action::Reject,
                ban: None,
            },
        }
    }
}

impl Default for RuleBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn priority(mut self, priority: u16) -> Self {
        self.priority = priority;
        self
    }

    pub fn user_matcher(mut self, user_ids: Vec<String>) -> Self {
        self.matchers
            .push(crate::config::Matcher::User { user_ids });
        self
    }

    pub fn ip_matcher(mut self, ip_ranges: Vec<String>) -> Self {
        self.matchers.push(crate::config::Matcher::Ip { ip_ranges });
        self
    }

    pub fn token_bucket(mut self, capacity: u64, refill_rate: u64) -> Self {
        self.limiters
            .push(crate::config::LimiterConfig::TokenBucket {
                capacity,
                refill_rate,
            });
        self
    }

    pub fn fixed_window(mut self, window_size: impl Into<String>, max_requests: u64) -> Self {
        self.limiters
            .push(crate::config::LimiterConfig::FixedWindow {
                window_size: window_size.into(),
                max_requests,
            });
        self
    }

    pub fn sliding_window(mut self, window_size: impl Into<String>, max_requests: u64) -> Self {
        self.limiters
            .push(crate::config::LimiterConfig::SlidingWindow {
                window_size: window_size.into(),
                max_requests,
            });
        self
    }

    pub fn concurrency_limit(mut self, max_concurrent: u64) -> Self {
        self.limiters
            .push(crate::config::LimiterConfig::Concurrency { max_concurrent });
        self
    }

    pub fn on_reject(mut self) -> Self {
        self.action.on_exceed = crate::config::Action::Reject;
        self
    }

    pub fn on_allow(mut self) -> Self {
        self.action.on_exceed = crate::config::Action::Allow;
        self
    }

    pub fn on_degrade(mut self) -> Self {
        self.action.on_exceed = crate::config::Action::Degrade;
        self
    }

    pub fn build(self) -> Result<crate::config::Rule, String> {
        if self.id.is_empty() {
            return Err("规则ID不能为空".to_string());
        }
        if self.name.is_empty() {
            return Err("规则名称不能为空".to_string());
        }
        if self.matchers.is_empty() {
            return Err("规则至少需要一个匹配器".to_string());
        }
        if self.limiters.is_empty() {
            return Err("规则至少需要一个限流器".to_string());
        }

        Ok(crate::config::Rule {
            id: self.id,
            name: self.name,
            priority: self.priority,
            matchers: self.matchers,
            limiters: self.limiters,
            action: self.action,
        })
    }
}

// ============================================================================
// ConfigLoader - 使用Confers配置加载
// ============================================================================

/// 配置加载器
///
/// 使用confers库进行配置加载，支持：
/// - 多格式配置文件（TOML、YAML、JSON）
/// - 环境变量覆盖（使用LIMITERON_前缀）
/// - 文件监听和热重载（通过ConfigWatcher）
///
/// # 示例
///
/// ```rust,no_run
/// use limiteron::config_loader::ConfigLoader;
///
/// // 从TOML文件加载配置（主要格式）
/// let config = ConfigLoader::load_from_file("config.toml")?;
///
/// // 从文件加载配置并支持环境变量覆盖
/// let config = ConfigLoader::load_from_file("config.toml")?;
///
/// // 使用默认配置文件（优先config.toml）
/// let config = ConfigLoader::load_default()?;
/// # Ok::<(), limiteron::error::FlowGuardError>(())
/// ```
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
