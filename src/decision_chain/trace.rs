// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 决策归因 trace 类型
//!
//! 拒绝结果可携带结构化归因（哪条规则、哪个节点、级联上其他规则的判定如何）。
//! trace 默认关闭；开启后热路径每次检查会有 O(rules×nodes) 的快照分配。

use serde::{Deserialize, Serialize};

/// 单个限流器节点的判定快照（一次 `check_with_trace` 产出）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeOutcome {
    /// 节点 ID
    pub node_id: String,
    /// 节点名称
    pub node_name: String,
    /// 该节点是否放行
    pub allowed: bool,
}

/// 级联中单条规则的判定结果
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CascadeOutcome {
    /// 放行
    Allowed,
    /// 拒绝
    Rejected,
    /// 封禁
    Banned,
    /// 执行出错
    Error,
    /// 未命中而跳过
    Skipped,
}

/// 级联中单条规则的判定条目
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CascadeEntry {
    /// 规则 ID
    pub rule_id: String,
    /// 该规则的判定结果
    pub outcome: CascadeOutcome,
    /// 链内裁决节点名（仅拒绝时有值）
    pub rejecting_node: Option<String>,
}

/// 拒绝归因快照（挂载在 `RejectionMetadata.trace`）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionTrace {
    /// 本次命中的全部规则 ID（按级联顺序）
    pub matched_rules: Vec<String>,
    /// 最终裁决规则
    pub rejecting_rule: Option<String>,
    /// 链内裁决节点名
    pub rejecting_node: Option<String>,
    /// 租户限定后的决策键（`identifier.key()`）
    pub dimension: Option<String>,
    /// 级联判定条目
    pub cascade: Vec<CascadeEntry>,
    /// 是否命中 L1 负缓存
    pub l1_cache_hit: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_outcome_serializes_as_lowercase_string() {
        assert_eq!(
            serde_json::to_string(&CascadeOutcome::Allowed).unwrap(),
            "\"allowed\""
        );
        assert_eq!(
            serde_json::to_string(&CascadeOutcome::Rejected).unwrap(),
            "\"rejected\""
        );
        assert_eq!(
            serde_json::to_string(&CascadeOutcome::Banned).unwrap(),
            "\"banned\""
        );
        assert_eq!(
            serde_json::to_string(&CascadeOutcome::Error).unwrap(),
            "\"error\""
        );
        assert_eq!(
            serde_json::to_string(&CascadeOutcome::Skipped).unwrap(),
            "\"skipped\""
        );
    }

    #[test]
    fn decision_trace_serde_round_trip() {
        let trace = DecisionTrace {
            matched_rules: vec!["rule-a".to_string(), "rule-b".to_string()],
            rejecting_rule: Some("rule-a".to_string()),
            rejecting_node: Some("token-bucket".to_string()),
            dimension: Some("tenant-1:user-42".to_string()),
            cascade: vec![CascadeEntry {
                rule_id: "rule-a".to_string(),
                outcome: CascadeOutcome::Rejected,
                rejecting_node: Some("token-bucket".to_string()),
            }],
            l1_cache_hit: false,
        };

        let json = serde_json::to_string(&trace).unwrap();
        let back: DecisionTrace = serde_json::from_str(&json).unwrap();
        assert_eq!(trace, back);
    }

    #[test]
    fn node_outcome_serde_round_trip() {
        let outcome = NodeOutcome {
            node_id: "node-1".to_string(),
            node_name: "sliding-window".to_string(),
            allowed: true,
        };
        let json = serde_json::to_string(&outcome).unwrap();
        let back: NodeOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(outcome, back);
    }
}
