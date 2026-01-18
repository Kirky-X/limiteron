//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 缓存模块
//!
//! 提供多级缓存实现，包括 L2 缓存（内存）、L3 缓存（Redis）和智能缓存策略。

pub mod l2;
pub mod l3;
pub mod smart;

// 重新导出 L2 缓存的公共 API
pub use l2::{
    CacheEntry, L2Cache, L2CacheConfig, DEFAULT_CACHE_CAPACITY, DEFAULT_CLEANUP_INTERVAL_SECS,
    DEFAULT_EVICTION_THRESHOLD, DEFAULT_TTL_SECS,
};

// 重新导出 L3 缓存的公共 API (仅在 redis 特性启用时)
#[cfg(feature = "redis")]
pub use l3::{L3Cache, L3CacheConfig, L3CacheStats};

#[cfg(not(feature = "redis"))]
pub mod l3_stub {
    use crate::cache::l2::L2Cache;

    // L3Cache 存根 - 当 redis 特性未启用时使用 L2Cache 作为替代
    pub type L3Cache = L2Cache;
}

#[cfg(not(feature = "redis"))]
pub use l3_stub::L3Cache;

// 重新导出智能缓存的公共 API
pub use smart::{CacheStats as SmartCacheStats, SmartCacheStrategy};
