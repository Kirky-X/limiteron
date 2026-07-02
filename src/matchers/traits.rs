//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 标识符提取器 trait 定义
//!
//! 定义了核心的 trait：Identifier、RequestContext 和 IdentifierExtractor。

use ahash::AHashMap as HashMap;

// ============================================================================
// 标识符
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

    /// 转换为 BanTarget（用于封禁管理）
    ///
    /// # 返回
    /// - `Some(BanTarget)`: 如果标识符类型支持封禁
    /// - `None`: 如果标识符类型不支持封禁（如 ApiKey, DeviceId）
    #[cfg(feature = "ban-manager")]
    pub fn to_ban_target(&self) -> Option<crate::storage::BanTarget> {
        match self {
            Identifier::UserId(id) => Some(crate::storage::BanTarget::UserId(id.clone())),
            Identifier::Ip(ip) => Some(crate::storage::BanTarget::Ip(ip.clone())),
            Identifier::Mac(mac) => Some(crate::storage::BanTarget::Mac(mac.clone())),
            // ApiKey 和 DeviceId 不支持封禁
            Identifier::ApiKey(_) | Identifier::DeviceId(_) => None,
        }
    }
}

// ============================================================================
// 请求上下文
// ============================================================================

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

    /// 设置请求方法
    pub fn with_method(mut self, method: &str) -> Self {
        self.method = method.to_string();
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

// ============================================================================
// 标识符提取器 trait
// ============================================================================

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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "ban-manager")]
    #[test]
    fn test_identifier_mac_to_ban_target() {
        let id = Identifier::Mac("AA:BB:CC:DD:EE:FF".to_string());
        let target = id.to_ban_target();
        assert!(target.is_some());
        match target.unwrap() {
            crate::storage::BanTarget::Mac(mac) => {
                assert_eq!(mac, "AA:BB:CC:DD:EE:FF");
            }
            other => panic!("expected Mac, got: {:?}", other),
        }
    }

    #[cfg(feature = "ban-manager")]
    #[test]
    fn test_identifier_apikey_to_ban_target_none() {
        let id = Identifier::ApiKey("key123".to_string());
        assert!(id.to_ban_target().is_none());
    }
}
