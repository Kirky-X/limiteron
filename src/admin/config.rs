// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Admin API configuration

use ahash::AHashMap as HashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// API key 最小长度（字节）
///
/// 凭证强度策略的唯一真源：`validate()`、路由鉴权（`routes::create_router`）
/// 与 `is_valid_api_key` 共用同一阈值。
pub(crate) const MIN_API_KEY_LEN: usize = 16;

/// 角色映射中的 key 是否可作为有效凭证
///
/// 与主 `api_key` 同口径：非空且长度 ≥ [`MIN_API_KEY_LEN`]。
/// 过短的 key 是可穷举的活凭证（映射为角色后即获管理面访问权），
/// 一律拒绝而非仅告警。
pub(crate) fn role_key_is_acceptable(key: &str) -> bool {
    !key.trim().is_empty() && key.len() >= MIN_API_KEY_LEN
}

/// Admin API configuration validation error
///
/// 双轨：Display 英文规范串 + [`crate::i18n::LocalizedMsg`]（键
/// `admin-config-*`，已英文错误对齐补键）。
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("API key is required when admin API is enabled")]
    ApiKeyRequired,
    #[error("API key must be at least 16 characters, got {0}")]
    ApiKeyTooShort(usize),
}

impl crate::i18n::LocalizedMsg for ConfigError {
    fn message_key(&self) -> &'static str {
        match self {
            ConfigError::ApiKeyRequired => "admin-config-api-key-required",
            ConfigError::ApiKeyTooShort(_) => "admin-config-api-key-too-short",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            ConfigError::ApiKeyRequired => vec![],
            ConfigError::ApiKeyTooShort(length) => vec![("length", length.to_string())],
        }
    }
}

/// 管理面角色
///
/// - `Admin`：读写全部端点（封禁增删、配额修改、热更新等）
/// - `Viewer`：只读端点（status / circuit-breaker / 探针自省查询等）
///
/// 越权（角色不足以访问目标端点）返回 403，与凭证无效的 401 区分。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdminRole {
    /// 管理员：全部端点
    Admin,
    /// 只读观察者：仅 GET/HEAD 端点
    Viewer,
}

impl AdminRole {
    /// 角色名（诊断/日志用）
    pub fn as_str(&self) -> &'static str {
        match self {
            AdminRole::Admin => "admin",
            AdminRole::Viewer => "viewer",
        }
    }

    /// 是否允许访问只读请求（GET/HEAD）
    pub fn allows_read(&self) -> bool {
        true
    }

    /// 是否允许访问写请求（POST/PUT/DELETE 等）
    pub fn allows_write(&self) -> bool {
        matches!(self, AdminRole::Admin)
    }
}

/// Admin API configuration
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminApiConfig {
    /// Listening host
    #[serde(default = "default_host")]
    pub host: String,
    /// Listening port
    #[serde(default = "default_port")]
    pub port: u16,
    /// API Key for authentication (required when enabled)
    pub api_key: String,
    /// Whether to enable the admin API
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// API key → operator identity mapping
    ///
    /// 当管理员通过不同 API key 鉴权时，绑定到对应的 operator 身份，
    /// 防止客户端在 JSON body 中伪造 `operator` 字段。mapping 为空时
    /// 回退到默认 `"admin-api"` 并记录 warn 日志（向后兼容）。
    #[serde(default)]
    pub api_key_operators: HashMap<String, String>,
    /// API key → 角色映射（RBAC）
    ///
    /// 配置后，鉴权中间件按 key 解析角色并执行端点级授权：
    /// viewer 访问写端点返回 403。**未在此映射中的合法 key**（含主
    /// `api_key`）默认 `Admin`，保持既有单 key 部署的行为逐位一致。
    #[serde(default)]
    pub api_key_roles: HashMap<String, AdminRole>,
    /// 按路径分组的速率限制配置
    ///
    /// key = 分组名（"ban" / "quota" / "metrics" / "default"），value = (max_requests, window_secs)。
    /// middleware 在鉴权之前检查速率限制，超限返回 429 + Retry-After。
    /// 默认：ban=(100,60), quota=(50,60), metrics=(600,60), default=(200,60)。
    /// 配置只覆盖部分分组时，未覆盖的分组回退到同表默认额度
    /// （见 `default_rate_limit_for`），不会因缺项而放宽限流。
    #[serde(default = "default_rate_limits")]
    pub rate_limits: HashMap<String, (u64, u64)>,
}

// 手动实现 Debug 并脱敏 api_key：派生的 Debug 会以明文打印
// `api_key`，误用 `println!("{config:?}")` 即泄露管理凭证
impl std::fmt::Debug for AdminApiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 两张映射表的 key 即 API key 本身（可直接登录管理面的活凭证），
        // Debug 只输出操作者/角色分布，绝不输出 key 原文。
        let operators: Vec<&str> = self
            .api_key_operators
            .values()
            .map(String::as_str)
            .collect();
        let roles: Vec<&str> = self.api_key_roles.values().map(|r| r.as_str()).collect();
        f.debug_struct("AdminApiConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field(
                "api_key",
                &format!("<redacted; {} chars>", self.api_key.chars().count()),
            )
            .field("enabled", &self.enabled)
            .field("api_key_operators", &operators)
            .field("api_key_roles", &roles)
            .field("rate_limits", &self.rate_limits)
            .finish()
    }
}

/// 分组默认额度（单一事实源）
///
/// 配置默认 map（[`default_rate_limits`]）与"分组缺席配置时的回退"
/// （`routes::create_router`）共用此表，二者不会漂移。
pub(crate) fn default_rate_limit_for(group: &str) -> (u64, u64) {
    match group {
        "ban" => (100, 60),
        "quota" => (50, 60),
        "metrics" => (600, 60),
        _ => (200, 60),
    }
}

/// 默认速率限制配置：ban=100/min, quota=50/min, metrics=600/min, default=200/min
fn default_rate_limits() -> HashMap<String, (u64, u64)> {
    let mut m = HashMap::new();
    for group in ["ban", "quota", "metrics", "default"] {
        m.insert(group.to_string(), default_rate_limit_for(group));
    }
    m
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    9090
}

fn default_enabled() -> bool {
    false
}

impl Default for AdminApiConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            api_key: String::new(),
            enabled: default_enabled(),
            api_key_operators: HashMap::new(),
            api_key_roles: HashMap::new(),
            rate_limits: default_rate_limits(),
        }
    }
}

impl AdminApiConfig {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            api_key: api_key.into(),
            enabled: true,
            api_key_operators: HashMap::new(),
            api_key_roles: HashMap::new(),
            rate_limits: default_rate_limits(),
        }
    }

    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = api_key.into();
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 添加 API key → operator 映射
    pub fn with_api_key_operator(
        mut self,
        api_key: impl Into<String>,
        operator: impl Into<String>,
    ) -> Self {
        self.api_key_operators
            .insert(api_key.into(), operator.into());
        self
    }

    /// 批量设置 API key → operator 映射
    pub fn with_api_key_operators(mut self, mapping: HashMap<String, String>) -> Self {
        self.api_key_operators = mapping;
        self
    }

    /// 添加 API key → 角色映射（RBAC）
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::admin::{AdminApiConfig, AdminRole};
    ///
    /// let config = AdminApiConfig::new("primary-key-16chars!!!")
    ///     .with_api_key_role("viewer-key-16chars!!", AdminRole::Viewer);
    /// ```
    pub fn with_api_key_role(mut self, api_key: impl Into<String>, role: AdminRole) -> Self {
        self.api_key_roles.insert(api_key.into(), role);
        self
    }

    /// 解析 API key 的角色
    ///
    /// 未在映射中的 key 默认 `Admin`——保持既有单 key 部署（主 key 即
    /// 管理员）的行为不变。
    pub fn role_for_api_key(&self, api_key: &str) -> AdminRole {
        self.api_key_roles
            .get(api_key)
            .copied()
            .unwrap_or(AdminRole::Admin)
    }

    /// 主 `api_key` 是否可作为有效凭证
    ///
    /// 与 [`Self::validate`] 同口径：非空且长度 ≥ [`MIN_API_KEY_LEN`]。
    /// 路由鉴权与 [`Self::is_valid_api_key`] 共用此判定，策略只写一处。
    pub(crate) fn primary_key_is_acceptable(&self) -> bool {
        !self.api_key.is_empty() && self.api_key.len() >= MIN_API_KEY_LEN
    }

    /// 判定 key 是否为合法凭证（多 key 支持）
    ///
    /// 主 `api_key` 须先通过强度校验（非空且长度 ≥ [`MIN_API_KEY_LEN`]，
    /// 同 `validate()`）方视为有效凭证；`api_key_roles` 中登记的 key
    /// 同口径要求 16 字符下限（过短即拒绝），空白 key 不作凭证。
    /// 空 key 一律不合法：主 key 为空（未配置）时，空串会与其相等，
    /// 从而让退化凭证通过判定。
    pub fn is_valid_api_key(&self, api_key: &str) -> bool {
        if api_key.is_empty() {
            return false;
        }
        let primary_ok = self.primary_key_is_acceptable()
            && crate::admin::routes::constant_time_eq(api_key, &self.api_key);
        let role_ok = self
            .api_key_roles
            .keys()
            .filter(|k| role_key_is_acceptable(k))
            .any(|k| crate::admin::routes::constant_time_eq(api_key, k));
        primary_ok || role_ok
    }

    /// 查找 API key 对应的 operator 身份
    ///
    /// 返回 `Some(operator)` 表示配置中存在显式映射；
    /// 返回 `None` 表示未配置映射，调用方应回退到默认 `"admin-api"` 并记录 warn。
    pub fn operator_for_api_key(&self, api_key: &str) -> Option<&str> {
        self.api_key_operators.get(api_key).map(|s| s.as_str())
    }

    /// 设置指定分组的速率限制
    ///
    /// # 参数
    /// - `group`: 分组名（"ban" / "quota" / "default"）
    /// - `max`: 窗口内最大请求数
    /// - `window_secs`: 窗口时长（秒）
    pub fn with_rate_limit(mut self, group: &str, max: u64, window_secs: u64) -> Self {
        self.rate_limits
            .insert(group.to_string(), (max, window_secs));
        self
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.enabled && self.api_key.is_empty() {
            return Err(ConfigError::ApiKeyRequired);
        }
        if !self.api_key.is_empty() && !self.primary_key_is_acceptable() {
            return Err(ConfigError::ApiKeyTooShort(self.api_key.len()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = AdminApiConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 9090);
        assert!(config.api_key.is_empty());
        assert!(!config.enabled);
    }

    #[test]
    fn test_new_sets_api_key_and_enabled() {
        let config = AdminApiConfig::new("my-secure-api-key-32chars");
        assert_eq!(config.api_key, "my-secure-api-key-32chars");
        assert!(config.enabled);
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 9090);
    }

    #[test]
    fn test_builder_methods() {
        let config = AdminApiConfig::new("initial-api-key-32chars!!")
            .with_host("0.0.0.0")
            .with_port(8080)
            .with_api_key("replacement-api-key-32ch")
            .with_enabled(false);
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert_eq!(config.api_key, "replacement-api-key-32ch");
        assert!(!config.enabled);
    }

    #[test]
    fn test_address_format() {
        let config = AdminApiConfig::new("my-secure-api-key-32chars")
            .with_host("192.168.1.1")
            .with_port(3000);
        assert_eq!(config.address(), "192.168.1.1:3000");
    }

    #[test]
    fn test_validate_enabled_empty_key_returns_api_key_required() {
        let config = AdminApiConfig {
            api_key: String::new(),
            enabled: true,
            ..Default::default()
        };
        match config.validate() {
            Err(ConfigError::ApiKeyRequired) => {}
            other => panic!("expected ApiKeyRequired, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_short_key_returns_too_short_with_length() {
        let config = AdminApiConfig {
            api_key: "short".to_string(),
            enabled: true,
            ..Default::default()
        };
        match config.validate() {
            Err(ConfigError::ApiKeyTooShort(5)) => {}
            other => panic!("expected ApiKeyTooShort(5), got {:?}", other),
        }
    }

    #[test]
    fn test_validate_disabled_empty_key_ok() {
        let config = AdminApiConfig {
            api_key: String::new(),
            enabled: false,
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_valid_config_ok() {
        let config = AdminApiConfig::new("this-is-a-valid-api-key-32ch");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_error_display_messages() {
        assert_eq!(
            ConfigError::ApiKeyRequired.to_string(),
            "API key is required when admin API is enabled"
        );
        assert_eq!(
            ConfigError::ApiKeyTooShort(10).to_string(),
            "API key must be at least 16 characters, got 10"
        );
    }

    // api_key_operators 映射

    #[test]
    fn test_default_config_has_empty_api_key_operators() {
        let config = AdminApiConfig::default();
        assert!(config.api_key_operators.is_empty());
    }

    #[test]
    fn test_with_api_key_operator_adds_mapping() {
        let config = AdminApiConfig::new("my-secure-api-key-32chars")
            .with_api_key_operator("my-secure-api-key-32chars", "admin-alice");
        assert_eq!(
            config.operator_for_api_key("my-secure-api-key-32chars"),
            Some("admin-alice")
        );
    }

    #[test]
    fn test_with_api_key_operators_replaces_mapping() {
        let mut mapping = HashMap::new();
        mapping.insert("key-a-16chars-long".to_string(), "admin-a".to_string());
        mapping.insert("key-b-16chars-long".to_string(), "admin-b".to_string());
        let config = AdminApiConfig::new("primary-key-16chars!!!").with_api_key_operators(mapping);
        assert_eq!(
            config.operator_for_api_key("key-a-16chars-long"),
            Some("admin-a")
        );
        assert_eq!(
            config.operator_for_api_key("key-b-16chars-long"),
            Some("admin-b")
        );
        // primary key 未在 mapping 中 → None（回退到默认 "admin-api"）
        assert_eq!(config.operator_for_api_key("primary-key-16chars!!!"), None);
    }

    #[test]
    fn test_operator_for_api_key_returns_none_when_not_configured() {
        let config = AdminApiConfig::new("my-secure-api-key-32chars");
        // 未配置 mapping → 任何 key 都返回 None
        assert_eq!(
            config.operator_for_api_key("my-secure-api-key-32chars"),
            None
        );
    }

    #[test]
    fn test_operator_for_api_key_returns_none_for_unknown_key() {
        let config = AdminApiConfig::new("my-secure-api-key-32chars")
            .with_api_key_operator("my-secure-api-key-32chars", "admin-alice");
        // 未在 mapping 中的 key → None
        assert_eq!(config.operator_for_api_key("unknown-key"), None);
    }

    #[test]
    fn test_is_valid_api_key_rejects_short_primary_api_key() {
        // 主 key 未过强度校验时不是有效凭证——与路由口径一致，
        // 外部消费方不得把它当成合法凭证
        let config = AdminApiConfig::new("k");
        assert!(
            !config.is_valid_api_key("k"),
            "a primary api_key shorter than the minimum length must not authenticate"
        );
        let just_below_min = "a".repeat(MIN_API_KEY_LEN - 1);
        let config = AdminApiConfig::new(just_below_min.clone());
        assert!(
            !config.is_valid_api_key(&just_below_min),
            "a primary api_key one character below the minimum must not authenticate"
        );
    }

    #[test]
    fn test_is_valid_api_key_accepts_primary_key_at_minimum_length() {
        assert_eq!(MIN_API_KEY_LEN, 16);
        let at_min = "a".repeat(MIN_API_KEY_LEN);
        let config = AdminApiConfig::new(at_min.clone());
        assert!(config.is_valid_api_key(&at_min));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_is_valid_api_key_rejects_blank_role_mapped_key() {
        // 路由剔除空白角色 key（否则退化为 "Bearer " 畸形凭证）
        let config = AdminApiConfig::new("primary-key-16chars!!!")
            .with_api_key_role("   ", AdminRole::Viewer);
        assert!(!config.is_valid_api_key("   "));
    }

    #[test]
    fn test_is_valid_api_key_rejects_short_role_mapped_key() {
        // 角色映射 key 与主 key 同口径：过短即拒绝（可穷举的活凭证），
        // 恰好达到下限仍有效
        let config = AdminApiConfig::new("primary-key-16chars!!!")
            .with_api_key_role("v", AdminRole::Viewer)
            .with_api_key_role("0123456789abcdef", AdminRole::Viewer);
        assert!(!config.is_valid_api_key("v"));
        assert!(config.is_valid_api_key("0123456789abcdef"));
        assert!(config.is_valid_api_key("primary-key-16chars!!!"));
    }

    #[test]
    fn test_is_valid_api_key_agrees_with_validate_for_primary_key() {
        // helper 与 validate() 对主 key 的判定必须同口径
        let keys = [
            String::new(),
            "k".to_string(),
            "a".repeat(MIN_API_KEY_LEN - 1),
            "a".repeat(MIN_API_KEY_LEN),
        ];
        for key in keys {
            let config = AdminApiConfig::new(key.clone());
            assert_eq!(
                config.is_valid_api_key(&key),
                config.validate().is_ok(),
                "is_valid_api_key must follow the same policy as validate() for key {key:?}"
            );
        }
    }
}

// RBAC 角色矩阵

#[cfg(test)]
mod t605_role_tests {
    use super::*;

    #[test]
    fn test_t605_role_for_api_key_defaults_to_admin() {
        let config = AdminApiConfig::new("primary-key-16chars!!!");
        assert_eq!(
            config.role_for_api_key("primary-key-16chars!!!"),
            AdminRole::Admin
        );
        assert_eq!(config.role_for_api_key("anything"), AdminRole::Admin);
    }

    #[test]
    fn test_t605_with_api_key_role_maps_viewer() {
        let config = AdminApiConfig::new("primary-key-16chars!!!")
            .with_api_key_role("viewer-key-16chars!!", AdminRole::Viewer);
        assert_eq!(
            config.role_for_api_key("viewer-key-16chars!!"),
            AdminRole::Viewer
        );
        assert_eq!(
            config.role_for_api_key("primary-key-16chars!!!"),
            AdminRole::Admin
        );
    }

    #[test]
    fn test_t605_is_valid_api_key_accepts_registered_keys() {
        let config = AdminApiConfig::new("primary-key-16chars!!!")
            .with_api_key_role("viewer-key-16chars!!", AdminRole::Viewer);
        assert!(config.is_valid_api_key("primary-key-16chars!!!"));
        assert!(config.is_valid_api_key("viewer-key-16chars!!"));
        assert!(!config.is_valid_api_key("rogue-key"));
    }

    #[test]
    fn test_t605_role_permissions_matrix() {
        // viewer：只读放行、写入拒绝
        assert!(AdminRole::Viewer.allows_read());
        assert!(!AdminRole::Viewer.allows_write());
        // admin：读写全部
        assert!(AdminRole::Admin.allows_read());
        assert!(AdminRole::Admin.allows_write());
    }

    #[test]
    fn test_t605_role_serde_lowercase() {
        assert_eq!(
            serde_json::to_string(&AdminRole::Viewer).unwrap(),
            "\"viewer\""
        );
        assert_eq!(
            serde_json::to_string(&AdminRole::Admin).unwrap(),
            "\"admin\""
        );
        let role: AdminRole = serde_json::from_str("\"viewer\"").unwrap();
        assert_eq!(role, AdminRole::Viewer);
    }

    #[test]
    fn test_t605_debug_output_redacts_api_keys() {
        // 映射表的 key 即活凭证：Debug 不得输出任何 key 原文
        let config = AdminApiConfig::new("primary-key-16chars!!!")
            .with_api_key_role("viewer-key-16chars!!", AdminRole::Viewer)
            .with_api_key_operator("viewer-key-16chars!!", "ops-alice");
        let debug = format!("{:?}", config);
        assert!(
            !debug.contains("primary-key-16chars!!!"),
            "primary api key leaked in Debug: {debug}"
        );
        assert!(
            !debug.contains("viewer-key-16chars!!"),
            "role-mapped api key leaked in Debug: {debug}"
        );
        assert!(
            debug.contains("<redacted"),
            "redaction marker missing in Debug: {debug}"
        );
    }

    #[test]
    fn test_debug_output_counts_api_key_in_chars() {
        // 多字节 key：字节数 ≠ 字符数，标签写 "chars" 必须按字符计
        // （"密钥abc" = 5 chars / 9 bytes）
        let config = AdminApiConfig::new("密钥abc");
        let debug = format!("{:?}", config);
        assert!(
            debug.contains("<redacted; 5 chars>"),
            "api key length must be counted in chars: {debug}"
        );
        assert!(
            !debug.contains("<redacted; 9 chars>"),
            "api key length must not be counted in bytes: {debug}"
        );
    }

    #[test]
    fn test_t605_is_valid_api_key_rejects_empty() {
        // 空 key 不得成为有效凭证（否则退化凭证可通过鉴权）
        let config = AdminApiConfig::new("primary-key-16chars!!!");
        assert!(!config.is_valid_api_key(""));
        let disabled = AdminApiConfig::default();
        assert!(!disabled.is_valid_api_key(""));
    }

    #[test]
    fn test_is_valid_api_key_rejects_short_primary_key() {
        // 主 key 过短时同样不是有效凭证（与路由鉴权 fail closed 同口径）
        let config = AdminApiConfig::new("k");
        assert!(!config.is_valid_api_key("k"));
    }

    #[test]
    fn test_default_rate_limits_match_group_fallback_table() {
        // 配置默认 map 与"分组缺席配置时的回退"共用同一张表：值必须一致
        let defaults = default_rate_limits();
        assert_eq!(defaults.get("ban"), Some(&(100, 60)));
        assert_eq!(defaults.get("quota"), Some(&(50, 60)));
        assert_eq!(defaults.get("metrics"), Some(&(600, 60)));
        assert_eq!(defaults.get("default"), Some(&(200, 60)));
        for group in ["ban", "quota", "metrics", "default"] {
            assert_eq!(
                defaults.get(group).copied(),
                Some(default_rate_limit_for(group)),
                "the default map and the group fallback must agree on {group}"
            );
        }
    }
}
