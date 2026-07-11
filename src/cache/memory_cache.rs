// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
// MemoryCache - In-memory cache implementation of CacheService
//
// Implements the CacheService trait with an in-memory backend using DashMap.

use crate::cache::CacheService;
use crate::error::StorageError;
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// In-memory cache entry with TTL
#[derive(Clone)]
struct CacheEntry {
    value: String,
    expiry: Option<u64>, // Unix timestamp in seconds
}

impl CacheEntry {
    fn new(value: String, ttl_seconds: Option<u64>) -> Self {
        let expiry = ttl_seconds.map(|ttl| {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            now + ttl
        });

        CacheEntry { value, expiry }
    }

    fn is_expired(&self) -> bool {
        if let Some(expiry) = self.expiry {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            now >= expiry
        } else {
            false
        }
    }

    fn value(&self) -> Option<&str> {
        if self.is_expired() {
            None
        } else {
            Some(&self.value)
        }
    }
}

/// In-memory cache implementation using DashMap
pub struct MemoryCache {
    storage: DashMap<String, CacheEntry>,
    default_ttl: Option<u64>,
}

impl MemoryCache {
    /// Create a new MemoryCache with default TTL
    pub fn new(default_ttl: Option<u64>) -> Self {
        Self {
            storage: DashMap::new(),
            default_ttl,
        }
    }

    /// Create a new MemoryCache with a specific default TTL in seconds
    pub fn with_ttl(ttl_seconds: u64) -> Self {
        Self::new(Some(ttl_seconds))
    }

    /// Create a new MemoryCache with no default TTL (entries don't expire)
    pub fn no_expiry() -> Self {
        Self::new(None)
    }

    /// Get a value without checking expiration (internal use)
    fn get_raw(&self, key: &str) -> Option<String> {
        if let Some(entry) = self.storage.get(key) {
            if !entry.is_expired() {
                Some(entry.value.clone())
            } else {
                // Remove expired entry
                self.storage.remove(key);
                None
            }
        } else {
            None
        }
    }
}

#[async_trait]
impl CacheService for MemoryCache {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        Ok(self.get_raw(key))
    }

    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError> {
        let effective_ttl = ttl.or(self.default_ttl);
        let entry = CacheEntry::new(value.to_string(), effective_ttl);
        self.storage.insert(key.to_string(), entry);
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.storage.remove(key);
        Ok(())
    }

    async fn set_with_ttl(&self, key: &str, value: &str, ttl: Duration) -> Result<(), StorageError> {
        let ttl_seconds = Some(ttl.as_secs());
        let entry = CacheEntry::new(value.to_string(), ttl_seconds);
        self.storage.insert(key.to_string(), entry);
        Ok(())
    }
}

impl Default for MemoryCache {
    fn default() -> Self {
        Self::new(Some(300)) // Default 5-minute TTL
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_memory_cache_basic_operations() {
        let cache = MemoryCache::default();

        // Test set and get
        cache.set("key1", "value1", None).await.unwrap();
        let result = cache.get("key1").await.unwrap();
        assert_eq!(result, Some("value1".to_string()));

        // Test delete
        cache.delete("key1").await.unwrap();
        let result = cache.get("key1").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_memory_cache_with_ttl() {
        let cache = MemoryCache::new(Some(1)); // 1-second TTL

        cache.set("expiring_key", "expiring_value", None).await.unwrap();
        let result = cache.get("expiring_key").await.unwrap();
        assert_eq!(result, Some("expiring_value".to_string()));

        // Wait for expiration
        sleep(Duration::from_secs(2)).await;
        let result = cache.get("expiring_key").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_memory_cache_set_with_specific_ttl() {
        let cache = MemoryCache::default();

        cache
            .set_with_ttl("specific_key", "specific_value", Duration::from_millis(500))
            .await
            .unwrap();
        let result = cache.get("specific_key").await.unwrap();
        assert_eq!(result, Some("specific_value".to_string()));

        // Wait for expiration
        sleep(Duration::from_secs(1)).await;
        let result = cache.get("specific_key").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_memory_cache_no_expiry() {
        let cache = MemoryCache::no_expiry();

        cache.set("permanent_key", "permanent_value", None).await.unwrap();
        let result = cache.get("permanent_key").await.unwrap();
        assert_eq!(result, Some("permanent_value".to_string()));

        // Even after some time, the value should still exist
        tokio::time::sleep(Duration::from_millis(100)).await;
        let result = cache.get("permanent_key").await.unwrap();
        assert_eq!(result, Some("permanent_value".to_string()));
    }
}
