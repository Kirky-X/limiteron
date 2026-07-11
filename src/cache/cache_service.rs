// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
// CacheService trait - Defines the unified cache service interface
//
// This trait provides a standardized interface for cache operations
// that supports dependency injection and can be implemented by various
// cache backends (memory, Redis, etc.).

use crate::error::StorageError;
use async_trait::async_trait;
use std::time::Duration;

/// Cache service trait defining the unified interface for cache operations
///
/// This trait enables dependency injection of cache services and supports
/// multiple backend implementations (memory, Redis, etc.).
#[async_trait]
pub trait CacheService: Send + Sync {
    /// Get a value from the cache
    ///
    /// # Arguments
    /// * `key` - The cache key to retrieve
    ///
    /// # Returns
    /// * `Ok(Some(value))` if the key exists
    /// * `Ok(None)` if the key doesn't exist or has expired
    /// * `Err(StorageError)` if an error occurred
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError>;

    /// Set a value in the cache with optional TTL
    ///
    /// # Arguments
    /// * `key` - The cache key to set
    /// * `value` - The value to store
    /// * `ttl` - Optional TTL in seconds. If None, uses default TTL
    ///
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err(StorageError)` if an error occurred
    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError>;

    /// Delete a value from the cache
    ///
    /// # Arguments
    /// * `key` - The cache key to delete
    ///
    /// # Returns
    /// * `Ok(())` on success (even if key didn't exist)
    /// * `Err(StorageError)` if an error occurred
    async fn delete(&self, key: &str) -> Result<(), StorageError>;

    /// Set a value in the cache with a specific TTL duration
    ///
    /// # Arguments
    /// * `key` - The cache key to set
    /// * `value` - The value to store
    /// * `ttl` - The TTL duration for this specific entry
    ///
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err(StorageError)` if an error occurred
    async fn set_with_ttl(&self, key: &str, value: &str, ttl: Duration)
    -> Result<(), StorageError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    // Mock implementation for testing
    struct MockCacheService;

    #[async_trait]
    impl CacheService for MockCacheService {
        async fn get(&self, _key: &str) -> Result<Option<String>, StorageError> {
            Ok(Some("test_value".to_string()))
        }

        async fn set(
            &self,
            _key: &str,
            _value: &str,
            _ttl: Option<u64>,
        ) -> Result<(), StorageError> {
            Ok(())
        }

        async fn delete(&self, _key: &str) -> Result<(), StorageError> {
            Ok(())
        }

        async fn set_with_ttl(
            &self,
            _key: &str,
            _value: &str,
            _ttl: Duration,
        ) -> Result<(), StorageError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_mock_cache_service() {
        let cache = MockCacheService;
        assert!(cache.get("test").await.is_ok());
        assert!(cache.set("test", "value", None).await.is_ok());
        assert!(cache.delete("test").await.is_ok());
        assert!(
            cache
                .set_with_ttl("test", "value", Duration::from_secs(60))
                .await
                .is_ok()
        );
    }
}
