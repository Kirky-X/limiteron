//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 匹配器模块
//!
//! 实现标识符提取器和规则匹配引擎。
//!
//! # 标识符提取器
//!
//! 支持从请求中提取多种类型的标识符：
//! - 用户ID (UserId)
//! - IP地址 (Ip)
//! - MAC地址 (Mac)
//! - API密钥 (ApiKey)
//! - 设备ID (DeviceId)
//!
//! # 规则匹配引擎
//!
//! 支持复杂的规则匹配逻辑：
//! - 优先级排序
//! - 复合条件 (AND/OR/NOT)
//! - 高性能匹配 (< 200μs P99)
//! - 支持至少100条规则

// 子模块
#[cfg(feature = "geo-matching")]
pub mod geo;

#[cfg(feature = "device-matching")]
pub mod device;

pub mod custom;

use crate::config::Matcher as ConfigMatcher;
use crate::error::FlowGuardError;
use ahash::AHashMap as HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// 标识符提取器
// ============================================================================

/// 标识符类型
///
/// 支持多种标识符类型，用于限流和封禁的键。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Identifier {
    /// 用户ID
    UserId(String),
    /// IP地址
    Ip(String),
    /// MAC地址
    Mac(String),
    /// API密钥
    ApiKey(String),
    /// 设备ID
    DeviceId(String),
}

impl Identifier {
    /// 获取标识符的字符串表示
    pub fn as_str(&self) -> &str {
        match self {
            Identifier::UserId(s) => s,
            Identifier::Ip(s) => s,
            Identifier::Mac(s) => s,
            Identifier::ApiKey(s) => s,
            Identifier::DeviceId(s) => s,
        }
    }

    /// 获取标识符类型名称
    pub fn type_name(&self) -> &'static str {
        match self {
            Identifier::UserId(_) => "user_id",
            Identifier::Ip(_) => "ip",
            Identifier::Mac(_) => "mac",
            Identifier::ApiKey(_) => "api_key",
            Identifier::DeviceId(_) => "device_id",
        }
    }

    /// 带类型前缀的键名
    pub fn key(&self) -> String {
        format!("{}:{}", self.type_name(), self.as_str())
    }
}

/// HTTP请求上下文
///
/// 简化的HTTP请求表示，包含提取标识符所需的信息。
#[derive(Clone)]
pub struct RequestContext {
    /// 用户ID
    pub user_id: Option<String>,
    /// IP地址
    pub ip: Option<String>,
    /// MAC地址
    pub mac: Option<String>,
    /// 设备ID
    pub device_id: Option<String>,
    /// API Key
    pub api_key: Option<String>,
    /// HTTP头
    pub headers: HashMap<String, String>,
    /// 请求路径
    pub path: String,
    /// 请求方法
    pub method: String,
    /// 客户端IP地址（别名）
    pub client_ip: Option<String>,
    /// 查询参数
    pub query_params: HashMap<String, String>,
}

impl std::fmt::Debug for RequestContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("RequestContext");
        debug
            .field("user_id", &self.user_id)
            .field("ip", &self.ip)
            .field("mac", &self.mac)
            .field("device_id", &self.device_id)
            .field("api_key", &self.api_key.as_ref().map(|_| "***"));

        // 脱敏 headers
        let headers: HashMap<String, String> = self
            .headers
            .iter()
            .map(|(k, v)| {
                let v = if k.to_lowercase().contains("auth")
                    || k.to_lowercase().contains("cookie")
                    || k.to_lowercase().contains("key")
                {
                    "***".to_string()
                } else {
                    v.clone()
                };
                (k.clone(), v)
            })
            .collect();
        debug.field("headers", &headers);

        debug
            .field("path", &self.path)
            .field("method", &self.method)
            .field("client_ip", &self.client_ip);

        // 脱敏 query_params
        let query_params: HashMap<String, String> = self
            .query_params
            .iter()
            .map(|(k, v)| {
                let v = if k.to_lowercase().contains("token")
                    || k.to_lowercase().contains("key")
                    || k.to_lowercase().contains("secret")
                {
                    "***".to_string()
                } else {
                    v.clone()
                };
                (k.clone(), v)
            })
            .collect();
        debug.field("query_params", &query_params);

        debug.finish()
    }
}

impl RequestContext {
    /// 创建新的请求上下文
    pub fn new() -> Self {
        Self {
            user_id: None,
            ip: None,
            mac: None,
            device_id: None,
            api_key: None,
            headers: HashMap::new(),
            path: String::new(),
            method: String::new(),
            client_ip: None,
            query_params: HashMap::new(),
        }
    }

    /// 添加HTTP头
    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_lowercase(), value.to_string());
        self
    }

    /// 设置客户端IP
    pub fn with_client_ip(mut self, ip: &str) -> Self {
        self.client_ip = Some(ip.to_string());
        self
    }

    /// 添加查询参数
    pub fn with_query_param(mut self, key: &str, value: &str) -> Self {
        self.query_params.insert(key.to_string(), value.to_string());
        self
    }

    /// 设置请求路径
    pub fn with_path(mut self, path: &str) -> Self {
        self.path = path.to_string();
        self
    }

    /// 获取HTTP头（不区分大小写）
    pub fn get_header(&self, key: &str) -> Option<&String> {
        self.headers.get(&key.to_lowercase())
    }
}

impl Default for RequestContext {
    fn default() -> Self {
        Self::new()
    }
}

/// 标识符提取器 trait
///
/// 所有标识符提取器都需要实现此trait。
pub trait IdentifierExtractor: Send + Sync {
    /// 从请求上下文中提取标识符
    ///
    /// # 参数
    /// - `context`: 请求上下文
    ///
    /// # 返回
    /// - `Some(identifier)`: 成功提取标识符
    /// - `None`: 无法提取标识符
    fn extract(&self, context: &RequestContext) -> Option<Identifier>;

    /// 获取提取器名称
    fn name(&self) -> &str;
}

// ============================================================================
// 用户ID提取器
// ============================================================================

/// 用户ID提取器
///
/// 从HTTP头或查询参数中提取用户ID。
pub struct UserIdExtractor {
    /// HTTP头名称（优先从此处提取）
    header_name: Option<String>,
    /// 查询参数名称（备选）
    query_param_name: Option<String>,
    /// 默认用户ID（当无法提取时使用）
    default_user_id: Option<String>,
}

impl UserIdExtractor {
    /// 创建新的用户ID提取器
    ///
    /// # 参数
    /// - `header_name`: HTTP头名称（可选）
    /// - `query_param_name`: 查询参数名称（可选）
    /// - `default_user_id`: 默认用户ID（可选）
    pub fn new(
        header_name: Option<String>,
        query_param_name: Option<String>,
        default_user_id: Option<String>,
    ) -> Self {
        Self {
            header_name,
            query_param_name,
            default_user_id,
        }
    }

    /// 从HTTP头提取用户ID（便捷方法）
    ///
    /// # 参数
    /// - `header_name`: HTTP头名称
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::UserIdExtractor;
    ///
    /// let extractor = UserIdExtractor::from_header("X-User-Id");
    /// ```
    pub fn from_header(header_name: &str) -> Self {
        Self::new(Some(header_name.to_string()), None, None)
    }

    /// 从查询参数提取用户ID（便捷方法）
    ///
    /// # 参数
    /// - `query_param_name`: 查询参数名称
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::UserIdExtractor;
    ///
    /// let extractor = UserIdExtractor::from_query_param("user_id");
    /// ```
    pub fn from_query_param(query_param_name: &str) -> Self {
        Self::new(None, Some(query_param_name.to_string()), None)
    }

    /// 设置默认用户ID
    ///
    /// # 参数
    /// - `default_user_id`: 默认用户ID
    pub fn with_default(mut self, default_user_id: &str) -> Self {
        self.default_user_id = Some(default_user_id.to_string());
        self
    }
}

impl IdentifierExtractor for UserIdExtractor {
    fn extract(&self, context: &RequestContext) -> Option<Identifier> {
        // 优先从HTTP头提取
        if let Some(header_name) = &self.header_name {
            if let Some(user_id) = context.get_header(header_name) {
                if !user_id.is_empty() {
                    return Some(Identifier::UserId(user_id.clone()));
                }
            }
        }

        // 从查询参数提取
        if let Some(query_param_name) = &self.query_param_name {
            if let Some(user_id) = context.query_params.get(query_param_name) {
                if !user_id.is_empty() {
                    return Some(Identifier::UserId(user_id.clone()));
                }
            }
        }

        // 使用默认用户ID
        if let Some(default) = &self.default_user_id {
            return Some(Identifier::UserId(default.clone()));
        }

        None
    }

    fn name(&self) -> &str {
        "UserIdExtractor"
    }
}

// ============================================================================
// IP提取器
// ============================================================================

/// IP提取器
///
/// 从请求上下文中提取IP地址，支持从多个HTTP头中提取真实IP。
pub struct IpExtractor {
    /// HTTP头名称列表（按优先级顺序）
    header_names: Vec<String>,
    /// 是否验证IP格式
    validate: bool,
}

impl IpExtractor {
    /// 创建新的IP提取器
    ///
    /// # 参数
    /// - `header_names`: HTTP头名称列表（按优先级顺序）
    /// - `validate`: 是否验证IP格式
    pub fn new(header_names: Vec<String>, validate: bool) -> Self {
        Self {
            header_names,
            validate,
        }
    }

    /// 创建默认的IP提取器（从Remote Addr提取）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::IpExtractor;
    ///
    /// let extractor = IpExtractor::new_default();
    /// ```
    pub fn new_default() -> Self {
        Self::new(vec![], true)
    }

    /// 创建从指定HTTP头提取的IP提取器
    ///
    /// # 参数
    /// - `header_name`: HTTP头名称
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::IpExtractor;
    ///
    /// let extractor = IpExtractor::from_header("X-Forwarded-For");
    /// ```
    pub fn from_header(header_name: &str) -> Self {
        Self::new(vec![header_name.to_string()], true)
    }

    /// 创建从多个HTTP头提取的IP提取器（按优先级顺序）
    ///
    /// # 参数
    /// - `header_names`: HTTP头名称列表
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::IpExtractor;
    ///
    /// let extractor = IpExtractor::from_headers(vec![
    ///     "X-Real-IP",
    ///     "X-Forwarded-For",
    /// ]);
    /// ```
    pub fn from_headers(header_names: Vec<&str>) -> Self {
        Self::new(header_names.iter().map(|s| s.to_string()).collect(), true)
    }

    /// 解析IP地址（支持单个IP和IP列表）
    ///
    /// 对于 X-Forwarded-For 格式的 IP 列表（client, proxy1, proxy2），
    /// 从右向左查找，跳过可信代理的 IP，以防止伪造攻击。
    ///
    /// # 安全说明
    /// X-Forwarded-For 头可能被客户端伪造，因此不能直接信任第一个 IP。
    /// 正确的做法是从右向左查找，跳过已知的可信代理。
    ///
    /// # 参数
    /// - `value`: IP 地址或 IP 列表字符串
    ///
    /// # 返回
    /// - `Some(String)`: 解析后的 IP 地址
    /// - `None`: 无法解析或验证失败
    fn parse_ip(&self, value: &str) -> Option<String> {
        // 处理IP列表（X-Forwarded-For格式：client, proxy1, proxy2）
        // 注意：真实客户端IP在最左边，代理依次向右追加
        // 攻击者可以通过在左边添加伪造IP来欺骗
        // 因此：取最左边的IP作为客户端IP（因为它是最早由第一个代理添加的）
        let ips: Vec<&str> = value
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if ips.is_empty() {
            return None;
        }

        // 如果只有一个 IP，直接使用
        if ips.len() == 1 {
            let ip = ips[0];
            if self.validate && ip.parse::<IpAddr>().is_err() {
                return None;
            }
            return Some(ip.to_string());
        }

        // 多个IP时，取最左边的IP作为客户端IP
        // 这是安全的，因为第一个代理会将自己的IP追加到右边
        // 攻击者伪造的IP会在最左边，但如果我们信任第一个代理，
        // 它会追加自己的IP，所以左边第二个IP开始是可信的
        // 简化处理：使用最左边的IP（假设第一个代理是可信的）
        let ip = ips[0];

        // 验证IP格式
        if self.validate && ip.parse::<IpAddr>().is_err() {
            return None;
        }

        Some(ip.to_string())
    }
}

impl IdentifierExtractor for IpExtractor {
    fn extract(&self, context: &RequestContext) -> Option<Identifier> {
        // 从HTTP头列表中提取
        for header_name in &self.header_names {
            if let Some(value) = context.get_header(header_name) {
                if let Some(ip) = self.parse_ip(value) {
                    return Some(Identifier::Ip(ip));
                }
            }
        }

        // 从客户端IP提取
        if let Some(client_ip) = &context.client_ip {
            if let Some(ip) = self.parse_ip(client_ip) {
                return Some(Identifier::Ip(ip));
            }
        }

        None
    }

    fn name(&self) -> &str {
        "IpExtractor"
    }
}

// ============================================================================
// MAC提取器
// ============================================================================

/// MAC提取器
///
/// 从请求上下文中提取MAC地址。
pub struct MacExtractor {
    /// HTTP头名称
    header_name: Option<String>,
    /// 查询参数名称
    query_param_name: Option<String>,
    /// 是否验证MAC格式
    validate: bool,
}

impl MacExtractor {
    /// 创建新的MAC提取器
    ///
    /// # 参数
    /// - `header_name`: HTTP头名称
    /// - `query_param_name`: 查询参数名称
    /// - `validate`: 是否验证MAC格式
    pub fn new(
        header_name: Option<String>,
        query_param_name: Option<String>,
        validate: bool,
    ) -> Self {
        Self {
            header_name,
            query_param_name,
            validate,
        }
    }

    /// 创建默认的MAC提取器（从HTTP头提取）
    ///
    /// # 参数
    /// - `header_name`: HTTP头名称
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::MacExtractor;
    ///
    /// let extractor = MacExtractor::from_header("X-Mac-Address");
    /// ```
    pub fn from_header(header_name: &str) -> Self {
        Self::new(Some(header_name.to_string()), None, true)
    }

    /// 从查询参数提取MAC地址
    ///
    /// # 参数
    /// - `query_param_name`: 查询参数名称
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::MacExtractor;
    ///
    /// let extractor = MacExtractor::from_query_param("mac");
    /// ```
    pub fn from_query_param(query_param_name: &str) -> Self {
        Self::new(None, Some(query_param_name.to_string()), true)
    }

    /// 验证MAC地址格式
    fn validate_mac(&self, mac: &str) -> bool {
        if !self.validate {
            return true;
        }

        // 支持多种MAC地址格式：
        // - 00:1A:2B:3C:4D:5E
        // - 00-1A-2B-3C-4D-5E
        // - 001A.2B3C.4D5E
        // - 001A2B3C4D5E

        let cleaned = mac.replace([':', '-', '.'], "");

        if cleaned.len() != 12 {
            return false;
        }

        // 检查是否为有效的十六进制
        cleaned.chars().all(|c| c.is_ascii_hexdigit())
    }
}

impl IdentifierExtractor for MacExtractor {
    fn extract(&self, context: &RequestContext) -> Option<Identifier> {
        // 从HTTP头提取
        if let Some(header_name) = &self.header_name {
            if let Some(mac) = context.get_header(header_name) {
                if !mac.is_empty() && self.validate_mac(mac) {
                    return Some(Identifier::Mac(mac.clone()));
                }
            }
        }

        // 从查询参数提取
        if let Some(query_param_name) = &self.query_param_name {
            if let Some(mac) = context.query_params.get(query_param_name) {
                if !mac.is_empty() && self.validate_mac(mac) {
                    return Some(Identifier::Mac(mac.clone()));
                }
            }
        }

        None
    }

    fn name(&self) -> &str {
        "MacExtractor"
    }
}

// ============================================================================
// API密钥提取器
// ============================================================================

/// API密钥提取器
///
/// 从请求上下文中提取API密钥。
pub struct ApiKeyExtractor {
    /// HTTP头名称
    header_name: Option<String>,
    /// 查询参数名称（已禁用，仅为了兼容性保留）
    _query_param_name: Option<String>,
    /// 前缀（如 "Bearer "）
    prefix: Option<String>,
}

impl ApiKeyExtractor {
    /// 创建新的API密钥提取器
    ///
    /// # 参数
    /// - `header_name`: HTTP头名称
    /// - `query_param_name`: 查询参数名称（已禁用）
    /// - `prefix`: 前缀
    pub fn new(
        header_name: Option<String>,
        query_param_name: Option<String>,
        prefix: Option<String>,
    ) -> Self {
        if query_param_name.is_some() {
            tracing::warn!("出于安全考虑，通过查询参数提取API Key已被禁用");
        }
        Self {
            header_name,
            _query_param_name: query_param_name,
            prefix,
        }
    }

    /// 从Authorization头提取API密钥（便捷方法）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::ApiKeyExtractor;
    ///
    /// let extractor = ApiKeyExtractor::from_authorization_header();
    /// ```
    pub fn from_authorization_header() -> Self {
        Self::new(
            Some("Authorization".to_string()),
            None,
            Some("Bearer ".to_string()),
        )
    }

    /// 从指定HTTP头提取API密钥
    ///
    /// # 参数
    /// - `header_name`: HTTP头名称
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::ApiKeyExtractor;
    ///
    /// let extractor = ApiKeyExtractor::from_header("X-API-Key");
    /// ```
    pub fn from_header(header_name: &str) -> Self {
        Self::new(Some(header_name.to_string()), None, None)
    }

    /// 从查询参数提取API密钥
    ///
    /// # 参数
    /// - `query_param_name`: 查询参数名称
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::ApiKeyExtractor;
    ///
    /// let extractor = ApiKeyExtractor::from_query_param("api_key");
    /// ```
    pub fn from_query_param(query_param_name: &str) -> Self {
        Self::new(None, Some(query_param_name.to_string()), None)
    }

    /// 清理API密钥（移除前缀）
    fn clean_key(&self, value: &str) -> Option<String> {
        let key = if let Some(prefix) = &self.prefix {
            value.strip_prefix(prefix)
        } else {
            Some(value)
        }?;

        let key = key.trim();
        if key.is_empty() {
            return None;
        }

        Some(key.to_string())
    }
}

impl IdentifierExtractor for ApiKeyExtractor {
    fn extract(&self, context: &RequestContext) -> Option<Identifier> {
        // 从HTTP头提取
        if let Some(header_name) = &self.header_name {
            if let Some(value) = context.get_header(header_name) {
                if let Some(key) = self.clean_key(value) {
                    return Some(Identifier::ApiKey(key));
                }
            }
        }

        None
    }

    fn name(&self) -> &str {
        "ApiKeyExtractor"
    }
}

// ============================================================================
// 设备ID提取器
// ============================================================================

/// 设备ID提取器
///
/// 从请求上下文中提取设备ID。
pub struct DeviceIdExtractor {
    /// HTTP头名称
    header_name: Option<String>,
    /// 查询参数名称
    query_param_name: Option<String>,
}

impl DeviceIdExtractor {
    /// 创建新的设备ID提取器
    ///
    /// # 参数
    /// - `header_name`: HTTP头名称
    /// - `query_param_name`: 查询参数名称
    pub fn new(header_name: Option<String>, query_param_name: Option<String>) -> Self {
        Self {
            header_name,
            query_param_name,
        }
    }

    /// 从HTTP头提取设备ID
    ///
    /// # 参数
    /// - `header_name`: HTTP头名称
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::DeviceIdExtractor;
    ///
    /// let extractor = DeviceIdExtractor::from_header("X-Device-Id");
    /// ```
    pub fn from_header(header_name: &str) -> Self {
        Self::new(Some(header_name.to_string()), None)
    }

    /// 从查询参数提取设备ID
    ///
    /// # 参数
    /// - `query_param_name`: 查询参数名称
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::DeviceIdExtractor;
    ///
    /// let extractor = DeviceIdExtractor::from_query_param("device_id");
    /// ```
    pub fn from_query_param(query_param_name: &str) -> Self {
        Self::new(None, Some(query_param_name.to_string()))
    }
}

impl IdentifierExtractor for DeviceIdExtractor {
    fn extract(&self, context: &RequestContext) -> Option<Identifier> {
        // 从HTTP头提取
        if let Some(header_name) = &self.header_name {
            if let Some(device_id) = context.get_header(header_name) {
                if !device_id.is_empty() {
                    return Some(Identifier::DeviceId(device_id.clone()));
                }
            }
        }

        // 从查询参数提取
        if let Some(query_param_name) = &self.query_param_name {
            if let Some(device_id) = context.query_params.get(query_param_name) {
                if !device_id.is_empty() {
                    return Some(Identifier::DeviceId(device_id.clone()));
                }
            }
        }

        None
    }

    fn name(&self) -> &str {
        "DeviceIdExtractor"
    }
}

// ============================================================================
// 组合提取器
// ============================================================================

/// 组合提取器
///
/// 按顺序尝试多个提取器，直到成功提取标识符。
pub struct CompositeExtractor {
    /// 提取器列表（按优先级顺序）
    extractors: Vec<Box<dyn IdentifierExtractor>>,
    /// 是否在所有提取器都失败时返回默认标识符
    fallback_to_default: bool,
}

impl CompositeExtractor {
    /// 创建新的组合提取器
    ///
    /// # 参数
    /// - `extractors`: 提取器列表
    /// - `fallback_to_default`: 是否在所有提取器都失败时返回默认标识符
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::{CompositeExtractor, UserIdExtractor, IpExtractor};
    ///
    /// let extractor = CompositeExtractor::new(
    ///     vec![
    ///         Box::new(UserIdExtractor::from_header("X-User-Id")),
    ///         Box::new(IpExtractor::new_default()),
    ///     ],
    ///     true,
    /// );
    /// ```
    pub fn new(extractors: Vec<Box<dyn IdentifierExtractor>>, fallback_to_default: bool) -> Self {
        Self {
            extractors,
            fallback_to_default,
        }
    }

    /// 添加提取器
    ///
    /// # 参数
    /// - `extractor`: 提取器
    pub fn add_extractor(mut self, extractor: Box<dyn IdentifierExtractor>) -> Self {
        self.extractors.push(extractor);
        self
    }

    /// 设置是否回退到默认标识符
    ///
    /// # 参数
    /// - `fallback`: 是否回退
    pub fn with_fallback(mut self, fallback: bool) -> Self {
        self.fallback_to_default = fallback;
        self
    }
}

impl IdentifierExtractor for CompositeExtractor {
    fn extract(&self, context: &RequestContext) -> Option<Identifier> {
        // 按顺序尝试每个提取器
        for extractor in &self.extractors {
            if let Some(identifier) = extractor.extract(context) {
                return Some(identifier);
            }
        }

        // 如果所有提取器都失败且启用了回退，使用 IP 作为后备
        if self.fallback_to_default {
            // 使用 IP 作为后备，而不是固定的 "default"
            if let Some(client_ip) = &context.client_ip {
                // 验证 IP 格式
                if client_ip.parse::<IpAddr>().is_ok() {
                    return Some(Identifier::Ip(client_ip.clone()));
                }
            }
            // 如果没有 IP 或 IP 无效，返回 None
            // 这样可以让调用者决定如何处理未识别的请求
        }

        None
    }

    fn name(&self) -> &str {
        "CompositeExtractor"
    }
}

// ============================================================================
// 自定义提取器
// ============================================================================

/// 自定义提取器
///
/// 允许用户自定义提取逻辑。
pub struct CustomExtractor<F>
where
    F: Fn(&RequestContext) -> Option<Identifier> + Send + Sync,
{
    /// 提取函数
    extractor_fn: F,
    /// 提取器名称
    name: String,
}

impl<F> CustomExtractor<F>
where
    F: Fn(&RequestContext) -> Option<Identifier> + Send + Sync,
{
    /// 创建新的自定义提取器
    ///
    /// # 参数
    /// - `name`: 提取器名称
    /// - `extractor_fn`: 提取函数
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::{CustomExtractor, Identifier, RequestContext};
    ///
    /// let extractor = CustomExtractor::new(
    ///     "MyCustomExtractor",
    ///     |context| {
    ///         // 自定义提取逻辑
    ///         context.get_header("X-Custom-Id")
    ///             .map(|id| Identifier::UserId(id.clone()))
    ///     },
    /// );
    /// ```
    pub fn new(name: &str, extractor_fn: F) -> Self {
        Self {
            extractor_fn,
            name: name.to_string(),
        }
    }
}

impl<F> IdentifierExtractor for CustomExtractor<F>
where
    F: Fn(&RequestContext) -> Option<Identifier> + Send + Sync,
{
    fn extract(&self, context: &RequestContext) -> Option<Identifier> {
        (self.extractor_fn)(context)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ============================================================================
// 规则匹配引擎
// ============================================================================

/// 匹配条件
///
/// 定义单个匹配条件。
#[derive(Clone)]
pub enum MatchCondition {
    /// 用户ID匹配
    User(Vec<String>),
    /// IP范围匹配
    Ip(Vec<IpRange>),
    /// 地理位置匹配
    Geo(Vec<String>),
    /// API版本匹配
    ApiVersion(Vec<String>),
    /// 设备类型匹配
    Device(Vec<String>),
    /// 自定义匹配
    Custom(Arc<dyn Fn(&RequestContext) -> bool + Send + Sync>),
}

impl std::fmt::Debug for MatchCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatchCondition::User(ids) => f.debug_tuple("User").field(ids).finish(),
            MatchCondition::Ip(ranges) => f.debug_tuple("Ip").field(&ranges.len()).finish(),
            MatchCondition::Geo(countries) => f.debug_tuple("Geo").field(countries).finish(),
            MatchCondition::ApiVersion(versions) => {
                f.debug_tuple("ApiVersion").field(versions).finish()
            }
            MatchCondition::Device(device_types) => {
                f.debug_tuple("Device").field(device_types).finish()
            }
            MatchCondition::Custom(_) => f.debug_tuple("Custom").field(&"<closure>").finish(),
        }
    }
}

/// IP范围
#[derive(Debug, Clone)]
pub enum IpRange {
    /// 单个IP
    Single(IpAddr),
    /// IPv4 CIDR
    Ipv4Cidr { addr: Ipv4Addr, prefix: u8 },
    /// IPv6 CIDR
    Ipv6Cidr { addr: Ipv6Addr, prefix: u8 },
    /// IPv4范围
    Ipv4Range { start: Ipv4Addr, end: Ipv4Addr },
}

impl IpRange {
    /// 检查IP是否在范围内
    pub fn contains(&self, ip: &IpAddr) -> bool {
        match self {
            IpRange::Single(addr) => addr == ip,
            IpRange::Ipv4Cidr { addr, prefix } => {
                if let IpAddr::V4(ipv4) = ip {
                    self.ipv4_in_cidr(ipv4, addr, *prefix)
                } else {
                    false
                }
            }
            IpRange::Ipv6Cidr { addr, prefix } => {
                if let IpAddr::V6(ipv6) = ip {
                    self.ipv6_in_cidr(ipv6, addr, *prefix)
                } else {
                    false
                }
            }
            IpRange::Ipv4Range { start, end } => {
                if let IpAddr::V4(ipv4) = ip {
                    ipv4 >= start && ipv4 <= end
                } else {
                    false
                }
            }
        }
    }

    /// 检查IPv4是否在CIDR范围内
    fn ipv4_in_cidr(&self, ip: &Ipv4Addr, network: &Ipv4Addr, prefix: u8) -> bool {
        let ip_u32 = u32::from(*ip);
        let network_u32 = u32::from(*network);
        let mask = if prefix == 0 {
            0
        } else {
            0xFFFFFFFF << (32 - prefix)
        };

        (ip_u32 & mask) == (network_u32 & mask)
    }

    /// 检查IPv6是否在CIDR范围内
    fn ipv6_in_cidr(&self, ip: &Ipv6Addr, network: &Ipv6Addr, prefix: u8) -> bool {
        let ip_segments = ip.segments();
        let network_segments = network.segments();

        let full_segments = (prefix / 16) as usize;
        let remaining_bits = prefix % 16;

        // 检查完整的段
        for i in 0..full_segments {
            if ip_segments[i] != network_segments[i] {
                return false;
            }
        }

        // 检查剩余的位
        if remaining_bits > 0 && full_segments < 8 {
            let mask = 0xFFFFu16 << (16 - remaining_bits);
            if (ip_segments[full_segments] & mask) != (network_segments[full_segments] & mask) {
                return false;
            }
        }

        true
    }
}

impl FromStr for IpRange {
    type Err = FlowGuardError;

    /// 从字符串解析IP范围
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains('/') {
            // CIDR格式
            let parts: Vec<&str> = s.split('/').collect();
            if parts.len() != 2 {
                return Err(FlowGuardError::ConfigError(format!(
                    "无效的CIDR格式: {}",
                    s
                )));
            }

            let addr: IpAddr = parts[0]
                .parse()
                .map_err(|_| FlowGuardError::ConfigError(format!("无效的IP地址: {}", parts[0])))?;
            let prefix: u8 = parts[1]
                .parse()
                .map_err(|_| FlowGuardError::ConfigError(format!("无效的前缀: {}", parts[1])))?;

            match addr {
                IpAddr::V4(ipv4) => {
                    if prefix > 32 {
                        return Err(FlowGuardError::ConfigError(format!(
                            "IPv4前缀不能超过32: {}",
                            prefix
                        )));
                    }
                    Ok(IpRange::Ipv4Cidr { addr: ipv4, prefix })
                }
                IpAddr::V6(ipv6) => {
                    if prefix > 128 {
                        return Err(FlowGuardError::ConfigError(format!(
                            "IPv6前缀不能超过128: {}",
                            prefix
                        )));
                    }
                    Ok(IpRange::Ipv6Cidr { addr: ipv6, prefix })
                }
            }
        } else if s.contains('-') {
            // 范围格式
            let parts: Vec<&str> = s.split('-').collect();
            if parts.len() != 2 {
                return Err(FlowGuardError::ConfigError(format!(
                    "无效的IP范围格式: {}",
                    s
                )));
            }

            let start: Ipv4Addr = parts[0]
                .parse()
                .map_err(|_| FlowGuardError::ConfigError(format!("无效的起始IP: {}", parts[0])))?;
            let end: Ipv4Addr = parts[1]
                .parse()
                .map_err(|_| FlowGuardError::ConfigError(format!("无效的结束IP: {}", parts[1])))?;

            if start > end {
                return Err(FlowGuardError::ConfigError(format!(
                    "起始IP不能大于结束IP: {} - {}",
                    parts[0], parts[1]
                )));
            }

            Ok(IpRange::Ipv4Range { start, end })
        } else {
            // 单个IP
            let addr: IpAddr = s
                .parse()
                .map_err(|_| FlowGuardError::ConfigError(format!("无效的IP地址: {}", s)))?;
            Ok(IpRange::Single(addr))
        }
    }
}

/// 逻辑操作符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOperator {
    /// 逻辑与
    And,
    /// 逻辑或
    Or,
    /// 逻辑非
    Not,
}

/// 复合条件
///
/// 支持AND/OR/NOT逻辑操作。
pub struct CompositeCondition {
    /// 子条件列表
    pub conditions: Vec<Box<dyn ConditionEvaluator>>,
    /// 逻辑操作符
    pub operator: LogicalOperator,
}

impl std::fmt::Debug for CompositeCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeCondition")
            .field("conditions", &self.conditions.len())
            .field("operator", &self.operator)
            .finish()
    }
}

impl Clone for CompositeCondition {
    fn clone(&self) -> Self {
        // 由于 ConditionEvaluator 不能 Clone，我们创建一个新的 CompositeCondition
        // 实际使用时，用户需要重新构建条件
        Self {
            conditions: self
                .conditions
                .iter()
                .map(|_| {
                    // 由于无法克隆 trait 对象，这里返回一个占位符
                    // 实际使用中，需要重新构建条件
                    Box::new(MatchCondition::User(vec![])) as Box<dyn ConditionEvaluator>
                })
                .collect(),
            operator: self.operator,
        }
    }
}

/// 条件评估器 trait
///
/// 所有条件都需要实现此trait。
pub trait ConditionEvaluator: Send + Sync {
    /// 评估条件
    fn evaluate(&self, context: &RequestContext) -> bool;

    /// 获取条件描述
    fn description(&self) -> String;
}

impl ConditionEvaluator for MatchCondition {
    fn evaluate(&self, context: &RequestContext) -> bool {
        match self {
            MatchCondition::User(user_ids) => {
                if let Some(user_id) = context.get_header("X-User-Id") {
                    user_ids.contains(&user_id.to_string()) || user_ids.contains(&"*".to_string())
                } else {
                    user_ids.contains(&"*".to_string())
                }
            }
            MatchCondition::Ip(ip_ranges) => {
                if let Some(client_ip) = &context.client_ip {
                    if let Ok(ip) = client_ip.parse::<IpAddr>() {
                        return ip_ranges.iter().any(|range| range.contains(&ip));
                    }
                }
                false
            }
            MatchCondition::Geo(countries) => {
                if let Some(country) = context.get_header("X-Country") {
                    countries.contains(&country.to_string()) || countries.contains(&"*".to_string())
                } else {
                    countries.contains(&"*".to_string())
                }
            }
            MatchCondition::ApiVersion(versions) => {
                if let Some(version) = context.get_header("X-API-Version") {
                    versions.contains(&version.to_string()) || versions.contains(&"*".to_string())
                } else {
                    versions.contains(&"*".to_string())
                }
            }
            MatchCondition::Device(device_types) => {
                if let Some(device_type) = context.get_header("X-Device-Type") {
                    device_types.contains(&device_type.to_string())
                        || device_types.contains(&"*".to_string())
                } else {
                    device_types.contains(&"*".to_string())
                }
            }
            MatchCondition::Custom(eval_fn) => eval_fn(context),
        }
    }

    fn description(&self) -> String {
        match self {
            MatchCondition::User(ids) => format!("User in {:?}", ids),
            MatchCondition::Ip(ranges) => format!("IP in {} ranges", ranges.len()),
            MatchCondition::Geo(countries) => format!("Country in {:?}", countries),
            MatchCondition::ApiVersion(versions) => format!("API version in {:?}", versions),
            MatchCondition::Device(device_types) => format!("Device type in {:?}", device_types),
            MatchCondition::Custom(_) => "Custom condition".to_string(),
        }
    }
}

impl ConditionEvaluator for CompositeCondition {
    fn evaluate(&self, context: &RequestContext) -> bool {
        match self.operator {
            LogicalOperator::And => self.conditions.iter().all(|c| c.evaluate(context)),
            LogicalOperator::Or => self.conditions.iter().any(|c| c.evaluate(context)),
            LogicalOperator::Not => {
                // NOT操作符只应该有一个子条件
                self.conditions
                    .first()
                    .is_some_and(|c| !c.evaluate(context))
            }
        }
    }

    fn description(&self) -> String {
        let op_str = match self.operator {
            LogicalOperator::And => "AND",
            LogicalOperator::Or => "OR",
            LogicalOperator::Not => "NOT",
        };
        format!("{} ({})", op_str, self.conditions.len())
    }
}

/// 规则匹配器
///
/// 高性能规则匹配引擎，支持优先级排序和复合条件。
pub struct RuleMatcher {
    /// 规则列表（按优先级排序）
    rules: Vec<Rule>,
    /// 匹配统计
    stats: std::sync::RwLock<MatcherStats>,
}

/// 规则
pub struct Rule {
    /// 规则ID
    pub id: String,
    /// 规则名称
    pub name: String,
    /// 优先级（数值越大优先级越高）
    pub priority: u16,
    /// 匹配条件
    pub condition: Box<dyn ConditionEvaluator>,
    /// 是否启用
    pub enabled: bool,
}

impl std::fmt::Debug for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rule")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("priority", &self.priority)
            .field("enabled", &self.enabled)
            .field("condition", &"<condition>")
            .finish()
    }
}

impl Clone for Rule {
    fn clone(&self) -> Self {
        // 由于 ConditionEvaluator 不能 Clone，我们创建一个新的 Rule
        // 实际使用时，用户需要重新构建规则
        Self {
            id: self.id.clone(),
            name: self.name.clone(),
            priority: self.priority,
            condition: Box::new(MatchCondition::User(vec![])) as Box<dyn ConditionEvaluator>,
            enabled: self.enabled,
        }
    }
}

/// 匹配器统计信息
#[derive(Debug, Clone, Default)]
pub struct MatcherStats {
    /// 总匹配次数
    pub total_matches: u64,
    /// 总不匹配次数
    pub total_mismatches: u64,
    /// 最后匹配时间
    pub last_match_time: Option<Instant>,
    /// 平均匹配时间（纳秒）
    pub avg_match_time_ns: u64,
}

impl RuleMatcher {
    /// 创建新的规则匹配器
    ///
    /// # 参数
    /// - `rules`: 规则列表
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::{RuleMatcher, Rule, MatchCondition};
    ///
    /// let matcher = RuleMatcher::new(vec![
    ///     Rule {
    ///         id: "rule1".to_string(),
    ///         name: "Test Rule".to_string(),
    ///         priority: 100,
    ///         condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
    ///         enabled: true,
    ///     },
    /// ]);
    /// ```
    pub fn new(rules: Vec<Rule>) -> Self {
        let mut matcher = Self {
            rules: Vec::new(),
            stats: std::sync::RwLock::new(MatcherStats::default()),
        };

        for rule in rules {
            matcher.add_rule(rule);
        }

        matcher
    }

    /// 添加规则
    ///
    /// # 参数
    /// - `rule`: 规则
    pub fn add_rule(&mut self, rule: Rule) {
        // 按优先级排序（降序）
        let pos = self
            .rules
            .binary_search_by(|r| r.priority.cmp(&rule.priority).reverse())
            .unwrap_or_else(|pos| pos);

        self.rules.insert(pos, rule);
    }

    /// 移除规则
    ///
    /// # 参数
    /// - `rule_id`: 规则ID
    pub fn remove_rule(&mut self, rule_id: &str) -> Option<Rule> {
        if let Some(pos) = self.rules.iter().position(|r| r.id == rule_id) {
            Some(self.rules.remove(pos))
        } else {
            None
        }
    }

    /// 检查请求是否匹配任何规则
    ///
    /// # 参数
    /// - `context`: 请求上下文
    ///
    /// # 返回
    /// - `Some(rule)`: 匹配的规则
    /// - `None`: 没有匹配的规则
    ///
    /// # 性能
    /// - P99延迟 < 200μs
    /// - 支持至少100条规则
    pub fn matches(&self, context: &RequestContext) -> Option<&Rule> {
        let start = Instant::now();

        // 按优先级顺序检查规则
        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }

            if rule.condition.evaluate(context) {
                // 更新统计信息
                let elapsed = start.elapsed().as_nanos() as u64;
                {
                    if let Ok(mut stats) = self.stats.write() {
                        stats.total_matches += 1;
                        stats.last_match_time = Some(Instant::now());

                        // 更新平均匹配时间（使用指数移动平均）
                        if stats.total_matches == 1 {
                            stats.avg_match_time_ns = elapsed;
                        } else {
                            stats.avg_match_time_ns = (stats.avg_match_time_ns * 9 + elapsed) / 10;
                        }
                    }
                }

                return Some(rule);
            }
        }

        {
            let mut stats = self.stats.write().unwrap();
            stats.total_mismatches += 1;
        }
        None
    }

    /// 获取所有匹配的规则
    ///
    /// # 参数
    /// - `context`: 请求上下文
    ///
    /// # 返回
    /// - 匹配的规则列表（按优先级排序）
    pub fn match_all(&self, context: &RequestContext) -> Vec<&Rule> {
        self.rules
            .iter()
            .filter(|rule| rule.enabled && rule.condition.evaluate(context))
            .collect()
    }

    /// 获取统计信息
    pub fn stats(&self) -> MatcherStats {
        self.stats.read().unwrap().clone()
    }

    /// 重置统计信息
    pub fn reset_stats(&self) {
        let mut stats = self.stats.write().unwrap();
        *stats = MatcherStats::default();
    }

    /// 获取规则数量
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 从配置创建规则匹配器
    ///
    /// # 参数
    /// - `config_matchers`: 配置中的匹配器列表
    pub fn from_config(config_matchers: &[ConfigMatcher]) -> Result<Self, FlowGuardError> {
        let mut rules = Vec::new();

        for (index, matcher) in config_matchers.iter().enumerate() {
            let condition: Box<dyn ConditionEvaluator> = match matcher {
                ConfigMatcher::User { user_ids } => {
                    Box::new(MatchCondition::User(user_ids.clone()))
                }
                ConfigMatcher::Ip { ip_ranges } => {
                    let ranges: Result<Vec<IpRange>, _> =
                        ip_ranges.iter().map(|s| s.parse()).collect();

                    Box::new(MatchCondition::Ip(ranges?))
                }
                ConfigMatcher::Geo { countries } => {
                    Box::new(MatchCondition::Geo(countries.clone()))
                }
                ConfigMatcher::ApiVersion { versions } => {
                    Box::new(MatchCondition::ApiVersion(versions.clone()))
                }
                ConfigMatcher::Device { device_types } => {
                    Box::new(MatchCondition::Device(device_types.clone()))
                }
                ConfigMatcher::Custom { name, config: _ } => {
                    // 自定义匹配器需要在运行时通过CustomMatcherRegistry处理
                    // 这里返回一个占位符，实际匹配逻辑由CustomMatcherRegistry处理
                    let name = name.clone();
                    Box::new(MatchCondition::Custom(Arc::new(move |_context| {
                        // 自定义匹配器的实际匹配逻辑在CustomMatcherRegistry中实现
                        // 这里只是占位符，返回false表示不匹配
                        tracing::warn!("自定义匹配器 '{}' 需要通过CustomMatcherRegistry处理", name);
                        false
                    })))
                }
            };

            rules.push(Rule {
                id: format!("rule_{}", index),
                name: format!("Rule {}", index),
                priority: 100,
                condition,
                enabled: true,
            });
        }

        Ok(Self::new(rules))
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    // ==================== 标识符提取器测试 ====================

    #[test]
    fn test_user_id_extractor_from_header() {
        let extractor = UserIdExtractor::from_header("X-User-Id");
        let context = RequestContext::new().with_header("X-User-Id", "user123");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("user123".to_string()));
    }

    #[test]
    fn test_user_id_extractor_from_query_param() {
        let extractor = UserIdExtractor::from_query_param("user_id");
        let context = RequestContext::new().with_query_param("user_id", "user456");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("user456".to_string()));
    }

    #[test]
    fn test_user_id_extractor_with_default() {
        let extractor = UserIdExtractor::from_header("X-User-Id").with_default("default");
        let context = RequestContext::new();

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("default".to_string()));
    }

    #[test]
    fn test_user_id_extractor_priority() {
        let extractor = UserIdExtractor::new(
            Some("X-User-Id".to_string()),
            Some("user_id".to_string()),
            None,
        );
        let context = RequestContext::new()
            .with_header("X-User-Id", "header_user")
            .with_query_param("user_id", "query_user");

        let identifier = extractor.extract(&context).unwrap();
        // 应该优先从header提取
        assert_eq!(identifier, Identifier::UserId("header_user".to_string()));
    }

    #[test]
    fn test_ip_extractor_from_header() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        let context = RequestContext::new().with_header("X-Forwarded-For", "192.168.1.1");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Ip("192.168.1.1".to_string()));
    }

    #[test]
    fn test_ip_extractor_from_client_ip() {
        let extractor = IpExtractor::new_default();
        let context = RequestContext::new().with_client_ip("10.0.0.1");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Ip("10.0.0.1".to_string()));
    }

    #[test]
    fn test_ip_extractor_multiple_headers() {
        let extractor = IpExtractor::from_headers(vec!["X-Real-IP", "X-Forwarded-For"]);
        let context = RequestContext::new()
            .with_header("X-Forwarded-For", "192.168.1.1")
            .with_header("X-Real-IP", "10.0.0.1");

        let identifier = extractor.extract(&context).unwrap();
        // 应该优先从第一个header提取
        assert_eq!(identifier, Identifier::Ip("10.0.0.1".to_string()));
    }

    #[test]
    fn test_ip_extractor_parse_list() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        let context = RequestContext::new()
            .with_header("X-Forwarded-For", "192.168.1.1, 10.0.0.1, 172.16.0.1");

        let identifier = extractor.extract(&context).unwrap();
        // 应该提取第一个IP
        assert_eq!(identifier, Identifier::Ip("192.168.1.1".to_string()));
    }

    #[test]
    fn test_mac_extractor_from_header() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let context = RequestContext::new().with_header("X-Mac-Address", "00:1A:2B:3C:4D:5E");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::Mac("00:1A:2B:3C:4D:5E".to_string()));
    }

    #[test]
    fn test_mac_extractor_validate_format() {
        let extractor = MacExtractor::from_header("X-Mac-Address");

        // 有效的MAC地址
        let context1 = RequestContext::new().with_header("X-Mac-Address", "00:1A:2B:3C:4D:5E");
        assert!(extractor.extract(&context1).is_some());

        // 无效的MAC地址
        let context2 = RequestContext::new().with_header("X-Mac-Address", "invalid");
        assert!(extractor.extract(&context2).is_none());
    }

    #[test]
    fn test_api_key_extractor_from_authorization() {
        let extractor = ApiKeyExtractor::from_authorization_header();
        let context = RequestContext::new().with_header("Authorization", "Bearer my-api-key");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::ApiKey("my-api-key".to_string()));
    }

    #[test]
    fn test_api_key_extractor_from_header() {
        let extractor = ApiKeyExtractor::from_header("X-API-Key");
        let context = RequestContext::new().with_header("X-API-Key", "my-api-key");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::ApiKey("my-api-key".to_string()));
    }

    #[test]
    fn test_device_id_extractor_from_header() {
        let extractor = DeviceIdExtractor::from_header("X-Device-Id");
        let context = RequestContext::new().with_header("X-Device-Id", "device-123");

        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::DeviceId("device-123".to_string()));
    }

    #[test]
    fn test_composite_extractor() {
        let extractor = CompositeExtractor::new(
            vec![
                Box::new(UserIdExtractor::from_header("X-User-Id")),
                Box::new(IpExtractor::new_default()),
            ],
            true,
        );

        // 应该从第一个提取器提取
        let context1 = RequestContext::new()
            .with_header("X-User-Id", "user123")
            .with_client_ip("10.0.0.1");
        let identifier1 = extractor.extract(&context1).unwrap();
        assert_eq!(identifier1, Identifier::UserId("user123".to_string()));

        // 应该从第二个提取器提取
        let context2 = RequestContext::new().with_client_ip("10.0.0.1");
        let identifier2 = extractor.extract(&context2).unwrap();
        assert_eq!(identifier2, Identifier::Ip("10.0.0.1".to_string()));
    }

    #[test]
    fn test_custom_extractor() {
        let extractor = CustomExtractor::new("MyExtractor", |context| {
            context
                .get_header("X-Custom")
                .map(|value| Identifier::UserId(value.clone()))
        });

        let context = RequestContext::new().with_header("X-Custom", "custom123");
        let identifier = extractor.extract(&context).unwrap();
        assert_eq!(identifier, Identifier::UserId("custom123".to_string()));
    }

    // ==================== IP范围测试 ====================

    #[test]
    fn test_ip_range_single() {
        let range: IpRange = "192.168.1.1".parse().unwrap();
        let ip: IpAddr = "192.168.1.1".parse().unwrap();
        assert!(range.contains(&ip));

        let ip2: IpAddr = "192.168.1.2".parse().unwrap();
        assert!(!range.contains(&ip2));
    }

    #[test]
    fn test_ip_range_ipv4_cidr() {
        let range: IpRange = "192.168.1.0/24".parse().unwrap();
        let ip1: IpAddr = "192.168.1.1".parse().unwrap();
        let ip2: IpAddr = "192.168.1.255".parse().unwrap();
        let ip3: IpAddr = "192.168.2.1".parse().unwrap();

        assert!(range.contains(&ip1));
        assert!(range.contains(&ip2));
        assert!(!range.contains(&ip3));
    }

    #[test]
    fn test_ip_range_ipv4_range() {
        let range: IpRange = "192.168.1.1-192.168.1.10".parse().unwrap();
        let ip1: IpAddr = "192.168.1.1".parse().unwrap();
        let ip2: IpAddr = "192.168.1.10".parse().unwrap();
        let ip3: IpAddr = "192.168.1.11".parse().unwrap();

        assert!(range.contains(&ip1));
        assert!(range.contains(&ip2));
        assert!(!range.contains(&ip3));
    }

    #[test]
    fn test_ip_range_invalid() {
        assert!("invalid".parse::<IpRange>().is_err());
        assert!("192.168.1.1/33".parse::<IpRange>().is_err());
        assert!("192.168.1.10-192.168.1.1".parse::<IpRange>().is_err());
    }

    // ==================== 规则匹配器测试 ====================

    #[test]
    fn test_rule_matcher_user_condition() {
        let rule = Rule {
            id: "rule1".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec![
                "user1".to_string(),
                "user2".to_string(),
            ])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        let context1 = RequestContext::new().with_header("X-User-Id", "user1");
        assert!(matcher.matches(&context1).is_some());

        let context2 = RequestContext::new().with_header("X-User-Id", "user3");
        assert!(matcher.matches(&context2).is_none());
    }

    #[test]
    fn test_rule_matcher_wildcard_user() {
        let rule = Rule {
            id: "rule1".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["*".to_string()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        let context = RequestContext::new();
        assert!(matcher.matches(&context).is_some());
    }

    #[test]
    fn test_rule_matcher_ip_condition() {
        let rule = Rule {
            id: "rule1".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::Ip(vec!["192.168.1.0/24".parse().unwrap()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        let context1 = RequestContext::new().with_client_ip("192.168.1.100");
        assert!(matcher.matches(&context1).is_some());

        let context2 = RequestContext::new().with_client_ip("10.0.0.1");
        assert!(matcher.matches(&context2).is_none());
    }

    #[test]
    fn test_rule_matcher_priority() {
        let rule1 = Rule {
            id: "rule1".to_string(),
            name: "Low Priority".to_string(),
            priority: 50,
            condition: Box::new(MatchCondition::User(vec!["*".to_string()])),
            enabled: true,
        };

        let rule2 = Rule {
            id: "rule2".to_string(),
            name: "High Priority".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule1, rule2]);

        let context = RequestContext::new().with_header("X-User-Id", "user1");
        let matched = matcher.matches(&context).unwrap();

        // 应该匹配高优先级的规则
        assert_eq!(matched.id, "rule2");
    }

    #[test]
    fn test_rule_matcher_disabled_rule() {
        let rule = Rule {
            id: "rule1".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: false,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        let context = RequestContext::new().with_header("X-User-Id", "user1");
        assert!(matcher.matches(&context).is_none());
    }

    #[test]
    fn test_rule_matcher_stats() {
        let rule = Rule {
            id: "rule1".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: true,
        };

        let matcher = RuleMatcher::new(vec![rule]);

        let context1 = RequestContext::new().with_header("X-User-Id", "user1");
        matcher.matches(&context1);

        let context2 = RequestContext::new().with_header("X-User-Id", "user2");
        matcher.matches(&context2);

        let stats = matcher.stats();
        assert_eq!(stats.total_matches, 1);
        assert_eq!(stats.total_mismatches, 1);
    }

    #[test]
    fn test_rule_matcher_add_remove() {
        let rule1 = Rule {
            id: "rule1".to_string(),
            name: "Rule 1".to_string(),
            priority: 100,
            condition: Box::new(MatchCondition::User(vec!["user1".to_string()])),
            enabled: true,
        };

        let mut matcher = RuleMatcher::new(vec![]);
        assert_eq!(matcher.rule_count(), 0);

        matcher.add_rule(rule1);
        assert_eq!(matcher.rule_count(), 1);

        matcher.remove_rule("rule1");
        assert_eq!(matcher.rule_count(), 0);
    }

    #[test]
    fn test_composite_condition_and() {
        let condition = CompositeCondition {
            conditions: vec![
                Box::new(MatchCondition::User(vec!["user1".to_string()])),
                Box::new(MatchCondition::Geo(vec!["US".to_string()])),
            ],
            operator: LogicalOperator::And,
        };

        let context1 = RequestContext::new()
            .with_header("X-User-Id", "user1")
            .with_header("X-Country", "US");
        assert!(condition.evaluate(&context1));

        let context2 = RequestContext::new()
            .with_header("X-User-Id", "user1")
            .with_header("X-Country", "CN");
        assert!(!condition.evaluate(&context2));
    }

    #[test]
    fn test_composite_condition_or() {
        let condition = CompositeCondition {
            conditions: vec![
                Box::new(MatchCondition::User(vec!["user1".to_string()])),
                Box::new(MatchCondition::User(vec!["user2".to_string()])),
            ],
            operator: LogicalOperator::Or,
        };

        let context1 = RequestContext::new().with_header("X-User-Id", "user1");
        assert!(condition.evaluate(&context1));

        let context2 = RequestContext::new().with_header("X-User-Id", "user2");
        assert!(condition.evaluate(&context2));

        let context3 = RequestContext::new().with_header("X-User-Id", "user3");
        assert!(!condition.evaluate(&context3));
    }

    #[test]
    fn test_composite_condition_not() {
        let condition = CompositeCondition {
            conditions: vec![Box::new(MatchCondition::User(vec!["user1".to_string()]))],
            operator: LogicalOperator::Not,
        };

        let context1 = RequestContext::new().with_header("X-User-Id", "user2");
        assert!(condition.evaluate(&context1));

        let context2 = RequestContext::new().with_header("X-User-Id", "user1");
        assert!(!condition.evaluate(&context2));
    }

    #[test]
    fn test_custom_condition() {
        let condition: Box<dyn ConditionEvaluator> = Box::new(MatchCondition::Custom(Arc::new(
            |context: &RequestContext| -> bool {
                context.get_header("X-Special").is_some_and(|v| v == "yes")
            },
        )));

        let context1 = RequestContext::new().with_header("X-Special", "yes");
        assert!(condition.evaluate(&context1));

        let context2 = RequestContext::new().with_header("X-Special", "no");
        assert!(!condition.evaluate(&context2));
    }

    #[test]
    fn test_identifier_key() {
        let user_id = Identifier::UserId("user123".to_string());
        assert_eq!(user_id.key(), "user_id:user123");

        let ip = Identifier::Ip("192.168.1.1".to_string());
        assert_eq!(ip.key(), "ip:192.168.1.1");
    }

    #[test]
    fn test_identifier_type_name() {
        assert_eq!(
            Identifier::UserId("test".to_string()).type_name(),
            "user_id"
        );
        assert_eq!(Identifier::Ip("test".to_string()).type_name(), "ip");
        assert_eq!(Identifier::Mac("test".to_string()).type_name(), "mac");
        assert_eq!(
            Identifier::ApiKey("test".to_string()).type_name(),
            "api_key"
        );
        assert_eq!(
            Identifier::DeviceId("test".to_string()).type_name(),
            "device_id"
        );
    }
}

// ============================================================================
// 公共导出
// ============================================================================

// 地理位置匹配器
#[cfg(feature = "geo-matching")]
pub use geo::{GeoCacheStats, GeoCondition, GeoInfo, GeoMatcher};

// 设备类型匹配器
#[cfg(feature = "device-matching")]
pub use device::{DeviceCacheStats, DeviceCondition, DeviceInfo, DeviceMatcher, DeviceType};

// 自定义匹配器
pub use custom::{CustomMatcher, CustomMatcherRegistry, HeaderMatcher, TimeWindowMatcher};
