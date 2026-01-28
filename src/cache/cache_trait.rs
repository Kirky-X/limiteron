// Copyright (c) 2026, Kirky.X
//
// MIT License
//
// Cache Service Trait
//
// Defines the interface for cache operations, enabling dependency injection
// and mock implementations for testing.

use crate::error::FlowGuardError;
use async_trait::async_trait;
use std::time::Duration;

/// Cache backend selection enum.
///
/// Specifies which cache backend to use for operations.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum CacheBackend {
    /// Use the in-memory cache (L1)
    #[default]
    Memory,
    /// Use the Redis cache (L2), falls back to memory if unavailable
    Redis,
}

/// Cache service trait for dependency injection.
///
/// This trait defines the interface for cache operations, allowing components
/// to depend on the abstraction rather than a concrete implementation.
///
/// # Example
///
/// ```rust
/// use limiteron::cache::{CacheService, CacheServiceConfig};
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = CacheServiceConfig::default();
///     let cache_service: Arc<dyn CacheService> = Arc::new(
///         limiteron::cache::OxCacheService::new(config).await?
///     );
///
///     // Use the cache service
///     cache_service.set("key", "value", None).await?;
///     let value = cache_service.get("key").await?;
///     println!("Retrieved: {:?}", value);
///
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait CacheService: Send + Sync {
    /// Get a value from the cache.
    ///
    /// # Parameters
    /// - `key`: The cache key
    ///
    /// # Returns
    /// - `Ok(Some(value))` if the key exists
    /// - `Ok(None)` if the key does not exist (cache miss)
    /// - `Err(...)` if an error occurs
    async fn get(&self, key: &str) -> Result<Option<String>, FlowGuardError>;

    /// Set a value in the cache.
    ///
    /// # Parameters
    /// - `key`: The cache key
    /// - `value`: The value to cache
    /// - `ttl`: Optional TTL override. If `None`, uses the default TTL.
    ///
    /// # Returns
    /// - `Ok(())` on success
    /// - `Err(...)` if an error occurs
    async fn set(
        &self,
        key: &str,
        value: &str,
        ttl: Option<Duration>,
    ) -> Result<(), FlowGuardError>;

    /// Delete a value from the cache.
    ///
    /// # Parameters
    /// - `key`: The cache key to delete
    ///
    /// # Returns
    /// - `Ok(())` on success (even if key didn't exist)
    /// - `Err(...)` if an error occurs
    async fn delete(&self, key: &str) -> Result<(), FlowGuardError>;

    /// Set a value with a specific TTL.
    ///
    /// This method allows setting a TTL per entry, overriding the default TTL.
    ///
    /// # Parameters
    /// - `key`: The cache key
    /// - `value`: The value to cache
    /// - `ttl`: The TTL for this entry
    ///
    /// # Returns
    /// - `Ok(())` on success
    /// - `Err(...)` if an error occurs
    async fn set_with_ttl(
        &self,
        key: &str,
        value: &str,
        ttl: Duration,
    ) -> Result<(), FlowGuardError>;
}

// ============================================================================
// Mock Cache Service (Available in all modes)
// ============================================================================

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Mock implementation of CacheService for testing and simple use cases.
///
/// This provides a thread-safe in-memory cache that implements the `CacheService`
/// trait, useful for unit tests or when you need a simple cache without external
/// dependencies.
#[derive(Clone, Default)]
pub struct MockCacheService {
    data: Arc<RwLock<HashMap<String, (String, Option<Duration>)>>>,
}

impl MockCacheService {
    /// Create a new MockCacheService.
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl CacheService for MockCacheService {
    async fn get(&self, key: &str) -> Result<Option<String>, FlowGuardError> {
        let data = self.data.read().await;
        Ok(data.get(key).map(|(value, _)| value.clone()))
    }

    async fn set(
        &self,
        key: &str,
        value: &str,
        _ttl: Option<Duration>,
    ) -> Result<(), FlowGuardError> {
        self.data
            .write()
            .await
            .insert(key.to_string(), (value.to_string(), None));
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), FlowGuardError> {
        self.data.write().await.remove(key);
        Ok(())
    }

    async fn set_with_ttl(
        &self,
        key: &str,
        value: &str,
        ttl: Duration,
    ) -> Result<(), FlowGuardError> {
        self.data
            .write()
            .await
            .insert(key.to_string(), (value.to_string(), Some(ttl)));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    #[tokio::test]
    async fn test_mock_cache_service_basic_operations() {
        let cache = MockCacheService::new();

        // Test set and get
        cache.set("key1", "value1", None).await.unwrap();
        let result = cache.get("key1").await.unwrap();
        assert_eq!(result, Some("value1".to_string()));

        // Test get non-existent key
        let result = cache.get("nonexistent").await.unwrap();
        assert_eq!(result, None);

        // Test delete
        cache.delete("key1").await.unwrap();
        let result = cache.get("key1").await.unwrap();
        assert_eq!(result, None);

        // Test set_with_ttl
        cache
            .set_with_ttl("key2", "value2", Duration::from_secs(60))
            .await
            .unwrap();
        let result = cache.get("key2").await.unwrap();
        assert_eq!(result, Some("value2".to_string()));
    }

    #[tokio::test]
    async fn test_mock_cache_service_clone() {
        let cache1 = MockCacheService::new();
        cache1.set("key", "value", None).await.unwrap();

        let cache2 = cache1.clone();
        let result = cache2.get("key").await.unwrap();
        assert_eq!(result, Some("value".to_string()));
    }

    #[tokio::test]
    async fn test_cache_backend_default() {
        assert_eq!(CacheBackend::default(), CacheBackend::Memory);
    }

    #[tokio::test]
    async fn test_cache_backend_serialization() {
        let backend = CacheBackend::Memory;
        let serialized = serde_json::to_string(&backend).unwrap();
        let deserialized: CacheBackend = serde_json::from_str(&serialized).unwrap();
        assert_eq!(backend, deserialized);

        let backend = CacheBackend::Redis;
        let serialized = serde_json::to_string(&backend).unwrap();
        let deserialized: CacheBackend = serde_json::from_str(&serialized).unwrap();
        assert_eq!(backend, deserialized);
    }
}
