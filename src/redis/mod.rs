//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! Redis storage module for distributed rate limiting.
//!
//! This module provides Redis-backed storage implementation and Lua script
//! management for distributed consistency across multiple instances.
//!
//! # Features
//!
//! - **RedisStorage** - Implements `Storage` trait using Redis backend
//! - **ScriptManager** - Manages Lua script loading and execution
//! - **GCRA** - Generic Cell Rate Algorithm for distributed rate limiting
//!
//! # Example
//!
//! ```rust,ignore
//! use limiteron::redis::{RedisStorage, ScriptManager};
//! use limiteron::storage::Storage;
//!
//! // Create Redis storage
//! let storage = RedisStorage::from_connection_string("redis://127.0.0.1:6379/")?;
//!
//! // Use storage
//! storage.set("key", "value", Some(60)).await?;
//! let value = storage.get("key").await?;
//! ```

pub mod manager;
pub mod scripts;
pub mod storage;

// Re-export public types
pub use manager::ScriptManager;
pub use scripts::{
    execute_gcra, execute_gcra_with_sha, load_gcra_script, GcraResult, ScriptType,
    FIXED_WINDOW_SCRIPT, GCRA_SCRIPT, SLIDING_WINDOW_SCRIPT, TOKEN_BUCKET_SCRIPT,
};
pub use storage::RedisStorage;
