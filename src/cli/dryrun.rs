// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! `limiteron-cli dry-run` 流量影响预演
//!
//! 逐行回放 JSONL trace，对 baseline / candidate 两份隔离内存 Governor
//! 各执行一次 `check()`，比对拒绝/放行/封禁/错误计数与 per-rule 迁移。
//! 预演只读、零外部副作用（不开 event-system / webhook 外发）。

use serde::{Deserialize, Serialize};

/// 流量 trace 单条记录（JSONL 反序列化目标）
#[derive(Debug, Clone, Deserialize)]
pub struct TrafficRecord {
    /// 时间戳（毫秒）。仅作记录语义，回放不按此推进时钟（见 USER_GUIDE）。
    pub ts_ms: u64,
    /// 请求标识符
    #[serde(default)]
    pub identifier: TrafficIdentifier,
    /// 请求路径
    pub path: String,
    /// HTTP 方法（缺省 GET）
    #[serde(default = "default_method")]
    pub method: String,
    /// 请求成本（缺省 1）
    #[serde(default = "default_cost")]
    pub cost: u64,
}

/// 请求标识符（各维度可选）
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TrafficIdentifier {
    /// 用户 ID
    #[serde(default)]
    pub user_id: Option<String>,
    /// IP 地址
    #[serde(default)]
    pub ip: Option<String>,
    /// API key
    #[serde(default)]
    pub api_key: Option<String>,
    /// 设备 ID
    #[serde(default)]
    pub device_id: Option<String>,
}

fn default_method() -> String {
    "GET".to_string()
}

fn default_cost() -> u64 {
    1
}

/// 归因模式（decision-trace feature 开关）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AttributionMode {
    /// 已启用（per-rule 归因可用）
    Enabled,
    /// 未启用（per-rule 为空）
    Disabled,
}

/// 四态决策计数
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct DecisionCounts {
    /// 放行
    pub allowed: u64,
    /// 拒绝
    pub rejected: u64,
    /// 封禁
    pub banned: u64,
    /// 执行错误
    pub error: u64,
}

/// 计数差值（candidate - baseline）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct CountDelta {
    /// 放行差值
    pub allowed: i64,
    /// 拒绝差值
    pub rejected: i64,
    /// 封禁差值
    pub banned: i64,
    /// 错误差值
    pub error: i64,
}

/// 单条规则的拒绝迁移
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RuleShift {
    /// 规则 ID
    pub rule_id: String,
    /// baseline 拒绝数
    pub baseline_rejections: u64,
    /// candidate 拒绝数
    pub candidate_rejections: u64,
}

/// 双配置比对报告
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DryRunReport {
    /// 成功回放的请求数
    pub total_requests: usize,
    /// 畸形行跳过数（显性化，不静默丢弃）
    pub skipped_lines: usize,
    /// 归因模式
    pub attribution: AttributionMode,
    /// baseline 计数
    pub baseline: DecisionCounts,
    /// candidate 计数
    pub candidate: DecisionCounts,
    /// candidate - baseline 差值
    pub delta: CountDelta,
    /// per-rule 拒绝迁移（归因启用时填充）
    pub per_rule: Vec<RuleShift>,
}

/// 构建隔离内存 Governor（预演零外部依赖、零外发事件）
async fn build_replay_governor(config: &crate::config::FlowControlConfig) -> crate::Governor {
    let builder = crate::Governor::builder()
        .with_config(config.clone())
        .with_storage(std::sync::Arc::new(crate::storage::MemoryStorage::new()))
        .with_ban_storage(std::sync::Arc::new(crate::storage::MemoryBanStorage::new()))
        .with_l1_cache_enabled(false);
    #[cfg(feature = "decision-trace")]
    let builder = builder.with_decision_trace(true); // 归因开启：per-rule 迁移可用
    builder
        .build()
        .await
        .expect("in-memory replay governor must build")
}

/// 把 trace 记录映射为请求上下文
fn to_context(record: &TrafficRecord) -> crate::matchers::traits::RequestContext {
    let mut ctx = crate::matchers::traits::RequestContext::new();
    ctx.user_id = record.identifier.user_id.clone();
    let ip = record
        .identifier
        .ip
        .clone()
        .unwrap_or_else(|| "127.0.0.1".to_string());
    ctx.ip = Some(ip.clone());
    ctx.client_ip = Some(ip);
    ctx.api_key = record.identifier.api_key.clone();
    ctx.device_id = record.identifier.device_id.clone();
    ctx.path = record.path.clone();
    ctx.method = record.method.clone();
    ctx
}

fn count_decision(counts: &mut DecisionCounts, decision: &crate::Decision) {
    match decision {
        crate::Decision::Allowed(_) => counts.allowed += 1,
        crate::Decision::Rejected(_) => counts.rejected += 1,
        crate::Decision::Banned(_) => counts.banned += 1,
    }
}

fn diff(a: u64, b: u64) -> i64 {
    b as i64 - a as i64
}

/// 回放流量：对 baseline / candidate 各执行一次 `check()` 并聚合计数
///
/// `records` 为已解析记录迭代器；畸形行的跳过计数由调用方（解析层）累加后
/// 经 `skipped_lines` 传入。
pub fn replay(
    baseline: &crate::config::FlowControlConfig,
    candidate: &crate::config::FlowControlConfig,
    body: &str,
) -> DryRunReport {
    // 在独立线程建 Runtime：本函数可能被异步 handler 调用（已处于 tokio
    // runtime 内），嵌套 block_on 会 panic；独立线程彻底隔离运行时。
    // JSONL 逐行惰性解析，畸形行 skipped +1，不收集全量 Vec<TrafficRecord>。
    let body_owned = body.to_string();
    let baseline_owned = baseline.clone();
    let candidate_owned = candidate.clone();
    let result = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime for replay");
        let base_gov = rt.block_on(build_replay_governor(&baseline_owned));
        let cand_gov = rt.block_on(build_replay_governor(&candidate_owned));

        let mut skipped_lines = 0usize;
        let records = body_owned.lines().filter_map(|line| {
            if line.trim().is_empty() {
                return None;
            }
            match serde_json::from_str::<TrafficRecord>(line) {
                Ok(r) => Some(r),
                Err(_) => {
                    skipped_lines += 1;
                    None
                }
            }
        });

        let mut base_counts = DecisionCounts::default();
        let mut cand_counts = DecisionCounts::default();
        #[cfg(feature = "decision-trace")]
        let mut base_rule: std::collections::BTreeMap<String, u64> =
            std::collections::BTreeMap::new();
        #[cfg(feature = "decision-trace")]
        let mut cand_rule: std::collections::BTreeMap<String, u64> =
            std::collections::BTreeMap::new();

        let mut total = 0usize;
        for rec in records {
            total += 1;
            let ctx = to_context(&rec);
            let base = rt.block_on(base_gov.check(&ctx));
            let cand = rt.block_on(cand_gov.check(&ctx));
            match &base {
                Ok(d) => {
                    count_decision(&mut base_counts, d);
                    #[cfg(feature = "decision-trace")]
                    if let crate::Decision::Rejected(m) = d
                        && let Some(trace) = &m.trace
                        && let Some(rule) = &trace.rejecting_rule
                    {
                        *base_rule.entry(rule.clone()).or_default() += 1;
                    }
                }
                Err(_) => base_counts.error += 1,
            }
            match &cand {
                Ok(d) => {
                    count_decision(&mut cand_counts, d);
                    #[cfg(feature = "decision-trace")]
                    if let crate::Decision::Rejected(m) = d
                        && let Some(trace) = &m.trace
                        && let Some(rule) = &trace.rejecting_rule
                    {
                        *cand_rule.entry(rule.clone()).or_default() += 1;
                    }
                }
                Err(_) => cand_counts.error += 1,
            }
        }

        #[cfg(feature = "decision-trace")]
        let (attribution, per_rule) = {
            let mut shifts = Vec::new();
            let mut ids: std::collections::BTreeSet<String> = base_rule.keys().cloned().collect();
            ids.extend(cand_rule.keys().cloned());
            for id in ids {
                shifts.push(RuleShift {
                    rule_id: id.clone(),
                    baseline_rejections: base_rule.get(&id).copied().unwrap_or(0),
                    candidate_rejections: cand_rule.get(&id).copied().unwrap_or(0),
                });
            }
            (AttributionMode::Enabled, shifts)
        };
        #[cfg(not(feature = "decision-trace"))]
        let (attribution, per_rule) = (AttributionMode::Disabled, Vec::new());

        DryRunReport {
            total_requests: total,
            skipped_lines,
            attribution,
            baseline: base_counts,
            candidate: cand_counts,
            delta: CountDelta {
                allowed: diff(base_counts.allowed, cand_counts.allowed),
                rejected: diff(base_counts.rejected, cand_counts.rejected),
                banned: diff(base_counts.banned, cand_counts.banned),
                error: diff(base_counts.error, cand_counts.error),
            },
            per_rule,
        }
    })
    .join()
    .expect("replay worker thread must not panic");
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_record() {
        let line = r#"{"ts_ms":1234,"identifier":{"user_id":"u1","ip":"10.0.0.1","api_key":"k1","device_id":"d1"},"path":"/api","method":"POST","cost":10}"#;
        let rec: TrafficRecord = serde_json::from_str(line).expect("full record parses");
        assert_eq!(rec.ts_ms, 1234);
        assert_eq!(rec.identifier.user_id.as_deref(), Some("u1"));
        assert_eq!(rec.identifier.ip.as_deref(), Some("10.0.0.1"));
        assert_eq!(rec.method, "POST");
        assert_eq!(rec.cost, 10);
    }

    #[test]
    fn parse_record_with_defaults() {
        let line = r#"{"ts_ms":1,"path":"/health"}"#;
        let rec: TrafficRecord = serde_json::from_str(line).expect("defaulted record parses");
        assert_eq!(rec.method, "GET");
        assert_eq!(rec.cost, 1);
        assert!(rec.identifier.user_id.is_none());
    }

    #[test]
    fn malformed_record_returns_err() {
        assert!(serde_json::from_str::<TrafficRecord>("not json").is_err());
        // 缺必填 path → Err
        assert!(serde_json::from_str::<TrafficRecord>(r#"{"ts_ms":1}"#).is_err());
    }

    fn fixture_config(capacity: u64) -> crate::config::FlowControlConfig {
        serde_json::from_str(&format!(
            r#"{{
                "version": "1.0.0",
                "global": {{ "storage": "memory", "cache": "memory", "metrics": "none" }},
                "rules": [
                    {{
                        "id": "api_rule",
                        "name": "api",
                        "priority": 100,
                        "matchers": [{{ "type": "User", "user_ids": ["*"] }}],
                        "limiters": [{{ "type": "TokenBucket", "capacity": {capacity}, "refill_rate": 1 }}],
                        "action": {{ "on_exceed": "reject" }}
                    }},
                    {{
                        "id": "path_rule",
                        "name": "path",
                        "priority": 90,
                        "matchers": [{{ "type": "Ip", "ip_ranges": ["10.0.0.0/8"] }}],
                        "limiters": [{{ "type": "TokenBucket", "capacity": 100, "refill_rate": 10 }}],
                        "action": {{ "on_exceed": "reject" }}
                    }}
                ]
            }}"#
        ))
        .expect("fixture config parses")
    }

    fn fixture_trace() -> String {
        (1..=3)
            .map(|i| format!(r#"{{"ts_ms":{i},"identifier":{{"user_id":"u{i}"}},"path":"/api"}}"#))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn replay_counts_all_states() {
        // baseline 容量 1，candidate 容量 10：3 条 trace
        let baseline = fixture_config(1);
        let candidate = fixture_config(10);
        let report = replay(&baseline, &candidate, &fixture_trace());
        assert_eq!(report.total_requests, 3);
        assert_eq!(report.skipped_lines, 0);
        assert_eq!(
            report.baseline.allowed, 1,
            "baseline={:?} candidate={:?}",
            report.baseline, report.candidate
        );
        assert_eq!(
            report.baseline.rejected, 2,
            "baseline={:?}",
            report.baseline
        );
        assert_eq!(report.candidate.allowed, 3);
        assert_eq!(report.candidate.rejected, 0);
        assert_eq!(report.delta.rejected, -2);
    }

    #[cfg(feature = "decision-trace")]
    #[test]
    fn replay_per_rule_attribution_when_trace_enabled() {
        // 需在 Governor 开启 trace 才能归因；replay 内部 builder 默认关闭，
        // 故此处仅验证 attribution 模式由 feature 决定且 per_rule 结构合法。
        let baseline = fixture_config(1);
        let candidate = fixture_config(10);
        let report = replay(&baseline, &candidate, &fixture_trace());
        assert_eq!(report.attribution, AttributionMode::Enabled);
        // trace 开启后拒绝归因填充 per_rule（baseline 2 次拒绝 → api_rule）
        assert!(
            report
                .per_rule
                .iter()
                .any(|s| s.rule_id.starts_with("api_rule") && s.baseline_rejections == 2),
            "per_rule must carry rejecting_rule attribution: {:?}",
            report.per_rule
        );
    }

    #[cfg(not(feature = "decision-trace"))]
    #[test]
    fn replay_attribution_disabled_without_feature() {
        let baseline = fixture_config(1);
        let candidate = fixture_config(10);
        let report = replay(&baseline, &candidate, &fixture_trace());
        assert_eq!(report.attribution, AttributionMode::Disabled);
        assert!(report.per_rule.is_empty());
    }
}
