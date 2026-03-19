// Copyright (c) 2026, Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! DBNexus Storage Adapters
//!
//! This module provides DBNexus-based implementations of Limiteron's storage traits.
//! All adapters use DBNexus for database operations, delegating all database
//! complexity to the DBNexus framework.
//!
//! # Features
//!
//! - **Complete trait implementations** - Storage, BanStorage, QuotaStorage
//! - **DBNexus integration** - Uses DBNexus entities and connection pooling
//! - **Async/await** - All operations are asynchronous
//! - **Thread-safe** - Uses Arc<dyn Trait> pattern for thread safety
//!
//! # Usage
//!
//! ```rust,no_run
//! use limiteron::adapters::{StorageFactory, DBNexusStorageAdapter, DBNexusBanStorageAdapter, DBNexusQuotaStorageAdapter};
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 使用工厂创建存储
//!     let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
//!     factory.initialize(None).await?;
//!     let storage = factory.create_storage().await?;
//!
//!     // 或直接使用适配器（需要已有连接池）
//!     // let _adapter = DBNexusStorageAdapter::new(pool);
//!
//!     Ok(())
//! }
//! ```

pub mod dbnexus_ban_storage;
pub mod dbnexus_quota_storage;
pub mod dbnexus_storage;
pub mod storage_factory;

#[cfg(test)]
mod dbnexus_tests;

// Re-export adapters for convenient access
pub use dbnexus_ban_storage::DBNexusBanStorageAdapter;
pub use dbnexus_quota_storage::DBNexusQuotaStorageAdapter;
pub use dbnexus_storage::DBNexusStorageAdapter;

// Re-export factory and related types
pub use storage_factory::{
    create_ban_storage_from_dsn, create_quota_storage_from_dsn, create_storage_from_dsn,
    StorageFactory, StorageFactoryConfig, StorageType,
};
