// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 规则级故障策略
//!
//! 决策链执行出错时，按规则声明的语义裁决：上抛错误、合成拒绝、或放行并告警。

use serde::{Deserialize, Serialize};

/// 规则级故障策略
///
/// 与组件级 `FallbackStrategy`（按存储组件配置降级）正交：
/// 本策略按单条规则声明链执行 `Err` 的裁决语义。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailurePolicy {
    /// 上抛错误，由调用方裁决（默认，等价现状）
    #[default]
    PropagateError,
    /// 合成拒绝（fail-closed）
    FailClosed,
    /// 视同放行并发出降级告警（fail-open with alert）
    FailOpenWithAlert,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_fail_closed() {
        let policy: FailurePolicy = serde_json::from_str("\"fail_closed\"").unwrap();
        assert_eq!(policy, FailurePolicy::FailClosed);
    }

    #[test]
    fn deserialize_fail_open_with_alert() {
        let policy: FailurePolicy = serde_json::from_str("\"fail_open_with_alert\"").unwrap();
        assert_eq!(policy, FailurePolicy::FailOpenWithAlert);
    }

    #[test]
    fn default_is_propagate_error() {
        assert_eq!(FailurePolicy::default(), FailurePolicy::PropagateError);
        let policy: FailurePolicy = serde_json::from_str("\"propagate_error\"").unwrap();
        assert_eq!(policy, FailurePolicy::PropagateError);
    }
}
