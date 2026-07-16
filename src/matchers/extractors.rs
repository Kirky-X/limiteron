// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
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

    /// 解析转发头中的 IP 链（X-Forwarded-For 格式）
    ///
    /// 对于 X-Forwarded-For 格式的 IP 列表（client, proxy1, proxy2），
    /// 从右向左查找，跳过可信代理的 IP，以防止伪造攻击。
    ///
    /// # 安全说明（vuln-0003 修复）
    /// 此方法仅应在已验证直接对端（`RequestContext.client_ip`）为可信代理后调用。
    /// 调用方（`extract`）必须确保直接 TCP 对端是可信代理时才信任转发头，
    /// 否则任意客户端均可伪造 X-Forwarded-For 实施 IP 欺骗。
    ///
    /// # 参数
    /// - `value`: X-Forwarded-For 头值（IP 地址或 IP 列表字符串）
    ///
    /// # 返回
    /// - `Some(String)`: 解析后的客户端 IP 地址
    /// - `None`: 无法解析、验证失败或超过最大跳数
    fn parse_forwarded_chain(&self, value: &str) -> Option<String> {
        let ips: Vec<&str> = value
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if ips.is_empty() {
            return None;
        }

        // 安全验证: 检查 IP 数量是否超过最大跳数限制
        if ips.len() > self.trusted_proxy_config.max_hops {
            log::warn!(
                "X-Forwarded-For 包含 {} 个 IP,超过最大限制 {}",
                ips.len(),
                self.trusted_proxy_config.max_hops
            );
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

        // 多个 IP 时的处理：从右向左查找第一个非可信代理 IP
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
    }

    /// 解析直接对端 IP 地址（单个 IP）
    ///
    /// 用于解析 `RequestContext.client_ip`（直接 TCP 对端地址）。
    /// 这是最可信的 IP 来源，不受客户端头伪造影响（vuln-0003 修复）。
    /// 直接对端 IP 应为单个 IP；若意外包含逗号分隔列表，取第一个有效 IP（防御性处理）。
    ///
    /// # 参数
    /// - `value`: 直接对端 IP 字符串
    ///
    /// # 返回
    /// - `Some(String)`: 解析后的 IP 地址
    /// - `None`: 为空或验证失败
    fn parse_direct_ip(&self, value: &str) -> Option<String> {
        let ip = value.split(',').map(|s| s.trim()).find(|s| !s.is_empty())?;
        if self.validate && ip.parse::<IpAddr>().is_err() {
            return None;
        }
        Some(ip.to_string())
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
        // vuln-0003 修复：X-Forwarded-For 仅在直接对端为可信代理时才被信任。
        // 任何非可信代理的直连客户端发送的转发头必须被忽略，
        // 回退到直接 TCP 对端地址（client_ip），防止 IP 伪造攻击。
        let remote_addr = context.client_ip.as_deref();
        let peer_is_trusted_proxy = self.trusted_proxy_config.enabled
            && remote_addr
                .map(|r| self.trusted_proxy_config.is_trusted(r))
                .unwrap_or(false);

        if peer_is_trusted_proxy {
            // 直接对端是可信代理：安全地处理 X-Forwarded-For 链
            for header_name in &self.header_names {
                if let Some(value) = context.get_header(header_name) {
                    if let Some(ip) = self.parse_forwarded_chain(value) {
                        return Some(Identifier::Ip(ip));
                    }
                }
            }
        } else if !self.header_names.is_empty() {
            // 配置了转发头但对端不可信：忽略转发头以防止 IP 伪造
            if self.trusted_proxy_config.enabled {
                log::warn!(
                    target: "ip-extractor",
                    "X-Forwarded-For 头被忽略：直接连接来自 '{}'，不在可信代理列表中（vuln-0003）",
                    remote_addr.unwrap_or("unknown")
                );
            } else {
                log::warn!(
                    target: "ip-extractor",
                    "已配置转发头提取但未启用可信代理模式；为防止 IP 伪造，忽略转发头并使用直接连接 IP（vuln-0003）"
                );
            }
        }

        // 回退到直接 TCP 对端地址（始终可信）
        if let Some(client_ip) = &context.client_ip {
            if let Some(ip) = self.parse_direct_ip(client_ip) {
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

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::config::TrustedProxyConfig;
    use crate::matchers::CompositeExtractor;

    // ==================== UserIdExtractor ====================

    #[test]
    fn test_user_id_name() {
        let extractor = UserIdExtractor::from_header("X-User-Id");
        assert_eq!(extractor.name(), "UserIdExtractor");
    }

    #[test]
    fn test_user_id_with_dependencies() {
        let extractor = UserIdExtractor::with_dependencies(
            Some("X-User-Id".into()),
            Some("uid".into()),
            Some("default".into()),
        );
        let ctx = RequestContext::new().with_header("X-User-Id", "dep-user");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::UserId("dep-user".into()))
        );
    }

    #[test]
    fn test_user_id_from_header() {
        let extractor = UserIdExtractor::from_header("X-User-Id");
        let ctx = RequestContext::new().with_header("X-User-Id", "user123");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::UserId("user123".into()))
        );
    }

    #[test]
    fn test_user_id_from_query_param() {
        let extractor = UserIdExtractor::from_query_param("user_id");
        let ctx = RequestContext::new().with_query_param("user_id", "quser");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::UserId("quser".into()))
        );
    }

    #[test]
    fn test_user_id_with_default() {
        let extractor = UserIdExtractor::from_header("X-User-Id").with_default("guest");
        let ctx = RequestContext::new();
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::UserId("guest".into()))
        );
    }

    #[test]
    fn test_user_id_missing() {
        let extractor = UserIdExtractor::from_header("X-User-Id");
        let ctx = RequestContext::new();
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_user_id_header_priority() {
        let extractor =
            UserIdExtractor::new(Some("X-User-Id".into()), Some("user_id".into()), None);
        let ctx = RequestContext::new()
            .with_header("X-User-Id", "from_header")
            .with_query_param("user_id", "from_query");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::UserId("from_header".into()))
        );
    }

    #[test]
    fn test_user_id_empty_header() {
        let extractor = UserIdExtractor::from_header("X-User-Id");
        let ctx = RequestContext::new().with_header("X-User-Id", "");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_user_id_builder() {
        let extractor = UserIdExtractor::builder()
            .header_name("X-User-Id")
            .query_param_name("uid")
            .default_user_id("fallback")
            .build();
        let ctx = RequestContext::new();
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::UserId("fallback".into()))
        );
    }

    // ==================== IpExtractor ====================

    #[test]
    fn test_ip_name() {
        let extractor = IpExtractor::new_default();
        assert_eq!(extractor.name(), "IpExtractor");
    }

    #[test]
    fn test_ip_with_dependencies() {
        let extractor = IpExtractor::with_dependencies(vec!["X-Real-IP".into()], true);
        // vuln-0003: 默认配置下转发头被忽略，使用直接对端 IP
        let ctx = RequestContext::new()
            .with_header("X-Real-IP", "10.0.0.5")
            .with_client_ip("10.0.0.5");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("10.0.0.5".into()))
        );
    }

    #[test]
    fn test_ip_parse_empty_after_filter() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        let ctx = RequestContext::new().with_header("X-Forwarded-For", " , ");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_ip_parse_max_hops_exceeded() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec![],
            max_hops: 2,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".into()], true, config);
        let ctx =
            RequestContext::new().with_header("X-Forwarded-For", "10.0.0.1, 10.0.0.2, 10.0.0.3");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_ip_trusted_proxy_invalid_in_chain() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".into(), "10.0.0.2".into()],
            max_hops: 10,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".into()], true, config);
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "203.0.113.5, not-an-ip, 10.0.0.1")
            .with_client_ip("10.0.0.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("203.0.113.5".into()))
        );
    }

    #[test]
    fn test_ip_trusted_proxy_all_trusted() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".into(), "10.0.0.2".into()],
            max_hops: 10,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".into()], true, config);
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "10.0.0.1, 10.0.0.2")
            .with_client_ip("10.0.0.2");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("10.0.0.2".into()))
        );
    }

    #[test]
    fn test_ip_trusted_proxy_all_trusted_invalid_last() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".into()],
            max_hops: 10,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".into()], true, config);
        let ctx = RequestContext::new().with_header("X-Forwarded-For", "10.0.0.1, not-an-ip");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_ip_trusted_proxy_all_trusted_no_validation() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".into()],
            max_hops: 10,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".into()], false, config);
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "10.0.0.1")
            .with_client_ip("10.0.0.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("10.0.0.1".into()))
        );
    }

    #[test]
    fn test_ip_no_headers_fallback_to_client_ip_none() {
        let extractor = IpExtractor::new_default();
        let ctx = RequestContext::new();
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_ip_new_default() {
        let extractor = IpExtractor::new_default();
        let ctx = RequestContext::new().with_client_ip("10.0.0.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("10.0.0.1".into()))
        );
    }

    #[test]
    fn test_ip_from_header() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        // vuln-0003: 默认配置下转发头被忽略，使用直接对端 IP
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "203.0.113.1")
            .with_client_ip("203.0.113.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("203.0.113.1".into()))
        );
    }

    #[test]
    fn test_ip_from_headers() {
        let extractor = IpExtractor::from_headers(vec!["X-Real-IP", "X-Forwarded-For"]);
        // vuln-0003: 默认配置下转发头被忽略，使用直接对端 IP
        // 头优先级在可信代理场景测试中覆盖（test_vuln_0003_trusted_peer_*）
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "203.0.113.1")
            .with_header("X-Real-IP", "10.0.0.1")
            .with_client_ip("10.0.0.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("10.0.0.1".into()))
        );
    }

    #[test]
    fn test_ip_single() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        // vuln-0003: 默认配置下转发头被忽略，使用直接对端 IP
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "192.168.1.1")
            .with_client_ip("192.168.1.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("192.168.1.1".into()))
        );
    }

    #[test]
    fn test_ip_multi_invalid_first_ip_default() {
        let extractor = IpExtractor::builder()
            .header_name("X-Forwarded-For")
            .validate(true)
            .build();
        let ctx = RequestContext::new().with_header("X-Forwarded-For", "not-an-ip, 10.0.0.1");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_ip_multi_default_leftmost() {
        // vuln-0003: 默认配置下多 IP 转发头被忽略，使用直接对端 IP
        // （旧的最左 IP 行为不安全，已被移除）
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "192.168.1.1, 10.0.0.1, 172.16.0.1")
            .with_client_ip("198.51.100.10");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("198.51.100.10".into()))
        );
    }

    #[test]
    fn test_ip_trusted_proxies() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".into(), "172.16.0.1".into()],
            max_hops: 10,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".into()], true, config);
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "203.0.113.5, 10.0.0.1, 172.16.0.1")
            .with_client_ip("172.16.0.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("203.0.113.5".into()))
        );
    }

    #[test]
    fn test_ip_invalid_returns_none() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        let ctx = RequestContext::new().with_header("X-Forwarded-For", "not-an-ip");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_ip_no_validation() {
        let extractor = IpExtractor::builder()
            .header_name("X-Forwarded-For")
            .validate(false)
            .build();
        // vuln-0003: 默认配置下转发头被忽略；validate=false 时直接对端 IP 不做格式校验
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "not-an-ip")
            .with_client_ip("not-an-ip");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("not-an-ip".into()))
        );
    }

    #[test]
    fn test_ip_empty_header() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        let ctx = RequestContext::new().with_header("X-Forwarded-For", "");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_ip_builder() {
        let extractor = IpExtractor::builder()
            .header_name("X-Real-IP")
            .header_name("X-Forwarded-For")
            .validate(true)
            .build();
        // vuln-0003: 默认配置下转发头被忽略，使用直接对端 IP
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "203.0.113.1")
            .with_client_ip("203.0.113.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("203.0.113.1".into()))
        );
    }

    #[test]
    fn test_ip_builder_header_names() {
        let extractor = IpExtractor::builder()
            .header_names(vec!["X-Real-IP", "X-Forwarded-For"])
            .validate(true)
            .build();
        // vuln-0003: 默认配置下转发头被忽略，使用直接对端 IP
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "10.0.0.1")
            .with_client_ip("10.0.0.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("10.0.0.1".into()))
        );
    }

    #[test]
    fn test_ip_builder_with_trusted_proxy_config() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".into()],
            max_hops: 10,
        };
        let extractor = IpExtractor::builder()
            .header_name("X-Forwarded-For")
            .validate(true)
            .trusted_proxy_config(config)
            .build();
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "203.0.113.5, 10.0.0.1")
            .with_client_ip("10.0.0.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("203.0.113.5".into()))
        );
    }

    // ========================================================================
    // vuln-0003 修复测试：X-Forwarded-For IP 伪造防护
    //
    // 核心安全模型：X-Forwarded-For 仅在直接连接来自可信代理时才被信任。
    // 任何非可信代理的直连客户端发送的 X-Forwarded-For 必须被忽略，
    // 回退到直接 TCP 对端地址（client_ip）。
    // ========================================================================

    /// vuln-0003: 启用可信代理模式但直接对端不在可信代理列表时，
    /// 必须忽略 X-Forwarded-For 并使用直接对端 IP。
    ///
    /// 攻击场景：攻击者直连服务器（client_ip=203.0.113.99），
    /// 发送伪造的 X-Forwarded-For: 1.2.3.4, 10.0.0.1 企图冒充可信代理链。
    /// 修复前：返回 "1.2.3.4"（伪造成功）。
    /// 修复后：返回 "203.0.113.99"（直接对端，忽略伪造头）。
    #[test]
    fn test_vuln_0003_untrusted_peer_ignores_x_forwarded_for() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".into()],
            max_hops: 10,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".into()], true, config);
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "1.2.3.4, 10.0.0.1")
            .with_client_ip("203.0.113.99");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("203.0.113.99".into())),
            "非可信代理直连时必须忽略 X-Forwarded-For，使用直接对端 IP"
        );
    }

    /// vuln-0003: 启用可信代理模式且直接对端是可信代理时，
    /// 正常处理 X-Forwarded-For 链（从右向左跳过可信代理）。
    #[test]
    fn test_vuln_0003_trusted_peer_processes_x_forwarded_for() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".into()],
            max_hops: 10,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".into()], true, config);
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "203.0.113.5, 10.0.0.1")
            .with_client_ip("10.0.0.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("203.0.113.5".into())),
            "可信代理直连时应正常解析 X-Forwarded-For 链"
        );
    }

    /// vuln-0003: 未启用可信代理模式时（默认配置），
    /// 必须忽略客户端可控的 X-Forwarded-For 头，使用直接对端 IP。
    ///
    /// 攻击场景：服务器未配置可信代理，攻击者发送 X-Forwarded-For: 1.2.3.4。
    /// 修复前：返回 "1.2.3.4"（使用最左 IP，可伪造）。
    /// 修复后：返回 "5.6.7.8"（直接对端，忽略伪造头）。
    #[test]
    fn test_vuln_0003_disabled_proxy_ignores_headers_uses_client_ip() {
        let extractor = IpExtractor::from_header("X-Forwarded-For");
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "1.2.3.4")
            .with_client_ip("5.6.7.8");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("5.6.7.8".into())),
            "未启用可信代理时必须忽略 X-Forwarded-For，使用直接对端 IP"
        );
    }

    /// vuln-0003: 攻击者伪造完整代理链（含可信代理 IP）也应被拒绝。
    ///
    /// 攻击场景：攻击者直连（client_ip=8.8.8.8），发送
    /// X-Forwarded-For: 10.0.0.1, 1.2.3.4 企图冒充经可信代理转发。
    /// 修复前：返回 "1.2.3.4"（从右向左跳过可信 10.0.0.1）。
    /// 修复后：返回 "8.8.8.8"（直接对端非可信代理，忽略整个头）。
    #[test]
    fn test_vuln_0003_forged_chain_from_untrusted_peer_rejected() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".into()],
            max_hops: 10,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".into()], true, config);
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "10.0.0.1, 1.2.3.4")
            .with_client_ip("8.8.8.8");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("8.8.8.8".into())),
            "非可信代理直连时伪造的代理链必须被拒绝"
        );
    }

    /// vuln-0003: 缺少直接对端地址时，无法验证代理链，必须忽略 X-Forwarded-For。
    ///
    /// 场景：RequestContext 未设置 client_ip（无 socket 信息）。
    /// 修复前：返回 "1.2.3.4"（处理头）。
    /// 修复后：返回 None（无法验证对端，不信任头）。
    #[test]
    fn test_vuln_0003_no_client_ip_ignores_headers() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".into()],
            max_hops: 10,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".into()], true, config);
        let ctx = RequestContext::new().with_header("X-Forwarded-For", "1.2.3.4");
        assert_eq!(
            extractor.extract(&ctx),
            None,
            "缺少直接对端地址时必须忽略 X-Forwarded-For"
        );
    }

    /// vuln-0003: 可信代理直连时，单个 IP 的 X-Forwarded-For 正常处理。
    #[test]
    fn test_vuln_0003_trusted_peer_single_ip_header() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".into()],
            max_hops: 10,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".into()], true, config);
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "203.0.113.50")
            .with_client_ip("10.0.0.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("203.0.113.50".into())),
            "可信代理直连时单个 IP 头应正常解析"
        );
    }

    /// vuln-0003: CIDR 形式的可信代理配置，对端在 CIDR 范围内时信任 X-Forwarded-For。
    #[test]
    fn test_vuln_0003_trusted_peer_cidr_match() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.0/8".into()],
            max_hops: 10,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".into()], true, config);
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "203.0.113.5, 10.0.0.1")
            .with_client_ip("10.255.0.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("203.0.113.5".into())),
            "对端在 CIDR 可信代理范围内时应解析 X-Forwarded-For"
        );
    }

    /// vuln-0003: 可信代理直连时，X-Forwarded-For 全为可信代理的场景。
    #[test]
    fn test_vuln_0003_trusted_peer_all_proxies_in_chain() {
        let config = TrustedProxyConfig {
            enabled: true,
            proxies: vec!["10.0.0.1".into(), "10.0.0.2".into()],
            max_hops: 10,
        };
        let extractor =
            IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".into()], true, config);
        let ctx = RequestContext::new()
            .with_header("X-Forwarded-For", "10.0.0.1, 10.0.0.2")
            .with_client_ip("10.0.0.2");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("10.0.0.2".into())),
            "可信代理直连且链全为可信代理时，使用最右 IP"
        );
    }

    // ==================== MacExtractor ====================

    #[test]
    fn test_mac_name() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        assert_eq!(extractor.name(), "MacExtractor");
    }

    #[test]
    fn test_mac_with_dependencies() {
        let extractor =
            MacExtractor::with_dependencies(Some("X-Mac-Address".into()), Some("mac".into()), true);
        let ctx = RequestContext::new().with_header("X-Mac-Address", "AA:BB:CC:DD:EE:FF");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Mac("AA:BB:CC:DD:EE:FF".into()))
        );
    }

    #[test]
    fn test_mac_query_param_invalid_value() {
        let extractor = MacExtractor::new(None, Some("mac".into()), true);
        let ctx = RequestContext::new().with_query_param("mac", "GG:GG:GG:GG:GG:GG");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_mac_query_param_empty_value() {
        let extractor = MacExtractor::new(None, Some("mac".into()), true);
        let ctx = RequestContext::new().with_query_param("mac", "");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_mac_query_param_not_present() {
        let extractor = MacExtractor::new(None, Some("mac".into()), true);
        let ctx = RequestContext::new();
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_mac_query_param_priority() {
        let extractor = MacExtractor::new(Some("X-Mac-Address".into()), Some("mac".into()), true);
        let ctx = RequestContext::new()
            .with_header("X-Mac-Address", "11:22:33:44:55:66")
            .with_query_param("mac", "AA:BB:CC:DD:EE:FF");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Mac("11:22:33:44:55:66".into()))
        );
    }

    #[test]
    fn test_mac_without_validation() {
        let extractor = MacExtractor::new(Some("X-Mac-Address".into()), None, false);
        let ctx = RequestContext::new().with_header("X-Mac-Address", "invalid-mac-value");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Mac("invalid-mac-value".into()))
        );
    }

    #[test]
    fn test_mac_short_hex() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let ctx = RequestContext::new().with_header("X-Mac-Address", "AABB");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_mac_from_header() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let ctx = RequestContext::new().with_header("X-Mac-Address", "00:1A:2B:3C:4D:5E");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Mac("00:1A:2B:3C:4D:5E".into()))
        );
    }

    #[test]
    fn test_mac_from_query_param() {
        let extractor = MacExtractor::from_query_param("mac");
        let ctx = RequestContext::new().with_query_param("mac", "00:1A:2B:3C:4D:5E");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Mac("00:1A:2B:3C:4D:5E".into()))
        );
    }

    #[test]
    fn test_mac_colon_format() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let ctx = RequestContext::new().with_header("X-Mac-Address", "AA:BB:CC:DD:EE:FF");
        assert!(extractor.extract(&ctx).is_some());
    }

    #[test]
    fn test_mac_hyphen_format() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let ctx = RequestContext::new().with_header("X-Mac-Address", "AA-BB-CC-DD-EE-FF");
        assert!(extractor.extract(&ctx).is_some());
    }

    #[test]
    fn test_mac_dot_format() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let ctx = RequestContext::new().with_header("X-Mac-Address", "AABB.CCDD.EEFF");
        assert!(extractor.extract(&ctx).is_some());
    }

    #[test]
    fn test_mac_plain_hex() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let ctx = RequestContext::new().with_header("X-Mac-Address", "AABBCCDDEEFF");
        assert!(extractor.extract(&ctx).is_some());
    }

    #[test]
    fn test_mac_invalid() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let ctx = RequestContext::new().with_header("X-Mac-Address", "GG:1A:2B:3C:4D:5E");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_mac_empty() {
        let extractor = MacExtractor::from_header("X-Mac-Address");
        let ctx = RequestContext::new().with_header("X-Mac-Address", "");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_mac_builder() {
        let extractor = MacExtractor::builder()
            .header_name("X-Mac-Address")
            .query_param_name("mac_addr")
            .validate(true)
            .build();
        let ctx = RequestContext::new().with_header("X-Mac-Address", "00:1A:2B:3C:4D:5E");
        assert!(extractor.extract(&ctx).is_some());
    }

    // ==================== ApiKeyExtractor ====================

    #[test]
    fn test_api_key_name() {
        let extractor = ApiKeyExtractor::from_authorization_header();
        assert_eq!(extractor.name(), "ApiKeyExtractor");
    }

    #[test]
    fn test_api_key_with_dependencies() {
        let extractor = ApiKeyExtractor::with_dependencies(
            Some("Authorization".into()),
            None,
            Some("Bearer ".into()),
        );
        let ctx = RequestContext::new().with_header("Authorization", "Bearer token-123");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::ApiKey("token-123".into()))
        );
    }

    #[test]
    fn test_api_key_from_authorization() {
        let extractor = ApiKeyExtractor::from_authorization_header();
        let ctx = RequestContext::new().with_header("Authorization", "Bearer sk-12345");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::ApiKey("sk-12345".into()))
        );
    }

    #[test]
    fn test_api_key_from_header() {
        let extractor = ApiKeyExtractor::from_header("X-API-Key");
        let ctx = RequestContext::new().with_header("X-API-Key", "my-api-key");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::ApiKey("my-api-key".into()))
        );
    }

    #[test]
    fn test_api_key_from_query_param() {
        let extractor = ApiKeyExtractor::from_query_param("api_key");
        // query param is disabled for security, but constructor still accepts it
        // No header configured → always returns None even if query param matches
        let ctx = RequestContext::new().with_query_param("api_key", "key-from-query");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_api_key_bearer_stripping() {
        let extractor = ApiKeyExtractor::from_authorization_header();
        let ctx = RequestContext::new().with_header("Authorization", "Bearer   sk-999  ");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::ApiKey("sk-999".into()))
        );
    }

    #[test]
    fn test_api_key_no_prefix() {
        let extractor = ApiKeyExtractor::from_header("X-API-Key");
        let ctx = RequestContext::new().with_header("X-API-Key", "raw-key-value");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::ApiKey("raw-key-value".into()))
        );
    }

    #[test]
    fn test_api_key_empty_after_prefix() {
        let extractor = ApiKeyExtractor::from_authorization_header();
        let ctx = RequestContext::new().with_header("Authorization", "Bearer ");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_api_key_missing_prefix() {
        let extractor = ApiKeyExtractor::from_authorization_header();
        let ctx = RequestContext::new().with_header("Authorization", "naked-key");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_api_key_empty_header() {
        let extractor = ApiKeyExtractor::from_header("X-API-Key");
        let ctx = RequestContext::new().with_header("X-API-Key", "");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_api_key_builder() {
        let extractor = ApiKeyExtractor::builder()
            .header_name("Authorization")
            .prefix("Bearer ")
            .build();
        let ctx = RequestContext::new().with_header("Authorization", "Bearer builder-key");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::ApiKey("builder-key".into()))
        );
    }

    // ==================== CompositeExtractor ====================

    #[test]
    fn test_composite_first_match() {
        let extractor = CompositeExtractor::new(
            vec![
                Box::new(UserIdExtractor::from_header("X-User-Id")),
                Box::new(IpExtractor::new_default()),
            ],
            false,
        );
        let ctx = RequestContext::new()
            .with_header("X-User-Id", "user99")
            .with_client_ip("10.0.0.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::UserId("user99".into()))
        );
    }

    #[test]
    fn test_composite_priority() {
        let extractor = CompositeExtractor::new(
            vec![
                Box::new(UserIdExtractor::from_header("X-User-Id")),
                Box::new(ApiKeyExtractor::from_header("X-API-Key")),
            ],
            false,
        );
        let ctx = RequestContext::new()
            .with_header("X-API-Key", "key-val")
            .with_client_ip("10.0.0.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::ApiKey("key-val".into()))
        );
    }

    // ==================== CustomExtractor ====================

    #[test]
    fn test_custom_name() {
        let extractor = CustomExtractor::new("MyCustom", |_| None);
        assert_eq!(extractor.name(), "MyCustom");
    }

    #[test]
    fn test_custom_closure() {
        let extractor = CustomExtractor::new("MyExt", |ctx| {
            ctx.get_header("X-Custom")
                .map(|v| Identifier::UserId(v.clone()))
        });
        let ctx = RequestContext::new().with_header("X-Custom", "custom-val");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::UserId("custom-val".into()))
        );
    }

    #[test]
    fn test_custom_returns_none() {
        let extractor = CustomExtractor::new("Empty", |_| None);
        let ctx = RequestContext::new();
        assert_eq!(extractor.extract(&ctx), None);
    }

    // ==================== DeviceIdExtractor ====================

    #[test]
    fn test_device_id_name() {
        let extractor = DeviceIdExtractor::from_header("X-Device-Id");
        assert_eq!(extractor.name(), "DeviceIdExtractor");
    }

    #[test]
    fn test_device_id_with_dependencies() {
        let extractor =
            DeviceIdExtractor::with_dependencies(Some("X-Device-Id".into()), Some("did".into()));
        let ctx = RequestContext::new().with_header("X-Device-Id", "dep-device");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::DeviceId("dep-device".into()))
        );
    }

    #[test]
    fn test_device_id_empty_query_param_fallback() {
        let extractor =
            DeviceIdExtractor::new(Some("X-Device-Id".into()), Some("device_id".into()));
        let ctx = RequestContext::new().with_header("X-Device-Id", "from-header");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::DeviceId("from-header".into()))
        );
    }

    #[test]
    fn test_device_id_query_param_empty() {
        let extractor = DeviceIdExtractor::new(None, Some("device_id".into()));
        let ctx = RequestContext::new().with_query_param("device_id", "");
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_device_id_query_param_not_present() {
        let extractor = DeviceIdExtractor::new(None, Some("device_id".into()));
        let ctx = RequestContext::new();
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_device_id_from_header() {
        let extractor = DeviceIdExtractor::from_header("X-Device-Id");
        let ctx = RequestContext::new().with_header("X-Device-Id", "device-abc");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::DeviceId("device-abc".into()))
        );
    }

    #[test]
    fn test_device_id_from_query_param() {
        let extractor = DeviceIdExtractor::from_query_param("device_id");
        let ctx = RequestContext::new().with_query_param("device_id", "dev-xyz");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::DeviceId("dev-xyz".into()))
        );
    }

    #[test]
    fn test_device_id_missing() {
        let extractor = DeviceIdExtractor::from_header("X-Device-Id");
        let ctx = RequestContext::new();
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_device_id_builder() {
        let extractor = DeviceIdExtractor::builder()
            .header_name("X-Device-Id")
            .query_param_name("did")
            .build();
        let ctx = RequestContext::new().with_header("X-Device-Id", "built-device");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::DeviceId("built-device".into()))
        );
    }

    // ==================== Builder edge cases ====================

    #[test]
    fn test_user_id_builder_empty() {
        let extractor = UserIdExtractor::builder().build();
        let ctx = RequestContext::new();
        assert_eq!(extractor.extract(&ctx), None);
    }

    #[test]
    fn test_ip_builder_empty_headers() {
        let extractor = IpExtractor::builder().validate(true).build();
        let ctx = RequestContext::new().with_client_ip("10.0.0.1");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Ip("10.0.0.1".into()))
        );
    }

    #[test]
    fn test_mac_builder_validate_off() {
        let extractor = MacExtractor::builder()
            .header_name("X-Mac-Address")
            .validate(false)
            .build();
        let ctx = RequestContext::new().with_header("X-Mac-Address", "invalid-mac");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::Mac("invalid-mac".into()))
        );
    }

    #[test]
    fn test_api_key_builder_no_prefix() {
        let extractor = ApiKeyExtractor::builder().header_name("X-API-Key").build();
        let ctx = RequestContext::new().with_header("X-API-Key", "plain-key");
        assert_eq!(
            extractor.extract(&ctx),
            Some(Identifier::ApiKey("plain-key".into()))
        );
    }
}
