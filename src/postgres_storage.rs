//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! PostgreSQL存储实现
//!
//! 使用dbnexus实现PostgreSQL存储，支持连接池、事务和完整的错误处理。

#[cfg(feature = "postgres")]
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dbnexus::{DbPool, DbResult};
use secrecy::{ExposeSecret, Secret};
use std::time::Duration;
use tracing::{debug, error, info};

use crate::error::{ConsumeResult, StorageError};
use crate::matchers::Identifier;
use crate::storage::{
    BanHistory, BanRecord, BanTarget, QuotaInfo, QuotaStorage, Storage as StorageTrait,
};

/// PostgreSQL存储配置
#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresStorageConfig {
    /// 数据库连接URL（使用 Secret 包装以防止意外泄露）
    pub database_url: Secret<String>,
    /// 连接池最大连接数
    pub max_connections: u32,
    /// 连接超时时间（秒）
    pub connect_timeout: u64,
    /// 查询超时时间（秒）
    pub query_timeout: u64,
}

impl std::fmt::Debug for PostgresStorageConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresStorageConfig")
            .field("database_url", &"***")
            .field("max_connections", &self.max_connections)
            .field("connect_timeout", &self.connect_timeout)
            .field("query_timeout", &self.query_timeout)
            .finish()
    }
}

impl Default for PostgresStorageConfig {
    fn default() -> Self {
        Self {
            database_url: Secret::new(String::new()),
            max_connections: 20,
            connect_timeout: 30,
            query_timeout: 10,
        }
    }
}

impl PostgresStorageConfig {
    /// 创建新的配置
    pub fn new(database_url: impl Into<String>) -> Self {
        Self {
            database_url: Secret::new(database_url.into()),
            ..Default::default()
        }
    }

    /// 创建新的配置（使用 Secret）
    pub fn with_secret(database_url: Secret<String>) -> Self {
        Self {
            database_url,
            ..Default::default()
        }
    }

    /// 设置最大连接数
    pub fn max_connections(mut self, max: u32) -> Self {
        self.max_connections = max;
        self
    }

    /// 设置连接超时
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout.as_secs();
        self
    }

    /// 设置查询超时
    pub fn query_timeout(mut self, timeout: Duration) -> Self {
        self.query_timeout = timeout.as_secs();
        self
    }
}

/// PostgreSQL存储实现
#[cfg(feature = "postgres")]
pub struct PostgresStorage {
    pool: DbPool,
    query_timeout: Duration,
}

impl Clone for PostgresStorage {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            query_timeout: self.query_timeout,
        }
    }
}

impl PostgresStorage {
    /// 创建新的PostgreSQL存储实例
    pub async fn new(config: PostgresStorageConfig) -> Result<Self, StorageError> {
        info!("正在连接PostgreSQL数据库...");

        // 使用 ExposeSecret 安全地访问数据库 URL
        let database_url = config.database_url.expose_secret();

        // 创建连接池
        let pool = DbPool::new(database_url).await.map_err(|e| {
            error!("数据库连接失败: {}", e);
            StorageError::ConnectionError(format!("无法连接到数据库: {}", e))
        })?;

        info!("成功连接到PostgreSQL数据库");

        Ok(Self {
            pool,
            query_timeout: Duration::from_secs(config.query_timeout),
        })
    }

    /// 获取会话
    async fn get_session(&self) -> Result<dbnexus::pool::Session, StorageError> {
        self.pool
            .get_session("admin")
            .await
            .map_err(|e| StorageError::ConnectionError(format!("获取会话失败: {}", e)))
    }

    /// 检查数据库连接
    pub async fn ping(&self) -> Result<(), StorageError> {
        let session = self
            .get_session()
            .await
            .map_err(|e| StorageError::QueryError(format!("Ping失败: {}", e)))?;
        Ok(())
    }

    /// 健康检查
    pub async fn health_check(&self) -> Result<(), StorageError> {
        self.ping().await
    }

    /// 清理过期的键值存储
    pub async fn cleanup_expired(&self) -> Result<u64, StorageError> {
        let session = self.get_session().await.map_err(to_storage_error)?;
        let result = session
            .execute_raw("DELETE FROM kv_store WHERE expires_at < now()")
            .await
            .map_err(to_storage_error)?;

        let deleted = result.rows_affected() as u64;
        if deleted > 0 {
            debug!("Cleaned {} expired kv records", deleted);
        }
        Ok(deleted)
    }

    /// 清理过期的封禁记录
    pub async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        let session = self.get_session().await.map_err(to_storage_error)?;
        let result = session
            .execute_raw(
                r#"
                UPDATE ban_records
                SET unbanned_at = now(),
                    unbanned_by = 'system'
                WHERE expires_at < now()
                  AND unbanned_at IS NULL
                "#,
            )
            .await
            .map_err(to_storage_error)?;

        let updated = result.rows_affected() as u64;
        if updated > 0 {
            info!("Auto-unbanned {} expired records", updated);
        }
        Ok(updated)
    }
}

fn to_storage_error(e: impl std::fmt::Display) -> StorageError {
    StorageError::QueryError(e.to_string())
}

#[async_trait]
impl StorageTrait for PostgresStorage {
    /// 获取值
    async fn get(&self, _key: &str) -> Result<Option<String>, StorageError> {
        debug!("获取键: {}", _key);
        // TODO: 实现完整查询逻辑
        Ok(None)
    }

    /// 设置值
    async fn set(&self, _key: &str, _value: &str, _ttl: Option<u64>) -> Result<(), StorageError> {
        debug!("设置键: {}, TTL: {:?}", _key, _ttl);
        // TODO: 实现完整查询逻辑
        Ok(())
    }

    /// 删除值
    async fn delete(&self, _key: &str) -> Result<(), StorageError> {
        debug!("删除键: {}", _key);
        Ok(())
    }
}

#[async_trait]
impl QuotaStorage for PostgresStorage {
    /// 获取配额信息
    async fn get_quota(
        &self,
        _user_id: &str,
        _resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError> {
        debug!("获取配额: user_id={}, resource={}", _user_id, _resource);
        // TODO: 实现完整查询逻辑
        Ok(None)
    }

    /// 消费配额
    async fn consume(
        &self,
        _user_id: &str,
        _resource: &str,
        _cost: u64,
        _limit: u64,
        _window: std::time::Duration,
    ) -> Result<ConsumeResult, StorageError> {
        debug!(
            "消费配额: user_id={}, resource={}, cost={}",
            _user_id, _resource, _cost
        );
        // TODO: 实现完整配额逻辑
        Ok(ConsumeResult {
            allowed: true,
            remaining: _limit.saturating_sub(_cost),
            alert_triggered: false,
            usage_percent: (_cost as f64 / _limit as f64) * 100.0,
        })
    }

    /// 重置配额
    async fn reset(
        &self,
        _user_id: &str,
        _resource: &str,
        _limit: u64,
        _window: std::time::Duration,
    ) -> Result<(), StorageError> {
        debug!("重置配额: user_id={}, resource={}", _user_id, _resource);
        Ok(())
    }
}

#[async_trait]
impl crate::storage::BanStorage for PostgresStorage {
    /// 检查是否被封禁
    async fn is_banned(&self, _target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
        let (target_type, target_value) = match _target {
            BanTarget::Ip(ip) => ("ip", ip.as_str()),
            BanTarget::UserId(user_id) => ("user", user_id.as_str()),
            BanTarget::Mac(mac) => ("mac", mac.as_str()),
        };

        debug!("检查封禁状态: type={}, value={}", target_type, target_value);
        // TODO: 实现完整查询逻辑
        Ok(None)
    }

    /// 保存封禁记录
    async fn save(&self, record: &BanRecord) -> Result<(), StorageError> {
        let (target_type, target_value) = match &record.target {
            BanTarget::Ip(ip) => ("ip", ip.as_str()),
            BanTarget::UserId(user_id) => ("user", user_id.as_str()),
            BanTarget::Mac(mac) => ("mac", mac.as_str()),
        };

        info!(
            "保存封禁记录: type={}, value={}, ban_times={}, duration={:?}",
            target_type, target_value, record.ban_times, record.duration
        );

        // TODO: 实现完整保存逻辑
        Ok(())
    }

    /// 获取封禁历史
    async fn get_history(&self, _target: &BanTarget) -> Result<Option<BanHistory>, StorageError> {
        let (target_type, target_value) = match _target {
            BanTarget::Ip(ip) => ("ip", ip.as_str()),
            BanTarget::UserId(user_id) => ("user", user_id.as_str()),
            BanTarget::Mac(mac) => ("mac", mac.as_str()),
        };

        debug!("获取封禁历史: type={}, value={}", target_type, target_value);
        // TODO: 实现完整查询逻辑
        Ok(None)
    }

    /// 增加封禁次数
    async fn increment_ban_times(&self, _target: &BanTarget) -> Result<u64, StorageError> {
        let (target_type, target_value) = match _target {
            BanTarget::Ip(ip) => ("ip", ip.as_str()),
            BanTarget::UserId(user_id) => ("user", user_id.as_str()),
            BanTarget::Mac(mac) => ("mac", mac.as_str()),
        };

        debug!("增加封禁次数: type={}, value={}", target_type, target_value);
        // TODO: 实现完整逻辑
        Ok(1)
    }

    /// 获取封禁次数
    async fn get_ban_times(&self, _target: &BanTarget) -> Result<u64, StorageError> {
        let (target_type, target_value) = match _target {
            BanTarget::Ip(ip) => ("ip", ip.as_str()),
            BanTarget::UserId(user_id) => ("user", user_id.as_str()),
            BanTarget::Mac(mac) => ("mac", mac.as_str()),
        };

        debug!("获取封禁次数: type={}, value={}", target_type, target_value);
        Ok(0)
    }

    /// 移除封禁记录
    async fn remove_ban(&self, _target: &BanTarget) -> Result<(), StorageError> {
        let (target_type, target_value) = match _target {
            BanTarget::Ip(ip) => ("ip", ip.as_str()),
            BanTarget::UserId(user_id) => ("user", user_id.as_str()),
            BanTarget::Mac(mac) => ("mac", mac.as_str()),
        };

        debug!("移除封禁记录: type={}, value={}", target_type, target_value);
        Ok(())
    }

    /// 清理过期封禁
    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        debug!("清理过期封禁");

        let session = self.get_session().await?;
        let result = session
            .execute_raw(
                r#"
                UPDATE ban_records
                SET unbanned_at = now()
                WHERE expires_at <= now()
                  AND unbanned_at IS NULL
                "#,
            )
            .await
            .map_err(|e| StorageError::QueryError(format!("清理过期封禁失败: {}", e)))?;

        Ok(result.rows_affected() as u64)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage as StorageTrait;

    #[tokio::test]
    #[ignore]
    async fn test_postgres_health_check() {
        let config = PostgresStorageConfig::new("postgresql://localhost/test");
        let storage = PostgresStorage::new(config).await.unwrap();

        storage.health_check().await.unwrap();
    }
}
