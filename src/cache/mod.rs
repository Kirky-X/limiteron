// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
// Re-export oxcache types directly - no wrapper layers
pub use oxcache::Cache;
pub use oxcache::traits::CacheKey;

#[cfg(feature = "cache-storage")]
pub mod ban_storage;
pub mod cache_service;
#[cfg(feature = "cache-storage")]
pub mod quota_storage;
#[cfg(feature = "cache-storage")]
pub mod storage;

#[cfg(feature = "cache-storage")]
pub use ban_storage::CacheBanStorage;
pub use cache_service::CacheService;
#[cfg(feature = "cache-storage")]
pub use quota_storage::CacheQuotaStorage;
#[cfg(feature = "cache-storage")]
pub use storage::CacheStorage;
