//! PostgreSQL存储实现
//!
//! 使用sqlx实现PostgreSQL存储，支持连接池、事务和完整的错误处理。
//!
//! # 数据库Schema
//!
//! ```sql
//! -- 配额使用表
//! CREATE TABLE quota_usage (
//!     id BIGSERIAL PRIMARY KEY,
//!     user_id VARCHAR(255) NOT NULL,
//!     resource_key VARCHAR(255) NOT NULL,
//!     quota_type VARCHAR(50) NOT NULL,
//!     consumed BIGINT NOT NULL DEFAULT 0,
//!     limit_value BIGINT NOT NULL,
//!     window_start TIMESTAMPTZ NOT NULL,
//!     window_end TIMESTAMPTZ NOT NULL,
//!     last_updated TIMESTAMPTZ NOT NULL DEFAULT now(),
//!     UNIQUE(user_id, resource_key, window_start)
//! );
//!
//! CREATE INDEX idx_quota_window
//!     ON quota_usage(user_id, resource_key, window_start);
//!
//! -- 封禁记录表
//! CREATE TABLE ban_records (
//!     id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
//!     target_type VARCHAR(20) NOT NULL,
//!     target_value VARCHAR(255) NOT NULL,
//!     reason TEXT,
//!     ban_times INTEGER NOT NULL DEFAULT 1,
//!     duration_secs BIGINT NOT NULL,
//!     banned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
//!     expires_at TIMESTAMPTZ NOT NULL,
//!     is_manual BOOLEAN NOT NULL DEFAULT false,
//!     unbanned_at TIMESTAMPTZ,
//!     unbanned_by VARCHAR(255)
//! );
//!
//! CREATE INDEX idx_ban_active
//!     ON ban_records(target_type, target_value, expires_at)
//!     WHERE unbanned_at IS NULL;
//!
//! -- 通用键值存储表
//! CREATE TABLE kv_store (
//!     key VARCHAR(255) PRIMARY KEY,
//!     value TEXT NOT NULL,
//!     expires_at TIMESTAMPTZ
//! );
//!
//! CREATE INDEX idx_kv_expires
//!     ON kv_store(expires_at)
//!     WHERE expires_at IS NOT NULL;
//! ```

use async_trait::async_trait;
use secrecy::{ExposeSecret, Secret};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::error::{ConsumeResult, StorageError};
use crate::storage::{
    BanHistory, BanRecord, BanTarget, QuotaInfo, QuotaStorage, Storage as StorageTrait,
};

/// PostgreSQL存储配置
#[derive(Clone)]
pub struct PostgresStorageConfig {
    /// 数据库连接URL（使用 Secret 包装以防止意外泄露）
    pub database_url: Secret<String>,
    /// 连接池最大连接数
    pub max_connections: u32,
    /// 连接池最小空闲连接数
    pub min_connections: u32,
    /// 连接超时时间（秒）
    pub connect_timeout: u64,
    /// 查询超时时间（秒）
    pub query_timeout: u64,
    /// 是否启用连接池
    pub enable_pool: bool,
}

impl std::fmt::Debug for PostgresStorageConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresStorageConfig")
            .field("database_url", &"***")
            .field("max_connections", &self.max_connections)
            .field("min_connections", &self.min_connections)
            .field("connect_timeout", &self.connect_timeout)
            .field("query_timeout", &self.query_timeout)
            .field("enable_pool", &self.enable_pool)
            .finish()
    }
}

impl Default for PostgresStorageConfig {
    fn default() -> Self {
        Self {
            database_url: secrecy::Secret::new(String::new()),
            max_connections: 20,
            min_connections: 5,
            connect_timeout: 30,
            query_timeout: 10,
            enable_pool: true,
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

    /// 设置最小连接数
    pub fn min_connections(mut self, min: u32) -> Self {
        self.min_connections = min;
        self
    }

    /// 设置连接池大小（别名）
    pub fn with_pool_size(mut self, size: u32) -> Self {
        self.max_connections = size;
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
pub struct PostgresStorage {
    pool: PgPool,
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
    ///
    /// # 参数
    ///
    /// * `config` - 存储配置
    ///
    /// # 错误
    ///
    /// 返回连接错误如果无法连接到数据库
    pub async fn new(config: PostgresStorageConfig) -> Result<Self, StorageError> {
        info!("正在连接PostgreSQL数据库...");

        // 使用 ExposeSecret 安全地访问数据库 URL
        let database_url = config.database_url.expose_secret();

        // 创建连接池
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(Duration::from_secs(config.connect_timeout))
            .idle_timeout(Duration::from_secs(600))
            .max_lifetime(Duration::from_secs(1800))
            .connect(database_url)
            .await
            .map_err(|e| {
                error!("数据库连接失败: {}", e);
                StorageError::ConnectionError(format!("无法连接到数据库: {}", e))
            })?;

        info!("成功连接到PostgreSQL数据库");

        Ok(Self {
            pool,
            query_timeout: Duration::from_secs(config.query_timeout),
        })
    }

    /// 从连接池创建存储实例
    pub fn from_pool(pool: PgPool) -> Self {
        Self {
            pool,
            query_timeout: Duration::from_secs(10),
        }
    }

    /// 检查数据库连接
    pub async fn ping(&self) -> Result<(), StorageError> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StorageError::QueryError(format!("Ping失败: {}", e)))?;
        Ok(())
    }

    /// 获取连接池引用
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// 开始事务
    pub async fn begin_transaction(&self) -> Result<Transaction<'_, Postgres>, StorageError> {
        self.pool
            .begin()
            .await
            .map_err(|e| StorageError::ConnectionError(format!("无法开始事务: {}", e)))
    }

    /// 健康检查
    pub async fn health_check(&self) -> Result<(), StorageError> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StorageError::ConnectionError(format!("健康检查失败: {}", e)))?;
        Ok(())
    }

    /// 清理过期的键值存储
    pub async fn cleanup_expired(&self) -> Result<u64, StorageError> {
        let result = sqlx::query("DELETE FROM kv_store WHERE expires_at < now()")
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::QueryError(format!("清理过期数据失败: {}", e)))?;

        let deleted = result.rows_affected();
        if deleted > 0 {
            debug!("清理了 {} 条过期的键值存储记录", deleted);
        }

        Ok(deleted)
    }

    /// 清理过期的封禁记录
    pub async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        let result = sqlx::query(
            r#"
            UPDATE ban_records
            SET unbanned_at = now(),
                unbanned_by = 'system'
            WHERE expires_at < now()
              AND unbanned_at IS NULL
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::QueryError(format!("清理过期封禁失败: {}", e)))?;

        let updated = result.rows_affected();
        if updated > 0 {
            info!("自动解封了 {} 条过期记录", updated);
        }

        Ok(updated)
    }
}

#[async_trait]
impl StorageTrait for PostgresStorage {
    /// 获取值
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        debug!("获取键: {}", key);

        let result = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT value
            FROM kv_store
            WHERE key = $1
              AND (expires_at IS NULL OR expires_at > now())
            "#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::QueryError(format!("获取键失败: {}", e)))?;

        Ok(result.map(|(value,)| value))
    }

    /// 设置值
    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError> {
        debug!("设置键: {}, TTL: {:?}", key, ttl);

        let expires_at = ttl.map(|seconds| Utc::now() + chrono::Duration::seconds(seconds as i64));

        sqlx::query(
            r#"
            INSERT INTO kv_store (key, value, expires_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (key) DO UPDATE
            SET value = $2,
                expires_at = $3
            "#,
        )
        .bind(key)
        .bind(value)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::QueryError(format!("设置键失败: {}", e)))?;

        Ok(())
    }

    /// 删除值
    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        debug!("删除键: {}", key);

        sqlx::query("DELETE FROM kv_store WHERE key = $1")
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::QueryError(format!("删除键失败: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl QuotaStorage for PostgresStorage {
    /// 获取配额信息
    async fn get_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError> {
        debug!("获取配额: user_id={}, resource={}", user_id, resource);

        let result = sqlx::query_as::<_, (i64, i64, DateTime<Utc>, DateTime<Utc>)>(
            r#"
            SELECT consumed, limit_value, window_start, window_end
            FROM quota_usage
            WHERE user_id = $1
              AND resource_key = $2
              AND window_end > now()
            ORDER BY window_start DESC
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(resource)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::QueryError(format!("获取配额信息失败: {}", e)))?;

        Ok(
            result.map(|(consumed, limit, window_start, window_end)| QuotaInfo {
                consumed: consumed as u64,
                limit: limit as u64,
                window_start,
                window_end,
            }),
        )
    }

    /// 消费配额
    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
    ) -> Result<ConsumeResult, StorageError> {
        debug!(
            "消费配额: user_id={}, resource={}, cost={}",
            user_id, resource, cost
        );

        // 使用事务确保原子性
        let mut tx = self.begin_transaction().await?;

        // 获取当前配额
        let current = sqlx::query_as::<_, (i64, i64, DateTime<Utc>, DateTime<Utc>)>(
            r#"
            SELECT consumed, limit_value, window_start, window_end
            FROM quota_usage
            WHERE user_id = $1
              AND resource_key = $2
              AND window_end > now()
            ORDER BY window_start DESC
            LIMIT 1
            FOR UPDATE
            "#,
        )
        .bind(user_id)
        .bind(resource)
        .fetch_optional(tx.as_mut())
        .await
        .map_err(|e| StorageError::QueryError(format!("获取当前配额失败: {}", e)))?;

        let (consumed, limit, remaining) = match current {
            Some((consumed, limit, window_start, window_end)) => {
                // 检查窗口是否过期
                if window_end < Utc::now() {
                    // 创建新窗口
                    let new_start = Utc::now();
                    let new_end = new_start + chrono::Duration::seconds(3600); // 默认1小时窗口
                    sqlx::query(
                        r#"
                        INSERT INTO quota_usage (user_id, resource_key, quota_type, consumed, limit_value, window_start, window_end)
                        VALUES ($1, $2, 'default', $3, $4, $5, $6)
                        "#,
                    )
                    .bind(user_id)
                    .bind(resource)
                    .bind(cost as i64)
                    .bind(limit)
                    .bind(new_start)
                    .bind(new_end)
                    .execute(tx.as_mut())
                    .await
                    .map_err(|e| StorageError::QueryError(format!("创建新配额窗口失败: {}", e)))?;

                    (cost as i64, limit, limit - cost as i64)
                } else {
                    // 检查是否超限
                    let new_consumed = consumed + cost as i64;
                    if new_consumed > limit {
                        tx.rollback().await?;
                        return Ok(ConsumeResult {
                            allowed: false,
                            remaining: 0,
                            alert_triggered: false,
                        });
                    }

                    // 更新配额
                    sqlx::query(
                        r#"
                        UPDATE quota_usage
                        SET consumed = consumed + $3,
                            last_updated = now()
                        WHERE user_id = $1
                          AND resource_key = $2
                          AND window_start = $4
                        "#,
                    )
                    .bind(user_id)
                    .bind(resource)
                    .bind(cost as i64)
                    .bind(window_start)
                    .execute(tx.as_mut())
                    .await
                    .map_err(|e| StorageError::QueryError(format!("更新配额失败: {}", e)))?;

                    (new_consumed, limit, limit - new_consumed)
                }
            }
            None => {
                // 创建新配额记录
                let window_start = Utc::now();
                let window_end = window_start + chrono::Duration::seconds(3600); // 默认1小时窗口
                let limit = 1000i64; // 默认限制

                sqlx::query(
                    r#"
                    INSERT INTO quota_usage (user_id, resource_key, quota_type, consumed, limit_value, window_start, window_end)
                    VALUES ($1, $2, 'default', $3, $4, $5, $6)
                    "#,
                )
                .bind(user_id)
                .bind(resource)
                .bind(cost as i64)
                .bind(limit)
                .bind(window_start)
                .bind(window_end)
                .execute(tx.as_mut())
                .await
                .map_err(|e| StorageError::QueryError(format!("创建配额记录失败: {}", e)))?;

                (cost as i64, limit, limit - cost as i64)
            }
        };

        // 提交事务
        tx.commit().await?;

        // 检查是否触发告警（使用率超过80%）
        let usage_ratio = consumed as f64 / limit as f64;
        let alert_triggered = usage_ratio > 0.8;

        if alert_triggered {
            warn!(
                "配额使用告警: user_id={}, resource={}, usage={:.1}%",
                user_id,
                resource,
                usage_ratio * 100.0
            );
        }

        Ok(ConsumeResult {
            allowed: true,
            remaining: remaining.max(0) as u64,
            alert_triggered,
        })
    }

    /// 重置配额
    async fn reset(&self, user_id: &str, resource: &str) -> Result<(), StorageError> {
        debug!("重置配额: user_id={}, resource={}", user_id, resource);

        sqlx::query(
            r#"
            UPDATE quota_usage
            SET consumed = 0,
                last_updated = now()
            WHERE user_id = $1
              AND resource_key = $2
              AND window_end > now()
            "#,
        )
        .bind(user_id)
        .bind(resource)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::QueryError(format!("重置配额失败: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl crate::storage::BanStorage for PostgresStorage {
    /// 检查是否被封禁
    async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
        let (target_type, target_value) = match target {
            BanTarget::Ip(ip) => ("ip", ip.as_str()),
            BanTarget::UserId(user_id) => ("user", user_id.as_str()),
            BanTarget::Mac(mac) => ("mac", mac.as_str()),
        };

        debug!("检查封禁状态: type={}, value={}", target_type, target_value);

        let result = sqlx::query_as::<_, (
            uuid::Uuid,
            String,
            i32,
            i64,
            DateTime<Utc>,
            DateTime<Utc>,
            bool,
            String,
        )>(
            r#"
            SELECT id, reason, ban_times, duration_secs, banned_at, expires_at, is_manual, target_value
            FROM ban_records
            WHERE target_type = $1
              AND target_value = $2
              AND expires_at > now()
              AND unbanned_at IS NULL
            ORDER BY banned_at DESC
            LIMIT 1
            "#,
        )
        .bind(target_type)
        .bind(target_value)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::QueryError(format!("检查封禁状态失败: {}", e)))?;

        Ok(result.map(
            |(_id, reason, ban_times, duration_secs, banned_at, expires_at, is_manual, _)| {
                BanRecord {
                    target: target.clone(),
                    ban_times: ban_times as u32,
                    duration: Duration::from_secs(duration_secs as u64),
                    banned_at,
                    expires_at,
                    is_manual,
                    reason,
                }
            },
        ))
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

        sqlx::query(
            r#"
            INSERT INTO ban_records (
                id, target_type, target_value, reason, ban_times, duration_secs,
                banned_at, expires_at, is_manual
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(uuid::Uuid::new_v4())
        .bind(target_type)
        .bind(target_value)
        .bind(&record.reason)
        .bind(record.ban_times as i32)
        .bind(record.duration.as_secs() as i64)
        .bind(record.banned_at)
        .bind(record.expires_at)
        .bind(record.is_manual)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::QueryError(format!("保存封禁记录失败: {}", e)))?;

        Ok(())
    }

    /// 获取封禁历史
    async fn get_history(&self, target: &BanTarget) -> Result<Option<BanHistory>, StorageError> {
        let (target_type, target_value) = match target {
            BanTarget::Ip(ip) => ("ip", ip.as_str()),
            BanTarget::UserId(user_id) => ("user", user_id.as_str()),
            BanTarget::Mac(mac) => ("mac", mac.as_str()),
        };

        debug!("获取封禁历史: type={}, value={}", target_type, target_value);

        let result = sqlx::query_as::<_, (i32, DateTime<Utc>)>(
            r#"
            SELECT MAX(ban_times) as ban_times,
                   MAX(banned_at) as last_banned_at
            FROM ban_records
            WHERE target_type = $1
              AND target_value = $2
            "#,
        )
        .bind(target_type)
        .bind(target_value)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::QueryError(format!("获取封禁历史失败: {}", e)))?;

        Ok(result.map(|(ban_times, last_banned_at)| BanHistory {
            ban_times: ban_times as u32,
            last_banned_at,
        }))
    }

    /// 增加封禁次数
    async fn increment_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let (target_type, target_value) = match target {
            BanTarget::Ip(ip) => ("ip", ip.as_str()),
            BanTarget::UserId(user_id) => ("user", user_id.as_str()),
            BanTarget::Mac(mac) => ("mac", mac.as_str()),
        };

        debug!("增加封禁次数: type={}, value={}", target_type, target_value);

        // 先尝试更新现有记录
        let updated = sqlx::query_as::<_, (i32,)>(
            r#"
            UPDATE ban_records
            SET ban_times = ban_times + 1,
                banned_at = now()
            WHERE target_type = $1
              AND target_value = $2
              AND unbanned_at IS NULL
              AND expires_at > now()
            RETURNING ban_times
            "#,
        )
        .bind(target_type)
        .bind(target_value)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::QueryError(format!("更新封禁次数失败: {}", e)))?;

        if let Some((ban_times,)) = updated {
            return Ok(ban_times as u64);
        }

        // 如果没有更新，插入新记录
        let inserted = sqlx::query_as::<_, (i32,)>(
            r#"
            INSERT INTO ban_records (
                id, target_type, target_value, reason, ban_times, duration_secs,
                banned_at, expires_at, is_manual
            )
            VALUES ($1, $2, $3, $4, 1, 86400, now(), now() + interval '24 hours', false)
            RETURNING ban_times
            "#,
        )
        .bind(uuid::Uuid::new_v4())
        .bind(target_type)
        .bind(target_value)
        .bind("increment")
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::QueryError(format!("插入封禁记录失败: {}", e)))?;

        Ok(inserted.0 as u64)
    }

    /// 获取封禁次数
    async fn get_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let (target_type, target_value) = match target {
            BanTarget::Ip(ip) => ("ip", ip.as_str()),
            BanTarget::UserId(user_id) => ("user", user_id.as_str()),
            BanTarget::Mac(mac) => ("mac", mac.as_str()),
        };

        debug!("获取封禁次数: type={}, value={}", target_type, target_value);

        let result = sqlx::query_as::<_, (i64,)>(
            r#"
            SELECT COALESCE(SUM(ban_times), 0) as total_ban_times
            FROM ban_records
            WHERE target_type = $1
              AND target_value = $2
              AND unbanned_at IS NULL
              AND expires_at > now()
            "#,
        )
        .bind(target_type)
        .bind(target_value)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::QueryError(format!("获取封禁次数失败: {}", e)))?;

        Ok(result.map(|(ban_times,)| ban_times as u64).unwrap_or(0))
    }

    /// 移除封禁记录
    async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError> {
        let (target_type, target_value) = match target {
            BanTarget::Ip(ip) => ("ip", ip.as_str()),
            BanTarget::UserId(user_id) => ("user", user_id.as_str()),
            BanTarget::Mac(mac) => ("mac", mac.as_str()),
        };

        debug!("移除封禁记录: type={}, value={}", target_type, target_value);

        sqlx::query(
            r#"
            UPDATE ban_records
            SET unbanned_at = now()
            WHERE target_type = $1
              AND target_value = $2
              AND unbanned_at IS NULL
            "#,
        )
        .bind(target_type)
        .bind(target_value)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::QueryError(format!("移除封禁记录失败: {}", e)))?;

        Ok(())
    }

    /// 清理过期封禁
    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        debug!("清理过期封禁");

        let result = sqlx::query(
            r#"
            UPDATE ban_records
            SET unbanned_at = now()
            WHERE expires_at <= now()
              AND unbanned_at IS NULL
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::QueryError(format!("清理过期封禁失败: {}", e)))?;

        Ok(result.rows_affected())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{BanStorage, QuotaStorage, Storage as StorageTrait};

    #[tokio::test]
    #[ignore] // 需要真实的PostgreSQL连接
    async fn test_postgres_storage_set_get() {
        let config = PostgresStorageConfig::new("postgresql://localhost/test");
        let storage = PostgresStorage::new(config).await.unwrap();

        storage
            .set("test_key", "test_value", Some(60))
            .await
            .unwrap();
        let value = storage.get("test_key").await.unwrap();
        assert_eq!(value, Some("test_value".to_string()));

        storage.delete("test_key").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_quota_consume() {
        let config = PostgresStorageConfig::new("postgresql://localhost/test");
        let storage = PostgresStorage::new(config).await.unwrap();

        let result = storage.consume("user1", "resource1", 10).await.unwrap();
        assert!(result.allowed);

        let quota = storage.get_quota("user1", "resource1").await.unwrap();
        assert!(quota.is_some());
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_ban_save() {
        let config = PostgresStorageConfig::new("postgresql://localhost/test");
        let storage = PostgresStorage::new(config).await.unwrap();

        let target = BanTarget::UserId("user1".to_string());
        let record = BanRecord {
            target: target.clone(),
            ban_times: 1,
            duration: Duration::from_secs(300),
            banned_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::seconds(300),
            is_manual: false,
            reason: "test".to_string(),
        };

        storage.save(&record).await.unwrap();

        let is_banned = storage.is_banned(&target).await.unwrap();
        assert!(is_banned.is_some());
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_health_check() {
        let config = PostgresStorageConfig::new("postgresql://localhost/test");
        let storage = PostgresStorage::new(config).await.unwrap();

        storage.health_check().await.unwrap();
    }
}
