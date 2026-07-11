// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 历史记录相关类型

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 配置变更来源
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChangeSource {
    /// 手动触发
    Manual { operator: String },
    /// 自动检测（轮询）
    Poll,
    /// 自动检测（Watch）
    Watch,
    /// API触发
    Api,
    /// 重新加载
    Reload,
    /// 回滚操作
    Rollback { target_version: String },
}

/// 配置变更记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigChangeRecord {
    pub timestamp: DateTime<Utc>,
    pub old_version: Option<String>,
    pub new_version: String,
    pub old_hash: Option<String>,
    pub new_hash: String,
    pub source: ChangeSource,
    pub changes: Vec<String>,
}

/// 配置变更历史
#[derive(Debug, Clone)]
pub struct ConfigHistory {
    records: Vec<ConfigChangeRecord>,
    max_records: usize,
}

impl ConfigHistory {
    pub fn new(max_records: usize) -> Self {
        Self {
            records: Vec::with_capacity(max_records),
            max_records,
        }
    }

    pub fn max_records(&self) -> usize {
        self.max_records
    }

    pub fn add_record(&mut self, record: ConfigChangeRecord) {
        self.records.push(record);
        if self.records.len() > self.max_records {
            self.records.remove(0);
        }
    }

    pub fn get_records(&self) -> &[ConfigChangeRecord] {
        &self.records
    }

    pub fn get_latest(&self) -> Option<&ConfigChangeRecord> {
        self.records.last()
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }
}

impl Default for ConfigHistory {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_record(version: &str) -> ConfigChangeRecord {
        ConfigChangeRecord {
            timestamp: Utc::now(),
            old_version: Some("v1".into()),
            new_version: version.into(),
            old_hash: Some("abc".into()),
            new_hash: "def".into(),
            source: ChangeSource::Manual {
                operator: "admin".into(),
            },
            changes: vec!["change1".into()],
        }
    }

    #[test]
    fn test_history_new() {
        let history = ConfigHistory::new(10);
        assert_eq!(history.max_records(), 10);
        assert!(history.get_records().is_empty());
    }

    #[test]
    fn test_history_default() {
        let history = ConfigHistory::default();
        assert_eq!(history.max_records(), 100);
    }

    #[test]
    fn test_history_add_record() {
        let mut history = ConfigHistory::new(10);
        history.add_record(sample_record("v2"));
        assert_eq!(history.get_records().len(), 1);
    }

    #[test]
    fn test_history_max_records_trim() {
        let mut history = ConfigHistory::new(2);
        history.add_record(sample_record("v2"));
        history.add_record(sample_record("v3"));
        history.add_record(sample_record("v4"));
        assert_eq!(history.get_records().len(), 2);
        assert_eq!(history.get_records()[0].new_version, "v3");
    }

    #[test]
    fn test_history_get_latest() {
        let mut history = ConfigHistory::new(10);
        assert!(history.get_latest().is_none());
        history.add_record(sample_record("v2"));
        assert!(history.get_latest().is_some());
        assert_eq!(history.get_latest().unwrap().new_version, "v2");
    }

    #[test]
    fn test_history_clear() {
        let mut history = ConfigHistory::new(10);
        history.add_record(sample_record("v2"));
        history.clear();
        assert!(history.get_records().is_empty());
    }

    #[test]
    fn test_change_source_manual() {
        let source = ChangeSource::Manual {
            operator: "admin".into(),
        };
        match source {
            ChangeSource::Manual { operator } => assert_eq!(operator, "admin"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_change_source_rollback() {
        let source = ChangeSource::Rollback {
            target_version: "v1".into(),
        };
        match source {
            ChangeSource::Rollback { target_version } => assert_eq!(target_version, "v1"),
            _ => panic!("wrong variant"),
        }
    }
}
