//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 标识符提取器实现
//!
//! 包含各种类型的提取器：UserIdExtractor, IpExtractor, MacExtractor,
//! ApiKeyExtractor, DeviceIdExtractor, CustomExtractor

use super::traits::{Identifier, IdentifierExtractor, RequestContext};
use crate::config::TrustedProxyConfig;
use std::net::IpAddr;

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
    /// 创建新的用户ID提取器（保持向后兼容）
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

    /// 创建设置器（Builder模式）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::UserIdExtractor;
    ///
    /// let extractor = UserIdExtractor::builder()
    ///     .header_name("X-User-Id")
    ///     .query_param_name("user_id")
    ///     .default_user_id("guest")
    ///     .build();
    /// ```
    pub fn builder() -> UserIdExtractorBuilder {
        UserIdExtractorBuilder::new()
    }

    /// 使用依赖注入创建（完整依赖模式）
    ///
    /// 对于UserIdExtractor，无需外部依赖，此方法主要用于API一致性
    ///
    /// # 参数
    /// - `header_name`: HTTP头名称（可选）
    /// - `query_param_name`: 查询参数名称（可选）
    /// - `default_user_id`: 默认用户ID（可选）
    pub fn with_dependencies(
        header_name: Option<String>,
        query_param_name: Option<String>,
        default_user_id: Option<String>,
    ) -> Self {
        Self::new(header_name, query_param_name, default_user_id)
    }
}

/// 用户ID提取器设置器
#[derive(Debug, Clone, Default)]
pub struct UserIdExtractorBuilder {
    header_name: Option<String>,
    query_param_name: Option<String>,
    default_user_id: Option<String>,
}

impl UserIdExtractorBuilder {
    /// 创建新的设置器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置HTTP头名称
    pub fn header_name(mut self, header_name: &str) -> Self {
        self.header_name = Some(header_name.to_string());
        self
    }

    /// 设置查询参数名称
    pub fn query_param_name(mut self, query_param_name: &str) -> Self {
        self.query_param_name = Some(query_param_name.to_string());
        self
    }

    /// 设置默认用户ID
    pub fn default_user_id(mut self, default_user_id: &str) -> Self {
        self.default_user_id = Some(default_user_id.to_string());
        self
    }

    /// 构建UserIdExtractor
    pub fn build(self) -> UserIdExtractor {
        UserIdExtractor::new(
            self.header_name,
            self.query_param_name,
            self.default_user_id,
        )
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
    /// 可信代理配置
    trusted_proxy_config: TrustedProxyConfig,
}

impl IpExtractor {
    /// 创建新的IP提取器（保持向后兼容）
    ///
    /// # 参数
    /// - `header_names`: HTTP头名称列表（按优先级顺序）
    /// - `validate`: 是否验证IP格式
    pub fn new(header_names: Vec<String>, validate: bool) -> Self {
        Self {
            header_names,
            validate,
            trusted_proxy_config: TrustedProxyConfig::default(),
        }
    }

    /// 创建带可信代理配置的IP提取器
    ///
    /// # 参数
    /// - `header_names`: HTTP头名称列表（按优先级顺序）
    /// - `validate`: 是否验证IP格式
    /// - `trusted_proxy_config`: 可信代理配置
    pub fn with_trusted_proxies(
        header_names: Vec<String>,
        validate: bool,
        trusted_proxy_config: TrustedProxyConfig,
    ) -> Self {
        Self {
            header_names,
            validate,
            trusted_proxy_config,
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

    /// 创建设置器（Builder模式）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::IpExtractor;
    ///
    /// let extractor = IpExtractor::builder()
    ///     .header_names(vec!["X-Real-IP", "X-Forwarded-For"])
    ///     .validate(true)
    ///     .build();
    /// ```
    pub fn builder() -> IpExtractorBuilder {
        IpExtractorBuilder::new()
    }

    /// 使用依赖注入创建（完整依赖模式）
    ///
    /// 对于IpExtractor，无需外部依赖，此方法主要用于API一致性
    ///
    /// # 参数
    /// - `header_names`: HTTP头名称列表（按优先级顺序）
    /// - `validate`: 是否验证IP格式
    pub fn with_dependencies(header_names: Vec<String>, validate: bool) -> Self {
        Self::new(header_names, validate)
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

        // 多个 IP 时的处理
        if self.trusted_proxy_config.enabled {
            // 从右向左查找第一个非可信代理 IP
            for ip_str in ips.iter().rev() {
                if self.validate && ip_str.parse::<IpAddr>().is_err() {
                    continue;
                }
                if !self.trusted_proxy_config.is_trusted(ip_str) {
                    return Some(ip_str.to_string());
                }
            }
            // 如果全是可信代理，使用最右边的 IP
            if let Some(&ip) = ips.last() {
                if self.validate && ip.parse::<IpAddr>().is_err() {
                    return None;
                }
                return Some(ip.to_string());
            }
            None
        } else {
            // 未启用可信代理模式：使用最左边的 IP（保持向后兼容）
            let ip = ips[0];
            if self.validate && ip.parse::<IpAddr>().is_err() {
                return None;
            }
            Some(ip.to_string())
        }
    }
}

/// IP提取器设置器
#[derive(Debug, Clone, Default)]
pub struct IpExtractorBuilder {
    header_names: Vec<String>,
    validate: bool,
    trusted_proxy_config: Option<TrustedProxyConfig>,
}

impl IpExtractorBuilder {
    /// 创建新的设置器
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加HTTP头名称
    pub fn header_name(mut self, header_name: &str) -> Self {
        self.header_names.push(header_name.to_string());
        self
    }

    /// 设置HTTP头名称列表
    pub fn header_names(mut self, header_names: Vec<&str>) -> Self {
        self.header_names = header_names.iter().map(|s| s.to_string()).collect();
        self
    }

    /// 设置是否验证IP格式
    pub fn validate(mut self, validate: bool) -> Self {
        self.validate = validate;
        self
    }

    /// 设置可信代理配置
    pub fn trusted_proxy_config(mut self, config: TrustedProxyConfig) -> Self {
        self.trusted_proxy_config = Some(config);
        self
    }

    /// 构建IpExtractor
    pub fn build(self) -> IpExtractor {
        match self.trusted_proxy_config {
            Some(config) => {
                IpExtractor::with_trusted_proxies(self.header_names, self.validate, config)
            }
            None => IpExtractor::new(self.header_names, self.validate),
        }
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
    /// 创建新的MAC提取器（保持向后兼容）
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

    /// 创建设置器（Builder模式）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::MacExtractor;
    ///
    /// let extractor = MacExtractor::builder()
    ///     .header_name("X-Mac-Address")
    ///     .query_param_name("mac")
    ///     .validate(true)
    ///     .build();
    /// ```
    pub fn builder() -> MacExtractorBuilder {
        MacExtractorBuilder::new()
    }

    /// 使用依赖注入创建（完整依赖模式）
    ///
    /// 对于MacExtractor，无需外部依赖，此方法主要用于API一致性
    ///
    /// # 参数
    /// - `header_name`: HTTP头名称
    /// - `query_param_name`: 查询参数名称
    /// - `validate`: 是否验证MAC格式
    pub fn with_dependencies(
        header_name: Option<String>,
        query_param_name: Option<String>,
        validate: bool,
    ) -> Self {
        Self::new(header_name, query_param_name, validate)
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

/// MAC提取器设置器
#[derive(Debug, Clone, Default)]
pub struct MacExtractorBuilder {
    header_name: Option<String>,
    query_param_name: Option<String>,
    validate: bool,
}

impl MacExtractorBuilder {
    /// 创建新的设置器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置HTTP头名称
    pub fn header_name(mut self, header_name: &str) -> Self {
        self.header_name = Some(header_name.to_string());
        self
    }

    /// 设置查询参数名称
    pub fn query_param_name(mut self, query_param_name: &str) -> Self {
        self.query_param_name = Some(query_param_name.to_string());
        self
    }

    /// 设置是否验证MAC格式
    pub fn validate(mut self, validate: bool) -> Self {
        self.validate = validate;
        self
    }

    /// 构建MacExtractor
    pub fn build(self) -> MacExtractor {
        MacExtractor::new(self.header_name, self.query_param_name, self.validate)
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
    /// 创建新的API密钥提取器（保持向后兼容）
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
            log::warn!("出于安全考虑，通过查询参数提取API Key已被禁用");
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

    /// 创建设置器（Builder模式）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::ApiKeyExtractor;
    ///
    /// let extractor = ApiKeyExtractor::builder()
    ///     .header_name("X-API-Key")
    ///     .prefix("Bearer ")
    ///     .build();
    /// ```
    pub fn builder() -> ApiKeyExtractorBuilder {
        ApiKeyExtractorBuilder::new()
    }

    /// 使用依赖注入创建（完整依赖模式）
    ///
    /// 对于ApiKeyExtractor，无需外部依赖，此方法主要用于API一致性
    ///
    /// # 参数
    /// - `header_name`: HTTP头名称
    /// - `query_param_name`: 查询参数名称（已禁用）
    /// - `prefix`: 前缀
    pub fn with_dependencies(
        header_name: Option<String>,
        query_param_name: Option<String>,
        prefix: Option<String>,
    ) -> Self {
        Self::new(header_name, query_param_name, prefix)
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

/// API密钥提取器设置器
#[derive(Debug, Clone, Default)]
pub struct ApiKeyExtractorBuilder {
    header_name: Option<String>,
    prefix: Option<String>,
}

impl ApiKeyExtractorBuilder {
    /// 创建新的设置器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置HTTP头名称
    pub fn header_name(mut self, header_name: &str) -> Self {
        self.header_name = Some(header_name.to_string());
        self
    }

    /// 设置前缀
    pub fn prefix(mut self, prefix: &str) -> Self {
        self.prefix = Some(prefix.to_string());
        self
    }

    /// 构建ApiKeyExtractor
    pub fn build(self) -> ApiKeyExtractor {
        ApiKeyExtractor::new(self.header_name, None, self.prefix)
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
    /// 创建新的设备ID提取器（保持向后兼容）
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

    /// 创建设置器（Builder模式）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::matchers::DeviceIdExtractor;
    ///
    /// let extractor = DeviceIdExtractor::builder()
    ///     .header_name("X-Device-Id")
    ///     .query_param_name("device_id")
    ///     .build();
    /// ```
    pub fn builder() -> DeviceIdExtractorBuilder {
        DeviceIdExtractorBuilder::new()
    }

    /// 使用依赖注入创建（完整依赖模式）
    ///
    /// 对于DeviceIdExtractor，无需外部依赖，此方法主要用于API一致性
    ///
    /// # 参数
    /// - `header_name`: HTTP头名称
    /// - `query_param_name`: 查询参数名称
    pub fn with_dependencies(
        header_name: Option<String>,
        query_param_name: Option<String>,
    ) -> Self {
        Self::new(header_name, query_param_name)
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

/// 设备ID提取器设置器
#[derive(Debug, Clone, Default)]
pub struct DeviceIdExtractorBuilder {
    header_name: Option<String>,
    query_param_name: Option<String>,
}

impl DeviceIdExtractorBuilder {
    /// 创建新的设置器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置HTTP头名称
    pub fn header_name(mut self, header_name: &str) -> Self {
        self.header_name = Some(header_name.to_string());
        self
    }

    /// 设置查询参数名称
    pub fn query_param_name(mut self, query_param_name: &str) -> Self {
        self.query_param_name = Some(query_param_name.to_string());
        self
    }

    /// 构建DeviceIdExtractor
    pub fn build(self) -> DeviceIdExtractor {
        DeviceIdExtractor::new(self.header_name, self.query_param_name)
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
