//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 配置加载器模块
//!
//! 支持两种配置加载模式：
//! - confers模式：使用confers库进行配置加载（启用confers特性时）
//! - 构造器模式：使用GovernorBuilder进行配置构建（默认模式）

use crate::config::FlowControlConfig;
use crate::error::FlowGuardError;

#[cfg(feature = "confers")]
use confers::{ConfigLoader as ConfersConfigLoader, OptionalValidate as ConfersOptionalValidate};

#[cfg(feature = "confers")]
use std::path::Path;

// ============================================================================
// 无confers特性：使用构造器模式
// ============================================================================

/// 配置构建器（当不启用confers特性时使用）
///
/// 提供流式API构建FlowControlConfig配置。
///
/// # 示例
///
/// ```rust
/// use limiteron::config_builder::ConfigBuilder;
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
    /// 规则列表
    rules: Vec<RuleBuilder>,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
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
                on_exceed: "reject".to_string(),
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
        self.action.on_exceed = "reject".to_string();
        self
    }

    pub fn on_allow(mut self) -> Self {
        self.action.on_exceed = "allow".to_string();
        self
    }

    pub fn on_degrade(mut self) -> Self {
        self.action.on_exceed = "degrade".to_string();
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
// 有confers特性：使用confers配置加载
// ============================================================================

/// 配置加载器（当启用confers特性时使用）
///
/// 使用confers库进行配置加载，支持：
/// - 多格式配置文件（YAML、TOML、JSON）
/// - 环境变量覆盖
/// - 文件监听和热重载
///
/// # 示例
///
/// ```rust,no_run
/// use limiteron::config_loader::ConfigLoader;
///
/// // 从文件加载配置
/// let config = ConfigLoader::load_from_file("config.yaml")?;
///
/// // 从文件加载配置并支持环境变量覆盖
/// let config = ConfigLoader::load_from_file_with_env("config.yaml")?;
///
/// // 使用confers的完整功能
/// use limiteron::config_loader::ConfersConfigLoader;
///
/// let config = ConfersConfigLoader::new()
///     .with_file("config.yaml")
///     .with_env_prefix("LIMITERON")
///     .load_sync()?;
/// # Ok::<(), limiteron::error::FlowGuardError>(())
/// ```
#[cfg(feature = "confers")]
#[derive(Clone)]
pub struct ConfigLoader;

#[cfg(feature = "confers")]
impl ConfigLoader {
    /// 从文件加载配置
    ///
    /// 支持YAML、TOML、JSON格式的配置文件。
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
    /// let config = ConfigLoader::load_from_file("config.yaml")?;
    /// # Ok::<(), limiteron::error::FlowGuardError>(())
    /// ```
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<FlowControlConfig, FlowGuardError> {
        // 使用confers的ConfigLoader进行配置加载
        let loader = ConfersConfigLoader::<FlowControlConfig>::new()
            .with_file(&path)
            .with_env_prefix("LIMITERON")
            .with_env(true);

        loader
            .load_sync()
            .map_err(|e| FlowGuardError::ConfigError(e.to_string()))
    }

    /// 从文件加载配置，支持环境变量覆盖
    ///
    /// 环境变量命名规则：`LIMITERON_<SECTION>_<FIELD>`
    ///
    /// # 参数
    /// - `path`: 配置文件路径
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
    /// let config = ConfigLoader::load_from_file_with_env("config.yaml")?;
    /// # Ok::<(), limiteron::error::FlowGuardError>(())
    /// ```
    pub fn load_from_file_with_env<P: AsRef<Path>>(
        path: P,
    ) -> Result<FlowControlConfig, FlowGuardError> {
        Self::load_from_file(path)
    }

    /// 使用默认配置文件名加载配置
    ///
    /// 按以下顺序查找配置文件：
    /// 1. 当前目录下的 `config.yaml`
    /// 2. 当前目录下的 `config.toml`
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
        let config_paths = ["config.yaml", "config.toml", "config.json"];

        for config_path in &config_paths {
            if Path::new(config_path).exists() {
                return Self::load_from_file(config_path);
            }
        }

        Err(FlowGuardError::ConfigError(
            "未找到默认配置文件 (config.yaml/toml/json)".to_string(),
        ))
    }
}

/// 实现confers的OptionalValidate trait
#[cfg(feature = "confers")]
impl ConfersOptionalValidate for FlowControlConfig {}

#[cfg(test)]
#[cfg(feature = "confers")]
mod confers_tests {
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

    #[test]
    fn test_load_yaml_config() {
        let temp_file = create_test_config_yaml();
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
}

#[cfg(test)]
mod builder_tests {
    use super::*;

    #[test]
    fn test_config_builder_basic() {
        let config = ConfigBuilder::new()
            .with_storage("memory")
            .with_cache("memory")
            .with_metrics("prometheus")
            .with_rule(|rule| {
                rule.id("default")
                    .name("Default Rule")
                    .priority(100)
                    .user_matcher(vec!["*".to_string()])
                    .token_bucket(1000, 100)
                    .on_reject()
            })
            .build()
            .unwrap();

        assert_eq!(config.global.storage, "memory");
        assert_eq!(config.global.cache, "memory");
        assert_eq!(config.global.metrics, "prometheus");
    }

    #[test]
    fn test_config_builder_with_rule() {
        let config = ConfigBuilder::new()
            .with_rule(|rule| {
                rule.id("test_rule")
                    .name("Test Rule")
                    .priority(100)
                    .user_matcher(vec!["*".to_string()])
                    .token_bucket(1000, 100)
                    .on_reject()
            })
            .build()
            .unwrap();

        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].id, "test_rule");
        assert_eq!(config.rules[0].priority, 100);
    }

    #[test]
    fn test_config_builder_empty_rules() {
        let result = ConfigBuilder::new().build();
        assert!(result.is_err());
    }

    #[test]
    fn test_rule_builder_missing_id() {
        let result = RuleBuilder::new()
            .name("Test Rule")
            .priority(100)
            .token_bucket(1000, 100)
            .build();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "规则ID不能为空");
    }

    #[test]
    fn test_rule_builder_missing_limiters() {
        let result = RuleBuilder::new()
            .id("test")
            .name("Test")
            .priority(100)
            .user_matcher(vec!["*".to_string()])
            .build();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "规则至少需要一个限流器");
    }

    #[test]
    fn test_multiple_rules() {
        let config = ConfigBuilder::new()
            .with_rule(|rule| {
                rule.id("rule1")
                    .name("Rule 1")
                    .priority(100)
                    .user_matcher(vec!["*".to_string()])
                    .token_bucket(1000, 100)
            })
            .with_rule(|rule| {
                rule.id("rule2")
                    .name("Rule 2")
                    .priority(50)
                    .ip_matcher(vec!["*".to_string()])
                    .fixed_window("1s", 100)
            })
            .build()
            .unwrap();

        assert_eq!(config.rules.len(), 2);
        assert_eq!(config.rules[0].id, "rule1");
        assert_eq!(config.rules[1].id, "rule2");
    }
}
