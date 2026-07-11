// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Storage Factory 示例
//!
//! 演示如何使用 StorageFactory 从 DSN 创建不同类型的存储后端
//! （Postgres / MySQL / SQLite）。
//!
//! # 涵盖 API
//!
//! - `StorageFactoryConfig`（`postgres`、`mysql`、`sqlite` 构造器）
//! - `StorageFactory::new` / `from_dsn`
//! - `StorageFactory::initialize` / `create_storage` / `create_ban_storage` / `create_quota_storage`
//! - 便捷函数 `create_storage_from_dsn` 等
//! - `StorageType` 枚举
//!
//! # 运行方式
//!
//! ```bash
//! cargo run --bin storage_factory --features postgres
//! ```
//!
//! # 注意
//!
//! 此示例需要实际的数据库连接才能完成 `initialize` 调用。
//! 在没有数据库的环境中，示例仅演示配置构建与 API 调用方式，
//! `initialize` 会返回错误并被捕获展示。

use limiteron::adapters::{
    create_ban_storage_from_dsn, create_quota_storage_from_dsn, create_storage_from_dsn,
    StorageFactory, StorageFactoryConfig, StorageType,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Storage Factory Demo ===\n");

    demo_config_builders();
    demo_factory_creation().await?;
    demo_convenience_functions().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

/// 演示 StorageFactoryConfig 的多种构造方式
fn demo_config_builders() {
    println!("--- 1. StorageFactoryConfig Builders ---\n");

    // Postgres 配置
    let pg_config = StorageFactoryConfig::postgres("postgres://user:pass@localhost:5432/limiteron");
    println!("  Postgres config:");
    println!("    type: {:?}", StorageType::DBNexusPostgres);
    println!("    connection_string: {}", pg_config.connection_string);
    println!("    pool_size: {}", pg_config.pool_size);

    // MySQL 配置（带连接池大小）
    let mysql_config = StorageFactoryConfig::mysql("mysql://user:pass@localhost:3306/limiteron")
        .with_pool_size(20)
        .with_connection_timeout(10)
        .with_idle_timeout(300);
    println!("\n  MySQL config (customized):");
    println!("    connection_string: {}", mysql_config.connection_string);
    println!("    pool_size: {}", mysql_config.pool_size);
    println!(
        "    connection_timeout: {}s",
        mysql_config.connection_timeout
    );
    println!("    idle_timeout: {}s", mysql_config.idle_timeout);

    // SQLite 配置
    let sqlite_config = StorageFactoryConfig::sqlite("/data/limiteron.db");
    println!("\n  SQLite config:");
    println!("    connection_string: {}", sqlite_config.connection_string);

    // 默认配置
    let default_config = StorageFactoryConfig::default();
    println!("\n  Default config:");
    println!("    pool_size: {}", default_config.pool_size);
    println!(
        "    connection_timeout: {}s",
        default_config.connection_timeout
    );
    println!("    idle_timeout: {}s", default_config.idle_timeout);

    // StorageType 枚举方法
    println!("\n  StorageType methods:");
    println!(
        "    DBNexusPostgres.as_str() = {:?}",
        StorageType::DBNexusPostgres.as_str()
    );
    println!(
        "    StorageType::parse('postgres') = {:?}",
        StorageType::parse("postgres")
    );
    println!(
        "    StorageType::parse('mysql') = {:?}",
        StorageType::parse("mysql")
    );
    println!(
        "    StorageType::parse('sqlite') = {:?}",
        StorageType::parse("sqlite")
    );
    println!(
        "    StorageType::parse('invalid') = {:?}",
        StorageType::parse("invalid")
    );

    println!();
}

/// 演示 StorageFactory 的创建与初始化流程
async fn demo_factory_creation() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 2. StorageFactory Creation & Initialization ---\n");

    // 方式 1：从配置创建
    let config = StorageFactoryConfig::postgres("postgres://user:pass@localhost:5432/limiteron");
    let mut factory = StorageFactory::new(config);
    println!("  Factory created from config");
    println!("    is_initialized: {}", factory.is_initialized());
    println!("    config type: {:?}", factory.config().storage_type);

    // 方式 2：从 DSN 字符串创建
    let factory_from_dsn =
        StorageFactory::from_dsn("postgres://user:pass@localhost:5432/limiteron");
    println!("\n  Factory created from DSN string");
    println!(
        "    config connection: {}",
        factory_from_dsn.config().connection_string
    );
    println!("    is_initialized: {}", factory_from_dsn.is_initialized());

    // 尝试初始化（需要真实数据库，此处预期失败）
    let init_result = factory.initialize(None).await;
    match &init_result {
        Ok(()) => println!("\n  Initialize: ✅ success (database available)"),
        Err(e) => println!("\n  Initialize: ❌ failed (expected without DB) - {}", e),
    }

    // 如果初始化成功，可以创建存储
    if init_result.is_ok() {
        match factory.create_storage().await {
            Ok(storage) => {
                println!("  create_storage: ✅ created Arc<dyn Storage>");
                let _ = storage;
            }
            Err(e) => println!("  create_storage: ❌ {}", e),
        }

        // create_all 一次性创建所有存储
        match factory.create_all().await {
            Ok((storage, ban_storage, quota_storage)) => {
                println!("  create_all: ✅ created storage + ban_storage + quota_storage");
                let _ = (storage, ban_storage, quota_storage);
            }
            Err(e) => println!("  create_all: ❌ {}", e),
        }
    }

    // 关闭工厂
    factory.close().await;
    println!("\n  Factory closed");
    println!();
    Ok(())
}

/// 演示便捷函数：直接从 DSN 创建存储
async fn demo_convenience_functions() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 3. Convenience Functions ---\n");

    let dsn = "postgres://user:pass@localhost:5432/limiteron";

    // 这些函数会自动初始化工厂并创建存储
    let storage_result = create_storage_from_dsn(dsn).await;
    println!(
        "  create_storage_from_dsn: {}",
        format_result(&storage_result)
    );

    let ban_result = create_ban_storage_from_dsn(dsn).await;
    println!(
        "  create_ban_storage_from_dsn: {}",
        format_result(&ban_result)
    );

    let quota_result = create_quota_storage_from_dsn(dsn).await;
    println!(
        "  create_quota_storage_from_dsn: {}",
        format_result(&quota_result)
    );

    println!("\n  (便捷函数内部管理工厂生命周期，无需手动关闭)");
    println!();
    Ok(())
}

fn format_result<T>(result: &Result<T, limiteron::StorageError>) -> String {
    match result {
        Ok(_) => "✅ success".to_string(),
        Err(e) => format!("❌ failed ({})", e),
    }
}
