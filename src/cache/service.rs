// Copyright (c) 2026, Kirky.X
//
// MIT License
//
// OxCacheService Implementation
//
// Provides the concrete implementation of CacheService using oxcache,
// with support for memory and Redis backends.

use crate::cache::cache_trait::{CacheBackend, CacheService};
use crate::cache::config::{CacheServiceConfig, MemoryCacheConfig, RedisCacheConfig};
use crate::error::FlowGuardError;
use async_trait::async_trait;
use oxcache::Cache;
use std::sync::Arc;
use std::time::Duration;

/// Unified cache service implementation.
///
/// This struct manages cache instances for both memory and Redis backends,
/// providing a unified interface for cache operations.
///
/// # Example
///
/// ```rust
/// use limiteron::cache::{CacheServiceConfig, OxCacheService, CacheService};
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = CacheServiceConfig::default();
///     let cache_service: Arc<dyn CacheService> = Arc::new(
///         OxCacheService::new(config).await?
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
pub struct OxCacheService {
    /// Memory cache instance.
    memory_cache: Arc<Cache<String, String>>,
    /// Optional Redis cache instance.
    redis_cache: Option<Arc<Cache<String, String>>>,
    /// Service configuration.
    config: CacheServiceConfig,
}

impl OxCacheService {
    /// Create a new OxCacheService with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Cache service configuration
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` - Successfully created service
    /// * `Err(FlowGuardError)` - Failed to create cache instances
    ///
    /// # Note
    ///
    /// Redis connection failures are logged but do not prevent service creation.
    /// The service will fall back to memory cache for Redis operations.
    pub async fn new(config: CacheServiceConfig) -> Result<Self, FlowGuardError> {
        // Create memory cache
        let memory_cache = Arc::new(
            Cache::builder()
                .capacity(config.memory.capacity)
                .ttl(config.memory.ttl)
                .build()
                .await
                .map_err(|e| {
                    FlowGuardError::StorageError(crate::error::StorageError::ConnectionError(
                        format!("Failed to create memory cache: {}", e),
                    ))
                })?,
        );

        // Optionally create Redis cache
        let redis_cache = if config.redis.enabled {
            match Cache::redis(&config.redis.url).await {
                Ok(cache) => Some(Arc::new(cache)),
                Err(e) => {
                    tracing::warn!("Failed to connect to Redis, falling back to memory: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            memory_cache,
            redis_cache,
            config,
        })
    }

    /// Get a reference to the memory cache.
    ///
    /// This is useful for operations that require direct access to the
    /// underlying oxcache instance.
    pub fn memory(&self) -> &Arc<Cache<String, String>> {
        &self.memory_cache
    }

    /// Get a reference to the Redis cache, if available.
    ///
    /// # Returns
    ///
    /// * `Some(&Arc<Cache<String, String>>)` if Redis is configured and connected
    /// * `None` if Redis is not enabled or connection failed
    pub async fn redis(&self) -> Option<&Arc<Cache<String, String>>> {
        self.redis_cache.as_ref()
    }

    /// Get a value from a specific backend.
    ///
    /// # Arguments
    ///
    /// * `key` - The cache key
    /// * `backend` - Which backend to use
    ///
    /// # Returns
    ///
    /// * `Ok(Some(value))` if the key exists
    /// * `Ok(None)` if the key does not exist
    /// * `Err(...)` if an error occurs
    pub async fn get_with_backend(
        &self,
        key: &str,
        backend: CacheBackend,
    ) -> Result<Option<String>, FlowGuardError> {
        match backend {
            CacheBackend::Memory => self.get(key).await,
            CacheBackend::Redis => {
                if let Some(redis) = &self.redis_cache {
                    let key = key.to_string();
                    redis.get(&key).await.map_err(|e| {
                        FlowGuardError::StorageError(crate::error::StorageError::ConnectionError(
                            e.to_string(),
                        ))
                    })
                } else {
                    // Fall back to memory
                    self.get(key).await
                }
            }
        }
    }

    /// Set a value in a specific backend.
    ///
    /// # Arguments
    ///
    /// * `key` - The cache key
    /// * `value` - The value to cache
    /// * `ttl` - Optional TTL override
    /// * `backend` - Which backend to use
    ///
    /// # Returns
    ///
    /// * `Ok(())` on success
    /// * `Err(...)` if an error occurs
    pub async fn set_with_backend(
        &self,
        key: &str,
        value: &str,
        ttl: Option<Duration>,
        backend: CacheBackend,
    ) -> Result<(), FlowGuardError> {
        match backend {
            CacheBackend::Memory => self.set(key, value, ttl).await,
            CacheBackend::Redis => {
                if let Some(redis) = &self.redis_cache {
                    let ttl = ttl.unwrap_or(self.config.redis.ttl);
                    let key = key.to_string();
                    let value = value.to_string();
                    redis
                        .set_with_ttl(&key, &value, Some(ttl))
                        .await
                        .map_err(|e| {
                            FlowGuardError::StorageError(
                                crate::error::StorageError::ConnectionError(e.to_string()),
                            )
                        })
                } else {
                    // Fall back to memory
                    self.set(key, value, ttl).await
                }
            }
        }
    }

    /// Delete a value from a specific backend.
    ///
    /// # Arguments
    ///
    /// * `key` - The cache key to delete
    /// * `backend` - Which backend to use
    ///
    /// # Returns
    ///
    /// * `Ok(())` on success
    /// * `Err(...)` if an error occurs
    pub async fn delete_with_backend(
        &self,
        key: &str,
        backend: CacheBackend,
    ) -> Result<(), FlowGuardError> {
        match backend {
            CacheBackend::Memory => self.delete(key).await,
            CacheBackend::Redis => {
                if let Some(redis) = &self.redis_cache {
                    let key = key.to_string();
                    redis.delete(&key).await.map_err(|e| {
                        FlowGuardError::StorageError(crate::error::StorageError::ConnectionError(
                            e.to_string(),
                        ))
                    })
                } else {
                    // Fall back to memory
                    self.delete(key).await
                }
            }
        }
    }

    /// Get the current configuration.
    pub fn config(&self) -> &CacheServiceConfig {
        &self.config
    }
}

#[async_trait]
impl CacheService for OxCacheService {
    async fn get(&self, key: &str) -> Result<Option<String>, FlowGuardError> {
        let key = key.to_string();
        self.memory_cache.get(&key).await.map_err(|e| {
            FlowGuardError::StorageError(crate::error::StorageError::ConnectionError(e.to_string()))
        })
    }

    async fn set(
        &self,
        key: &str,
        value: &str,
        ttl: Option<Duration>,
    ) -> Result<(), FlowGuardError> {
        let ttl = ttl.unwrap_or(self.config.default_ttl);
        let key = key.to_string();
        let value = value.to_string();

        if self.config.enable_per_entry_ttl {
            self.memory_cache
                .set_with_ttl(&key, &value, Some(ttl))
                .await
                .map_err(|e| {
                    FlowGuardError::StorageError(crate::error::StorageError::ConnectionError(
                        e.to_string(),
                    ))
                })?;
        } else {
            self.memory_cache.set(&key, &value).await.map_err(|e| {
                FlowGuardError::StorageError(crate::error::StorageError::ConnectionError(
                    e.to_string(),
                ))
            })?;
        }

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), FlowGuardError> {
        let key = key.to_string();
        self.memory_cache.delete(&key).await.map_err(|e| {
            FlowGuardError::StorageError(crate::error::StorageError::ConnectionError(e.to_string()))
        })
    }

    async fn set_with_ttl(
        &self,
        key: &str,
        value: &str,
        ttl: Duration,
    ) -> Result<(), FlowGuardError> {
        let key = key.to_string();
        let value = value.to_string();
        self.memory_cache
            .set_with_ttl(&key, &value, Some(ttl))
            .await
            .map_err(|e| {
                FlowGuardError::StorageError(crate::error::StorageError::ConnectionError(
                    e.to_string(),
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::cache_trait::CacheService;

    #[tokio::test]
    async fn test_ox_cache_service_basic_operations() {
        let config = CacheServiceConfig {
            memory: MemoryCacheConfig {
                capacity: 100,
                ttl: Duration::from_secs(60),
            },
            redis: RedisCacheConfig {
                enabled: false,
                ..Default::default()
            },
            default_ttl: Duration::from_secs(300),
            enable_per_entry_ttl: true,
        };

        let service = OxCacheService::new(config).await.unwrap();

        // Test set and get
        service.set("key1", "value1", None).await.unwrap();
        let result = service.get("key1").await.unwrap();
        assert_eq!(result, Some("value1".to_string()));

        // Test get non-existent key
        let result = service.get("nonexistent").await.unwrap();
        assert_eq!(result, None);

        // Test delete
        service.delete("key1").await.unwrap();
        let result = service.get("key1").await.unwrap();
        assert_eq!(result, None);

        // Test set_with_ttl
        service
            .set_with_ttl("key2", "value2", Duration::from_secs(60))
            .await
            .unwrap();
        let result = service.get("key2").await.unwrap();
        assert_eq!(result, Some("value2".to_string()));
    }

    #[tokio::test]
    async fn test_ox_cache_service_with_custom_ttl() {
        let config = CacheServiceConfig {
            memory: MemoryCacheConfig {
                capacity: 100,
                ttl: Duration::from_secs(60),
            },
            redis: RedisCacheConfig::default(),
            default_ttl: Duration::from_secs(300),
            enable_per_entry_ttl: true,
        };

        let service = OxCacheService::new(config).await.unwrap();

        // Set with custom TTL
        service
            .set("key1", "value1", Some(Duration::from_secs(120)))
            .await
            .unwrap();

        // Verify value is set
        let result = service.get("key1").await.unwrap();
        assert_eq!(result, Some("value1".to_string()));
    }

    #[tokio::test]
    async fn test_ox_cache_service_get_with_backend() {
        let config = CacheServiceConfig::default();
        let service = OxCacheService::new(config).await.unwrap();

        service.set("key", "value", None).await.unwrap();

        // Test with Memory backend
        let result = service
            .get_with_backend("key", CacheBackend::Memory)
            .await
            .unwrap();
        assert_eq!(result, Some("value".to_string()));

        // Test with Redis backend (falls back to memory since Redis is not enabled)
        let result = service
            .get_with_backend("key", CacheBackend::Redis)
            .await
            .unwrap();
        assert_eq!(result, Some("value".to_string()));
    }

    #[tokio::test]
    async fn test_ox_cache_service_memory_reference() {
        let config = CacheServiceConfig::default();
        let service = OxCacheService::new(config).await.unwrap();

        let memory = service.memory();
        // Just verify we can access the memory cache
        let _ = memory.as_ref();
    }

    #[tokio::test]
    async fn test_ox_cache_service_config() {
        let config = CacheServiceConfig::default();
        let service = OxCacheService::new(config).await.unwrap();

        let retrieved_config = service.config();
        assert_eq!(retrieved_config.default_ttl, Duration::from_secs(300));
        assert!(retrieved_config.enable_per_entry_ttl);
    }

    #[tokio::test]
    async fn test_ox_cache_service_shared_state() {
        let config = CacheServiceConfig::default();
        let service1 = OxCacheService::new(config).await.unwrap();

        // Share the service via Arc<dyn CacheService>
        let cache_service: Arc<dyn CacheService> = Arc::new(service1);
        let cache_service2 = cache_service.clone();

        cache_service.set("key", "value", None).await.unwrap();

        // Both references should see the same data since they share the underlying service
        let result = cache_service2.get("key").await.unwrap();
        assert_eq!(result, Some("value".to_string()));
    }
}
