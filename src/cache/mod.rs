// Re-export oxcache types directly - no wrapper layers
pub use oxcache::traits::CacheKey;
pub use oxcache::traits::Cacheable;
pub use oxcache::Cache;

pub mod cache_service;
