//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 存储抽象层
//!
//! 定义存储接口和基本实现。

use crate::error::{ConsumeResult, StorageError};
use async_trait::async_trait;

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
        window: std::time::Duration,
    ) -> Result<ConsumeResult, StorageError>;

    /// 重置配额
    async fn reset(
        &self,
        user_id: &str,
        resource: &str,
        limit: u64,
        window: std::time::Duration,
    ) -> Result<(), StorageError>;
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

    /// 获取Any引用（用于类型转换）
    fn as_any(&self) -> &dyn std::any::Any;
}

/// 配额信息
#[derive(Debug, Clone)]
pub struct QuotaInfo {
    pub consumed: u64,
    pub limit: u64,
    pub window_start: chrono::DateTime<chrono::Utc>,
    pub window_end: chrono::DateTime<chrono::Utc>,
}

/// 封禁目标
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum BanTarget {
    Ip(String),
    UserId(String),
    Mac(String),
}

/// 封禁范围
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BanScope {
    Ip,
    UserId,
    Mac,
}

/// 封禁记录
#[derive(Debug, Clone)]
pub struct BanRecord {
    pub target: BanTarget,
    pub ban_times: u32,
    pub duration: std::time::Duration,
    pub banned_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub is_manual: bool,
    pub reason: String,
}

/// 封禁历史
#[derive(Debug, Clone)]
pub struct BanHistory {
    pub ban_times: u32,
    pub last_banned_at: chrono::DateTime<chrono::Utc>,
}

/// 封禁配置
#[derive(Debug, Clone)]
pub struct BanConfig {
    pub initial_duration: std::time::Duration,
    pub backoff_multiplier: f64,
    pub max_duration: std::time::Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ban_target_equality() {
        let target1 = BanTarget::UserId("user1".to_string());
        let target2 = BanTarget::UserId("user1".to_string());
        assert_eq!(target1, target2);
    }

    #[test]
    fn test_ban_target_hash() {
        let target1 = BanTarget::UserId("user1".to_string());
        let target2 = BanTarget::UserId("user1".to_string());
        use std::hash::{Hash, Hasher};
        let mut hasher1 = std::collections::hash_map::DefaultHasher::new();
        let mut hasher2 = std::collections::hash_map::DefaultHasher::new();
        target1.hash(&mut hasher1);
        target2.hash(&mut hasher2);
        assert_eq!(hasher1.finish(), hasher2.finish());
    }
}
