// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! e2e：Governor 决策路径 span 属性（OTLP 导出全链路）
//!
//! - Governor 注入 OTLP sink tracer 后，`check()` 导出的 `governor_check`
//!   span 携带 rule.outcome 属性；拒绝（链上与负缓存命中）另携带归因的
//!   rule.id；全放行无单一裁决规则、缓存条目无归因时，不虚构 rule.id

#![cfg(feature = "otlp")]

use limiteron::config::{
    Action, ActionConfig, CacheBackend, FlowControlConfig, LimiterConfig, Matcher, MetricsBackend,
    Rule, StorageType,
};
use limiteron::telemetry::Tracer;
use limiteron::telemetry::otlp::{InMemoryTransport, OtlpSpanExporter};
use limiteron::{Governor, RequestContext};
use std::sync::Arc;
use std::time::Duration;

async fn build_governor_with_tracer(tracer: Arc<Tracer>) -> Governor {
    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: StorageType::Memory,
            cache: CacheBackend::Memory,
            metrics: MetricsBackend::Prometheus,
            trusted_proxies: Default::default(),
        },
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 1000,
                refill_rate: 100,
            }],
            action: ActionConfig {
                on_exceed: Action::Reject,
                ban: None,
            },
        }],
    };
    Governor::builder()
        .with_config(config)
        .with_storage(Arc::new(limiteron::storage::MemoryStorage::new()))
        .with_ban_storage(Arc::new(limiteron::storage::MemoryBanStorage::new()))
        .with_tracer(tracer)
        .build()
        .await
        .expect("Failed to create governor")
}

async fn wait_for_exports(transport: &InMemoryTransport) -> Vec<(String, serde_json::Value)> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let requests = transport.requests();
        if !requests.is_empty() {
            return requests;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no OTLP export received within 5s"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn exported_span(requests: &[(String, serde_json::Value)]) -> &serde_json::Value {
    assert_eq!(
        requests.len(),
        1,
        "a single check should export exactly one span"
    );
    &requests[0].1["resourceSpans"][0]["scopeSpans"][0]["spans"][0]
}

fn span_attribute(span: &serde_json::Value, key: &str) -> Option<String> {
    span["attributes"]
        .as_array()?
        .iter()
        .find(|a| a["key"] == key)
        .map(|a| {
            a["value"]["stringValue"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
}

/// check 放行路径：governor_check span 记录 rule.outcome=allowed；全放行
/// 无单一裁决规则，不虚构 rule.id 属性（拒绝路径才有唯一裁决规则）
#[tokio::test]
async fn test_governor_check_span_records_rule_attributes() {
    let transport = Arc::new(InMemoryTransport::new());
    let exporter = OtlpSpanExporter::new(
        "limiteron-test",
        transport.clone(),
        "http://mock-collector:4318/v1/traces",
    );
    let (sink, worker) = OtlpSpanExporter::spawn_worker(exporter, 64);
    tokio::spawn(worker.run());

    let governor =
        build_governor_with_tracer(Arc::new(Tracer::with_otlp_sink(true, Some(Arc::new(sink)))))
            .await;

    let ctx = RequestContext::new()
        .with_header("x-user-id", "u_span_ok")
        .with_method("GET");
    let decision = governor.check(&ctx).await.expect("check should succeed");
    assert!(matches!(decision, limiteron::Decision::Allowed(_)));

    let requests = wait_for_exports(&transport).await;
    let span = exported_span(&requests);
    assert_eq!(span["name"], "governor_check");
    assert_eq!(
        span_attribute(span, "rule.id"),
        None,
        "with an all-allow chain there is no single deciding rule; do not fabricate rule.id"
    );
    assert_eq!(
        span_attribute(span, "rule.outcome").as_deref(),
        Some("allowed"),
        "span should record rule-dimension results"
    );
}

/// 等待导出的 span 总数达到 `min_spans`（跨导出请求聚合；批量导出可能
/// 把多个 span 合并在同一请求内）
async fn wait_for_span_exports(
    transport: &InMemoryTransport,
    min_spans: usize,
) -> Vec<serde_json::Value> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let spans: Vec<serde_json::Value> = transport
            .requests()
            .iter()
            .flat_map(|r| {
                r.1["resourceSpans"][0]["scopeSpans"][0]["spans"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .collect();
        if spans.len() >= min_spans {
            return spans;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "did not collect {min_spans} exported spans within 5s"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// check 拒绝路径：链上拒绝与负缓存命中的拒绝 span 均携带 rule.id 与
/// rule.outcome=rejected（缓存命中经缓存内裁决规则 ID 归因，观测面不留盲区）
#[tokio::test]
async fn test_governor_check_span_records_rejection_attributes() {
    let transport = Arc::new(InMemoryTransport::new());
    let exporter = OtlpSpanExporter::new(
        "limiteron-test",
        transport.clone(),
        "http://mock-collector:4318/v1/traces",
    );
    let (sink, worker) = OtlpSpanExporter::spawn_worker(exporter, 64);
    tokio::spawn(worker.run());

    // capacity=1、refill_rate=1/s：同秒连发首个放行其后拒绝并入负缓存
    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: StorageType::Memory,
            cache: CacheBackend::Memory,
            metrics: MetricsBackend::Prometheus,
            trusted_proxies: Default::default(),
        },
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 1,
                refill_rate: 1,
            }],
            action: ActionConfig {
                on_exceed: Action::Reject,
                ban: None,
            },
        }],
    };
    let governor = Governor::builder()
        .with_config(config)
        .with_storage(Arc::new(limiteron::storage::MemoryStorage::new()))
        .with_ban_storage(Arc::new(limiteron::storage::MemoryBanStorage::new()))
        .with_tracer(Arc::new(Tracer::with_otlp_sink(true, Some(Arc::new(sink)))))
        .build()
        .await
        .expect("Failed to create governor");

    let ctx = RequestContext::new()
        .with_header("x-user-id", "u_span_reject")
        .with_method("GET");
    // 1 放行（链上）+ 2 拒绝：第 2 次链上拒绝并写缓存，第 3 次负缓存命中
    for _ in 0..3 {
        let _ = governor.check(&ctx).await;
    }

    let spans = wait_for_span_exports(&transport, 3).await;
    assert_eq!(
        spans.len(),
        3,
        "three checks should export exactly three spans"
    );

    let outcomes: Vec<Option<String>> = spans
        .iter()
        .map(|s| span_attribute(s, "rule.outcome"))
        .collect();
    let rule_ids: Vec<Option<String>> =
        spans.iter().map(|s| span_attribute(s, "rule.id")).collect();

    let outcome_refs: Vec<Option<&str>> = outcomes.iter().map(|o| o.as_deref()).collect();
    let rule_id_refs: Vec<Option<&str>> = rule_ids.iter().map(|r| r.as_deref()).collect();

    assert_eq!(
        outcome_refs,
        vec![Some("allowed"), Some("rejected"), Some("rejected")],
        "the span result sequence for three checks should be allowed/rejected/rejected"
    );
    assert_eq!(
        rule_id_refs,
        vec![None, Some("test_rule"), Some("test_rule")],
        "rejected spans (chain and negative-cache hits) should be attributed to the deciding rule; allow spans do not fabricate rule.id"
    );
}

/// 禁用 tracer 时 check 正常返回且不产生导出（观测缺席不阻塞决策路径）
#[tokio::test]
async fn test_governor_check_with_disabled_tracer_exports_nothing() {
    let transport = Arc::new(InMemoryTransport::new());
    let exporter = OtlpSpanExporter::new(
        "limiteron-test",
        transport.clone(),
        "http://mock-collector:4318/v1/traces",
    );
    let (sink, worker) = OtlpSpanExporter::spawn_worker(exporter, 64);
    tokio::spawn(worker.run());

    let governor = build_governor_with_tracer(Arc::new(Tracer::with_otlp_sink(
        false,
        Some(Arc::new(sink)),
    )))
    .await;

    let ctx = RequestContext::new()
        .with_header("x-user-id", "u_span_off")
        .with_method("GET");
    let result = governor.check(&ctx).await;
    assert!(
        result.is_ok(),
        "disabling the tracer should not affect decisions: {result:?}"
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        transport.requests().is_empty(),
        "no OTLP export should be produced when the tracer is disabled"
    );
}
