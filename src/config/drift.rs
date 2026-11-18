// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 配置漂移检测
//!
//! 运行时配置语义哈希与加载源文件哈希的比对结果。用语义哈希（而非文件字节哈希）
//! 比对，格式化差异与注释不构成漂移。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 运行时配置与持久化源的语义哈希比对结果
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigDrift {
    /// 运行时配置哈希（`FlowControlConfig::compute_hash()`）
    pub runtime_hash: String,
    /// 加载时的源配置语义哈希（无源文件时为 `None`）
    pub source_hash: Option<String>,
    /// 检测时间
    pub detected_at: DateTime<Utc>,
}

impl ConfigDrift {
    /// 构造一次检测结果
    pub fn new(runtime_hash: String, source_hash: Option<String>) -> Self {
        Self {
            runtime_hash,
            source_hash,
            detected_at: Utc::now(),
        }
    }

    /// 是否检测到漂移
    ///
    /// 无源文件（`source_hash == None`，如容器注入配置内容）时无漂移可比，
    /// 返回 `false`；两哈希不等时为 `true`。
    pub fn is_drifted(&self) -> bool {
        self.source_hash
            .as_ref()
            .is_some_and(|source| source != &self.runtime_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trip() {
        let drift = ConfigDrift::new("abc123".to_string(), Some("def456".to_string()));
        let json = serde_json::to_string(&drift).unwrap();
        let back: ConfigDrift = serde_json::from_str(&json).unwrap();
        assert_eq!(drift, back);
    }

    #[test]
    fn none_source_means_no_drift() {
        let drift = ConfigDrift::new("abc123".to_string(), None);
        assert!(!drift.is_drifted());
    }

    #[test]
    fn equal_hashes_no_drift_and_mismatch_drifts() {
        let same = ConfigDrift::new("abc".to_string(), Some("abc".to_string()));
        assert!(!same.is_drifted());
        let diff = ConfigDrift::new("abc".to_string(), Some("xyz".to_string()));
        assert!(diff.is_drifted());
    }
}
