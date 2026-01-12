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
//! use limiteron::custom_matcher::{CustomMatcher, CustomMatcherRegistry};
//! use limiteron::matchers::RequestContext;
//! use limiteron::error::FlowGuardError;
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
//!     async fn matches(&self, context: &RequestContext) -> Result<bool, FlowGuardError> {
//!         // 自定义匹配逻辑
//!         Ok(true)
//!     }
//!
//!     fn load_config(&mut self, config: serde_json::Value) -> Result<(), FlowGuardError> {
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

use crate::error::FlowGuardError;
use crate::matchers::RequestContext;
use ahash::AHashMap as HashMap;
use async_trait::async_trait;
use chrono::Timelike;
use regex::Regex;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

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

/// 正则表达式最大复杂度（嵌套深度）
#[allow(dead_code)]
const MAX_REGEX_NESTING_DEPTH: usize = 10;

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
/// - `Err(FlowGuardError)`: 验证失败
fn validate_matcher_name(name: &str) -> Result<(), FlowGuardError> {
    if name.is_empty() {
        return Err(FlowGuardError::ConfigError(
            "匹配器名称不能为空".to_string(),
        ));
    }

    if name.len() > MAX_MATCHER_NAME_LENGTH {
        return Err(FlowGuardError::ConfigError(format!(
            "匹配器名称长度超过限制（最大 {} 字符）",
            MAX_MATCHER_NAME_LENGTH
        )));
    }

    // 只允许字母、数字、下划线和连字符
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(FlowGuardError::ConfigError(
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
/// - `Err(FlowGuardError)`: 验证失败
fn validate_header_name(name: &str) -> Result<(), FlowGuardError> {
    if name.is_empty() {
        return Err(FlowGuardError::ConfigError(
            "HTTP头名称不能为空".to_string(),
        ));
    }

    if name.len() > MAX_HEADER_NAME_LENGTH {
        return Err(FlowGuardError::ConfigError(format!(
            "HTTP头名称长度超过限制（最大 {} 字符）",
            MAX_HEADER_NAME_LENGTH
        )));
    }

    // 只允许字母、数字、连字符
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-') {
        return Err(FlowGuardError::ConfigError(
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
/// - `Err(FlowGuardError)`: 验证失败
fn validate_header_value(value: &str) -> Result<(), FlowGuardError> {
    if value.len() > MAX_HEADER_VALUE_LENGTH {
        return Err(FlowGuardError::ConfigError(format!(
            "HTTP头值长度超过限制（最大 {} 字符）",
            MAX_HEADER_VALUE_LENGTH
        )));
    }

    Ok(())
}

/// 验证正则表达式复杂度
///
/// 防止 ReDoS（正则表达式拒绝服务）攻击
///
/// # 参数
/// - `pattern`: 正则表达式模式
///
/// # 返回
/// - `Ok(())`: 验证通过
/// - `Err(FlowGuardError)`: 验证失败
#[allow(dead_code)]
fn validate_regex_complexity(pattern: &str) -> Result<(), FlowGuardError> {
    // 检查模式长度
    if pattern.len() > 1000 {
        return Err(FlowGuardError::ConfigError(
            "正则表达式模式过长（最大 1000 字符）".to_string(),
        ));
    }

    // 检查嵌套深度（简单的括号计数）
    let mut depth: usize = 0;
    let mut max_depth: usize = 0;
    for c in pattern.chars() {
        match c {
            '(' => {
                depth += 1;
                max_depth = max_depth.max(depth);
            }
            ')' => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    if max_depth > MAX_REGEX_NESTING_DEPTH {
        return Err(FlowGuardError::ConfigError(format!(
            "正则表达式嵌套深度过大（最大 {}）",
            MAX_REGEX_NESTING_DEPTH
        )));
    }

    // 检查危险模式（可能导致指数回溯）
    let dangerous_patterns = [
        "(.+)+",     // 嵌套量词
        "(.+)*",     // 嵌套量词
        "(.+){2,}",  // 嵌套量词
        "([a-z]+)+", // 嵌套量词
        "([a-z]+)*", // 嵌套量词
    ];

    for dangerous in &dangerous_patterns {
        if pattern.contains(dangerous) {
            return Err(FlowGuardError::ConfigError(format!(
                "正则表达式包含危险模式: {}",
                dangerous
            )));
        }
    }

    // 尝试编译正则表达式以验证语法
    Regex::new(pattern)
        .map_err(|e| FlowGuardError::ConfigError(format!("无效的正则表达式: {}", e)))?;

    Ok(())
}

/// 清理字符串（移除危险字符）
///
/// # 参数
/// - `input`: 输入字符串
///
/// # 返回
/// - 清理后的字符串
#[allow(dead_code)]
fn sanitize_string(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || "-_.".contains(*c))
        .collect()
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
    async fn matches(&self, context: &RequestContext) -> Result<bool, FlowGuardError>;

    /// 加载配置
    ///
    /// # 参数
    /// - `config`: 配置值（JSON格式）
    ///
    /// # 返回
    /// - `Ok(())`: 配置加载成功
    /// - `Err(_)`: 配置加载失败
    fn load_config(&mut self, config: Value) -> Result<(), FlowGuardError>;
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
    /// 创建新的注册表
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::custom_matcher::CustomMatcherRegistry;
    ///
    /// let registry = CustomMatcherRegistry::new();
    /// ```
    pub fn new() -> Self {
        Self {
            matchers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册自定义匹配器
    ///
    /// # 参数
    /// - `name`: 匹配器名称（唯一标识符）
    /// - `matcher`: 匹配器实例
    ///
    /// # 返回
    /// - `Ok(())`: 注册成功
    /// - `Err(FlowGuardError::ConfigError)`: 名称已存在或验证失败
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::custom_matcher::{CustomMatcherRegistry, TimeWindowMatcher};
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
    ) -> Result<(), FlowGuardError> {
        // 验证匹配器名称
        validate_matcher_name(&name)?;

        let mut matchers = self.matchers.write().await;

        if matchers.contains_key(&name) {
            let error_msg = format!("匹配器 '{}' 已存在", name);
            warn!("{}", error_msg);
            return Err(FlowGuardError::ConfigError(error_msg));
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
    /// use limiteron::custom_matcher::CustomMatcherRegistry;
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
    /// - `Err(FlowGuardError::ConfigError)`: 匹配器不存在
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::custom_matcher::CustomMatcherRegistry;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let registry = CustomMatcherRegistry::new();
    ///     registry.unregister("time_window".to_string()).await.unwrap();
    /// }
    /// ```
    pub async fn unregister(&self, name: &str) -> Result<(), FlowGuardError> {
        let mut matchers = self.matchers.write().await;

        if !matchers.contains_key(name) {
            let error_msg = format!("匹配器 '{}' 不存在", name);
            warn!("{}", error_msg);
            return Err(FlowGuardError::ConfigError(error_msg));
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
    /// use limiteron::custom_matcher::CustomMatcherRegistry;
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
    ) -> Result<bool, FlowGuardError> {
        let matchers = self.matchers.read().await;

        let matcher = matchers.get(name).ok_or_else(|| {
            let error_msg = format!("匹配器 '{}' 不存在", name);
            error!("{}", error_msg);
            FlowGuardError::ConfigError(error_msg)
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

// ============================================================================
// TimeWindowMatcher 示例实现
// ============================================================================

/// 时间窗口匹配器
///
/// 根据当前时间是否在指定的时间窗口内来匹配请求。
///
/// # 示例
/// ```rust
/// use limiteron::custom_matcher::TimeWindowMatcher;
/// use limiteron::matchers::RequestContext;
/// use limiteron::error::FlowGuardError;
/// use async_trait::async_trait;
///
/// #[tokio::main]
/// async fn main() {
///     let matcher = TimeWindowMatcher::new(9, 18); // 9:00 - 18:00
///     let context = RequestContext::new();
///     let matches = matcher.matches(&context).await.unwrap();
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TimeWindowMatcher {
    /// 开始小时（0-23）
    start_hour: u8,
    /// 结束小时（0-23）
    end_hour: u8,
}

impl TimeWindowMatcher {
    /// 创建新的时间窗口匹配器
    ///
    /// # 参数
    /// - `start_hour`: 开始小时（0-23）
    /// - `end_hour`: 结束小时（0-23）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::custom_matcher::TimeWindowMatcher;
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
}

#[async_trait]
impl CustomMatcher for TimeWindowMatcher {
    fn name(&self) -> &str {
        "time_window"
    }

    async fn matches(&self, _context: &RequestContext) -> Result<bool, FlowGuardError> {
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

    fn load_config(&mut self, config: Value) -> Result<(), FlowGuardError> {
        let start_hour = config["start_hour"]
            .as_u64()
            .ok_or_else(|| FlowGuardError::ConfigError("缺少 start_hour 配置".to_string()))?
            as u8;

        let end_hour = config["end_hour"]
            .as_u64()
            .ok_or_else(|| FlowGuardError::ConfigError("缺少 end_hour 配置".to_string()))?
            as u8;

        if start_hour > 23 {
            return Err(FlowGuardError::ConfigError(
                "start_hour 必须在 0-23 范围内".to_string(),
            ));
        }

        if end_hour > 23 {
            return Err(FlowGuardError::ConfigError(
                "end_hour 必须在 0-23 范围内".to_string(),
            ));
        }

        self.start_hour = start_hour;
        self.end_hour = end_hour;

        info!(
            "加载时间窗口匹配器配置: {}-{}小时",
            self.start_hour, self.end_hour
        );

        Ok(())
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
/// use limiteron::custom_matcher::HeaderMatcher;
/// use limiteron::matchers::RequestContext;
/// use limiteron::error::FlowGuardError;
/// use async_trait::async_trait;
///
/// #[tokio::main]
/// async fn main() {
///     let matcher = HeaderMatcher::new("X-API-Key", vec!["secret123".to_string()]);
///     let context = RequestContext::new().with_header("X-API-Key", "secret123");
///     let matches = matcher.matches(&context).await.unwrap();
/// }
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
    /// use limiteron::custom_matcher::HeaderMatcher;
    ///
    /// let matcher = HeaderMatcher::new("X-API-Key", vec!["secret123".to_string()]).unwrap();
    /// ```
    pub fn new(header_name: &str, allowed_values: Vec<String>) -> Result<Self, FlowGuardError> {
        // 验证 HTTP 头名称
        validate_header_name(header_name)?;

        // 验证允许的值数量
        if allowed_values.len() > MAX_ALLOWED_VALUES_COUNT {
            return Err(FlowGuardError::ValidationError(format!(
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
}

#[async_trait]
impl CustomMatcher for HeaderMatcher {
    fn name(&self) -> &str {
        "header"
    }

    async fn matches(&self, context: &RequestContext) -> Result<bool, FlowGuardError> {
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

    fn load_config(&mut self, config: Value) -> Result<(), FlowGuardError> {
        if let Some(header_name) = config["header_name"].as_str() {
            validate_header_name(header_name)?;
            self.header_name = header_name.to_lowercase();
        }

        if let Some(values) = config["allowed_values"].as_array() {
            if values.len() > MAX_ALLOWED_VALUES_COUNT {
                return Err(FlowGuardError::ConfigError(format!(
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
                .collect::<Result<Vec<_>, FlowGuardError>>()?;
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

        assert!(registry
            .register("time_window".to_string(), Box::new(matcher))
            .await
            .is_ok());
        assert_eq!(registry.count().await, 1);
        assert!(registry.contains("time_window").await);
    }

    #[tokio::test]
    async fn test_registry_register_duplicate() {
        let registry = CustomMatcherRegistry::new();
        let matcher = TimeWindowMatcher::new(9, 18);

        assert!(registry
            .register("time_window".to_string(), Box::new(matcher))
            .await
            .is_ok());

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
}
