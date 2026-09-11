// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! MySQL Storage 集成测试（T607）
//!
//! 与 postgres_storage.rs 同一套契约断言（经 contract.rs 共享），
//! 跑在 MySQL DSN 上。dbnexus 驱动互斥：mysql 与 postgres/sqlite
//! 不能同时启用。
//!
//! 运行前请准备 MySQL（如 Docker）：
//!   docker run -d -p 3306:3306 -e MYSQL_USER=limiteron \
//!     -e MYSQL_PASSWORD=limiteron_dev -e MYSQL_DATABASE=limiteron_test mysql:8
//!
//! 运行命令: `cargo test --features mysql --test integration_tests real_storage::mysql -- --ignored`

#[cfg(test)]
#[cfg(feature = "mysql")]
mod tests {
    use limiteron::adapters::{StorageFactory, StorageFactoryConfig};
    use limiteron::error::StorageError;
    use serial_test::serial;
    use std::sync::Arc;
    use std::time::Duration;

    const MYSQL_DSN: &str = "mysql://limiteron:limiteron_dev@localhost:3306/limiteron_test";

    /// 辅助函数：创建 StorageFactory 并初始化（契约套件复用）
    async fn create_mysql_factory() -> Result<
        (
            StorageFactory,
            Arc<dyn limiteron::Storage>,
            Arc<dyn limiteron::BanStorage>,
            Arc<dyn limiteron::QuotaStorage>,
        ),
        StorageError,
    > {
        let mut factory = StorageFactory::new(StorageFactoryConfig::mysql(MYSQL_DSN));
        factory.initialize(None).await.map_err(|e| {
            StorageError::ConnectionError(format!(
                "Failed to connect to MySQL at {MYSQL_DSN}: {e}. 请先启动 MySQL 实例"
            ))
        })?;
        let storage = factory.create_storage().await?;
        let ban = factory.create_ban_storage().await?;
        let quota = factory.create_quota_storage().await?;
        Ok((factory, storage, ban, quota))
    }

    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_mysql_storage_connection() {
        let result = create_mysql_factory().await;
        assert!(result.is_ok(), "MySQL 连接失败：请先启动 MySQL 实例");
    }

    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_mysql_storage_contract() {
        let (_f, storage, _ban, _quota) = create_mysql_factory().await.unwrap();
        super::super::contract::storage_contract(&storage)
            .await
            .expect("Storage 契约");
    }

    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_mysql_ban_storage_contract() {
        let (_f, _storage, ban, _quota) = create_mysql_factory().await.unwrap();
        super::super::contract::ban_contract(&ban)
            .await
            .expect("BanStorage 契约");
    }

    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_mysql_quota_storage_contract() {
        let (_f, _storage, _ban, quota) = create_mysql_factory().await.unwrap();
        super::super::contract::quota_contract(&quota)
            .await
            .expect("QuotaStorage 契约");
    }

    /// 配置解析层契约：无需真实连接（非 ignored，门控编译即验证）
    #[test]
    #[serial]
    fn test_mysql_storage_factory_config_parses() {
        let config = StorageFactoryConfig::mysql(MYSQL_DSN);
        assert_eq!(config.connection_string, MYSQL_DSN);
        let factory = StorageFactory::from_dsn("mysql://localhost/limiteron");
        // from_dsn 默认 postgres——显式 mysql 必须经 StorageFactoryConfig（文档化语义）
        assert!(!factory.is_initialized());
        let _ = Duration::from_secs(config.connection_timeout);
    }
}
