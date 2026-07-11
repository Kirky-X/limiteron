// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 存储适配器模块集成测试

#[cfg(feature = "postgres")]
use limiteron::adapters::StorageFactory;

#[cfg(feature = "postgres")]
#[tokio::test]
async fn test_storage_factory_new() {
    let config = limiteron::adapters::StorageFactoryConfig::postgres("postgresql://localhost/test");
    let _ = StorageFactory::new(config);
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn test_storage_factory_from_dsn() {
    let factory = StorageFactory::from_dsn("postgresql://localhost/test");
    assert!(!factory.is_initialized());
}
