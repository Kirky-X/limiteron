// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! OTLP 追踪导出（T606，`otlp` feature）
//!
//! 将 [`Tracer`](super::Tracer) 决策链路 span（规则匹配/封禁检查/限流器执行）
//! 以 OTLP/HTTP JSON 形态（`resourceSpans` 信封）导出到 OTLP 端点，替代
//! rc3 之前的"简化模式"stub：
//!
//! - **传输**：[`HttpTransport`] —— 手工 HTTP/1.1 POST over TcpStream
//!   （阻塞 IO 经 `spawn_blocking` 隔离），MVP 无新增依赖（与 dbnexus
//!   T412 同范式）
//! - **mock**：[`InMemoryTransport`] —— 测试/离线验证用内存收集器
//! - **接线**：[`SpanSink`] 经无界 channel 解耦 `Span::finish()`（同步）
//!   与网络导出（异步后台任务）
//!
//! # Example
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use limiteron::telemetry::otlp::{OtlpSpanExporter, HttpTransport};
//! use std::sync::Arc;
//!
//! let exporter = OtlpSpanExporter::new(
//!     "limiteron".to_string(),
//!     Arc::new(HttpTransport::new(3000)),
//!     "http://localhost:4318/v1/traces".to_string(),
//! );
//! let (sink, worker) = OtlpSpanExporter::spawn_worker(exporter, 1024);
//! tokio::spawn(worker.run());
//! // sink 注入 Tracer 后，Span::finish() 自动导出
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

/// 单个已完成 span 的 OTLP 导出数据（T606）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OtlpSpanData {
    /// span 名（如 `governor_check`）
    pub name: String,
    /// 服务名（resource.service.name）
    pub service_name: String,
    /// trace id（16 进制）
    pub trace_id: String,
    /// span id（16 进制）
    pub span_id: String,
    /// 开始时间（Unix 纳秒）
    pub start_unix_nano: u64,
    /// 结束时间（Unix 纳秒）
    pub end_unix_nano: u64,
    /// 属性键值对
    pub attributes: Vec<(String, String)>,
    /// 事件（名 + 属性）
    pub events: Vec<(String, Vec<(String, String)>)>,
    /// 错误信息（record_error 记录）
    pub error: Option<String>,
}

/// OTLP 传输抽象（T606：mock collector / 真实 HTTP 皆可实现）
#[async_trait]
pub trait OtlpTransport: Send + Sync {
    /// 发送 OTLP 请求体（JSON 形态）到端点
    async fn send(&self, endpoint: &str, body: &serde_json::Value) -> Result<(), String>;
}

/// OTLP/HTTP 传输（手工 HTTP/1.1 POST over TcpStream，无新增依赖）
///
/// 阻塞 IO 经 `tokio::task::spawn_blocking` 隔离，读写超时 `timeout_ms` 兜底。
pub struct HttpTransport {
    timeout_ms: u64,
}

impl HttpTransport {
    /// 创建传输（连接/读写超时毫秒）
    pub fn new(timeout_ms: u64) -> Self {
        Self { timeout_ms }
    }
}

#[async_trait]
impl OtlpTransport for HttpTransport {
    async fn send(&self, endpoint: &str, body: &serde_json::Value) -> Result<(), String> {
        let endpoint = endpoint.to_string();
        let payload = serde_json::to_vec(body).map_err(|e| format!("serialize failed: {e}"))?;
        let timeout_ms = self.timeout_ms;
        tokio::task::spawn_blocking(move || http_post(&endpoint, &payload, timeout_ms))
            .await
            .map_err(|e| format!("otlp http task failed: {e}"))?
    }
}

/// 内存传输（T606 mock collector：记录请求体供断言）
#[derive(Default)]
pub struct InMemoryTransport {
    requests: std::sync::Mutex<Vec<(String, serde_json::Value)>>,
}

impl InMemoryTransport {
    /// 创建空收集器
    pub fn new() -> Self {
        Self::default()
    }

    /// 已收到的 (endpoint, body) 请求
    pub fn requests(&self) -> Vec<(String, serde_json::Value)> {
        self.requests.lock().unwrap().clone()
    }

    /// 清空收集
    pub fn clear(&self) {
        self.requests.lock().unwrap().clear();
    }
}

#[async_trait]
impl OtlpTransport for InMemoryTransport {
    async fn send(&self, endpoint: &str, body: &serde_json::Value) -> Result<(), String> {
        self.requests
            .lock()
            .unwrap()
            .push((endpoint.to_string(), body.clone()));
        Ok(())
    }
}

/// 解析 `http://host:port/path` 并发出 JSON POST（阻塞实现）
fn http_post(endpoint: &str, body: &[u8], timeout_ms: u64) -> Result<(), String> {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    let (host, port, path) = parse_endpoint(endpoint)?;
    let addr = format!("{host}:{port}");
    let mut stream = TcpStream::connect(&addr)
        .map_err(|e| format!("connect to {addr} failed: {e}"))
        .and_then(|s| {
            s.set_read_timeout(Some(Duration::from_millis(timeout_ms)))
                .and_then(|_| s.set_write_timeout(Some(Duration::from_millis(timeout_ms))))
                .map(|_| s)
                .map_err(|e| e.to_string())
        })
        .map_err(|e| format!("prepare connection failed: {e}"))?;

    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .and_then(|_| stream.write_all(body))
        .map_err(|e| format!("write failed: {e}"))?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| format!("read failed: {e}"))?;

    let text = String::from_utf8_lossy(&response);
    let status = text.split_whitespace().nth(1).unwrap_or("000").to_string();
    if status.starts_with('2') {
        Ok(())
    } else {
        Err(format!("otlp endpoint returned status {status}"))
    }
}

/// 解析 `http://host:port/path`（仅 http；缺省 path = `/`）
fn parse_endpoint(endpoint: &str) -> Result<(String, u16, String), String> {
    let rest = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| format!("only http endpoints supported, got: {endpoint}"))?;
    let (host_port, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    let (host, port) = match host_port.rfind(':') {
        Some(idx) => (
            host_port[..idx].to_string(),
            host_port[idx + 1..]
                .parse::<u16>()
                .map_err(|e| format!("invalid port: {e}"))?,
        ),
        None => (host_port.to_string(), 80u16),
    };
    Ok((host, port, path.to_string()))
}

/// OTLP span 导出器：组装 `resourceSpans` 信封并经传输发送
pub struct OtlpSpanExporter {
    service_name: String,
    transport: Arc<dyn OtlpTransport>,
    endpoint: String,
}

impl OtlpSpanExporter {
    /// 创建导出器
    pub fn new(
        service_name: impl Into<String>,
        transport: Arc<dyn OtlpTransport>,
        endpoint: impl Into<String>,
    ) -> Self {
        Self {
            service_name: service_name.into(),
            transport,
            endpoint: endpoint.into(),
        }
    }

    /// 导出单个 span（封装为 resourceSpans 信封）
    pub async fn export(&self, span: &OtlpSpanData) -> Result<(), String> {
        let body = self.envelope(span);
        self.transport.send(&self.endpoint, &body).await
    }

    /// 组装 OTLP/HTTP JSON `resourceSpans` 信封
    pub fn envelope(&self, span: &OtlpSpanData) -> serde_json::Value {
        let mut attributes: Vec<serde_json::Value> = span
            .attributes
            .iter()
            .map(|(k, v)| attribute(k, v))
            .collect();
        if let Some(err) = &span.error {
            attributes.push(attribute("error.message", err));
        }
        let events: Vec<serde_json::Value> = span
            .events
            .iter()
            .map(|(name, attrs)| {
                serde_json::json!({
                    "name": name,
                    "timeUnixNano": span.end_unix_nano.to_string(),
                    "attributes": attrs.iter().map(|(k, v)| attribute(k, v)).collect::<Vec<_>>(),
                })
            })
            .collect();

        serde_json::json!({
            "resourceSpans": [{
                "resource": {
                    "attributes": [attribute("service.name", &self.service_name)],
                },
                "scopeSpans": [{
                    "scope": { "name": "limiteron", "version": env!("CARGO_PKG_VERSION") },
                    "spans": [{
                        "name": span.name,
                        "traceId": span.trace_id,
                        "spanId": span.span_id,
                        "kind": "SPAN_KIND_INTERNAL",
                        "startTimeUnixNano": span.start_unix_nano.to_string(),
                        "endTimeUnixNano": span.end_unix_nano.to_string(),
                        "attributes": attributes,
                        "events": events,
                        "status": if span.error.is_some() { "STATUS_CODE_ERROR" } else { "STATUS_CODE_UNSET" },
                    }],
                }],
            }]
        })
    }

    /// 生成 (sink, 后台导出 worker)
    ///
    /// `Span::finish()` 经 sink 同步入队（channel 满时丢弃并告警——观测
    /// 数据不得阻塞决策热路径）；worker 在独立任务中逐 span 导出。
    pub fn spawn_worker(
        exporter: OtlpSpanExporter,
        channel_capacity: usize,
    ) -> (SpanSink, SpanExportWorker) {
        let (tx, rx) = tokio::sync::mpsc::channel(channel_capacity.max(1));
        let sink = SpanSink {
            service_name: exporter.service_name.clone(),
            sender: tx,
        };
        (
            sink,
            SpanExportWorker {
                exporter: Arc::new(exporter),
                receiver: rx,
            },
        )
    }
}

fn attribute(k: &str, v: &str) -> serde_json::Value {
    serde_json::json!({ "key": k, "value": { "stringValue": v } })
}

/// span 提交句柄（`Tracer` 持有；`Span::finish()` 调用）
#[derive(Clone)]
pub struct SpanSink {
    service_name: String,
    sender: tokio::sync::mpsc::Sender<OtlpSpanData>,
}

impl SpanSink {
    /// 服务名
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// 提交已完成 span（非阻塞；通道满时丢弃并告警）
    pub fn submit(&self, span: OtlpSpanData) {
        if let Err(e) = self.sender.try_send(span) {
            log::warn!(target: "telemetry", "otlp span dropped: {e}");
        }
    }
}

/// 后台导出循环（`tokio::spawn(worker.run())` 启动）
pub struct SpanExportWorker {
    exporter: Arc<OtlpSpanExporter>,
    receiver: tokio::sync::mpsc::Receiver<OtlpSpanData>,
}

impl SpanExportWorker {
    /// 运行导出循环（channel 关闭后退出）
    pub async fn run(mut self) {
        while let Some(span) = self.receiver.recv().await {
            if let Err(e) = self.exporter.export(&span).await {
                log::warn!(target: "telemetry", "otlp export failed for span {}: {e}", span.name);
            }
        }
    }
}

/// 以进程内单调计数生成伪随机 16 进制 id（MVP：不追求密码学强度）
pub(crate) fn next_id_hex(bytes: usize) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0x9e37_79b9_7f4a_7c15);
    const GOLDEN: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut out = String::with_capacity(bytes * 2);
    while out.len() < bytes * 2 {
        let n = COUNTER.fetch_add(GOLDEN, Ordering::Relaxed);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        out.push_str(&format!("{:016x}", n.rotate_left(17) ^ ts));
    }
    out.truncate(bytes * 2);
    out
}

/// 当前 Unix 纳秒时间戳
pub(crate) fn unix_nano_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_span() -> OtlpSpanData {
        OtlpSpanData {
            name: "governor_check".to_string(),
            service_name: "limiteron".to_string(),
            trace_id: "0af7651916cd43dd8448eb211c80319c".to_string(),
            span_id: "b7ad6b7169203331".to_string(),
            start_unix_nano: 1_000,
            end_unix_nano: 2_000,
            attributes: vec![("rule.id".to_string(), "r1".to_string())],
            events: vec![],
            error: None,
        }
    }

    #[test]
    fn test_t606_envelope_structure() {
        let exporter = OtlpSpanExporter::new(
            "limiteron",
            Arc::new(InMemoryTransport::new()),
            "http://127.0.0.1:4318/v1/traces",
        );
        let env = exporter.envelope(&sample_span());
        let rs = env["resourceSpans"].as_array().unwrap();
        assert_eq!(rs.len(), 1);
        assert_eq!(rs[0]["scopeSpans"][0]["spans"][0]["name"], "governor_check");
        let attrs = rs[0]["scopeSpans"][0]["spans"][0]["attributes"]
            .as_array()
            .unwrap();
        assert_eq!(attrs[0]["key"], "rule.id");
        assert_eq!(attrs[0]["value"]["stringValue"], "r1");
    }

    #[tokio::test]
    async fn test_t606_in_memory_transport_receives_export() {
        let transport = Arc::new(InMemoryTransport::new());
        let exporter = OtlpSpanExporter::new(
            "limiteron",
            transport.clone(),
            "http://collector:4318/v1/traces",
        );
        exporter.export(&sample_span()).await.unwrap();
        let requests = transport.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0, "http://collector:4318/v1/traces");
        assert!(requests[0].1["resourceSpans"].is_array());
    }

    #[test]
    fn test_t606_parse_endpoint() {
        assert_eq!(
            parse_endpoint("http://localhost:4318/v1/traces").unwrap(),
            ("localhost".to_string(), 4318u16, "/v1/traces".to_string())
        );
        assert_eq!(
            parse_endpoint("http://collector").unwrap(),
            ("collector".to_string(), 80u16, "/".to_string())
        );
        assert!(parse_endpoint("https://collector").is_err());
    }
}
