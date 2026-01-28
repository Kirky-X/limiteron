// Copyright (c) 2026, Kirky.X
//
// MIT License
//
// Cache Configuration Types
//
// Defines configuration structures for the unified cache service,
// including settings for memory and Redis backends.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for the unified cache service.
///
/// This struct holds all configuration options for the cache service,
/// including settings for both memory and Redis backends.
///
/// # Example
///
/// ```rust
/// use limiteron::cache::{CacheServiceConfig, MemoryCacheConfig, RedisCacheConfig};
/// use std::time::Duration;
///
/// let config = CacheServiceConfig {
///     memory: MemoryCacheConfig {
///         capacity: 10_000,
///         ttl: Duration::from_secs(300),
///     },
///     redis: RedisCacheConfig {
///         enabled: true,
///         url: "redis://127.0.0.1:6379".to_string(),
///         capacity: 100_000,
///         ttl: Duration::from_secs(3600),
///     },
///     default_ttl: Duration::from_secs(600),
///     enable_per_entry_ttl: true,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheServiceConfig {
    /// Configuration for the memory cache backend.
    pub memory: MemoryCacheConfig,
    /// Configuration for the Redis cache backend.
    pub redis: RedisCacheConfig,
    /// Default TTL for cache entries when not specified.
    #[serde(default = "default_ttl")]
    pub default_ttl: Duration,
    /// Whether to enable per-entry TTL support.
    #[serde(default = "default_true")]
    pub enable_per_entry_ttl: bool,
}

impl Default for CacheServiceConfig {
    fn default() -> Self {
        Self {
            memory: MemoryCacheConfig::default(),
            redis: RedisCacheConfig::default(),
            default_ttl: Duration::from_secs(300),
            enable_per_entry_ttl: true,
        }
    }
}

fn default_ttl() -> Duration {
    Duration::from_secs(300)
}

fn default_true() -> bool {
    true
}

/// Configuration for the memory cache backend.
///
/// The memory cache is always available and provides fast access
/// for frequently used data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCacheConfig {
    /// Maximum number of entries in the cache.
    ///
    /// When the cache reaches this capacity, least recently used
    /// entries will be evicted.
    #[serde(default = "default_memory_capacity")]
    pub capacity: u64,
    /// Time-to-live for cache entries.
    ///
    /// Entries older than this duration will be considered stale
    /// and may be evicted.
    #[serde(default = "default_memory_ttl")]
    pub ttl: Duration,
}

impl Default for MemoryCacheConfig {
    fn default() -> Self {
        Self {
            capacity: default_memory_capacity(),
            ttl: default_memory_ttl(),
        }
    }
}

fn default_memory_capacity() -> u64 {
    10_000
}

fn default_memory_ttl() -> Duration {
    Duration::from_secs(300)
}

/// Configuration for the Redis cache backend.
///
/// Redis provides distributed caching capabilities for scenarios
/// requiring shared state across multiple instances.
///
/// # Note
///
/// Redis connection failures will cause the service to fall back
/// to the memory cache for that operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedisCacheConfig {
    /// Whether to enable Redis backend.
    ///
    /// When disabled, Redis operations will silently fall back to memory.
    #[serde(default)]
    pub enabled: bool,
    /// Redis connection URL.
    ///
    /// Format: `redis://host:port`
    #[serde(default = "default_redis_url")]
    pub url: String,
    /// Maximum number of entries in the Redis cache.
    #[serde(default = "default_redis_capacity")]
    pub capacity: u64,
    /// Time-to-live for cache entries in Redis.
    #[serde(default = "default_redis_ttl")]
    pub ttl: Duration,
}

impl Default for RedisCacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: default_redis_url(),
            capacity: default_redis_capacity(),
            ttl: default_redis_ttl(),
        }
    }
}

fn default_redis_url() -> String {
    "redis://127.0.0.1:6379".to_string()
}

fn default_redis_capacity() -> u64 {
    100_000
}

fn default_redis_ttl() -> Duration {
    Duration::from_secs(3600)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_cache_service_config_default() {
        let config = CacheServiceConfig::default();

        assert_eq!(config.memory.capacity, 10_000);
        assert_eq!(config.memory.ttl, Duration::from_secs(300));
        assert!(!config.redis.enabled);
        assert_eq!(config.redis.url, "redis://127.0.0.1:6379");
        assert_eq!(config.default_ttl, Duration::from_secs(300));
        assert!(config.enable_per_entry_ttl);
    }

    #[test]
    fn test_memory_cache_config_default() {
        let config = MemoryCacheConfig::default();

        assert_eq!(config.capacity, 10_000);
        assert_eq!(config.ttl, Duration::from_secs(300));
    }

    #[test]
    fn test_redis_cache_config_default() {
        let config = RedisCacheConfig::default();

        assert!(!config.enabled);
        assert_eq!(config.url, "redis://127.0.0.1:6379");
        assert_eq!(config.capacity, 100_000);
        assert_eq!(config.ttl, Duration::from_secs(3600));
    }

    #[test]
    fn test_cache_service_config_serialization() {
        let config = CacheServiceConfig::default();
        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: CacheServiceConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_memory_cache_config_serialization() {
        let config = MemoryCacheConfig {
            capacity: 5000,
            ttl: Duration::from_secs(600),
        };
        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: MemoryCacheConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_redis_cache_config_serialization() {
        let config = RedisCacheConfig {
            enabled: true,
            url: "redis://localhost:6379".to_string(),
            capacity: 50000,
            ttl: Duration::from_secs(7200),
        };
        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: RedisCacheConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_cache_service_config_clone() {
        let config = CacheServiceConfig::default();
        let cloned = config.clone();

        assert_eq!(config, cloned);
    }
}
