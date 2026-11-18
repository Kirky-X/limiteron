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
}
