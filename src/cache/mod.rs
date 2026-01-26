//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 缓存模块
//!
//! 直接导出 oxcache 库，无封装层。
//!
//! ## 架构
//!
//! - **L1 Cache**: 本地内存缓存 (oxcache MemoryBackend)
//! - **L2 Cache**: 分布式缓存 (oxcache TieredCache with Redis)
//!
//! ## 使用方式
//!
//! ```rust,ignore
//! use limiteron::cache::l1::L1Cache;
//! use limiteron::cache::l2::L2Cache;
//!
//! // L1 缓存（内存）
//! let cache = L1Cache::new(10000, Duration::from_secs(60));
//!
//! // L2 缓存（分布式）
//! #[cfg(feature = "redis")]
//! use limiteron::cache::l2::L2Cache;
//! ```
//!
//! ## 特性标志
//!
//! - `redis`: 启用 L2 分布式缓存
//! - 不启用 `redis` 时，仅 L1 缓存可用
//!
//! ## oxcache 集成
//!
//! 此模块直接代理 oxcache 库的公共 API，不进行任何封装。
//! 有关 oxcache 的详细信息，请参阅 [oxcache 文档](/home/project/oxcache/).

pub mod l1;
#[cfg(feature = "redis")]
pub mod l2;

// 直接导出 oxcache 类型
pub use l1::L1Cache;
pub use l1::{CacheEntry, CacheStats, L1CacheConfig};

#[cfg(feature = "redis")]
pub use l2::{L2Cache, L2CacheConfig, L2CacheStats};
