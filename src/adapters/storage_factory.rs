// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
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
//! use limiteron::storage::Storage;
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
use crate::storage::{BanStorage, QuotaStorage, Storage};
use dbnexus::DbPool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 存储类型枚举
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

impl StorageType {
    /// 从字符串解析
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "dbnexus_postgres" | "postgresql" | "postgres" => Some(Self::DBNexusPostgres),
            "dbnexus_mysql" | "mysql" => Some(Self::DBNexusMySQL),
            "dbnexus_sqlite" | "sqlite" => Some(Self::DBNexusSQLite),
            #[cfg(test)]
            "memory" => Some(Self::Memory),
            _ => None,
        }
    }

    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DBNexusPostgres => "dbnexus_postgres",
            Self::DBNexusMySQL => "dbnexus_mysql",
            Self::DBNexusSQLite => "dbnexus_sqlite",
            #[cfg(test)]
            Self::Memory => "memory",
        }
    }
}

impl From<&str> for StorageType {
    fn from(s: &str) -> Self {
        Self::parse(s).unwrap_or_default()
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
        assert_eq!(StorageType::Memory.to_string(), "Memory");
    }

    #[test]
    fn test_storage_type_parse_postgres_variants() {
        assert_eq!(
            StorageType::parse("dbnexus_postgres"),
            Some(StorageType::DBNexusPostgres)
        );
        assert_eq!(
            StorageType::parse("postgresql"),
            Some(StorageType::DBNexusPostgres)
        );
        assert_eq!(
            StorageType::parse("postgres"),
            Some(StorageType::DBNexusPostgres)
        );
    }

    #[test]
    fn test_storage_type_parse_mysql() {
        assert_eq!(
            StorageType::parse("dbnexus_mysql"),
            Some(StorageType::DBNexusMySQL)
        );
        assert_eq!(StorageType::parse("mysql"), Some(StorageType::DBNexusMySQL));
    }

    #[test]
    fn test_storage_type_parse_sqlite() {
        assert_eq!(
            StorageType::parse("dbnexus_sqlite"),
            Some(StorageType::DBNexusSQLite)
        );
        assert_eq!(
            StorageType::parse("sqlite"),
            Some(StorageType::DBNexusSQLite)
        );
    }

    #[test]
    fn test_storage_type_parse_memory() {
        assert_eq!(StorageType::parse("memory"), Some(StorageType::Memory));
    }

    #[test]
    fn test_storage_type_parse_case_insensitive() {
        assert_eq!(
            StorageType::parse("POSTGRES"),
            Some(StorageType::DBNexusPostgres)
        );
        assert_eq!(StorageType::parse("MySQL"), Some(StorageType::DBNexusMySQL));
    }

    #[test]
    fn test_storage_type_parse_unknown() {
        assert_eq!(StorageType::parse("unknown_db"), None);
        assert_eq!(StorageType::parse(""), None);
    }

    #[test]
    fn test_storage_type_as_str() {
        assert_eq!(StorageType::DBNexusPostgres.as_str(), "dbnexus_postgres");
        assert_eq!(StorageType::DBNexusMySQL.as_str(), "dbnexus_mysql");
        assert_eq!(StorageType::DBNexusSQLite.as_str(), "dbnexus_sqlite");
        assert_eq!(StorageType::Memory.as_str(), "memory");
    }

    #[test]
    fn test_storage_type_from_str_known() {
        let st: StorageType = "postgres".into();
        assert_eq!(st, StorageType::DBNexusPostgres);
        let st: StorageType = "mysql".into();
        assert_eq!(st, StorageType::DBNexusMySQL);
        let st: StorageType = "sqlite".into();
        assert_eq!(st, StorageType::DBNexusSQLite);
    }

    #[test]
    fn test_storage_type_from_str_unknown_falls_back_to_default() {
        let st: StorageType = "unknown_db".into();
        assert_eq!(st, StorageType::DBNexusPostgres);
    }

    #[test]
    fn test_storage_type_default() {
        let st = StorageType::default();
        assert_eq!(st, StorageType::DBNexusPostgres);
    }

    #[test]
    fn test_config_mysql() {
        let config = StorageFactoryConfig::mysql("mysql://localhost/test");
        assert_eq!(config.storage_type, StorageType::DBNexusMySQL);
        assert_eq!(config.connection_string, "mysql://localhost/test");
        assert_eq!(config.pool_size, 10);
    }

    #[test]
    fn test_config_sqlite() {
        let config = StorageFactoryConfig::sqlite("/tmp/test.db");
        assert_eq!(config.storage_type, StorageType::DBNexusSQLite);
        assert_eq!(config.connection_string, "sqlite:/tmp/test.db");
    }

    #[test]
    fn test_config_with_idle_timeout() {
        let config = StorageFactoryConfig::default().with_idle_timeout(600);
        assert_eq!(config.idle_timeout, 600);
    }

    #[test]
    fn test_config_postgres_full_builder() {
        let config = StorageFactoryConfig::postgres("postgresql://localhost/test")
            .with_pool_size(20)
            .with_connection_timeout(15)
            .with_idle_timeout(120);
        assert_eq!(config.storage_type, StorageType::DBNexusPostgres);
        assert_eq!(config.pool_size, 20);
        assert_eq!(config.connection_timeout, 15);
        assert_eq!(config.idle_timeout, 120);
    }

    #[tokio::test]
    async fn test_factory_new_with_config() {
        let config = StorageFactoryConfig::sqlite("/tmp/test.db");
        let factory = StorageFactory::new(config.clone());
        assert!(!factory.is_initialized());
        assert_eq!(factory.config().storage_type, config.storage_type);
        assert_eq!(factory.config().connection_string, config.connection_string);
    }

    #[tokio::test]
    async fn test_factory_config_accessor() {
        let factory = StorageFactory::from_dsn("postgresql://localhost/test");
        assert_eq!(factory.config().storage_type, StorageType::DBNexusPostgres);
        assert_eq!(
            factory.config().connection_string,
            "postgresql://localhost/test"
        );
    }

    #[tokio::test]
    async fn test_factory_close_is_noop() {
        let factory = StorageFactory::from_dsn("postgresql://localhost/test");
        // close should not panic even when not initialized
        factory.close().await;
        assert!(!factory.is_initialized());
    }

    #[tokio::test]
    async fn test_create_ban_storage_not_initialized() {
        let factory = StorageFactory::from_dsn("postgresql://localhost/test");
        let result = factory.create_ban_storage().await;
        assert!(result.is_err());
        // 使用 err() 避免 trait object 需要 Debug 的约束
        let err = result.err().unwrap();
        assert!(matches!(err, StorageError::ConnectionError(_)));
    }

    #[tokio::test]
    async fn test_create_quota_storage_not_initialized() {
        let factory = StorageFactory::from_dsn("postgresql://localhost/test");
        let result = factory.create_quota_storage().await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(matches!(err, StorageError::ConnectionError(_)));
    }

    #[tokio::test]
    async fn test_create_all_not_initialized() {
        let factory = StorageFactory::from_dsn("postgresql://localhost/test");
        let result = factory.create_all().await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(matches!(err, StorageError::ConnectionError(_)));
    }

    #[tokio::test]
    async fn test_factory_clone() {
        let factory = StorageFactory::from_dsn("postgresql://localhost/test");
        let cloned = factory.clone();
        assert_eq!(
            factory.config().connection_string,
            cloned.config().connection_string
        );
        assert_eq!(factory.is_initialized(), cloned.is_initialized());
    }

    #[test]
    fn test_storage_type_equality_and_clone() {
        let st1 = StorageType::DBNexusMySQL;
        let st2 = st1.clone();
        assert_eq!(st1, st2);
        assert_ne!(st1, StorageType::DBNexusSQLite);
    }

    #[test]
    fn test_storage_type_serde_roundtrip() {
        // serde rename_all="lowercase" => "dbnexussqlite" (no underscore)
        let st = StorageType::DBNexusSQLite;
        let json = serde_json::to_string(&st).unwrap();
        assert_eq!(json, "\"dbnexussqlite\"");
        let deserialized: StorageType = serde_json::from_str(&json).unwrap();
        assert_eq!(st, deserialized);
    }

    #[test]
    fn test_storage_type_serde_postgres() {
        let st = StorageType::DBNexusPostgres;
        let json = serde_json::to_string(&st).unwrap();
        assert_eq!(json, "\"dbnexuspostgres\"");
    }

    #[test]
    fn test_storage_type_serde_mysql() {
        let st = StorageType::DBNexusMySQL;
        let json = serde_json::to_string(&st).unwrap();
        assert_eq!(json, "\"dbnexusmysql\"");
    }

    #[test]
    fn test_config_debug_format() {
        let config = StorageFactoryConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("StorageFactoryConfig"));
        assert!(debug_str.contains("DBNexusPostgres"));
    }
}
