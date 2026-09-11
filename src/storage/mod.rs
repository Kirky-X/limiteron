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
    /// CIDR 网段封禁（T604，IPv4/IPv6，如 "10.0.0.0/8"、"2001:db8::/32"）
    ///
    /// 命中语义：查询目标为 [`BanTarget::Ip`] 且精确未命中时，按
    /// 最长前缀匹配网段封禁记录（见 MemoryBanStorage::is_banned 两级检查）。
    #[serde(rename = "cidr")]
    Cidr(String),
}

impl BanTarget {
    /// 网段封禁是否包含指定 IP（仅 [`BanTarget::Cidr`] 有意义）
    ///
    /// 解析失败的 CIDR 串视为不包含（不 panic、不放行语义歧义）。
    pub fn contains_ip(&self, ip: &std::net::IpAddr) -> bool {
        match self {
            BanTarget::Cidr(cidr) => cidr
                .parse::<ipnet::IpNet>()
                .map(|net| net.contains(ip))
                .unwrap_or(false),
            _ => false,
        }
    }

    /// 网段前缀长度（仅 [`BanTarget::Cidr`]；解析失败返回 `None`）
    ///
    /// 用于多网段命中时的最长前缀优先选择。
    pub fn prefix_len(&self) -> Option<u8> {
        match self {
            BanTarget::Cidr(cidr) => cidr
                .parse::<ipnet::IpNet>()
                .ok()
                .map(|net| net.prefix_len()),
            _ => None,
        }
    }
}

/// 取 BanTarget 的可限定值（T602）
///
/// 返回 `Some(value)` 表示该变体携带可被租户命名空间限定的字符串值；
/// `Geo` 变体按国家码全局生效，返回 `None`。
#[cfg(feature = "multi-tenant")]
pub(crate) fn ban_target_value(target: &BanTarget) -> Option<&str> {
    match target {
        BanTarget::Ip(v) | BanTarget::UserId(v) | BanTarget::Mac(v) => Some(v),
        // Geo 按国家码、Cidr 按网段全局生效，不做租户限定
        BanTarget::Geo { .. } | BanTarget::Cidr(_) => None,
    }
}

/// 以租户命名空间限定封禁目标（T602）
///
/// 保持 [`BanTarget`] 变体类型不变，仅将字符串值替换为
/// `namespace.qualify_key(value)`；`Geo` 变体不限定（返回 `None`）。
#[cfg(feature = "multi-tenant")]
pub(crate) fn qualify_ban_target(
    target: &BanTarget,
    namespace: &crate::tenant::Namespace,
) -> Option<BanTarget> {
    let qualified = namespace.qualify_key(ban_target_value(target)?);
    Some(match target {
        BanTarget::Ip(_) => BanTarget::Ip(qualified),
        BanTarget::UserId(_) => BanTarget::UserId(qualified),
        BanTarget::Mac(_) => BanTarget::Mac(qualified),
        // Geo/Cidr 全局生效，不参与租户限定
        BanTarget::Geo { .. } | BanTarget::Cidr(_) => return None,
    })
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

    /// 插入或更新封禁记录，并将 `ban_times` 在**已存值**基础上原子 +1，
    /// 返回写入后的最终计数
    ///
    /// 用于 `create_ban` 等需要精确计数的写入路径：并发创建同一目标时，
    /// 计数按「已存值 + 1」收敛，不会被调用方持有的过期快照覆盖
    /// （ban-5）。默认实现为「读取当前计数 → 整条保存」的两步非原子
    /// 版本；Memory 后端以单写锁、DBNexus 后端以 UPSERT + 自增 SQL 提供
    /// 真正的原子语义，其他实现者可按自身能力重写。
    async fn upsert_ban_record(&self, record: &BanRecord) -> Result<u64, StorageError> {
        let current = u32::try_from(self.get_ban_times(&record.target).await?)
            .unwrap_or(u32::MAX)
            .saturating_add(1);
        let mut stored = record.clone();
        stored.ban_times = stored.ban_times.max(current);
        self.save(&stored).await?;
        Ok(u64::from(stored.ban_times))
    }

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
