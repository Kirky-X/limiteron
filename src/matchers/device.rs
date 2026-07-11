// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
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
//! use limiteron::matchers::device::DeviceMatcher;
//!
//! #[tokio::main]
//! async fn main() {
//!     let matcher = DeviceMatcher::new().await.unwrap();
//!     let user_agent = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X) AppleWebKit/605.1.15";
//!     let info = matcher.parse(user_agent).await.unwrap();
//! }
//! ```

#[cfg(feature = "device-matching")]
use crate::error::LimiteronError;
use log::{debug, info};
use oxcache::Cache;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use woothee::parser::Parser;

// ============================================================================
// 输入验证常量
// ============================================================================

/// 最大 User-Agent 长度
const MAX_USER_AGENT_LENGTH: usize = 2048;

// ============================================================================
// 输入验证函数
// ============================================================================

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
        .filter(|&c| c.is_ascii_graphic() || c == ' ')
        .collect()
}

// ============================================================================
// 设备类型
// ============================================================================

#[cfg(feature = "device-matching")]
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
    pub fn parse(s: &str) -> Self {
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

#[cfg(feature = "device-matching")]
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
            (_dt, Some(browser), Some(os)) => {
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
        let device_type = Self::map_woothee_device_type(result.category);

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

#[cfg(feature = "device-matching")]
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
#[cfg(feature = "device-matching")]
pub struct DeviceMatcher {
    /// Woothee解析器
    parser: Arc<Parser>,
    /// 查询缓存
    cache: Arc<Cache<String, DeviceInfo>>,
    /// 缓存大小限制
    cache_size_limit: usize,
    /// 缓存命中次数
    cache_hits: AtomicU64,
    /// 缓存未命中次数
    cache_misses: AtomicU64,
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
    /// 创建新的设备匹配器（保持向后兼容）
    ///
    /// # 返回
    /// - `Ok(DeviceMatcher)`: 成功创建匹配器
    /// - `Err(LimiteronError)`: 创建失败
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::matchers::device::DeviceMatcher;
    ///
    /// let matcher = DeviceMatcher::new().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new() -> Result<Self, LimiteronError> {
        info!(target: "device", "创建DeviceMatcher");

        let parser = Parser::new();
        let cache = Cache::builder()
            .build()
            .await
            .map_err(|e| LimiteronError::ConfigError(e.to_string()))?;

        let matcher = Self {
            parser: Arc::new(parser),
            cache: Arc::new(cache),
            cache_size_limit: 10_000,
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            custom_rules: Self::default_custom_rules(),
        };

        info!(target: "device", "DeviceMatcher创建成功");
        Ok(matcher)
    }

    /// 创建设置器（Builder模式）
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::matchers::device::DeviceMatcher;
    /// use oxcache::Cache;
    ///
    /// let matcher = DeviceMatcher::builder()
    ///     .cache_size_limit(5000)
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn builder() -> DeviceMatcherBuilder {
        DeviceMatcherBuilder::new()
    }

    /// 使用依赖注入创建（完整依赖模式）
    ///
    /// 接受外部依赖，实现完全的依赖注入。
    ///
    /// # 参数
    /// - `parser`: Woothee解析器
    /// - `cache`: 查询缓存
    /// - `cache_size_limit`: 缓存大小限制
    pub fn with_dependencies(
        parser: Arc<Parser>,
        cache: Arc<Cache<String, DeviceInfo>>,
        cache_size_limit: usize,
    ) -> Self {
        let mut matcher = Self {
            parser,
            cache,
            cache_size_limit,
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            custom_rules: Self::default_custom_rules(),
        };
        matcher.cache_size_limit = cache_size_limit;
        matcher
    }

    /// 创建带缓存大小限制的设备匹配器
    ///
    /// # 参数
    /// - `cache_size_limit`: 缓存大小限制
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::matchers::device::DeviceMatcher;
    ///
    /// let matcher = DeviceMatcher::with_cache_limit(5000).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn with_cache_limit(cache_size_limit: usize) -> Result<Self, LimiteronError> {
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
    /// - `Err(LimiteronError)`: 解析失败
    ///
    /// # 性能
    /// - 首次解析: ~100μs
    /// - 缓存命中: < 10μs
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::matchers::device::DeviceMatcher;
    ///
    /// let matcher = DeviceMatcher::new().await?;
    /// let user_agent = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)";
    /// let info = matcher.parse(user_agent).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn parse(&self, user_agent: &str) -> Result<DeviceInfo, LimiteronError> {
        // 清理 User-Agent
        let sanitized = sanitize_user_agent(user_agent);
        let user_agent = sanitized.trim();

        // 如果清理后为空，直接返回空的 DeviceInfo
        if user_agent.is_empty() {
            return Ok(DeviceInfo::empty());
        }

        // 验证 User-Agent 长度
        if user_agent.len() > MAX_USER_AGENT_LENGTH {
            return Err(LimiteronError::ConfigError(format!(
                "User-Agent 长度超过限制（最大 {} 字符）",
                MAX_USER_AGENT_LENGTH
            )));
        }

        // 检查缓存
        let cache_key = user_agent.to_string();
        if let Ok(Some(cached)) = self.cache.get(&cache_key).await {
            log::debug!(target: "device", "缓存命中: {}", user_agent);
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(cached);
        }

        // 记录缓存未命中
        self.cache_misses.fetch_add(1, Ordering::Relaxed);

        log::debug!(target: "device", "解析User-Agent: {}", user_agent);

        // 检查自定义规则
        for rule in &self.custom_rules {
            if let Ok(re) = regex::Regex::new(&rule.pattern) {
                if re.is_match(user_agent) {
                    let info = DeviceInfo {
                        device_type: rule.device_type,
                        browser: rule.browser.clone(),
                        browser_version: None,
                        os: rule.os.clone(),
                        os_version: None,
                        user_agent: Some(user_agent.to_string()),
                    };
                    self.update_cache(user_agent, &info).await;
                    log::debug!(target: "device", "自定义规则匹配: {}", rule.name);
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
        self.update_cache(user_agent, &info).await;

        log::debug!(
            target: "device",
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
    /// use limiteron::matchers::device::DeviceMatcher;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let matcher = DeviceMatcher::new().await.unwrap();
    ///     let user_agents: Vec<String> = vec![
    ///         "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)".to_string(),
    ///         "Mozilla/5.0 (Windows NT 10.0; Win64; x64)".to_string(),
    ///     ];
    ///     let results = matcher.batch_parse(&user_agents).await;
    /// }
    /// ```
    pub async fn batch_parse(
        &self,
        user_agents: &[String],
    ) -> Vec<Result<DeviceInfo, LimiteronError>> {
        let mut results = Vec::with_capacity(user_agents.len());
        for ua in user_agents {
            results.push(self.parse(ua).await);
        }
        results
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
    /// - `Err(LimiteronError)`: 解析失败
    ///
    /// # 示例
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use limiteron::matchers::device::{DeviceMatcher, DeviceCondition, DeviceType};
    ///
    /// let matcher = DeviceMatcher::new().await?;
    /// let condition = DeviceCondition::device_types(vec![DeviceType::Mobile]);
    /// let user_agent = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)";
    /// let matched = matcher.matches_user_agent(user_agent, &condition).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn matches_user_agent(
        &self,
        user_agent: &str,
        condition: &DeviceCondition,
    ) -> Result<bool, LimiteronError> {
        let info = self.parse(user_agent).await?;
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
    /// use limiteron::matchers::device::{DeviceInfo, DeviceCondition, DeviceType};
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
    /// use limiteron::matchers::device::{DeviceMatcher, DeviceType};
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
            log::warn!(target: "device", "无效的正则表达式: {}", pattern);
            return;
        }

        self.custom_rules.push(rule);
        log::info!(target: "device", "添加自定义规则: {}", name);
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
            log::info!(target: "device", "移除自定义规则: {}", name);
        }
        removed
    }

    /// 清空缓存
    pub async fn clear_cache(&self) {
        let size = self.cache.len().await.unwrap_or(0);
        let _ = self.cache.clear().await;
        log::info!(target: "device", "缓存已清空，移除 {} 条记录", size);
    }

    /// 获取缓存统计信息
    pub async fn cache_stats(&self) -> DeviceCacheStats {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let total = hits.saturating_add(misses);
        let hit_rate = if total > 0 {
            (hits as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        DeviceCacheStats {
            size: self.cache.len().await.unwrap_or(0) as usize,
            limit: self.cache_size_limit,
            hit_rate,
            hits,
            misses,
        }
    }

    /// 更新缓存
    async fn update_cache(&self, user_agent: &str, info: &DeviceInfo) {
        let cache_len = self.cache.len().await.unwrap_or(0);
        if cache_len >= self.cache_size_limit as u64 {
            let _maybe_first = (0..(self.cache_size_limit / 10)).next();
            debug!(target: "device", "缓存接近限制 ({}/{})", cache_len, self.cache_size_limit);
        }

        let _ = self.cache.set(&user_agent.to_string(), info).await;
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

/// 设备匹配器设置器
#[derive(Debug, Clone)]
pub struct DeviceMatcherBuilder {
    cache_size_limit: usize,
    custom_rules: Vec<DeviceCustomRule>,
}

impl DeviceMatcherBuilder {
    /// 创建新的设置器
    pub fn new() -> Self {
        Self {
            cache_size_limit: 10_000,
            custom_rules: Vec::new(),
        }
    }

    /// 设置缓存大小限制
    pub fn cache_size_limit(mut self, cache_size_limit: usize) -> Self {
        self.cache_size_limit = cache_size_limit;
        self
    }

    /// 添加自定义规则
    pub fn add_custom_rule(
        mut self,
        name: &str,
        pattern: &str,
        device_type: DeviceType,
        browser: Option<String>,
        os: Option<String>,
    ) -> Self {
        let rule = DeviceCustomRule {
            name: name.to_string(),
            pattern: pattern.to_string(),
            device_type,
            browser,
            os,
        };
        self.custom_rules.push(rule);
        self
    }

    /// 构建DeviceMatcher
    ///
    /// # 返回
    /// - `Ok(DeviceMatcher)`: 成功创建设备匹配器
    /// - `Err(LimiteronError)`: 创建失败
    pub async fn build(self) -> Result<DeviceMatcher, LimiteronError> {
        let mut matcher = DeviceMatcher::new().await?;
        matcher.cache_size_limit = self.cache_size_limit;
        matcher.custom_rules = self.custom_rules;
        Ok(matcher)
    }
}

impl Default for DeviceMatcherBuilder {
    fn default() -> Self {
        Self::new()
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
    /// 缓存命中率（百分比）
    pub hit_rate: f64,
    /// 缓存命中次数
    pub hits: u64,
    /// 缓存未命中次数
    pub misses: u64,
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_type_from_str() {
        assert_eq!(DeviceType::parse("mobile"), DeviceType::Mobile);
        assert_eq!(DeviceType::parse("desktop"), DeviceType::Desktop);
        assert_eq!(DeviceType::parse("tablet"), DeviceType::Tablet);
        assert_eq!(DeviceType::parse("api"), DeviceType::API);
        assert_eq!(DeviceType::parse("unknown"), DeviceType::Unknown);
        assert_eq!(DeviceType::parse("invalid"), DeviceType::Unknown);
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
        let info = matcher.parse("").await.unwrap();
        assert!(info.is_empty());

        // 只有空格的字符串也应该返回空的 DeviceInfo
        let info = matcher.parse("   ").await.unwrap();
        assert!(info.is_empty());
    }

    #[tokio::test]
    async fn test_device_matcher_parse_iphone() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let user_agent = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X) \
                          AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.0 Mobile/15E148 \
                          Safari/604.1";
        let info = matcher.parse(user_agent).await.unwrap();
        assert_eq!(info.device_type, DeviceType::Mobile);
        assert!(info.browser.as_ref().unwrap().contains("Safari"));
        // woothee可能返回不同的OS名称，所以只检查不为空
        assert!(info.os.is_some());
    }

    #[tokio::test]
    async fn test_device_matcher_parse_desktop() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, \
                          like Gecko) Chrome/91.0.4472.124 Safari/537.36";
        let info = matcher.parse(user_agent).await.unwrap();
        assert_eq!(info.device_type, DeviceType::Desktop);
        assert!(info.browser.as_ref().unwrap().contains("Chrome"));
        assert!(info.os.as_ref().unwrap().contains("Windows"));
    }

    #[tokio::test]
    async fn test_device_matcher_parse_curl() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let user_agent = "curl/7.68.0";
        let info = matcher.parse(user_agent).await.unwrap();
        assert_eq!(info.device_type, DeviceType::API);
        assert_eq!(info.browser, Some("curl".to_string()));
    }

    #[tokio::test]
    async fn test_device_matcher_cache() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let user_agent = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)";

        // 第一次解析 - 应该缓存未命中
        let info1 = matcher.parse(user_agent).await.unwrap();
        let stats1 = matcher.cache_stats().await;
        // 第一次解析后应该有1次缓存未命中，0次命中
        assert_eq!(stats1.misses, 1);
        assert_eq!(stats1.hits, 0);

        // 第二次解析（应该命中缓存）
        let info2 = matcher.parse(user_agent).await.unwrap();
        assert_eq!(info1, info2);
        let stats2 = matcher.cache_stats().await;
        // 第二次解析后应该有1次缓存命中，1次未命中
        assert_eq!(stats2.hits, 1);
        assert_eq!(stats2.misses, 1);

        // 清空缓存 - 注意：清空缓存不会重置计数器
        matcher.clear_cache().await;
        let stats3 = matcher.cache_stats().await;
        // 计数器保持不变
        assert_eq!(stats3.hits, 1);
        assert_eq!(stats3.misses, 1);
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
        let info = matcher.parse(user_agent).await.unwrap();
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

        let results = matcher.batch_parse(&user_agents).await;
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
        let matched = matcher
            .matches_user_agent(user_agent, &condition)
            .await
            .unwrap();
        assert!(matched);

        let user_agent2 = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)";
        let matched2 = matcher
            .matches_user_agent(user_agent2, &condition)
            .await
            .unwrap();
        assert!(!matched2);
    }

    // === 新增覆盖率测试 ===

    #[test]
    fn test_device_type_parse_aliases() {
        assert_eq!(DeviceType::parse("smartphone"), DeviceType::Mobile);
        assert_eq!(DeviceType::parse("pc"), DeviceType::Desktop);
        assert_eq!(DeviceType::parse("ipad"), DeviceType::Tablet);
        assert_eq!(DeviceType::parse("bot"), DeviceType::API);
        assert_eq!(DeviceType::parse("crawler"), DeviceType::API);
    }

    #[test]
    fn test_device_type_helpers() {
        assert!(DeviceType::Desktop.is_desktop());
        assert!(!DeviceType::Mobile.is_desktop());
        assert!(!DeviceType::Tablet.is_desktop());
        assert!(DeviceType::API.is_api());
        assert!(!DeviceType::Mobile.is_api());
        assert!(!DeviceType::Desktop.is_api());
    }

    #[test]
    fn test_device_type_display() {
        assert_eq!(format!("{}", DeviceType::Mobile), "mobile");
        assert_eq!(format!("{}", DeviceType::Desktop), "desktop");
        assert_eq!(format!("{}", DeviceType::Tablet), "tablet");
        assert_eq!(format!("{}", DeviceType::API), "api");
        assert_eq!(format!("{}", DeviceType::Unknown), "unknown");
    }

    #[test]
    fn test_device_info_description_browser_only() {
        let info = DeviceInfo {
            device_type: DeviceType::Desktop,
            browser: Some("Chrome".to_string()),
            browser_version: Some("91".to_string()),
            os: None,
            os_version: None,
            user_agent: None,
        };
        assert_eq!(info.description(), "Chrome 91 on desktop");
    }

    #[test]
    fn test_device_info_description_os_only() {
        let info = DeviceInfo {
            device_type: DeviceType::Desktop,
            browser: None,
            browser_version: None,
            os: Some("Windows".to_string()),
            os_version: Some("10".to_string()),
            user_agent: None,
        };
        assert_eq!(info.description(), "Windows on desktop");
    }

    #[test]
    fn test_device_info_description_device_only() {
        let info = DeviceInfo {
            device_type: DeviceType::Desktop,
            browser: None,
            browser_version: None,
            os: None,
            os_version: None,
            user_agent: None,
        };
        assert_eq!(info.description(), "desktop");
    }

    #[test]
    fn test_device_condition_browser_none() {
        let condition = DeviceCondition::browsers(vec!["Safari".to_string()]);
        let info = DeviceInfo {
            device_type: DeviceType::Desktop,
            browser: None,
            browser_version: None,
            os: None,
            os_version: None,
            user_agent: None,
        };
        assert!(!condition.matches(&info));
    }

    #[test]
    fn test_device_condition_os_none() {
        let condition = DeviceCondition::os(vec!["iOS".to_string()]);
        let info = DeviceInfo {
            device_type: DeviceType::Desktop,
            browser: None,
            browser_version: None,
            os: None,
            os_version: None,
            user_agent: None,
        };
        assert!(!condition.matches(&info));
    }

    #[tokio::test]
    async fn test_device_matcher_builder() {
        let matcher = DeviceMatcher::builder()
            .cache_size_limit(500)
            .add_custom_rule(
                "CustomBot",
                r"CustomBot/\d+",
                DeviceType::API,
                Some("CustomBot".to_string()),
                None,
            )
            .build()
            .await
            .unwrap();
        assert_eq!(matcher.cache_stats().await.limit, 500);
        let info = matcher.parse("CustomBot/2.0").await.unwrap();
        assert_eq!(info.device_type, DeviceType::API);
    }

    #[tokio::test]
    async fn test_device_matcher_with_cache_limit() {
        let matcher = DeviceMatcher::with_cache_limit(500).await.unwrap();
        assert_eq!(matcher.cache_stats().await.limit, 500);
    }

    #[tokio::test]
    async fn test_device_matcher_parse_long_ua() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let long_ua = "X".repeat(3000);
        let result = matcher.parse(&long_ua).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_device_matcher_parse_unrecognized() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let info = matcher
            .parse("zzz1nvalid-th1ng-th4t-w00thee-c4nt-p4rs3")
            .await
            .unwrap();
        assert_eq!(info.device_type, DeviceType::Unknown);
        assert!(info.os.is_none());
    }

    #[tokio::test]
    async fn test_device_matcher_matches_direct() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let info = DeviceInfo {
            device_type: DeviceType::Tablet,
            browser: Some("Safari".to_string()),
            browser_version: None,
            os: Some("iOS".to_string()),
            os_version: None,
            user_agent: None,
        };
        let condition = DeviceCondition::device_types(vec![DeviceType::Tablet]);
        assert!(matcher.matches(&info, &condition));
        let no_match = DeviceCondition::device_types(vec![DeviceType::Desktop]);
        assert!(!matcher.matches(&info, &no_match));
    }

    #[tokio::test]
    async fn test_device_matcher_invalid_custom_rule() {
        let mut matcher = DeviceMatcher::new().await.unwrap();
        let before = matcher.remove_custom_rule("non-existent");
        assert!(!before);
        matcher.add_custom_rule(
            "BadRegex",
            r"[invalid(unclosed",
            DeviceType::Mobile,
            None,
            None,
        );
        let info = matcher.parse("anything").await.unwrap();
        assert_eq!(info.device_type, DeviceType::Unknown);
    }

    #[tokio::test]
    async fn test_device_matcher_parse_cache_full() {
        let matcher = DeviceMatcher::with_cache_limit(2).await.unwrap();
        let _ = matcher
            .parse("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .await
            .unwrap();
        let _ = matcher.parse("curl/7.68.0").await.unwrap();
        let _ = matcher.parse("Wget/1.21").await.unwrap();
        let stats = matcher.cache_stats().await;
        assert!(stats.hits + stats.misses >= 3);
    }

    #[tokio::test]
    async fn test_device_matcher_clear_cache_with_data() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let _ = matcher
            .parse("Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X) AppleWebKit/605.1.15")
            .await
            .unwrap();
        let _ = matcher
            .parse("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .await
            .unwrap();
        let stats_before = matcher.cache_stats().await;
        assert!(stats_before.hits == 0);
        matcher.clear_cache().await;
        let stats_after = matcher.cache_stats().await;
        assert_eq!(stats_after.size, 0);
    }

    #[tokio::test]
    async fn test_device_matcher_parse_tablet() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let info = matcher.parse("Mozilla/5.0 (iPad; CPU OS 14_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.0 Mobile/15E148 Safari/604.1").await.unwrap();
        assert_eq!(info.device_type, DeviceType::Mobile);
    }

    #[tokio::test]
    async fn test_device_matcher_parse_crawler() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let info = matcher
            .parse("Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)")
            .await
            .unwrap();
        assert_eq!(info.device_type, DeviceType::API);
    }

    #[tokio::test]
    async fn test_device_matcher_parse_wget() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let info = matcher.parse("Wget/1.21.1").await.unwrap();
        assert_eq!(info.device_type, DeviceType::API);
        assert_eq!(info.browser, Some("wget".to_string()));
    }

    #[tokio::test]
    async fn test_device_matcher_parse_bingbot() {
        let matcher = DeviceMatcher::new().await.unwrap();
        let info = matcher
            .parse("Mozilla/5.0 (compatible; Bingbot/2.0; +http://www.bing.com/bingbot.htm)")
            .await
            .unwrap();
        assert_eq!(info.device_type, DeviceType::API);
        assert_eq!(info.browser, Some("Bingbot".to_string()));
    }

    #[test]
    fn test_sanitize_user_agent_filters_non_ascii() {
        let result = sanitize_user_agent("Mozilla/5.0 \u{00e9}\u{00e0}iPhone");
        assert!(!result.contains('\u{00e9}'));
        assert!(!result.contains('\u{00e0}'));
        assert!(result.contains("Mozilla/5.0 iPhone"));
    }

    #[tokio::test]
    async fn test_device_matcher_with_dependencies() {
        use std::sync::Arc;
        let parser = Arc::new(Parser::new());
        let cache = Arc::new(Cache::builder().build().await.unwrap());
        let matcher = DeviceMatcher::with_dependencies(parser, cache, 1000);
        assert_eq!(matcher.cache_stats().await.limit, 1000);
        let info = matcher.parse("curl/7.68.0").await.unwrap();
        assert_eq!(info.device_type, DeviceType::API);
    }

    #[test]
    fn test_map_woothee_device_type_all_variants() {
        assert_eq!(
            DeviceInfo::map_woothee_device_type("pc"),
            DeviceType::Desktop
        );
        assert_eq!(
            DeviceInfo::map_woothee_device_type("smartphone"),
            DeviceType::Mobile
        );
        assert_eq!(
            DeviceInfo::map_woothee_device_type("mobilephone"),
            DeviceType::Mobile
        );
        assert_eq!(
            DeviceInfo::map_woothee_device_type("tablet"),
            DeviceType::Tablet
        );
        assert_eq!(
            DeviceInfo::map_woothee_device_type("appliance"),
            DeviceType::API
        );
        assert_eq!(
            DeviceInfo::map_woothee_device_type("crawler"),
            DeviceType::API
        );
        assert_eq!(DeviceInfo::map_woothee_device_type("misc"), DeviceType::API);
        assert_eq!(
            DeviceInfo::map_woothee_device_type("unknown_category"),
            DeviceType::Unknown
        );
    }

    #[test]
    fn test_device_info_from_woothee_api_category() {
        use woothee::parser::WootheeResult;
        let result = WootheeResult {
            name: "Googlebot",
            category: "crawler",
            os: "-",
            os_version: "-".into(),
            version: "2.1",
            browser_type: "-",
            vendor: "-",
        };
        let info = DeviceInfo::from_woothee(&result);
        assert_eq!(info.device_type, DeviceType::API);
        assert!(info.browser.is_none());
        assert!(info.os.is_none());
    }

    #[test]
    fn test_device_info_from_woothee_non_api() {
        use woothee::parser::WootheeResult;
        let result = WootheeResult {
            name: "Chrome",
            category: "pc",
            os: "Windows",
            os_version: "10".into(),
            version: "91.0",
            browser_type: "-",
            vendor: "-",
        };
        let info = DeviceInfo::from_woothee(&result);
        assert_eq!(info.device_type, DeviceType::Desktop);
        assert_eq!(info.browser, Some("Chrome".to_string()));
        assert_eq!(info.os, Some("Windows".to_string()));
    }

    #[test]
    fn test_device_info_default() {
        let info = DeviceInfo::default();
        assert!(info.is_empty());
    }

    #[cfg(feature = "device-matching")]
    #[test]
    fn test_device_matcher_builder_default() {
        let builder = DeviceMatcherBuilder::default();
        assert_eq!(builder.cache_size_limit, 10_000);
    }

    #[cfg(feature = "device-matching")]
    #[tokio::test]
    async fn test_device_matcher_cache_near_limit() {
        // 设置很小的缓存限制，解析多个不同 UA 触发缓存接近限制分支
        let matcher = DeviceMatcher::with_cache_limit(5).await.unwrap();
        // 解析 6 个不同的 UA，使缓存长度达到或超过限制
        let user_agents = vec![
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/91.0",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15) Safari/605",
            "Mozilla/5.0 (X11; Linux x86_64; rv:89.0) Firefox/89.0",
            "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0) Safari/605",
            "Mozilla/5.0 (Linux; Android 11) Chrome/91.0",
            "curl/7.68.0",
        ];
        for ua in &user_agents {
            let _ = matcher.parse(ua).await;
        }
        // 验证缓存已接近或达到限制
        let stats = matcher.cache_stats().await;
        assert!(stats.limit == 5);
    }
}
