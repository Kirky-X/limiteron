// Copyright (c) 2026, Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! DBNexus 存储工厂
//!
//! 提供统一的存储实例创建接口，支持通过配置或 DSN 创建各种存储适配器。
//!
//! # 特性
//!
//! - **统一创建接口** - 通过数据库连接字符串或配置创建存储
//! - **DBNexus 集成** - 自动使用 DBNexus 实体和适配器
//! - **类型安全** - 编译时类型检查
//! - **异步支持** - 所有操作异步执行
//!
//! # 使用示例
//!
//! ```rust,no_run
//! use limiteron::adapters::StorageFactory;
//! use limiteron::storage_trait::Storage;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 通过 DSN 创建存储
//!     let mut factory = StorageFactory::from_dsn(
//!         "postgresql://user:pass@localhost/limiteron",
//!     );
//!     factory.initialize(None).await?;
//!     let storage: Arc<dyn Storage> = factory.create_storage().await?;
//!
//!     // 使用存储
//!     storage.set("key", "value", None).await?;
//!     let value = storage.get("key").await?;
//!     println!("Value: {:?}", value);
//!
//!     Ok(())
//! }
//! ```

use crate::adapters::DBNexusBanStorageAdapter;
use crate::adapters::DBNexusQuotaStorageAdapter;
use crate::adapters::DBNexusStorageAdapter;
use crate::error::StorageError;
use crate::storage_trait::{BanStorage, QuotaStorage, Storage};
use dbnexus::DbPool;
use std::sync::Arc;

/// 存储类型枚举
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum StorageType {
    /// DBNexus PostgreSQL 存储
    #[default]
    DBNexusPostgres,
    /// DBNexus MySQL 存储
    DBNexusMySQL,
    /// DBNexus SQLite 存储
    DBNexusSQLite,
    /// 内存存储（仅用于测试）
    #[cfg(test)]
    Memory,
}

impl std::fmt::Display for StorageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageType::DBNexusPostgres => write!(f, "DBNexus PostgreSQL"),
            StorageType::DBNexusMySQL => write!(f, "DBNexus MySQL"),
            StorageType::DBNexusSQLite => write!(f, "DBNexus SQLite"),
            #[cfg(test)]
            StorageType::Memory => write!(f, "Memory"),
        }
    }
}

/// 存储工厂配置
#[derive(Debug, Clone)]
pub struct StorageFactoryConfig {
    /// 存储类型
    pub storage_type: StorageType,
    /// 数据库连接 URL 或 DSN
    pub connection_string: String,
    /// 连接池大小
    pub pool_size: u32,
    /// 连接超时（秒）
    pub connection_timeout: u64,
    /// 空闲连接超时（秒）
    pub idle_timeout: u64,
}

impl Default for StorageFactoryConfig {
    fn default() -> Self {
        Self {
            storage_type: StorageType::DBNexusPostgres,
            connection_string: "postgresql://localhost/limiteron".to_string(),
            pool_size: 10,
            connection_timeout: 30,
            idle_timeout: 300,
        }
    }
}

impl StorageFactoryConfig {
    /// 创建 PostgreSQL 配置
    pub fn postgres(connection_string: impl Into<String>) -> Self {
        Self {
            storage_type: StorageType::DBNexusPostgres,
            connection_string: connection_string.into(),
            ..Default::default()
        }
    }

    /// 创建 MySQL 配置
    pub fn mysql(connection_string: impl Into<String>) -> Self {
        Self {
            storage_type: StorageType::DBNexusMySQL,
            connection_string: connection_string.into(),
            ..Default::default()
        }
    }

    /// 创建 SQLite 配置
    pub fn sqlite(path: impl Into<String>) -> Self {
        Self {
            storage_type: StorageType::DBNexusSQLite,
            connection_string: format!("sqlite:{}", path.into()),
            ..Default::default()
        }
    }

    /// 设置连接池大小
    pub fn with_pool_size(mut self, size: u32) -> Self {
        self.pool_size = size;
        self
    }

    /// 设置连接超时
    pub fn with_connection_timeout(mut self, seconds: u64) -> Self {
        self.connection_timeout = seconds;
        self
    }

    /// 设置空闲连接超时
    pub fn with_idle_timeout(mut self, seconds: u64) -> Self {
        self.idle_timeout = seconds;
        self
    }
}

/// 存储工厂
///
/// 提供统一的存储实例创建接口。
///
/// # 示例
///
/// ```rust,no_run
/// use limiteron::adapters::{StorageFactory, StorageFactoryConfig};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = StorageFactoryConfig::postgres("postgresql://localhost/limiteron");
///     let mut factory = StorageFactory::new(config);
///     factory.initialize(None).await?;
///
///     let storage = factory.create_storage().await?;
///     let ban_storage = factory.create_ban_storage().await?;
///     let quota_storage = factory.create_quota_storage().await?;
///
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct StorageFactory {
    /// 工厂配置
    config: StorageFactoryConfig,
    /// DBNexus 连接池
    pool: Option<Arc<DbPool>>,
}

impl StorageFactory {
    /// 从配置创建工厂
    pub fn new(config: StorageFactoryConfig) -> Self {
        Self { config, pool: None }
    }

    /// 从 DSN 创建工厂（PostgreSQL）
    ///
    /// # 参数
    /// - `dsn`: PostgreSQL 连接字符串
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use limiteron::adapters::StorageFactory;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut factory = StorageFactory::from_dsn("postgresql://user:pass@localhost/db");
    ///     factory.initialize(None).await?;
    ///     Ok(())
    /// }
    /// ```
    pub fn from_dsn(dsn: impl Into<String>) -> Self {
        Self::new(StorageFactoryConfig::postgres(dsn))
    }

    /// 初始化连接池
    ///
    /// # 参数
    /// - `config`: 可选的工厂配置，如果未提供则使用默认配置
    ///
    /// # 返回
    /// - `Ok(())`: 初始化成功
    /// - `Err(StorageError)`: 初始化失败
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use limiteron::adapters::StorageFactory;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    ///     factory.initialize(None).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn initialize(
        &mut self,
        config: Option<StorageFactoryConfig>,
    ) -> Result<(), StorageError> {
        if let Some(cfg) = config {
            self.config = cfg;
        }

        // 创建 DBNexus 连接池
        let pool = DbPool::new(&self.config.connection_string)
            .await
            .map_err(|e| StorageError::ConnectionError(e.to_string()))?;

        self.pool = Some(Arc::new(pool));
        Ok(())
    }

    /// 检查工厂是否已初始化
    pub fn is_initialized(&self) -> bool {
        self.pool.is_some()
    }

    /// 获取连接池引用
    fn pool_ref(&self) -> Result<&Arc<DbPool>, StorageError> {
        self.pool.as_ref().ok_or_else(|| {
            StorageError::ConnectionError(
                "StorageFactory not initialized. Call initialize() first.".to_string(),
            )
        })
    }

    /// 创建存储适配器
    ///
    /// # 返回
    /// - `Ok(Arc<dyn Storage>)`: 创建成功的存储适配器
    /// - `Err(StorageError)`: 创建失败
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use limiteron::adapters::StorageFactory;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    ///     factory.initialize(None).await?;
    ///     let storage = factory.create_storage().await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn create_storage(&self) -> Result<Arc<dyn Storage>, StorageError> {
        let pool = self.pool_ref()?;
        Ok(Arc::new(DBNexusStorageAdapter::new(pool.clone())))
    }

    /// 创建封禁存储适配器
    ///
    /// # 返回
    /// - `Ok(Arc<dyn BanStorage>)`: 创建成功的封禁存储适配器
    /// - `Err(StorageError)`: 创建失败
    pub async fn create_ban_storage(&self) -> Result<Arc<dyn BanStorage>, StorageError> {
        let pool = self.pool_ref()?;
        Ok(Arc::new(DBNexusBanStorageAdapter::new(pool.clone())))
    }

    /// 创建配额存储适配器
    ///
    /// # 返回
    /// - `Ok(Arc<dyn QuotaStorage>)`: 创建成功的配额存储适配器
    /// - `Err(StorageError)`: 创建失败
    pub async fn create_quota_storage(&self) -> Result<Arc<dyn QuotaStorage>, StorageError> {
        let pool = self.pool_ref()?;
        Ok(Arc::new(DBNexusQuotaStorageAdapter::new(pool.clone())))
    }

    /// 创建所有存储适配器
    ///
    /// # 返回
    /// - `(storage, ban_storage, quota_storage)`: 所有存储适配器元组
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use limiteron::adapters::StorageFactory;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    ///     factory.initialize(None).await?;
    ///     let (storage, ban_storage, quota_storage) = factory.create_all().await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn create_all(
        &self,
    ) -> Result<(Arc<dyn Storage>, Arc<dyn BanStorage>, Arc<dyn QuotaStorage>), StorageError> {
        let pool = self.pool_ref()?;
        let storage: Arc<dyn Storage> = Arc::new(DBNexusStorageAdapter::new(pool.clone()));
        let ban_storage: Arc<dyn BanStorage> =
            Arc::new(DBNexusBanStorageAdapter::new(pool.clone()));
        let quota_storage: Arc<dyn QuotaStorage> =
            Arc::new(DBNexusQuotaStorageAdapter::new(pool.clone()));
        Ok((storage, ban_storage, quota_storage))
    }

    /// 获取当前配置
    pub fn config(&self) -> &StorageFactoryConfig {
        &self.config
    }

    /// 关闭连接池
    ///
    /// 释放所有连接池资源。
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::adapters::StorageFactory;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    ///     // ... 使用工厂 ...
    ///     factory.close().await;
    ///     Ok(())
    /// }
    /// ```
    pub async fn close(&self) {
        // DbPool is automatically cleaned up via Drop when StorageFactory is dropped.
        // No explicit close needed.
    }
}

/// 从 DSN 创建存储的便捷函数
///
/// # 参数
/// - `dsn`: 数据库连接字符串
///
/// # 返回
/// - `Ok(Arc<dyn Storage>)`: 创建成功的存储适配器
/// - `Err(StorageError)`: 创建失败
///
/// # 示例
///
/// ```rust,no_run
/// use limiteron::adapters;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let storage = adapters::create_storage_from_dsn("postgresql://localhost/limiteron").await?;
///     Ok(())
/// }
/// ```
pub async fn create_storage_from_dsn(dsn: &str) -> Result<Arc<dyn Storage>, StorageError> {
    let mut factory = StorageFactory::from_dsn(dsn);
    factory.initialize(None).await?;
    factory.create_storage().await
}

/// 从 DSN 创建封禁存储的便捷函数
pub async fn create_ban_storage_from_dsn(dsn: &str) -> Result<Arc<dyn BanStorage>, StorageError> {
    let mut factory = StorageFactory::from_dsn(dsn);
    factory.initialize(None).await?;
    factory.create_ban_storage().await
}

/// 从 DSN 创建配额存储的便捷函数
pub async fn create_quota_storage_from_dsn(
    dsn: &str,
) -> Result<Arc<dyn QuotaStorage>, StorageError> {
    let mut factory = StorageFactory::from_dsn(dsn);
    factory.initialize(None).await?;
    factory.create_quota_storage().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_factory_config_defaults() {
        let config = StorageFactoryConfig::default();
        assert_eq!(config.storage_type, StorageType::DBNexusPostgres);
        assert_eq!(config.pool_size, 10);
        assert_eq!(config.connection_timeout, 30);
    }

    #[tokio::test]
    async fn test_factory_config_builders() {
        let config = StorageFactoryConfig::postgres("postgresql://localhost/test")
            .with_pool_size(5)
            .with_connection_timeout(60);

        assert_eq!(config.storage_type, StorageType::DBNexusPostgres);
        assert_eq!(config.pool_size, 5);
        assert_eq!(config.connection_timeout, 60);
    }

    #[tokio::test]
    async fn test_factory_not_initialized() {
        let factory = StorageFactory::from_dsn("postgresql://localhost/test");
        assert!(!factory.is_initialized());

        // 尝试创建存储应该失败
        let result = factory.create_storage().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_storage_type_display() {
        assert_eq!(
            StorageType::DBNexusPostgres.to_string(),
            "DBNexus PostgreSQL"
        );
        assert_eq!(StorageType::DBNexusMySQL.to_string(), "DBNexus MySQL");
        assert_eq!(StorageType::DBNexusSQLite.to_string(), "DBNexus SQLite");
    }
}
