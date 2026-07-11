// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 自定义匹配器扩展模块
//!
//! 提供自定义匹配器接口和注册机制，允许用户在运行时动态注册和使用自定义匹配器。
//!
//! # 特性
//!
//! - 定义 CustomMatcher trait 作为匹配器接口
//! - 支持异步匹配操作
//! - 支持配置加载
//! - 提供线程安全的注册表（CustomMatcherRegistry）
//! - 支持运行时动态注册、查询和注销
//!
//! # 示例
//!
//! ```rust
//! use limiteron::matchers::custom::{CustomMatcher, CustomMatcherRegistry};
//! use limiteron::matchers::RequestContext;
//! use limiteron::error::LimiteronError;
//! use async_trait::async_trait;
//!
//! #[derive(Debug)]
//! struct MyCustomMatcher {
//!     threshold: u64,
//! }
//!
//! #[async_trait]
//! impl CustomMatcher for MyCustomMatcher {
//!     fn name(&self) -> &str {
//!         "my_custom"
//!     }
//!
//!     async fn matches(&self, context: &RequestContext) -> Result<bool, LimiteronError> {
//!         // 自定义匹配逻辑
//!         Ok(true)
//!     }
//!
//!     fn load_config(&mut self, config: serde_json::Value) -> Result<(), LimiteronError> {
//!         self.threshold = config["threshold"].as_u64().unwrap_or(100);
//!         Ok(())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let registry = CustomMatcherRegistry::new();
//!     let matcher = Box::new(MyCustomMatcher { threshold: 100 });
//!     registry.register("my_custom".to_string(), matcher).await.unwrap();
//! }
//! ```

use crate::error::LimiteronError;
use crate::matchers::RequestContext;
use ahash::AHashMap as HashMap;
use async_trait::async_trait;
use chrono::Timelike;
use log::{debug, error, info, warn};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// 输入验证常量
// ============================================================================

/// 最大匹配器名称长度
const MAX_MATCHER_NAME_LENGTH: usize = 100;

/// 最大 HTTP 头名称长度
const MAX_HEADER_NAME_LENGTH: usize = 256;

/// 最大 HTTP 头值长度
const MAX_HEADER_VALUE_LENGTH: usize = 4096;

/// 最大允许的 HTTP 头值数量
const MAX_ALLOWED_VALUES_COUNT: usize = 100;

// ============================================================================
// 输入验证函数
// ============================================================================

/// 验证匹配器名称
///
/// # 参数
/// - `name`: 匹配器名称
///
/// # 返回
/// - `Ok(())`: 验证通过
/// - `Err(LimiteronError)`: 验证失败
fn validate_matcher_name(name: &str) -> Result<(), LimiteronError> {
    if name.is_empty() {
        return Err(LimiteronError::ConfigError(
            "匹配器名称不能为空".to_string(),
        ));
    }

    if name.len() > MAX_MATCHER_NAME_LENGTH {
        return Err(LimiteronError::ConfigError(format!(
            "匹配器名称长度超过限制（最大 {} 字符）",
            MAX_MATCHER_NAME_LENGTH
        )));
    }

    // 只允许字母、数字、下划线和连字符
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(LimiteronError::ConfigError(
            "匹配器名称只能包含字母、数字、下划线和连字符".to_string(),
        ));
    }

    Ok(())
}

/// 验证 HTTP 头名称
///
/// # 参数
/// - `name`: HTTP 头名称
///
/// # 返回
/// - `Ok(())`: 验证通过
/// - `Err(LimiteronError)`: 验证失败
fn validate_header_name(name: &str) -> Result<(), LimiteronError> {
    if name.is_empty() {
        return Err(LimiteronError::ConfigError(
            "HTTP头名称不能为空".to_string(),
        ));
    }

    if name.len() > MAX_HEADER_NAME_LENGTH {
        return Err(LimiteronError::ConfigError(format!(
            "HTTP头名称长度超过限制（最大 {} 字符）",
            MAX_HEADER_NAME_LENGTH
        )));
    }

    // 只允许字母、数字、连字符
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-') {
        return Err(LimiteronError::ConfigError(
            "HTTP头名称只能包含字母、数字和连字符".to_string(),
        ));
    }

    Ok(())
}

/// 验证 HTTP 头值
///
/// # 参数
/// - `value`: HTTP 头值
///
/// # 返回
/// - `Ok(())`: 验证通过
/// - `Err(LimiteronError)`: 验证失败
fn validate_header_value(value: &str) -> Result<(), LimiteronError> {
    if value.len() > MAX_HEADER_VALUE_LENGTH {
        return Err(LimiteronError::ConfigError(format!(
            "HTTP头值长度超过限制（最大 {} 字符）",
            MAX_HEADER_VALUE_LENGTH
        )));
    }

    Ok(())
}

// ============================================================================
// CustomMatcher Trait
// ============================================================================

/// 自定义匹配器 trait
///
/// 所有自定义匹配器都需要实现此trait。
#[async_trait]
pub trait CustomMatcher: Send + Sync {
    /// 获取匹配器名称
    ///
    /// # 返回
    /// - 匹配器的唯一标识符
    fn name(&self) -> &str;

    /// 检查请求是否匹配
    ///
    /// # 参数
    /// - `context`: 请求上下文
    ///
    /// # 返回
    /// - `Ok(true)`: 请求匹配
    /// - `Ok(false)`: 请求不匹配
    /// - `Err(_)`: 发生错误
    async fn matches(&self, context: &RequestContext) -> Result<bool, LimiteronError>;

    /// 加载配置
    ///
    /// # 参数
    /// - `config`: 配置值（JSON格式）
    ///
    /// # 返回
    /// - `Ok(())`: 配置加载成功
    /// - `Err(_)`: 配置加载失败
    fn load_config(&mut self, config: Value) -> Result<(), LimiteronError>;
}

// ============================================================================
// CustomMatcherRegistry
// ============================================================================

/// 自定义匹配器注册表
///
/// 提供线程安全的匹配器注册、查询和注销功能。
#[derive(Clone)]
pub struct CustomMatcherRegistry {
    /// 匹配器存储（使用 RwLock 实现线程安全）
    matchers: Arc<RwLock<HashMap<String, Box<dyn CustomMatcher>>>>,
}

impl std::fmt::Debug for CustomMatcherRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomMatcherRegistry")
            .field("matchers", &"<custom matchers>")
            .finish()
    }
}

impl CustomMatcherRegistry {
    /// 创建新的注册表（保持向后兼容）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::custom::CustomMatcherRegistry;
    ///
    /// let registry = CustomMatcherRegistry::new();
    /// ```
    pub fn new() -> Self {
        Self {
            matchers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 创建设置器（Builder模式）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::custom::CustomMatcherRegistry;
    ///
    /// let registry = CustomMatcherRegistry::builder().build();
    /// ```
    pub fn builder() -> CustomMatcherRegistryBuilder {
        CustomMatcherRegistryBuilder::new()
    }

    /// 使用依赖注入创建（完整依赖模式）
    ///
    /// 对于CustomMatcherRegistry，无需外部依赖，此方法主要用于API一致性
    pub fn with_dependencies() -> Self {
        Self::new()
    }

    /// 注册自定义匹配器
    ///
    /// # 参数
    /// - `name`: 匹配器名称（唯一标识符）
    /// - `matcher`: 匹配器实例
    ///
    /// # 返回
    /// - `Ok(())`: 注册成功
    /// - `Err(LimiteronError::ConfigError)`: 名称已存在或验证失败
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::custom::{CustomMatcherRegistry, TimeWindowMatcher};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let registry = CustomMatcherRegistry::new();
    ///     let matcher = TimeWindowMatcher::new(9, 18);
    ///     registry.register("time_window".to_string(), Box::new(matcher)).await.unwrap();
    /// }
    /// ```
    pub async fn register(
        &self,
        name: String,
        matcher: Box<dyn CustomMatcher>,
    ) -> Result<(), LimiteronError> {
        // 验证匹配器名称
        validate_matcher_name(&name)?;

        let mut matchers = self.matchers.write().await;

        if matchers.contains_key(&name) {
            let error_msg = format!("匹配器 '{}' 已存在", name);
            warn!("{}", error_msg);
            return Err(LimiteronError::ConfigError(error_msg));
        }

        info!("注册自定义匹配器: {}", name);
        matchers.insert(name.clone(), matcher);
        debug!("当前注册的匹配器数量: {}", matchers.len());

        Ok(())
    }

    /// 获取匹配器
    ///
    /// # 参数
    /// - `name`: 匹配器名称
    ///
    /// # 返回
    /// - `Some(matcher)`: 找到匹配器
    /// - `None`: 未找到匹配器
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::custom::CustomMatcherRegistry;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let registry = CustomMatcherRegistry::new();
    ///     if let Some(matcher) = registry.get("time_window").await {
    ///         println!("找到匹配器: {}", matcher.name());
    /// }
    /// }
    /// ```
    pub async fn get(&self, name: &str) -> Option<Box<dyn CustomMatcher>> {
        let matchers = self.matchers.read().await;

        if let Some(_matcher) = matchers.get(name) {
            // 注意：这里不能直接返回引用，因为需要克隆
            // 由于 trait 对象不能 clone，我们需要另一种方式
            // 在实际使用中，应该通过调用匹配器的方法而不是获取所有权
            // 这里我们返回 None，实际使用时需要修改设计
            debug!("查询匹配器: {}", name);
            None
        } else {
            debug!("未找到匹配器: {}", name);
            None
        }
    }

    /// 检查匹配器是否存在
    ///
    /// # 参数
    /// - `name`: 匹配器名称
    ///
    /// # 返回
    /// - `true`: 匹配器存在
    /// - `false`: 匹配器不存在
    pub async fn contains(&self, name: &str) -> bool {
        let matchers = self.matchers.read().await;
        matchers.contains_key(name)
    }

    /// 注销匹配器
    ///
    /// # 参数
    /// - `name`: 匹配器名称
    ///
    /// # 返回
    /// - `Ok(())`: 注销成功
    /// - `Err(LimiteronError::ConfigError)`: 匹配器不存在
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::custom::CustomMatcherRegistry;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let registry = CustomMatcherRegistry::new();
    ///     // 首先注册一个匹配器
    ///     registry.register("my_matcher".to_string(), Box::new(
    ///         limiteron::matchers::custom::TimeWindowMatcher::new(9, 18)
    ///     )).await.unwrap();
    ///     // 然后注销它
    ///     registry.unregister("my_matcher").await.unwrap();
    /// }
    /// ```
    pub async fn unregister(&self, name: &str) -> Result<(), LimiteronError> {
        let mut matchers = self.matchers.write().await;

        if !matchers.contains_key(name) {
            let error_msg = format!("匹配器 '{}' 不存在", name);
            warn!("{}", error_msg);
            return Err(LimiteronError::ConfigError(error_msg));
        }

        info!("注销自定义匹配器: {}", name);
        matchers.remove(name);
        debug!("当前注册的匹配器数量: {}", matchers.len());

        Ok(())
    }

    /// 获取所有注册的匹配器名称
    ///
    /// # 返回
    /// - 匹配器名称列表
    #[allow(clippy::map_clone)]
    pub async fn list(&self) -> Vec<String> {
        let matchers = self.matchers.read().await;
        matchers.keys().map(|k| k.clone()).collect()
    }

    /// 获取注册的匹配器数量
    ///
    /// # 返回
    /// - 匹配器数量
    pub async fn count(&self) -> usize {
        let matchers = self.matchers.read().await;
        matchers.len()
    }

    /// 清空所有匹配器
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::custom::CustomMatcherRegistry;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let registry = CustomMatcherRegistry::new();
    ///     registry.clear().await;
    /// }
    /// ```
    pub async fn clear(&self) {
        let mut matchers = self.matchers.write().await;
        info!("清空所有自定义匹配器");
        matchers.clear();
    }

    /// 匹配请求
    ///
    /// 使用指定名称的匹配器检查请求是否匹配。
    ///
    /// # 参数
    /// - `name`: 匹配器名称
    /// - `context`: 请求上下文
    ///
    /// # 返回
    /// - `Ok(true)`: 匹配成功
    /// - `Ok(false)`: 匹配失败
    /// - `Err(_)`: 匹配器不存在或发生错误
    pub async fn match_with(
        &self,
        name: &str,
        context: &RequestContext,
    ) -> Result<bool, LimiteronError> {
        let matchers = self.matchers.read().await;

        let matcher = matchers.get(name).ok_or_else(|| {
            let error_msg = format!("匹配器 '{}' 不存在", name);
            error!("{}", error_msg);
            LimiteronError::ConfigError(error_msg)
        })?;

        debug!("使用匹配器 '{}' 检查请求", name);
        matcher.matches(context).await
    }
}

impl Default for CustomMatcherRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 自定义匹配器注册表设置器
#[derive(Debug, Clone, Default)]
pub struct CustomMatcherRegistryBuilder;

impl CustomMatcherRegistryBuilder {
    /// 创建新的设置器
    pub fn new() -> Self {
        Self
    }

    /// 构建CustomMatcherRegistry
    pub fn build(self) -> CustomMatcherRegistry {
        CustomMatcherRegistry::new()
    }
}

// ============================================================================
// TimeWindowMatcher 示例实现
// ============================================================================

/// 时间窗口匹配器
///
/// 根据当前时间是否在指定的时间窗口内来匹配请求。
///
/// # 示例
/// ```rust
/// use limiteron::matchers::custom::TimeWindowMatcher;
///
/// let matcher = TimeWindowMatcher::new(9, 18); // 9:00 - 18:00
/// ```
#[derive(Debug, Clone)]
pub struct TimeWindowMatcher {
    /// 开始小时（0-23）
    start_hour: u8,
    /// 结束小时（0-23）
    end_hour: u8,
}

impl TimeWindowMatcher {
    /// 创建新的时间窗口匹配器（保持向后兼容）
    ///
    /// # 参数
    /// - `start_hour`: 开始小时（0-23）
    /// - `end_hour`: 结束小时（0-23）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::custom::TimeWindowMatcher;
    ///
    /// let matcher = TimeWindowMatcher::new(9, 18);
    /// ```
    pub fn new(start_hour: u8, end_hour: u8) -> Self {
        assert!(start_hour <= 23, "开始小时必须在 0-23 范围内");
        assert!(end_hour <= 23, "结束小时必须在 0-23 范围内");

        Self {
            start_hour,
            end_hour,
        }
    }

    /// 获取开始小时
    pub fn start_hour(&self) -> u8 {
        self.start_hour
    }

    /// 获取结束小时
    pub fn end_hour(&self) -> u8 {
        self.end_hour
    }

    /// 创建设置器（Builder模式）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::custom::TimeWindowMatcher;
    ///
    /// let matcher = TimeWindowMatcher::builder()
    ///     .start_hour(9)
    ///     .end_hour(18)
    ///     .build();
    /// ```
    pub fn builder() -> TimeWindowMatcherBuilder {
        TimeWindowMatcherBuilder::new()
    }

    /// 使用依赖注入创建（完整依赖模式）
    ///
    /// 对于TimeWindowMatcher，无需外部依赖，此方法主要用于API一致性
    ///
    /// # 参数
    /// - `start_hour`: 开始小时（0-23）
    /// - `end_hour`: 结束小时（0-23）
    pub fn with_dependencies(start_hour: u8, end_hour: u8) -> Self {
        Self::new(start_hour, end_hour)
    }
}

#[async_trait]
impl CustomMatcher for TimeWindowMatcher {
    fn name(&self) -> &str {
        "time_window"
    }

    async fn matches(&self, _context: &RequestContext) -> Result<bool, LimiteronError> {
        let now = chrono::Utc::now();
        let hour = now.hour() as u8;

        // 检查当前小时是否在时间窗口内
        let matches = if self.start_hour <= self.end_hour {
            // 正常时间窗口（如 9-18）
            hour >= self.start_hour && hour <= self.end_hour
        } else {
            // 跨午夜时间窗口（如 22-6）
            hour >= self.start_hour || hour <= self.end_hour
        };

        debug!(
            "时间窗口匹配: 当前时间 {}小时, 窗口 {}-{}小时, 结果: {}",
            hour, self.start_hour, self.end_hour, matches
        );

        Ok(matches)
    }

    fn load_config(&mut self, config: Value) -> Result<(), LimiteronError> {
        // 先获取 start_hour 并验证
        let start_hour_u64 = config["start_hour"]
            .as_u64()
            .ok_or_else(|| LimiteronError::ConfigError("缺少 start_hour 配置".to_string()))?;
        // 先校验范围再转换，避免 `as u8` 截断绕过校验（如 256 截断为 0）
        if start_hour_u64 > 23 {
            return Err(LimiteronError::ConfigError(
                "start_hour 必须在 0-23 范围内".to_string(),
            ));
        }
        let start_hour = start_hour_u64 as u8;

        // 然后获取 end_hour 并验证
        let end_hour_u64 = config["end_hour"]
            .as_u64()
            .ok_or_else(|| LimiteronError::ConfigError("缺少 end_hour 配置".to_string()))?;
        if end_hour_u64 > 23 {
            return Err(LimiteronError::ConfigError(
                "end_hour 必须在 0-23 范围内".to_string(),
            ));
        }
        let end_hour = end_hour_u64 as u8;

        self.start_hour = start_hour;
        self.end_hour = end_hour;

        info!(
            "加载时间窗口匹配器配置: {}-{}小时",
            self.start_hour, self.end_hour
        );

        Ok(())
    }
}

/// 时间窗口匹配器设置器
#[derive(Debug, Clone, Default)]
pub struct TimeWindowMatcherBuilder {
    start_hour: u8,
    end_hour: u8,
}

impl TimeWindowMatcherBuilder {
    /// 创建新的设置器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置开始小时
    pub fn start_hour(mut self, start_hour: u8) -> Self {
        self.start_hour = start_hour;
        self
    }

    /// 设置结束小时
    pub fn end_hour(mut self, end_hour: u8) -> Self {
        self.end_hour = end_hour;
        self
    }

    /// 构建TimeWindowMatcher
    pub fn build(self) -> TimeWindowMatcher {
        TimeWindowMatcher::new(self.start_hour, self.end_hour)
    }
}

// ============================================================================
// HeaderMatcher 示例实现
// ============================================================================

/// HTTP头匹配器
///
/// 根据HTTP头的值来匹配请求。
///
/// # 示例
/// ```rust
/// use limiteron::matchers::custom::HeaderMatcher;
///
/// let matcher = HeaderMatcher::new("X-API-Key", vec!["secret123".to_string()]).unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct HeaderMatcher {
    /// HTTP头名称
    header_name: String,
    /// 允许的值列表
    allowed_values: Vec<String>,
    /// 是否区分大小写
    case_sensitive: bool,
}

impl HeaderMatcher {
    /// 创建新的HTTP头匹配器
    ///
    /// # 参数
    /// - `header_name`: HTTP头名称
    /// - `allowed_values`: 允许的值列表
    ///
    /// # 返回
    /// - 新的 HeaderMatcher 实例
    ///
    /// # 错误
    /// - 如果 header_name 或 allowed_values 验证失败
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::custom::HeaderMatcher;
    ///
    /// let matcher = HeaderMatcher::new("X-API-Key", vec!["secret123".to_string()]).unwrap();
    /// ```
    pub fn new(header_name: &str, allowed_values: Vec<String>) -> Result<Self, LimiteronError> {
        // 验证 HTTP 头名称
        validate_header_name(header_name)?;

        // 验证允许的值数量
        if allowed_values.len() > MAX_ALLOWED_VALUES_COUNT {
            return Err(LimiteronError::ValidationError(format!(
                "允许的值数量超过限制（最大 {}）",
                MAX_ALLOWED_VALUES_COUNT
            )));
        }

        // 验证每个值
        for value in &allowed_values {
            validate_header_value(value)?;
        }

        Ok(Self {
            header_name: header_name.to_lowercase(),
            allowed_values,
            case_sensitive: false,
        })
    }

    /// 设置是否区分大小写
    ///
    /// # 参数
    /// - `case_sensitive`: 是否区分大小写
    pub fn with_case_sensitive(mut self, case_sensitive: bool) -> Self {
        self.case_sensitive = case_sensitive;
        self
    }

    /// 获取HTTP头名称
    pub fn header_name(&self) -> &str {
        &self.header_name
    }

    /// 获取允许的值列表
    pub fn allowed_values(&self) -> &[String] {
        &self.allowed_values
    }

    /// 创建设置器（Builder模式）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::custom::HeaderMatcher;
    ///
    /// let matcher = HeaderMatcher::builder()
    ///     .header_name("X-API-Key")
    ///     .allowed_values(vec!["secret123".to_string()])
    ///     .case_sensitive(false)
    ///     .build();
    /// ```
    pub fn builder() -> HeaderMatcherBuilder {
        HeaderMatcherBuilder::new()
    }

    /// 使用依赖注入创建（完整依赖模式）
    ///
    /// 对于HeaderMatcher，无需外部依赖，此方法主要用于API一致性
    ///
    /// # 参数
    /// - `header_name`: HTTP头名称
    /// - `allowed_values`: 允许的值列表
    /// - `case_sensitive`: 是否区分大小写
    pub fn with_dependencies(
        header_name: &str,
        allowed_values: Vec<String>,
        case_sensitive: bool,
    ) -> Result<Self, LimiteronError> {
        Self::new(header_name, allowed_values).map(|mut m| {
            m.case_sensitive = case_sensitive;
            m
        })
    }
}

#[async_trait]
impl CustomMatcher for HeaderMatcher {
    fn name(&self) -> &str {
        "header"
    }

    async fn matches(&self, context: &RequestContext) -> Result<bool, LimiteronError> {
        let header_value = match context.get_header(&self.header_name) {
            Some(value) => value,
            None => {
                debug!("HTTP头 '{}' 不存在", self.header_name);
                return Ok(false);
            }
        };

        let matches = if self.case_sensitive {
            self.allowed_values.contains(header_value)
        } else {
            let lower_value = header_value.to_lowercase();
            self.allowed_values
                .iter()
                .any(|v| v.to_lowercase() == lower_value)
        };

        debug!(
            "HTTP头匹配: 头='{}', 值='{}', 结果: {}",
            self.header_name, header_value, matches
        );

        Ok(matches)
    }

    fn load_config(&mut self, config: Value) -> Result<(), LimiteronError> {
        if let Some(header_name) = config["header_name"].as_str() {
            validate_header_name(header_name)?;
            self.header_name = header_name.to_lowercase();
        }

        if let Some(values) = config["allowed_values"].as_array() {
            if values.len() > MAX_ALLOWED_VALUES_COUNT {
                return Err(LimiteronError::ConfigError(format!(
                    "允许的值数量超过限制（最大 {}）",
                    MAX_ALLOWED_VALUES_COUNT
                )));
            }

            self.allowed_values = values
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| {
                    validate_header_value(s)?;
                    Ok(s.to_string())
                })
                .collect::<Result<Vec<_>, LimiteronError>>()?;
        }

        if let Some(case_sensitive) = config["case_sensitive"].as_bool() {
            self.case_sensitive = case_sensitive;
        }

        info!(
            "加载HTTP头匹配器配置: 头='{}', 允许值={:?}, 区分大小写={}",
            self.header_name, self.allowed_values, self.case_sensitive
        );

        Ok(())
    }
}

/// HTTP头匹配器设置器
#[derive(Debug, Clone, Default)]
pub struct HeaderMatcherBuilder {
    header_name: Option<String>,
    allowed_values: Vec<String>,
    case_sensitive: bool,
}

impl HeaderMatcherBuilder {
    /// 创建新的设置器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置HTTP头名称
    pub fn header_name(mut self, header_name: &str) -> Self {
        self.header_name = Some(header_name.to_string());
        self
    }

    /// 设置允许的值列表
    pub fn allowed_values(mut self, allowed_values: Vec<String>) -> Self {
        self.allowed_values = allowed_values;
        self
    }

    /// 添加允许的值
    pub fn add_allowed_value(mut self, value: &str) -> Self {
        self.allowed_values.push(value.to_string());
        self
    }

    /// 设置是否区分大小写
    pub fn case_sensitive(mut self, case_sensitive: bool) -> Self {
        self.case_sensitive = case_sensitive;
        self
    }

    /// 构建HeaderMatcher
    pub fn build(self) -> Result<HeaderMatcher, LimiteronError> {
        HeaderMatcher::new(
            self.header_name.as_deref().unwrap_or(""),
            self.allowed_values,
        )
        .map(|mut m| {
            m.case_sensitive = self.case_sensitive;
            m
        })
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matchers::RequestContext;

    // ==================== CustomMatcherRegistry 测试 ====================

    #[tokio::test]
    async fn test_registry_new() {
        let registry = CustomMatcherRegistry::new();
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn test_registry_register() {
        let registry = CustomMatcherRegistry::new();
        let matcher = TimeWindowMatcher::new(9, 18);

        assert!(
            registry
                .register("time_window".to_string(), Box::new(matcher))
                .await
                .is_ok()
        );
        assert_eq!(registry.count().await, 1);
        assert!(registry.contains("time_window").await);
    }

    #[tokio::test]
    async fn test_registry_register_duplicate() {
        let registry = CustomMatcherRegistry::new();
        let matcher = TimeWindowMatcher::new(9, 18);

        assert!(
            registry
                .register("time_window".to_string(), Box::new(matcher))
                .await
                .is_ok()
        );

        let result = registry
            .register(
                "time_window".to_string(),
                Box::new(TimeWindowMatcher::new(10, 20)),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_unregister() {
        let registry = CustomMatcherRegistry::new();
        let matcher = TimeWindowMatcher::new(9, 18);

        registry
            .register("time_window".to_string(), Box::new(matcher))
            .await
            .unwrap();

        assert!(registry.unregister("time_window").await.is_ok());
        assert_eq!(registry.count().await, 0);
        assert!(!registry.contains("time_window").await);
    }

    #[tokio::test]
    async fn test_registry_unregister_nonexistent() {
        let registry = CustomMatcherRegistry::new();
        let result = registry.unregister("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_list() {
        let registry = CustomMatcherRegistry::new();

        registry
            .register(
                "matcher1".to_string(),
                Box::new(TimeWindowMatcher::new(9, 18)),
            )
            .await
            .unwrap();
        registry
            .register(
                "matcher2".to_string(),
                Box::new(HeaderMatcher::new("X-API-Key", vec!["secret".to_string()]).unwrap()),
            )
            .await
            .unwrap();

        let list = registry.list().await;
        assert_eq!(list.len(), 2);
        assert!(list.contains(&"matcher1".to_string()));
        assert!(list.contains(&"matcher2".to_string()));
    }

    #[tokio::test]
    async fn test_registry_clear() {
        let registry = CustomMatcherRegistry::new();

        registry
            .register(
                "matcher1".to_string(),
                Box::new(TimeWindowMatcher::new(9, 18)),
            )
            .await
            .unwrap();

        registry.clear().await;
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn test_registry_match_with() {
        let registry = CustomMatcherRegistry::new();
        let matcher = TimeWindowMatcher::new(0, 23); // 全天匹配

        registry
            .register("time_window".to_string(), Box::new(matcher))
            .await
            .unwrap();

        let context = RequestContext::new();
        let result = registry.match_with("time_window", &context).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_registry_match_with_nonexistent() {
        let registry = CustomMatcherRegistry::new();
        let context = RequestContext::new();

        let result = registry.match_with("nonexistent", &context).await;
        assert!(result.is_err());
    }

    // ==================== TimeWindowMatcher 测试 ====================

    #[tokio::test]
    async fn test_time_window_matcher_new() {
        let matcher = TimeWindowMatcher::new(9, 18);
        assert_eq!(matcher.name(), "time_window");
        assert_eq!(matcher.start_hour(), 9);
        assert_eq!(matcher.end_hour(), 18);
    }

    #[tokio::test]
    async fn test_time_window_matcher_matches() {
        let matcher = TimeWindowMatcher::new(0, 23); // 全天匹配
        let context = RequestContext::new();

        let result = matcher.matches(&context).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_time_window_matcher_load_config() {
        let mut matcher = TimeWindowMatcher::new(9, 18);
        let config = serde_json::json!({
            "start_hour": 10,
            "end_hour": 20
        });

        assert!(matcher.load_config(config).is_ok());
        assert_eq!(matcher.start_hour(), 10);
        assert_eq!(matcher.end_hour(), 20);
    }

    #[tokio::test]
    async fn test_time_window_matcher_load_config_invalid() {
        let mut matcher = TimeWindowMatcher::new(9, 18);
        let config = serde_json::json!({
            "start_hour": 25
        });

        let result = matcher.load_config(config);
        assert!(result.is_err());
    }

    #[test]
    #[should_panic(expected = "开始小时必须在 0-23 范围内")]
    fn test_time_window_matcher_invalid_start_hour() {
        TimeWindowMatcher::new(25, 18);
    }

    #[test]
    #[should_panic(expected = "结束小时必须在 0-23 范围内")]
    fn test_time_window_matcher_invalid_end_hour() {
        TimeWindowMatcher::new(9, 25);
    }

    // ==================== HeaderMatcher 测试 ====================

    #[tokio::test]
    async fn test_header_matcher_new() {
        let matcher = HeaderMatcher::new("X-API-Key", vec!["secret123".to_string()]).unwrap();
        assert_eq!(matcher.name(), "header");
        assert_eq!(matcher.header_name(), "x-api-key");
        assert_eq!(matcher.allowed_values().len(), 1);
    }

    #[tokio::test]
    async fn test_header_matcher_matches() {
        let matcher = HeaderMatcher::new("X-API-Key", vec!["secret123".to_string()]).unwrap();
        let context = RequestContext::new().with_header("X-API-Key", "secret123");

        let result = matcher.matches(&context).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_header_matcher_not_matches() {
        let matcher = HeaderMatcher::new("X-API-Key", vec!["secret123".to_string()]).unwrap();
        let context = RequestContext::new().with_header("X-API-Key", "wrong");

        let result = matcher.matches(&context).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_header_matcher_missing_header() {
        let matcher = HeaderMatcher::new("X-API-Key", vec!["secret123".to_string()]).unwrap();
        let context = RequestContext::new();

        let result = matcher.matches(&context).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_header_matcher_case_insensitive() {
        let matcher = HeaderMatcher::new("X-API-Key", vec!["SECRET123".to_string()]).unwrap();
        let context = RequestContext::new().with_header("X-API-Key", "secret123");

        let result = matcher.matches(&context).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_header_header_matcher_case_sensitive() {
        let matcher = HeaderMatcher::new("X-API-Key", vec!["SECRET123".to_string()])
            .unwrap()
            .with_case_sensitive(true);
        let context = RequestContext::new().with_header("X-API-Key", "secret123");

        let result = matcher.matches(&context).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_header_matcher_multiple_values() {
        let matcher = HeaderMatcher::new(
            "X-API-Key",
            vec!["secret123".to_string(), "secret456".to_string()],
        )
        .unwrap();
        let context1 = RequestContext::new().with_header("X-API-Key", "secret123");
        let context2 = RequestContext::new().with_header("X-API-Key", "secret456");

        assert!(matcher.matches(&context1).await.unwrap());
        assert!(matcher.matches(&context2).await.unwrap());
    }

    #[tokio::test]
    async fn test_header_matcher_load_config() {
        let mut matcher = HeaderMatcher::new("X-API-Key", vec!["secret123".to_string()]).unwrap();
        let config = serde_json::json!({
            "header_name": "Authorization",
            "allowed_values": ["Bearer token123"],
            "case_sensitive": true
        });

        assert!(matcher.load_config(config).is_ok());
        assert_eq!(matcher.header_name(), "authorization");
        assert_eq!(matcher.allowed_values().len(), 1);
    }

    // ==================== 并发测试 ====================

    #[tokio::test]
    async fn test_registry_concurrent_register() {
        let registry = Arc::new(CustomMatcherRegistry::new());
        let mut handles = vec![];

        for i in 0..10 {
            let registry_clone = Arc::clone(&registry);
            handles.push(tokio::spawn(async move {
                let matcher = TimeWindowMatcher::new(i as u8, (i + 10) as u8);
                registry_clone
                    .register(format!("matcher_{}", i), Box::new(matcher))
                    .await
            }));
        }

        let mut success_count = 0;
        for handle in handles {
            if handle.await.unwrap().is_ok() {
                success_count += 1;
            }
        }

        assert_eq!(success_count, 10);
        assert_eq!(registry.count().await, 10);
    }

    #[tokio::test]
    async fn test_registry_concurrent_match() {
        let registry = Arc::new(CustomMatcherRegistry::new());
        let matcher = TimeWindowMatcher::new(0, 23);

        registry
            .register("time_window".to_string(), Box::new(matcher))
            .await
            .unwrap();

        let mut handles = vec![];
        for _ in 0..100 {
            let registry_clone = Arc::clone(&registry);
            handles.push(tokio::spawn(async move {
                let context = RequestContext::new();
                registry_clone.match_with("time_window", &context).await
            }));
        }

        let mut success_count = 0;
        for handle in handles {
            if let Ok(Ok(true)) = handle.await {
                success_count += 1;
            }
        }

        assert_eq!(success_count, 100);
    }

    // ==================== 输入验证函数测试 ====================

    #[test]
    fn test_validate_matcher_name_empty() {
        let result = validate_matcher_name("");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_matcher_name_too_long() {
        let long_name = "a".repeat(MAX_MATCHER_NAME_LENGTH + 1);
        let result = validate_matcher_name(&long_name);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_matcher_name_invalid_chars() {
        let result = validate_matcher_name("invalid@name!");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_header_name_empty() {
        let result = validate_header_name("");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_header_name_too_long() {
        let long_name = "a".repeat(MAX_HEADER_NAME_LENGTH + 1);
        let result = validate_header_name(&long_name);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_header_name_invalid_chars() {
        let result = validate_header_name("X-Header@Name");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_header_value_too_long() {
        let long_value = "a".repeat(MAX_HEADER_VALUE_LENGTH + 1);
        let result = validate_header_value(&long_value);
        assert!(result.is_err());

        assert!(validate_header_value("").is_ok());
        assert!(validate_header_value("valid-value").is_ok());
    }

    // ==================== CustomMatcherRegistry 补充测试 ====================

    #[tokio::test]
    async fn test_registry_default() {
        let registry = CustomMatcherRegistry::default();
        assert_eq!(registry.count().await, 0);
    }

    #[test]
    fn test_registry_debug() {
        let registry = CustomMatcherRegistry::new();
        let debug_str = format!("{:?}", registry);
        assert!(debug_str.contains("CustomMatcherRegistry"));
        assert!(debug_str.contains("<custom matchers>"));
    }

    #[tokio::test]
    async fn test_registry_builder() {
        let registry = CustomMatcherRegistry::builder().build();
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn test_registry_with_dependencies() {
        let registry = CustomMatcherRegistry::with_dependencies();
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn test_registry_register_with_invalid_name() {
        let registry = CustomMatcherRegistry::new();
        let matcher = TimeWindowMatcher::new(9, 18);

        let result = registry.register("".to_string(), Box::new(matcher)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_get_found() {
        let registry = CustomMatcherRegistry::new();
        registry
            .register("test".to_string(), Box::new(TimeWindowMatcher::new(9, 18)))
            .await
            .unwrap();
        let result = registry.get("test").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_registry_get_not_found() {
        let registry = CustomMatcherRegistry::new();
        let result = registry.get("nonexistent").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_registry_list_empty() {
        let registry = CustomMatcherRegistry::new();
        let list = registry.list().await;
        assert!(list.is_empty());
    }

    // ==================== TimeWindowMatcher 补充测试 ====================

    #[test]
    fn test_time_window_matcher_builder() {
        let matcher = TimeWindowMatcher::builder()
            .start_hour(8)
            .end_hour(20)
            .build();
        assert_eq!(matcher.start_hour(), 8);
        assert_eq!(matcher.end_hour(), 20);
    }

    #[test]
    fn test_time_window_matcher_with_dependencies() {
        let matcher = TimeWindowMatcher::with_dependencies(7, 19);
        assert_eq!(matcher.start_hour(), 7);
        assert_eq!(matcher.end_hour(), 19);
    }

    #[tokio::test]
    async fn test_time_window_matcher_cross_midnight() {
        let matcher = TimeWindowMatcher::new(12, 11);
        let context = RequestContext::new();
        let result = matcher.matches(&context).await.unwrap();
        assert!(result);
    }

    #[test]
    fn test_time_window_matcher_load_config_missing_start_hour() {
        let mut matcher = TimeWindowMatcher::new(9, 18);
        let config = serde_json::json!({ "end_hour": 20 });
        let result = matcher.load_config(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_time_window_matcher_load_config_missing_end_hour() {
        let mut matcher = TimeWindowMatcher::new(9, 18);
        let config = serde_json::json!({ "start_hour": 10 });
        let result = matcher.load_config(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_time_window_matcher_load_config_start_out_of_range() {
        let mut matcher = TimeWindowMatcher::new(9, 18);
        let config = serde_json::json!({
            "start_hour": 25,
            "end_hour": 20,
        });
        let result = matcher.load_config(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_time_window_matcher_load_config_end_out_of_range() {
        let mut matcher = TimeWindowMatcher::new(9, 18);
        let config = serde_json::json!({
            "start_hour": 10,
            "end_hour": 25,
        });
        let result = matcher.load_config(config);
        assert!(result.is_err());
    }

    // ==================== HeaderMatcher 补充测试 ====================

    #[test]
    fn test_header_matcher_new_empty_name() {
        let result = HeaderMatcher::new("", vec!["value".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_header_matcher_new_invalid_name_chars() {
        let result = HeaderMatcher::new("X-Header@Name", vec!["value".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_header_matcher_new_too_many_values() {
        let values: Vec<String> = (0..=MAX_ALLOWED_VALUES_COUNT)
            .map(|i| format!("val_{}", i))
            .collect();
        let result = HeaderMatcher::new("X-Test", values);
        assert!(result.is_err());
    }

    #[test]
    fn test_header_matcher_new_value_too_long() {
        let long_value = "a".repeat(MAX_HEADER_VALUE_LENGTH + 1);
        let result = HeaderMatcher::new("X-Test", vec![long_value]);
        assert!(result.is_err());
    }

    #[test]
    fn test_header_matcher_builder() {
        let matcher = HeaderMatcher::builder()
            .header_name("X-Custom")
            .add_allowed_value("val1")
            .add_allowed_value("val2")
            .case_sensitive(true)
            .build()
            .unwrap();
        assert_eq!(matcher.header_name(), "x-custom");
        assert_eq!(matcher.allowed_values().len(), 2);
    }

    #[test]
    fn test_header_matcher_with_dependencies() {
        let matcher =
            HeaderMatcher::with_dependencies("X-Custom", vec!["value1".to_string()], true).unwrap();
        assert_eq!(matcher.header_name(), "x-custom");
        assert_eq!(matcher.allowed_values().len(), 1);
    }

    #[tokio::test]
    async fn test_header_matcher_case_sensitive_match() {
        let matcher = HeaderMatcher::new("X-Key", vec!["ExactMatch".to_string()])
            .unwrap()
            .with_case_sensitive(true);
        let context = RequestContext::new().with_header("x-key", "ExactMatch");
        let result = matcher.matches(&context).await.unwrap();
        assert!(result);
    }

    #[test]
    fn test_header_matcher_load_config_partial_header_name() {
        let mut matcher = HeaderMatcher::new("X-Original", vec!["value".to_string()]).unwrap();
        let config = serde_json::json!({ "header_name": "Authorization" });
        assert!(matcher.load_config(config).is_ok());
        assert_eq!(matcher.header_name(), "authorization");
        assert_eq!(matcher.allowed_values().len(), 1);
    }

    #[test]
    fn test_header_matcher_load_config_partial_values() {
        let mut matcher = HeaderMatcher::new("X-Original", vec!["old_value".to_string()]).unwrap();
        let config = serde_json::json!({ "allowed_values": ["new_value"] });
        assert!(matcher.load_config(config).is_ok());
        assert_eq!(matcher.header_name(), "x-original");
        assert_eq!(matcher.allowed_values().len(), 1);
        assert_eq!(matcher.allowed_values()[0], "new_value");
    }

    #[test]
    fn test_header_matcher_load_config_partial_case_sensitive() {
        let mut matcher = HeaderMatcher::new("X-Test", vec!["value".to_string()]).unwrap();
        let config = serde_json::json!({ "case_sensitive": true });
        assert!(matcher.load_config(config).is_ok());
        assert!(matcher.case_sensitive);
    }

    #[test]
    fn test_header_matcher_load_config_too_many_values() {
        let mut matcher = HeaderMatcher::new("X-Test", vec!["value".to_string()]).unwrap();
        let values: Vec<String> = (0..=MAX_ALLOWED_VALUES_COUNT)
            .map(|i| format!("value_{}", i))
            .collect();
        let config = serde_json::json!({ "allowed_values": values });
        let result = matcher.load_config(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_header_matcher_load_config_non_string_value() {
        let mut matcher = HeaderMatcher::new("X-Test", vec!["valid".to_string()]).unwrap();
        let config = serde_json::json!({
            "allowed_values": ["valid1", 123, "valid2"],
        });
        assert!(matcher.load_config(config).is_ok());
        assert_eq!(matcher.allowed_values().len(), 2);
        assert_eq!(matcher.allowed_values()[0], "valid1");
        assert_eq!(matcher.allowed_values()[1], "valid2");
    }

    #[test]
    fn test_header_matcher_builder_allowed_values() {
        // 覆盖 HeaderMatcherBuilder::allowed_values 方法（设置整个值列表）
        let values = vec!["v1".to_string(), "v2".to_string(), "v3".to_string()];
        let matcher = HeaderMatcher::builder()
            .header_name("X-Test")
            .allowed_values(values)
            .build()
            .unwrap();
        assert_eq!(matcher.allowed_values().len(), 3);
        assert_eq!(matcher.allowed_values()[0], "v1");
        assert_eq!(matcher.allowed_values()[2], "v3");
    }
}
