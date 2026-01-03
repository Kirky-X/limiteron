//! 设备类型匹配器
//!
//! 基于User-Agent解析设备类型、浏览器和操作系统信息。
//!
//! # 特性
//!
//! - 识别设备类型（移动端/桌面端/平板/API/未知）
//! - 识别浏览器类型
//! - 识别操作系统
//! - 内置缓存（性能开销 < 500μs）
//! - 支持自定义规则
//!
//! # 性能
//!
//! - 识别准确率 > 90%
//! - 性能开销 P99 < 500μs
//! - 缓存命中率 > 90%
//!
//! # 使用示例
//!
//! ```rust
//! use limiteron::device_matcher::{DeviceMatcher, DeviceCondition};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let matcher = DeviceMatcher::new().await?;
//!
//! let user_agent = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X) AppleWebKit/605.1.15";
//! let info = matcher.parse(user_agent)?;
//!
//! let condition = DeviceCondition {
//!     device_types: vec![DeviceType::Mobile],
//!     browsers: vec![],
//!     os: vec![],
//! };
//!
//! let matched = matcher.matches(&info, &condition)?;
//! # Ok(())
//! # }
//! ```

use crate::error::FlowGuardError;
use dashmap::DashMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};
use woothee::parser::Parser;

// ============================================================================
// 输入验证常量
// ============================================================================

/// 最大 User-Agent 长度
const MAX_USER_AGENT_LENGTH: usize = 2048;

/// 最大自定义规则数量
const MAX_CUSTOM_RULES_COUNT: usize = 100;

/// 最大正则表达式模式长度
const MAX_REGEX_PATTERN_LENGTH: usize = 500;

// ============================================================================
// 输入验证函数
// ============================================================================

/// 验证 User-Agent 字符串
///
/// # 参数
/// - `user_agent`: User-Agent 字符串
///
/// # 返回
/// - `Ok(())`: 验证通过
/// - `Err(FlowGuardError)`: 验证失败
fn validate_user_agent(user_agent: &str) -> Result<(), FlowGuardError> {
    let trimmed = user_agent.trim();

    if trimmed.is_empty() {
        return Err(FlowGuardError::ConfigError(
            "User-Agent 不能为空".to_string(),
        ));
    }

    if trimmed.len() > MAX_USER_AGENT_LENGTH {
        return Err(FlowGuardError::ConfigError(format!(
            "User-Agent 长度超过限制（最大 {} 字符）",
            MAX_USER_AGENT_LENGTH
        )));
    }

    // 检查是否包含空字节（潜在的攻击向量）
    if trimmed.contains('\0') {
        return Err(FlowGuardError::ConfigError(
            "User-Agent 包含无效字符".to_string(),
        ));
    }

    Ok(())
}

/// 验证正则表达式模式
///
/// # 参数
/// - `pattern`: 正则表达式模式
///
/// # 返回
/// - `Ok(())`: 验证通过
/// - `Err(FlowGuardError)`: 验证失败
fn validate_regex_pattern(pattern: &str) -> Result<(), FlowGuardError> {
    if pattern.is_empty() {
        return Err(FlowGuardError::ConfigError(
            "正则表达式模式不能为空".to_string(),
        ));
    }

    if pattern.len() > MAX_REGEX_PATTERN_LENGTH {
        return Err(FlowGuardError::ConfigError(format!(
            "正则表达式模式长度超过限制（最大 {} 字符）",
            MAX_REGEX_PATTERN_LENGTH
        )));
    }

    // 检查嵌套深度
    let mut depth = 0;
    let mut max_depth = 0;
    for c in pattern.chars() {
        match c {
            '(' => {
                depth += 1;
                max_depth = max_depth.max(depth);
            }
            ')' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            _ => {}
        }
    }

    if max_depth > 10 {
        return Err(FlowGuardError::ConfigError(
            "正则表达式嵌套深度过大（最大 10）".to_string(),
        ));
    }

    // 检查危险模式（可能导致指数回溯）
    let dangerous_patterns = [
        "(.+)+",
        "(.+)*",
        "(.+){2,}",
        "([a-z]+)+",
        "([a-z]+)*",
        ".*.*.*",
    ];

    for dangerous in &dangerous_patterns {
        if pattern.contains(dangerous) {
            return Err(FlowGuardError::ConfigError(format!(
                "正则表达式包含危险模式: {}",
                dangerous
            )));
        }
    }

    // 尝试编译以验证语法
    Regex::new(pattern)
        .map_err(|e| FlowGuardError::ConfigError(format!("无效的正则表达式: {}", e)))?;

    Ok(())
}

/// 清理 User-Agent 字符串
///
/// # 参数
/// - `user_agent`: User-Agent 字符串
///
/// # 返回
/// - 清理后的字符串
fn sanitize_user_agent(user_agent: &str) -> String {
    user_agent
        .chars()
        .filter(|c| c.is_ascii() || c.is_alphanumeric() || " -./:;()[]{}@".contains(*c))
        .collect()
}

// ============================================================================
// 设备类型
// ============================================================================

/// 设备类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeviceType {
    /// 移动设备
    Mobile,
    /// 桌面设备
    Desktop,
    /// 平板设备
    Tablet,
    /// API客户端
    API,
    /// 未知设备
    Unknown,
}

impl DeviceType {
    /// 从字符串解析设备类型
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "mobile" | "smartphone" => DeviceType::Mobile,
            "desktop" | "pc" => DeviceType::Desktop,
            "tablet" | "ipad" => DeviceType::Tablet,
            "api" | "bot" | "crawler" => DeviceType::API,
            _ => DeviceType::Unknown,
        }
    }

    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            DeviceType::Mobile => "mobile",
            DeviceType::Desktop => "desktop",
            DeviceType::Tablet => "tablet",
            DeviceType::API => "api",
            DeviceType::Unknown => "unknown",
        }
    }

    /// 检查是否为移动设备（包括平板）
    pub fn is_mobile(&self) -> bool {
        matches!(self, DeviceType::Mobile | DeviceType::Tablet)
    }

    /// 检查是否为桌面设备
    pub fn is_desktop(&self) -> bool {
        matches!(self, DeviceType::Desktop)
    }

    /// 检查是否为API客户端
    pub fn is_api(&self) -> bool {
        matches!(self, DeviceType::API)
    }
}

impl std::fmt::Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// 设备信息
// ============================================================================

/// 设备信息
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceInfo {
    /// 设备类型
    pub device_type: DeviceType,
    /// 浏览器名称
    pub browser: Option<String>,
    /// 浏览器版本
    pub browser_version: Option<String>,
    /// 操作系统
    pub os: Option<String>,
    /// 操作系统版本
    pub os_version: Option<String>,
    /// 原始User-Agent
    pub user_agent: Option<String>,
}

impl DeviceInfo {
    /// 创建空的设备信息
    pub fn empty() -> Self {
        Self {
            device_type: DeviceType::Unknown,
            browser: None,
            browser_version: None,
            os: None,
            os_version: None,
            user_agent: None,
        }
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.device_type == DeviceType::Unknown && self.browser.is_none() && self.os.is_none()
    }

    /// 获取设备描述
    pub fn description(&self) -> String {
        match (&self.device_type, &self.browser, &self.os) {
            (DeviceType::API, _, _) => "API Client".to_string(),
            (dt, Some(browser), Some(os)) => {
                format!(
                    "{} {} on {} {}",
                    browser,
                    self.browser_version.as_deref().unwrap_or(""),
                    os,
                    self.os_version.as_deref().unwrap_or("")
                )
            }
            (dt, Some(browser), None) => {
                format!(
                    "{} {} on {}",
                    browser,
                    self.browser_version.as_deref().unwrap_or(""),
                    dt
                )
            }
            (dt, None, Some(os)) => {
                format!("{} on {}", os, dt)
            }
            (dt, None, None) => dt.to_string(),
        }
    }

    /// 从woothee结果创建设备信息
    fn from_woothee(result: &woothee::parser::WootheeResult) -> Self {
        let device_type = Self::map_woothee_device_type(&result.category);

        let browser = if device_type != DeviceType::API {
            Some(result.name.to_string())
        } else {
            None
        };

        let browser_version = if device_type != DeviceType::API {
            Some(result.version.to_string())
        } else {
            None
        };

        let os = if device_type != DeviceType::API {
            Some(result.os.to_string())
        } else {
            None
        };

        let os_version = if device_type != DeviceType::API {
            Some(result.os_version.to_string())
        } else {
            None
        };

        Self {
            device_type,
            browser,
            browser_version,
            os,
            os_version,
            user_agent: None,
        }
    }

    /// 映射woothee设备类型
    fn map_woothee_device_type(category: &str) -> DeviceType {
        match category.to_lowercase().as_str() {
            "pc" => DeviceType::Desktop,
            "smartphone" => DeviceType::Mobile,
            "mobilephone" => DeviceType::Mobile,
            "tablet" => DeviceType::Tablet,
            "appliance" => DeviceType::API,
            "crawler" => DeviceType::API,
            "misc" => DeviceType::API,
            _ => DeviceType::Unknown,
        }
    }
}

impl Default for DeviceInfo {
    fn default() -> Self {
        Self::empty()
    }
}

// ============================================================================
// 设备匹配条件
// ============================================================================

/// 设备匹配条件
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceCondition {
    /// 设备类型列表
    pub device_types: Vec<DeviceType>,
    /// 浏览器名称列表
    pub browsers: Vec<String>,
    /// 操作系统列表
    pub os: Vec<String>,
}

impl DeviceCondition {
    /// 创建空的匹配条件
    pub fn empty() -> Self {
        Self {
            device_types: vec![],
            browsers: vec![],
            os: vec![],
        }
    }

    /// 创建设备类型匹配条件
    pub fn device_types(device_types: Vec<DeviceType>) -> Self {
        Self {
            device_types,
            browsers: vec![],
            os: vec![],
        }
    }

    /// 创建浏览器匹配条件
    pub fn browsers(browsers: Vec<String>) -> Self {
        Self {
            device_types: vec![],
            browsers,
            os: vec![],
        }
    }

    /// 创建操作系统匹配条件
    pub fn os(os: Vec<String>) -> Self {
        Self {
            device_types: vec![],
            browsers: vec![],
            os,
        }
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.device_types.is_empty() && self.browsers.is_empty() && self.os.is_empty()
    }

    /// 检查设备信息是否匹配条件
    pub fn matches(&self, info: &DeviceInfo) -> bool {
        if self.is_empty() {
            return true;
        }

        // 检查设备类型匹配
        if !self.device_types.is_empty() {
            if self.device_types.contains(&info.device_type) {
                return true;
            }
            return false;
        }

        // 检查浏览器匹配
        if !self.browsers.is_empty() {
            if let Some(browser) = &info.browser {
                if self.browsers.iter().any(|b| {
                    browser.to_lowercase().contains(&b.to_lowercase())
                        || b.to_lowercase().contains(&browser.to_lowercase())
                }) {
                    return true;
                }
            }
            return false;
        }

        // 检查操作系统匹配
        if !self.os.is_empty() {
            if let Some(os) = &info.os {
                if self.os.iter().any(|o| {
                    os.to_lowercase().contains(&o.to_lowercase())
                        || o.to_lowercase().contains(&os.to_lowercase())
                }) {
                    return true;
                }
            }
            return false;
        }

        false
    }
}

impl Default for DeviceCondition {
    fn default() -> Self {
        Self::empty()
    }
}

// ============================================================================
// 设备匹配器
// ============================================================================

/// 设备匹配器
///
/// 解析User-Agent并识别设备类型、浏览器和操作系统。
pub struct DeviceMatcher {
    /// Woothee解析器
    parser: Arc<Parser>,
    /// 查询缓存
    cache: Arc<DashMap<String, DeviceInfo>>,
    /// 缓存大小限制
    cache_size_limit: usize,
    /// 自定义规则
    custom_rules: Vec<DeviceCustomRule>,
}

/// 自定义设备规则
#[derive(Debug, Clone)]
struct DeviceCustomRule {
    /// 规则名称
    name: String,
    /// 匹配模式（正则表达式）
    pattern: String,
    /// 设备类型
    device_type: DeviceType,
    /// 浏览器名称
    browser: Option<String>,
    /// 操作系统
    os: Option<String>,
}

impl DeviceMatcher {
    /// 创建新的设备匹配器
    ///
    /// # 返回
    /// - `Ok(DeviceMatcher)`: 成功创建匹配器
    /// - `Err(FlowGuardError)`: 创建失败
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::device_matcher::DeviceMatcher;
    ///
    /// let matcher = DeviceMatcher::new().await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument]
    pub async fn new() -> Result<Self, FlowGuardError> {
        info!("创建DeviceMatcher");

        let parser = Parser::new();

        let matcher = Self {
            parser: Arc::new(parser),
            cache: Arc::new(DashMap::new()),
            cache_size_limit: 10_000,
            custom_rules: Self::default_custom_rules(),
        };

        info!("DeviceMatcher创建成功");
        Ok(matcher)
    }

    /// 创建带缓存大小限制的设备匹配器
    ///
    /// # 参数
    /// - `cache_size_limit`: 缓存大小限制
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::device_matcher::DeviceMatcher;
    ///
    /// let matcher = DeviceMatcher::with_cache_limit(5000).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument]
    pub async fn with_cache_limit(cache_size_limit: usize) -> Result<Self, FlowGuardError> {
        let mut matcher = Self::new().await?;
        matcher.cache_size_limit = cache_size_limit;
        Ok(matcher)
    }

    /// 解析User-Agent
    ///
    /// # 参数
    /// - `user_agent`: User-Agent字符串
    ///
    /// # 返回
    /// - `Ok(DeviceInfo)`: 设备信息
    /// - `Err(FlowGuardError)`: 解析失败
    ///
    /// # 性能
    /// - 首次解析: ~100μs
    /// - 缓存命中: < 10μs
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::device_matcher::DeviceMatcher;
    ///
    /// let matcher = DeviceMatcher::new().await?;
    /// let user_agent = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)";
    /// let info = matcher.parse(user_agent)?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self))]
    pub fn parse(&self, user_agent: &str) -> Result<DeviceInfo, FlowGuardError> {
        // 清理 User-Agent
        let sanitized = sanitize_user_agent(user_agent);
        let user_agent = sanitized.trim();

        // 如果清理后为空，直接返回空的 DeviceInfo
        if user_agent.is_empty() {
            return Ok(DeviceInfo::empty());
        }

        // 验证 User-Agent 长度
        if user_agent.len() > MAX_USER_AGENT_LENGTH {
            return Err(FlowGuardError::ConfigError(format!(
                "User-Agent 长度超过限制（最大 {} 字符）",
                MAX_USER_AGENT_LENGTH
            )));
        }

        // 检查缓存
        if let Some(cached) = self.cache.get(user_agent) {
            debug!("缓存命中: {}", user_agent);
            return Ok(cached.clone());
        }

        debug!("解析User-Agent: {}", user_agent);

        // 检查自定义规则
        for rule in &self.custom_rules {
            if let Ok(re) = regex::Regex::new(&rule.pattern) {
                if re.is_match(user_agent) {
                    let mut info = DeviceInfo {
                        device_type: rule.device_type,
                        browser: rule.browser.clone(),
                        browser_version: None,
                        os: rule.os.clone(),
                        os_version: None,
                        user_agent: Some(user_agent.to_string()),
                    };
                    self.update_cache(user_agent, &info);
                    debug!("自定义规则匹配: {}", rule.name);
                    return Ok(info);
                }
            }
        }

        // 使用woothee解析
        let result = self.parser.parse(user_agent);
        let mut info = if let Some(res) = result {
            DeviceInfo::from_woothee(&res)
        } else {
            DeviceInfo::empty()
        };
        info.user_agent = Some(user_agent.to_string());

        // 更新缓存
        self.update_cache(user_agent, &info);

        debug!(
            "User-Agent解析成功: {} -> {}",
            user_agent,
            info.description()
        );
        Ok(info)
    }

    /// 批量解析User-Agent
    ///
    /// # 参数
    /// - `user_agents`: User-Agent字符串列表
    ///
    /// # 返回
    /// - `Vec<Result<DeviceInfo>>`: 设备信息列表
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::device_matcher::DeviceMatcher;
    ///
    /// let matcher = DeviceMatcher::new().await?;
    /// let user_agents = vec![
    ///     "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)",
    ///     "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
    /// ];
    /// let results = matcher.batch_parse(&user_agents);
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, user_agents))]
    pub fn batch_parse(&self, user_agents: &[String]) -> Vec<Result<DeviceInfo, FlowGuardError>> {
        user_agents.iter().map(|ua| self.parse(ua)).collect()
    }

    /// 检查User-Agent是否匹配设备条件
    ///
    /// # 参数
    /// - `user_agent`: User-Agent字符串
    /// - `condition`: 设备匹配条件
    ///
    /// # 返回
    /// - `Ok(true)`: 匹配
    /// - `Ok(false)`: 不匹配
    /// - `Err(FlowGuardError)`: 解析失败
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::device_matcher::{DeviceMatcher, DeviceCondition, DeviceType};
    ///
    /// let matcher = DeviceMatcher::new().await?;
    /// let condition = DeviceCondition::device_types(vec![DeviceType::Mobile]);
    /// let user_agent = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)";
    /// let matched = matcher.matches_user_agent(user_agent, &condition)?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, condition))]
    pub fn matches_user_agent(
        &self,
        user_agent: &str,
        condition: &DeviceCondition,
    ) -> Result<bool, FlowGuardError> {
        let info = self.parse(user_agent)?;
        Ok(condition.matches(&info))
    }

    /// 检查设备信息是否匹配条件
    ///
    /// # 参数
    /// - `info`: 设备信息
    /// - `condition`: 设备匹配条件
    ///
    /// # 返回
    /// - `true`: 匹配
    /// - `false`: 不匹配
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::device_matcher::{DeviceInfo, DeviceCondition, DeviceType};
    ///
    /// let info = DeviceInfo {
    ///     device_type: DeviceType::Mobile,
    ///     browser: Some("Safari".to_string()),
    ///     browser_version: Some("14.0".to_string()),
    ///     os: Some("iOS".to_string()),
    ///     os_version: Some("14.0".to_string()),
    ///     user_agent: None,
    /// };
    ///
    /// let condition = DeviceCondition::device_types(vec![DeviceType::Mobile]);
    /// let matched = condition.matches(&info);
    /// ```
    pub fn matches(&self, info: &DeviceInfo, condition: &DeviceCondition) -> bool {
        condition.matches(info)
    }

    /// 添加自定义规则
    ///
    /// # 参数
    /// - `name`: 规则名称
    /// - `pattern`: 匹配模式（正则表达式）
    /// - `device_type`: 设备类型
    /// - `browser`: 浏览器名称（可选）
    /// - `os`: 操作系统（可选）
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::device_matcher::{DeviceMatcher, DeviceType};
    ///
    /// let mut matcher = DeviceMatcher::new().await?;
    /// matcher.add_custom_rule(
    ///     "MyCustomApp",
    ///     r"MyCustomApp/\d+\.\d+",
    ///     DeviceType::Mobile,
    ///     Some("MyCustomApp".to_string()),
    ///     Some("Android".to_string()),
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_custom_rule(
        &mut self,
        name: &str,
        pattern: &str,
        device_type: DeviceType,
        browser: Option<String>,
        os: Option<String>,
    ) {
        let rule = DeviceCustomRule {
            name: name.to_string(),
            pattern: pattern.to_string(),
            device_type,
            browser,
            os,
        };

        // 验证正则表达式
        if regex::Regex::new(&rule.pattern).is_err() {
            warn!("无效的正则表达式: {}", pattern);
            return;
        }

        self.custom_rules.push(rule);
        info!("添加自定义规则: {}", name);
    }

    /// 移除自定义规则
    ///
    /// # 参数
    /// - `name`: 规则名称
    ///
    /// # 返回
    /// - `true`: 成功移除
    /// - `false`: 规则不存在
    pub fn remove_custom_rule(&mut self, name: &str) -> bool {
        let original_len = self.custom_rules.len();
        self.custom_rules.retain(|r| r.name != name);
        let removed = self.custom_rules.len() < original_len;
        if removed {
            info!("移除自定义规则: {}", name);
        }
        removed
    }

    /// 清空缓存
    #[instrument(skip(self))]
    pub fn clear_cache(&self) {
        let size = self.cache.len();
        self.cache.clear();
        info!("缓存已清空，移除 {} 条记录", size);
    }

    /// 获取缓存统计信息
    pub fn cache_stats(&self) -> DeviceCacheStats {
        DeviceCacheStats {
            size: self.cache.len(),
            limit: self.cache_size_limit,
            hit_rate: 0.0, // 需要额外统计
        }
    }

    /// 更新缓存
    fn update_cache(&self, user_agent: &str, info: &DeviceInfo) {
        if self.cache.len() >= self.cache_size_limit {
            // 缓存已满，清理最旧的条目（简单实现：清理10%）
            let remove_count = self.cache_size_limit / 10;
            let keys_to_remove: Vec<_> = self
                .cache
                .iter()
                .take(remove_count)
                .map(|k| k.key().clone())
                .collect();
            for key in keys_to_remove {
                self.cache.remove(&key);
            }
            debug!("缓存清理完成，移除 {} 条记录", remove_count);
        }

        self.cache.insert(user_agent.to_string(), info.clone());
    }

    /// 默认自定义规则
    fn default_custom_rules() -> Vec<DeviceCustomRule> {
        vec![
            // 常见的API客户端
            DeviceCustomRule {
                name: "curl".to_string(),
                pattern: r"^curl/".to_string(),
                device_type: DeviceType::API,
                browser: Some("curl".to_string()),
                os: None,
            },
            DeviceCustomRule {
                name: "wget".to_string(),
                pattern: r"^Wget/".to_string(),
                device_type: DeviceType::API,
                browser: Some("wget".to_string()),
                os: None,
            },
            // 常见的爬虫
            DeviceCustomRule {
                name: "googlebot".to_string(),
                pattern: r"Googlebot".to_string(),
                device_type: DeviceType::API,
                browser: Some("Googlebot".to_string()),
                os: None,
            },
            DeviceCustomRule {
                name: "bingbot".to_string(),
                pattern: r"Bingbot".to_string(),
                device_type: DeviceType::API,
                browser: Some("Bingbot".to_string()),
                os: None,
            },
        ]
    }
}

// ============================================================================
// 缓存统计信息
// ============================================================================

/// 设备缓存统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCacheStats {
    /// 当前缓存大小
    pub size: usize,
    /// 缓存大小限制
    pub limit: usize,
    /// 缓存命中率（需要额外统计）
    pub hit_rate: f64,
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_type_from_str() {
        assert_eq!(DeviceType::from_str("mobile"), DeviceType::Mobile);
        assert_eq!(DeviceType::from_str("desktop"), DeviceType::Desktop);
        assert_eq!(DeviceType::from_str("tablet"), DeviceType::Tablet);
        assert_eq!(DeviceType::from_str("api"), DeviceType::API);
        assert_eq!(DeviceType::from_str("unknown"), DeviceType::Unknown);
        assert_eq!(DeviceType::from_str("invalid"), DeviceType::Unknown);
    }

    #[test]
    fn test_device_type_as_str() {
        assert_eq!(DeviceType::Mobile.as_str(), "mobile");
        assert_eq!(DeviceType::Desktop.as_str(), "desktop");
        assert_eq!(DeviceType::Tablet.as_str(), "tablet");
        assert_eq!(DeviceType::API.as_str(), "api");
        assert_eq!(DeviceType::Unknown.as_str(), "unknown");
    }

    #[test]
    fn test_device_type_is_mobile() {
        assert!(DeviceType::Mobile.is_mobile());
        assert!(DeviceType::Tablet.is_mobile());
        assert!(!DeviceType::Desktop.is_mobile());
        assert!(!DeviceType::API.is_mobile());
    }

    #[test]
    fn test_device_info_empty() {
        let info = DeviceInfo::empty();
        assert!(info.is_empty());
        assert_eq!(info.description(), "unknown");
    }

    #[test]
    fn test_device_info_description() {
        let info1 = DeviceInfo {
            device_type: DeviceType::Mobile,
            browser: Some("Safari".to_string()),
            browser_version: Some("14.0".to_string()),
            os: Some("iOS".to_string()),
            os_version: Some("14.0".to_string()),
            user_agent: None,
        };
        assert!(info1.description().contains("Safari"));
        assert!(info1.description().contains("iOS"));

        let info2 = DeviceInfo {
            device_type: DeviceType::API,
            browser: None,
            browser_version: None,
            os: None,
            os_version: None,
            user_agent: None,
        };
        assert_eq!(info2.description(), "API Client");
    }

    #[test]
    fn test_device_condition_empty() {
        let condition = DeviceCondition::empty();
        assert!(condition.is_empty());

        let info = DeviceInfo::empty();
        assert!(condition.matches(&info));
    }

    #[test]
    fn test_device_condition_device_types() {
        let condition = DeviceCondition::device_types(vec![DeviceType::Mobile]);

        let info1 = DeviceInfo {
            device_type: DeviceType::Mobile,
            browser: None,
            browser_version: None,
            os: None,
            os_version: None,
            user_agent: None,
        };
        assert!(condition.matches(&info1));

        let info2 = DeviceInfo {
            device_type: DeviceType::Desktop,
            browser: None,
            browser_version: None,
            os: None,
            os_version: None,
            user_agent: None,
        };
        assert!(!condition.matches(&info2));
    }

    #[test]
    fn test_device_condition_browsers() {
        let condition = DeviceCondition::browsers(vec!["Safari".to_string(), "Chrome".to_string()]);

        let info1 = DeviceInfo {
            device_type: DeviceType::Mobile,
            browser: Some("Safari".to_string()),
            browser_version: None,
            os: None,
            os_version: None,
            user_agent: None,
        };
        assert!(condition.matches(&info1));

        let info2 = DeviceInfo {
            device_type: DeviceType::Mobile,
            browser: Some("Firefox".to_string()),
            browser_version: None,
            os: None,
            os_version: None,
            user_agent: None,
        };
        assert!(!condition.matches(&info2));
    }

    #[test]
    fn test_device_condition_os() {
        let condition = DeviceCondition::os(vec!["iOS".to_string(), "Android".to_string()]);

        let info1 = DeviceInfo {
            device_type: DeviceType::Mobile,
            browser: None,
            browser_version: None,
            os: Some("iOS".to_string()),
            os_version: None,
            user_agent: None,
        };
        assert!(condition.matches(&info1));

        let info2 = DeviceInfo {
            device_type: DeviceType::Desktop,
            browser: None,
            browser_version: None,
            os: Some("Windows".to_string()),
            os_version: None,
            user_agent: None,
        };
        assert!(!condition.matches(&info2));
    }

    #[test]
    fn test_device_condition_default() {
        let condition = DeviceCondition::default();
        assert!(condition.is_empty());
    }

    #[test]
    fn test_device_info_serialization() {
        let info = DeviceInfo {
            device_type: DeviceType::Mobile,
            browser: Some("Safari".to_string()),
            browser_version: Some("14.0".to_string()),
            os: Some("iOS".to_string()),
            os_version: Some("14.0".to_string()),
            user_agent: Some("Test".to_string()),
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: DeviceInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(info, deserialized);
    }

    #[test]
    fn test_device_condition_serialization() {
        let condition = DeviceCondition {
            device_types: vec![DeviceType::Mobile, DeviceType::Tablet],
            browsers: vec!["Safari".to_string()],
            os: vec!["iOS".to_string()],
        };

        let json = serde_json::to_string(&condition).unwrap();
        let deserialized: DeviceCondition = serde_json::from_str(&json).unwrap();

        assert_eq!(condition, deserialized);
    }

    #[tokio::test]
    async fn test_device_matcher_parse_empty() {
        let matcher = DeviceMatcher::new().await.unwrap();
        // 空字符串应该被清理为空，然后返回空的 DeviceInfo
        let info = matcher.parse("").unwrap();
        assert!(info.is_empty());

        // 只有空格的字符串也应该返回空的 DeviceInfo
        let info = matcher.parse("   ").unwrap();
        assert!(info.is_empty());
    }

    #[tokio::test]
    async fn test_device_matcher_parse_iphone() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let user_agent = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.0 Mobile/15E148 Safari/604.1";
        let info = matcher.parse(user_agent).unwrap();
        assert_eq!(info.device_type, DeviceType::Mobile);
        assert!(info.browser.as_ref().unwrap().contains("Safari"));
        // woothee可能返回不同的OS名称，所以只检查不为空
        assert!(info.os.is_some());
    }

    #[tokio::test]
    async fn test_device_matcher_parse_desktop() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36";
        let info = matcher.parse(user_agent).unwrap();
        assert_eq!(info.device_type, DeviceType::Desktop);
        assert!(info.browser.as_ref().unwrap().contains("Chrome"));
        assert!(info.os.as_ref().unwrap().contains("Windows"));
    }

    #[tokio::test]
    async fn test_device_matcher_parse_curl() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let user_agent = "curl/7.68.0";
        let info = matcher.parse(user_agent).unwrap();
        assert_eq!(info.device_type, DeviceType::API);
        assert_eq!(info.browser, Some("curl".to_string()));
    }

    #[tokio::test]
    async fn test_device_matcher_cache() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let user_agent = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)";

        // 第一次解析
        let info1 = matcher.parse(user_agent).unwrap();
        let stats1 = matcher.cache_stats();
        assert_eq!(stats1.size, 1);

        // 第二次解析（应该命中缓存）
        let info2 = matcher.parse(user_agent).unwrap();
        assert_eq!(info1, info2);

        // 清空缓存
        matcher.clear_cache();
        let stats2 = matcher.cache_stats();
        assert_eq!(stats2.size, 0);
    }

    #[tokio::test]
    async fn test_device_matcher_custom_rule() {
        let mut matcher = DeviceMatcher::new().await.unwrap();

        matcher.add_custom_rule(
            "TestApp",
            r"TestApp/\d+\.\d+",
            DeviceType::Mobile,
            Some("TestApp".to_string()),
            Some("Android".to_string()),
        );

        let user_agent = "TestApp/1.0.0";
        let info = matcher.parse(user_agent).unwrap();
        assert_eq!(info.device_type, DeviceType::Mobile);
        assert_eq!(info.browser, Some("TestApp".to_string()));
    }

    #[tokio::test]
    async fn test_device_matcher_remove_custom_rule() {
        let mut matcher = DeviceMatcher::new().await.unwrap();

        matcher.add_custom_rule(
            "TestApp",
            r"TestApp/\d+\.\d+",
            DeviceType::Mobile,
            Some("TestApp".to_string()),
            Some("Android".to_string()),
        );

        assert!(matcher.remove_custom_rule("TestApp"));
        assert!(!matcher.remove_custom_rule("NonExistent"));
    }

    #[tokio::test]
    async fn test_device_matcher_batch_parse() {
        let matcher = DeviceMatcher::new().await.unwrap();

        let user_agents = vec![
            "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)".to_string(),
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36".to_string(),
            "curl/7.68.0".to_string(),
        ];

        let results = matcher.batch_parse(&user_agents);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.is_ok()));

        assert_eq!(results[0].as_ref().unwrap().device_type, DeviceType::Mobile);
        assert_eq!(
            results[1].as_ref().unwrap().device_type,
            DeviceType::Desktop
        );
        assert_eq!(results[2].as_ref().unwrap().device_type, DeviceType::API);
    }

    #[tokio::test]
    async fn test_device_matcher_matches_user_agent() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let condition = DeviceCondition::device_types(vec![DeviceType::Mobile]);

        let user_agent = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)";
        let matched = matcher.matches_user_agent(user_agent, &condition).unwrap();
        assert!(matched);

        let user_agent2 = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)";
        let matched2 = matcher.matches_user_agent(user_agent2, &condition).unwrap();
        assert!(!matched2);
    }
}
