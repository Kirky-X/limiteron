// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Admin API 薄客户端（`admin-client` feature）
//!
//! 面向 sdforge 侧用户从 limiteron 侧消费 Admin API 的最小闭环：与
//! [`AdminService`](super::service::AdminService) 契约一一对应的类型化
//! 方法 + Bearer 认证。**临时验证面**——sdforge-R11（多协议客户端 SDK
//! 生成器）落地后由生成的 client 接替本模块，届时本模块按其演进注记
//! 退役或转为薄包装。
//!
//! 实现取向：hyper 1.x http1 client（已是 axum 传递依赖，不新增依赖树
//! 重量）；每请求新建连接（管理面低频，免去连接池复杂度）；TLS 不在
//! 最小闭环范围（Admin API 设计为内网/回环使用，暴露公网须前置反向
//! 代理终止 TLS——见 AdminApiConfig 绑定警示）。

use http_body_util::{BodyExt, Full};
use hyper::Request;
use hyper::body::Bytes;
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use serde::de::DeserializeOwned;
use std::time::Duration;

/// Admin API 客户端
#[derive(Debug, Clone)]
pub struct AdminClient {
    /// 形如 `http://127.0.0.1:9090` 的基址（不含路径）
    base_url: String,
    /// Bearer API key
    api_key: String,
    /// 请求超时（默认 10s；管理面低频操作，长超时无收益）
    timeout: Duration,
}

impl AdminClient {
    /// 以基址与 API key 构造客户端
    #[must_use]
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            timeout: Duration::from_secs(10),
        }
    }

    /// 自定义请求超时
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// GET：解析 JSON 响应体（非 2xx 报错并携带状态码与响应文本）
    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, AdminClientError> {
        let body = self.send(hyper::Method::GET, path, None).await?;
        serde_json::from_slice(&body).map_err(|e| AdminClientError::Decode(e.to_string()))
    }

    /// POST/PUT/DELETE：JSON 请求体；返回原始响应字节
    async fn send(
        &self,
        method: hyper::Method,
        path: &str,
        json_body: Option<serde_json::Value>,
    ) -> Result<Bytes, AdminClientError> {
        // 明文 HTTP/1 客户端无 TLS connector：https base_url 在任何 IO
        // 之前显性拒绝（Bearer API key 不允许经伪装 TLS 语义的明文通道发送）
        if self.base_url.starts_with("https://") {
            return Err(AdminClientError::InvalidBaseUrl(
                "https:// base_url is not supported: this client speaks plaintext \
                 HTTP/1 only (no TLS connector); terminate TLS at a reverse proxy \
                 and use http://"
                    .to_string(),
            ));
        }
        let (host_name, port) = self.host()?;
        let host_header = if host_name.contains(':') {
            format!("[{host_name}]:{port}")
        } else {
            format!("{host_name}:{port}")
        };
        let uri = format!("{}{}", self.base_url, path);
        let mut builder = Request::builder()
            .method(method)
            .uri(&uri)
            .header(
                hyper::header::AUTHORIZATION,
                format!("Bearer {}", self.api_key),
            )
            .header(hyper::header::HOST, host_header);
        if json_body.is_some() {
            builder = builder.header(hyper::header::CONTENT_TYPE, "application/json");
        }
        let body_bytes = json_body
            .map(|v| serde_json::to_vec(&v))
            .transpose()
            .map_err(|e| AdminClientError::Encode(e.to_string()))?
            .unwrap_or_default();
        let request = builder
            .body(Full::new(Bytes::from(body_bytes)))
            .map_err(|e| AdminClientError::Request(e.to_string()))?;

        let connect = tokio::net::TcpStream::connect((&*host_name, port));
        let stream = tokio::time::timeout(self.timeout, connect)
            .await
            .map_err(|_| AdminClientError::Timeout)?
            .map_err(|e| AdminClientError::Connect(e.to_string()))?;

        let (mut sender, connection) =
            tokio::time::timeout(self.timeout, http1::handshake(TokioIo::new(stream)))
                .await
                .map_err(|_| AdminClientError::Timeout)?
                .map_err(|e| AdminClientError::Connect(e.to_string()))?;
        tokio::spawn(async move {
            // 连接级错误仅影响本请求（每请求新建连接），无需上报调用方
            let _ = connection.await;
        });

        let response = tokio::time::timeout(self.timeout, sender.send_request(request))
            .await
            .map_err(|_| AdminClientError::Timeout)?
            .map_err(|e| AdminClientError::Request(e.to_string()))?;

        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|e| AdminClientError::Request(e.to_string()))?
            .to_bytes();
        if !status.is_success() {
            return Err(AdminClientError::Status {
                status: status.to_string(),
                body: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        Ok(bytes)
    }

    /// 从 base_url 拆出 (host, port)
    ///
    /// 支持括号形式 IPv6（`http://[::1]:9090`，HOST 头按规范补括号）；
    /// 缺省端口 80（内网明文闭环场景）；端口非数字或 IPv6 括号未闭合
    /// 显性报错，不静默回退。
    fn host(&self) -> Result<(String, u16), AdminClientError> {
        let rest = self.base_url.trim_start_matches("http://");
        if let Some(inner) = rest.strip_prefix('[') {
            let Some((h, tail)) = inner.split_once(']') else {
                return Err(AdminClientError::InvalidBaseUrl(format!(
                    "base_url has unterminated IPv6 bracket: {}",
                    self.base_url
                )));
            };
            let port = match tail.strip_prefix(':') {
                Some(p) => p.parse().map_err(|_| self.invalid_port_error(p))?,
                None => 80,
            };
            return Ok((h.to_string(), port));
        }
        match rest.split_once(':') {
            Some((h, p)) => {
                let port = p.parse().map_err(|_| self.invalid_port_error(p))?;
                Ok((h.to_string(), port))
            }
            None => Ok((rest.to_string(), 80)),
        }
    }

    fn invalid_port_error(&self, port: &str) -> AdminClientError {
        AdminClientError::InvalidBaseUrl(format!(
            "base_url has invalid port {port:?}: {}",
            self.base_url
        ))
    }

    /// GET /api/v1/status —— 系统状态（`ApiResponse` envelope）
    pub async fn status(&self) -> Result<ApiResponse, AdminClientError> {
        self.get_json("/api/v1/status").await
    }

    /// GET /api/v1/introspect —— 自省快照
    pub async fn introspect(&self) -> Result<serde_json::Value, AdminClientError> {
        self.get_json("/api/v1/introspect").await
    }

    /// GET /api/v1/status/circuit-breaker —— 熔断器状态
    pub async fn circuit_breaker_status(&self) -> Result<ApiResponse, AdminClientError> {
        self.get_json("/api/v1/status/circuit-breaker").await
    }

    /// GET /healthz —— 存活探针
    pub async fn healthz(&self) -> Result<serde_json::Value, AdminClientError> {
        self.get_json("/healthz").await
    }

    /// POST /api/v1/ban —— 创建封禁
    pub async fn create_ban(
        &self,
        req: &serde_json::Value,
    ) -> Result<ApiResponse, AdminClientError> {
        let body = self
            .send(hyper::Method::POST, "/api/v1/ban", Some(req.clone()))
            .await?;
        serde_json::from_slice(&body).map_err(|e| AdminClientError::Decode(e.to_string()))
    }

    /// DELETE /api/v1/ban/{target} —— 解除封禁
    pub async fn delete_ban(
        &self,
        target: &str,
        target_type: Option<&str>,
        reason: Option<&str>,
    ) -> Result<ApiResponse, AdminClientError> {
        let path = match target_type {
            // target 与 target_type 均为用户输入：query 值与路径段同用
            // 百分号编码，&/#/= 无法追加或篡改 query 参数
            Some(t) => format!(
                "/api/v1/ban/{}?type={}",
                encode_path(target),
                encode_path(t)
            ),
            None => format!("/api/v1/ban/{}", encode_path(target)),
        };
        let body = self
            .send(
                hyper::Method::DELETE,
                &path,
                Some(serde_json::json!({ "reason": reason })),
            )
            .await?;
        serde_json::from_slice(&body).map_err(|e| AdminClientError::Decode(e.to_string()))
    }

    /// PUT /api/v1/quota/{user_id} —— 按 user 重置配额使用量（new_limit=0）
    pub async fn reset_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<ApiResponse, AdminClientError> {
        let body = self
            .send(
                hyper::Method::PUT,
                &format!("/api/v1/quota/{}", encode_path(user_id)),
                Some(serde_json::json!({ "resource": resource, "new_limit": 0 })),
            )
            .await?;
        serde_json::from_slice(&body).map_err(|e| AdminClientError::Decode(e.to_string()))
    }

    /// POST /api/v1/config —— 规则热更新
    pub async fn apply_config(
        &self,
        config: &serde_json::Value,
    ) -> Result<serde_json::Value, AdminClientError> {
        let body = self
            .send(hyper::Method::POST, "/api/v1/config", Some(config.clone()))
            .await?;
        serde_json::from_slice(&body).map_err(|e| AdminClientError::Decode(e.to_string()))
    }

    /// POST /api/v1/check/batch —— 批量检查
    pub async fn check_batch(
        &self,
        requests: &[serde_json::Value],
    ) -> Result<serde_json::Value, AdminClientError> {
        let body = self
            .send(
                hyper::Method::POST,
                "/api/v1/check/batch",
                Some(serde_json::json!({ "requests": requests })),
            )
            .await?;
        serde_json::from_slice(&body).map_err(|e| AdminClientError::Decode(e.to_string()))
    }
}

/// 路径段 / query 值百分号编码（target/user_id/target_type 为用户
/// 输入，防路径穿越与 query 篡改）
fn encode_path(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// `ApiResponse` envelope 的客户端投影（data 保留原始 JSON）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ApiResponse {
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
    /// 数据（原始 JSON；类型化由消费方按 schema 完成）
    pub data: Option<serde_json::Value>,
}

/// 客户端错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminClientError {
    /// base_url 不合法（https 无 TLS 支撑 / IPv6 括号未闭合 / 端口无法
    /// 解析——fail-loud，不静默回退）
    InvalidBaseUrl(String),
    /// 连接失败
    Connect(String),
    /// 请求/响应 IO 失败
    Request(String),
    /// 请求体序列化失败
    Encode(String),
    /// 响应体反序列化失败
    Decode(String),
    /// 非 2xx 响应（携带状态码与响应文本）
    Status { status: String, body: String },
    /// 超时
    Timeout,
}

impl std::fmt::Display for AdminClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBaseUrl(m) => write!(f, "invalid base_url: {m}"),
            Self::Connect(m) | Self::Request(m) | Self::Encode(m) | Self::Decode(m) => {
                write!(f, "{m}")
            }
            Self::Status { status, body } => write!(f, "HTTP {status}: {body}"),
            Self::Timeout => write!(f, "request timed out"),
        }
    }
}

impl std::error::Error for AdminClientError {}

impl AdminClientError {
    /// 非 2xx 响应中的 HTTP 状态码文本（如 "404 Not Found"）
    #[must_use]
    pub fn status_text(&self) -> Option<&str> {
        match self {
            Self::Status { status, .. } => Some(status.as_str()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::{AdminApiConfig, AdminServer};

    /// 起一个回环 AdminServer，返回 (base_url, api_key)
    async fn spawn_admin_server() -> (String, &'static str) {
        let governor = std::sync::Arc::new(crate::admin::handlers::tests::make_governor().await);
        let config = AdminApiConfig {
            enabled: true,
            api_key: "test-api-key-16chars!!".to_string(),
            ..Default::default()
        };
        let server = AdminServer::new(governor, config);
        // 绑定随机端口：先经 into_router 拿 Router，自管 listener 以取端口
        let router = server.into_router().expect("valid config yields router");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        (format!("http://{addr}"), "test-api-key-16chars!!")
    }

    #[tokio::test]
    async fn healthz_and_status_roundtrip() {
        let (base, key) = spawn_admin_server().await;
        let client = AdminClient::new(base, key);

        let health = client.healthz().await.unwrap();
        assert_eq!(health["status"], "ok");

        let status = client.status().await.unwrap();
        assert!(status.success, "status 响应应成功: {status:?}");
        let data = status.data.expect("status data");
        assert_eq!(data["total_requests"], 0);
    }

    #[tokio::test]
    async fn bad_key_yields_status_error() {
        let (base, _) = spawn_admin_server().await;
        let client = AdminClient::new(base, "wrong-key-16chars!!!");

        let err = client.status().await.unwrap_err();
        assert_eq!(err.status_text(), Some("401 Unauthorized"));
    }

    #[tokio::test]
    async fn check_batch_roundtrip() {
        let (base, key) = spawn_admin_server().await;
        let client = AdminClient::new(base, key);

        let result = client
            .check_batch(&[serde_json::json!({"user_id": "u1", "ip": "10.0.0.1"})])
            .await
            .unwrap();
        assert_eq!(
            result["data"]["results"].as_array().expect("results").len(),
            1
        );
    }

    #[tokio::test]
    async fn connect_failure_is_explicit() {
        // 关闭端口 → Connect 错误显性上报（Rule 11）
        let client = AdminClient::new("http://127.0.0.1:1", "k");
        let err = client.status().await.unwrap_err();
        assert!(matches!(err, AdminClientError::Connect(_)), "got {err:?}");
    }

    #[test]
    fn path_encoding_blocks_traversal() {
        assert_eq!(encode_path("1.2.3.4"), "1.2.3.4");
        // `.` 属于未保留字符无需编码；路径穿越的实际向量是 `/`，编码后
        // `..` 无法完成段跳跃
        assert_eq!(encode_path("../etc"), "..%2Fetc");
        assert_eq!(encode_path("a b/c"), "a%20b%2Fc");
    }

    #[test]
    fn query_value_encoding_neutralizes_injection_chars() {
        // delete_ban 的 target_type query 值复用同一编码器：&、#、= 被
        // 百分号编码，无法追加/篡改 query 参数
        assert_eq!(encode_path("a&b=c#d"), "a%26b%3Dc%23d");
        assert_eq!(encode_path("ip"), "ip");
    }

    #[test]
    fn host_parsing_supports_ipv6_and_rejects_bad_ports() {
        // IPv6 括号形式：host 部分不得被 split_once(':') 误拆
        let c = AdminClient::new("http://[::1]:9090", "k");
        assert_eq!(c.host().unwrap(), ("::1".to_string(), 9090));

        let c = AdminClient::new("http://[::1]", "k");
        assert_eq!(c.host().unwrap(), ("::1".to_string(), 80));

        let c = AdminClient::new("http://admin.internal:9090", "k");
        assert_eq!(c.host().unwrap(), ("admin.internal".to_string(), 9090));

        let c = AdminClient::new("http://admin.internal", "k");
        assert_eq!(c.host().unwrap(), ("admin.internal".to_string(), 80));

        // 端口非数字：显性报错，不静默落 80
        let c = AdminClient::new("http://admin.internal:foo", "k");
        assert!(
            matches!(c.host(), Err(AdminClientError::InvalidBaseUrl(_))),
            "端口解析失败应显性报错: {:?}",
            c.host()
        );

        // IPv6 括号未闭合：显性报错
        let c = AdminClient::new("http://[::1:9090", "k");
        assert!(matches!(c.host(), Err(AdminClientError::InvalidBaseUrl(_))));
    }

    #[tokio::test]
    async fn https_base_url_fails_fast_without_io() {
        // 明文 HTTP/1 客户端无 TLS connector：https base_url 在发起任何
        // IO 前显性拒绝（Bearer API key 不得经明文通道发送）
        let client = AdminClient::new("https://admin.example.com", "secret-key");
        let err = client.status().await.unwrap_err();
        assert!(
            matches!(err, AdminClientError::InvalidBaseUrl(ref m) if m.contains("https")),
            "https base_url 应显性报错: {err:?}"
        );
    }
}
