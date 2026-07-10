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
