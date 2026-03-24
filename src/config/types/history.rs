// Copyright (c) 2026, Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

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
