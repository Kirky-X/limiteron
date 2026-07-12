// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Storage trait definitions
//!
//! This module provides the Storage, QuotaStorage, and BanStorage traits
//! that were previously defined in storage.rs.

// 子模块
#[cfg(feature = "parallel-checker")]
pub mod parallel_checker;

// 重新导出 parallel_checker 模块的公共类型
#[cfg(feature = "parallel-checker")]
pub use parallel_checker::ParallelBanChecker;

use crate::error::{ConsumeResult, StorageError};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// 存储接口
#[async_trait]
pub trait Storage: Send + Sync {
    /// 获取值
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError>;

    /// 设置值
    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError>;

    /// 删除值
    async fn delete(&self, key: &str) -> Result<(), StorageError>;
}

/// 配额信息
#[derive(Debug, Clone)]
pub struct QuotaInfo {
    /// 已消耗配额
    pub consumed: u64,
    /// 配额上限
    pub limit: u64,
    /// 窗口开始时间
    pub window_start: DateTime<Utc>,
    /// 窗口结束时间
    pub window_end: DateTime<Utc>,
}

/// 配额存储接口
#[async_trait]
pub trait QuotaStorage: Send + Sync {
    /// 获取配额信息
    async fn get_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError>;

    /// 消费配额
    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
        limit: u64,
        window: Duration,
    ) -> Result<ConsumeResult, StorageError>;

    /// 重置配额
    async fn reset(
        &self,
        user_id: &str,
        resource: &str,
        limit: u64,
        window: Duration,
    ) -> Result<(), StorageError>;
}

/// 封禁目标类型
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum BanTarget {
    /// IP地址封禁
    #[serde(rename = "ip")]
    Ip(String),
    /// 用户ID封禁
    #[serde(rename = "user")]
    UserId(String),
    /// MAC地址封禁
    #[serde(rename = "mac")]
    Mac(String),
    /// 地理位置封禁（国家代码，ISO 3166-1 alpha-2）
    #[serde(rename = "geo")]
    Geo { country_code: String },
}

/// 封禁记录
#[derive(Debug, Clone)]
pub struct BanRecord {
    /// 封禁目标
    pub target: BanTarget,
    /// 封禁次数
    pub ban_times: u32,
    /// 封禁时长
    pub duration: Duration,
    /// 封禁时间
    pub banned_at: DateTime<Utc>,
    /// 过期时间
    pub expires_at: DateTime<Utc>,
    /// 是否手动封禁
    pub is_manual: bool,
    /// 封禁原因
    pub reason: String,
}

/// 封禁历史
#[derive(Debug, Clone)]
pub struct BanHistory {
    /// 封禁次数
    pub ban_times: u32,
    /// 最后封禁时间
    pub last_banned_at: DateTime<Utc>,
}

/// 封禁存储接口
#[async_trait]
pub trait BanStorage: Send + Sync {
    /// 检查是否被封禁
    async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError>;

    /// 获取封禁记录（别名）
    async fn get_ban(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
        self.is_banned(target).await
    }

    /// 保存封禁记录（别名）
    async fn add_ban(&self, record: &BanRecord) -> Result<(), StorageError> {
        self.save(record).await
    }

    /// 保存封禁记录
    async fn save(&self, record: &BanRecord) -> Result<(), StorageError>;

    /// 获取封禁历史
    async fn get_history(&self, target: &BanTarget) -> Result<Option<BanHistory>, StorageError>;

    /// 增加封禁次数
    async fn increment_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError>;

    /// 获取封禁次数
    async fn get_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError>;

    /// 移除封禁记录
    async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError>;

    /// 清理过期封禁
    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError>;

    /// 列出所有封禁记录（支持分页）
    ///
    /// # 参数
    /// - `active_only`: 是否只返回未过期的封禁
    /// - `offset`: 分页偏移
    /// - `limit`: 每页数量限制
    ///
    /// # 返回
    /// - 封禁记录列表
    async fn list_bans(
        &self,
        active_only: bool,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BanRecord>, StorageError>;

    /// 获取Any引用（用于类型转换）
    fn as_any(&self) -> &dyn std::any::Any;
}

// ============================================================================
// In-Memory Storage Implementations (Default Dependencies)
// ============================================================================
// These implementations are provided for the "out-of-the-box" pattern (new())
// where components need default dependencies without external configuration.

use ahash::AHashMap as HashMap;
use tokio::sync::RwLock;

/// In-memory storage implementation for Storage trait
///
/// This is a simple in-memory key-value store with TTL support.
/// It is suitable for testing, development, or single-instance deployments.
///
/// **Note**: This implementation is not suitable for production use with
/// multiple instances as data is not shared across processes.
pub struct MemoryStorage {
    /// Key-value data storage
    data: RwLock<HashMap<String, String>>,
    /// Expiration times (key -> expiration timestamp in seconds)
    expiration: RwLock<HashMap<String, u64>>,
}

/// In-memory ban storage implementation for BanStorage trait
///
/// This is a simple in-memory ban record store.
/// It is suitable for testing, development, or single-instance deployments.
///
/// **Note**: This implementation is not suitable for production use with
/// multiple instances as data is not shared across processes.
pub struct MemoryBanStorage {
    /// Ban records storage
    bans: RwLock<HashMap<BanTarget, BanRecord>>,
    /// Expiration tracking (target -> expires_at timestamp)
    expiration: RwLock<HashMap<BanTarget, i64>>,
}

mod storage_impl;
