// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
// Re-export oxcache types directly - no wrapper layers
pub use oxcache::Cache;
pub use oxcache::traits::CacheKey;

#[cfg(feature = "cache-redis")]
pub mod ban_storage;
pub mod cache_service;
#[cfg(feature = "cache-redis")]
pub mod quota_storage;
#[cfg(feature = "cache-redis")]
pub mod storage;

#[cfg(feature = "cache-redis")]
pub use ban_storage::CacheBanStorage;
pub mod memory_cache;
pub use cache_service::CacheService;
pub use memory_cache::MemoryCache;
#[cfg(feature = "cache-redis")]
pub use quota_storage::CacheQuotaStorage;
#[cfg(feature = "cache-redis")]
pub use storage::CacheStorage;
