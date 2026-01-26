//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 缓存模块
//!
//! 提供多级缓存实现，基于 oxcache 库构建。
//! - L1: 本地内存缓存 (oxcache MemoryBackend)
//! - L2: 分布式缓存 (oxcache TieredCache with Redis)

pub mod l1;
pub mod l2;
pub mod smart;

// 重新导出 L1 缓存的公共 API
pub use l1::{
    CacheEntry, CacheStats, L1Cache, L1CacheConfig, DEFAULT_CACHE_CAPACITY,
    DEFAULT_CLEANUP_INTERVAL_SECS, DEFAULT_EVICTION_THRESHOLD, DEFAULT_TTL_SECS,
};

// 重新导出 L2 缓存的公共 API (仅在 redis 特性启用时)
#[cfg(feature = "redis")]
pub use l2::{L2Cache, L2CacheConfig, L2CacheStats};

// L2Cache 存根 - 当 redis 特性未启用时
#[cfg(not(feature = "redis"))]
pub mod l2_stub {
    use crate::cache::l1::L1Cache;

    // L2Cache 存根 - 当 redis 特性未启用时使用 L1Cache 作为替代
    pub type L2Cache = L1Cache;
}

#[cfg(not(feature = "redis"))]
pub use l2_stub::L2Cache;

// 重新导出智能缓存的公共 API
pub use smart::{CacheStats as SmartCacheStats, SmartCacheStrategy};
