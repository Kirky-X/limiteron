// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! e2e：OTLP 追踪导出（mock collector）
//!
//! - Tracer 注入 OTLP sink 后，`Span::finish()` 自动导出（InMemoryTransport 收集）
//! - HttpTransport 对本地 mock TCP collector（HTTP/1.1 POST）端到端发送

#![cfg(feature = "otlp")]

use limiteron::telemetry::Tracer;
use limiteron::telemetry::otlp::{
    HttpTransport, InMemoryTransport, OtlpSpanExporter, OtlpTransport,
};
use std::sync::Arc;
use std::time::Duration;

/// Tracer → Span::finish → 后台 worker → mock collector 全链路
#[tokio::test]
async fn test_t606_tracer_exports_finished_spans() {
    let transport = Arc::new(InMemoryTransport::new());
    let exporter = OtlpSpanExporter::new(
        "limiteron-test",
        transport.clone(),
        "http://mock-collector:4318/v1/traces",
    );
    let (sink, worker) = OtlpSpanExporter::spawn_worker(exporter, 64);
    tokio::spawn(worker.run());

    let tracer = Tracer::with_otlp_sink(true, Some(Arc::new(sink)));
    {
        let span = tracer.start_span("governor_check");
        span.set_attribute("rule.id", "t606_rule");
        span.set_attribute("decision", "Allowed");
        span.add_event(
            "limiter.executed",
            vec![("cost".to_string(), "1".to_string())],
        );
        span.finish();
    }

    // 等待后台 worker 导出（channel + 异步传输）
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if !transport.requests().is_empty() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "5s 内未收到 OTLP 导出"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].0, "http://mock-collector:4318/v1/traces");
    let body = &requests[0].1;
    let span = &body["resourceSpans"][0]["scopeSpans"][0]["spans"][0];
    assert_eq!(span["name"], "governor_check");
    assert_eq!(
        span["attributes"][0]["key"], "rule.id",
        "span 属性应进入 OTLP attributes"
    );
    assert_eq!(span["attributes"][0]["value"]["stringValue"], "t606_rule");
    let events = span["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["name"], "limiter.executed");
}

/// 禁用 Tracer（或无 sink）时不得产生导出
#[tokio::test]
async fn test_t606_disabled_tracer_exports_nothing() {
    let transport = Arc::new(InMemoryTransport::new());
    let exporter = OtlpSpanExporter::new(
        "limiteron-test",
        transport.clone(),
        "http://mock-collector:4318/v1/traces",
    );
    let (sink, worker) = OtlpSpanExporter::spawn_worker(exporter, 64);
    tokio::spawn(worker.run());

    let tracer = Tracer::with_otlp_sink(false, Some(Arc::new(sink)));
    let span = tracer.start_span("should_not_export");
    span.finish();

    let disabled = Tracer::new(true);
    disabled.start_span("no_sink_span").finish();

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        transport.requests().is_empty(),
        "禁用 tracer / 无 sink 时不应有导出"
    );
}

/// HttpTransport 对本地 mock TCP collector 发送合法 HTTP/1.1 POST
#[tokio::test]
async fn test_t606_http_transport_posts_to_mock_collector() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock collector");
    let addr = listener.local_addr().unwrap();

    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        // 读取到 headers 结束后按 Content-Length 读满 body（不等待 EOF，
        // 避免与客户端 read_to_end 互等）
        let mut buf = Vec::new();
        let header_end;
        loop {
            let mut chunk = [0u8; 1024];
            let n = stream.read(&mut chunk).expect("read headers");
            if n == 0 {
                panic!("collector: 客户端提前关闭");
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
                header_end = pos + 4;
                break;
            }
        }
        let text = String::from_utf8_lossy(&buf).to_string();
        let content_length: usize = text
            .lines()
            .find_map(|l| {
                let (k, v) = l.split_once(':')?;
                k.eq_ignore_ascii_case("content-length")
                    .then(|| v.trim().parse().ok())?
            })
            .expect("Content-Length header");
        let body_len = content_length - (buf.len() - header_end);
        while buf.len() < header_end + body_len {
            let mut chunk = [0u8; 1024];
            let n = stream.read(&mut chunk).expect("read body");
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        let raw = String::from_utf8_lossy(&buf).to_string();
        // 响应 200 后连接关闭（Connection: close）
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        raw
    });

    fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    let transport = HttpTransport::new(3000);
    let body = serde_json::json!({
        "resourceSpans": [{
            "resource": { "attributes": [] },
            "scopeSpans": [{ "scope": { "name": "limiteron" }, "spans": [] }]
        }]
    });
    transport
        .send(&format!("http://{addr}/v1/traces"), &body)
        .await
        .expect("POST 应成功");

    let raw = handle.join().expect("collector thread");
    assert!(
        raw.starts_with("POST /v1/traces HTTP/1.1\r\n"),
        "应为 OTLP/HTTP JSON POST，实际前缀: {}",
        raw.chars().take(40).collect::<String>()
    );
    assert!(raw.contains("Content-Type: application/json"));
    assert!(raw.contains("\"resourceSpans\""), "请求体应包含 OTLP 信封");
}
