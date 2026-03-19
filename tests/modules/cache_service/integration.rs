//! 缓存服务模块集成测试
//!
//! 测试缓存服务 trait 和 oxcache 集成

use limiteron::cache::{Cache, CacheKey, Cacheable};
use limiteron::error::StorageError;
use async_trait::async_trait;
use std::time::Duration;

// ============================================================================
// Mock CacheService for trait testing
// ============================================================================

struct MockCache {
    data: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl MockCache {
    fn new() -> Self {
        Self {
            data: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

#[async_trait]
impl limiteron::cache_service::CacheService for MockCache {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        let data = self.data.lock().unwrap();
        Ok(data.get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str, _ttl: Option<u64>) -> Result<(), StorageError> {
        let mut data = self.data.lock().unwrap();
        data.insert(key.to_string(), value.to_string());
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let mut data = self.data.lock().unwrap();
        data.remove(key);
        Ok(())
    }

    async fn set_with_ttl(
        &self,
        key: &str,
        value: &str,
        _ttl: Duration,
    ) -> Result<(), StorageError> {
        let mut data = self.data.lock().unwrap();
        data.insert(key.to_string(), value.to_string());
        Ok(())
    }
}

// ============================================================================
// CacheKey Tests
// ============================================================================

#[test]
fn test_cache_key_rate_limit() {
    let key = CacheKey::rate_limit("192.168.1.1", "default");
    assert!(!key.to_string().is_empty());
    assert!(format!("{:?}", key).contains("rate_limit"));
}

#[test]
fn test_cache_key_ban() {
    let key = CacheKey::ban("user-123");
    assert!(!key.to_string().is_empty());
    assert!(format!("{:?}", key).contains("ban"));
}

#[test]
fn test_cache_key_quota() {
    let key = CacheKey::quota("user-456", "monthly");
    assert!(!key.to_string().is_empty());
    assert!(format!("{:?}", key).contains("quota"));
}

#[test]
fn test_cache_key_custom() {
    let key = CacheKey::custom("my-prefix", "my-key");
    let key_str = key.to_string();
    assert!(key_str.contains("my-prefix"));
    assert!(key_str.contains("my-key"));
}

// ============================================================================
// Cacheable Tests
// ============================================================================

#[test]
fn test_cacheable_string() {
    let s = "hello world".to_string();
    assert!(s.cache_key("test-key").is_ok());
    let bytes = s.to_cache_bytes().unwrap();
    let restored = String::from_cache_bytes(&bytes).unwrap();
    assert_eq!(restored, "hello world");
}

#[test]
fn test_cacheable_u64() {
    let n: u64 = 42;
    let bytes = n.to_cache_bytes().unwrap();
    let restored = u64::from_cache_bytes(&bytes).unwrap();
    assert_eq!(restored, 42);
}

#[test]
fn test_cacheable_bool() {
    let b = true;
    let bytes = b.to_cache_bytes().unwrap();
    let restored = bool::from_cache_bytes(&bytes).unwrap();
    assert_eq!(restored, true);
}

#[test]
fn test_cacheable_vec() {
    let v = vec![1u64, 2, 3];
    let bytes = v.to_cache_bytes().unwrap();
    let restored: Vec<u64> = Vec::from_cache_bytes(&bytes).unwrap();
    assert_eq!(restored, vec![1, 2, 3]);
}

// ============================================================================
// Cache (oxcache) Tests
// ============================================================================

#[tokio::test]
async fn test_cache_basic_set_get() {
    let cache = Cache::new_memory(100);
    cache.set("k1", "value1", Some(60)).await.unwrap();
    let val = cache.get("k1").await.unwrap();
    assert_eq!(val, Some("value1".to_string()));
}

#[tokio::test]
async fn test_cache_get_missing() {
    let cache = Cache::new_memory(100);
    let val = cache.get("nonexistent").await.unwrap();
    assert!(val.is_none());
}

#[tokio::test]
async fn test_cache_delete() {
    let cache = Cache::new_memory(100);
    cache.set("k1", "value1", Some(60)).await.unwrap();
    cache.delete("k1").await.unwrap();
    let val = cache.get("k1").await.unwrap();
    assert!(val.is_none());
}

#[tokio::test]
async fn test_cache_overwrite() {
    let cache = Cache::new_memory(100);
    cache.set("k1", "value1", Some(60)).await.unwrap();
    cache.set("k1", "value2", Some(60)).await.unwrap();
    let val = cache.get("k1").await.unwrap();
    assert_eq!(val, Some("value2".to_string()));
}

#[tokio::test]
async fn test_cache_multiple_keys() {
    let cache = Cache::new_memory(100);
    cache.set("k1", "v1", Some(60)).await.unwrap();
    cache.set("k2", "v2", Some(60)).await.unwrap();
    cache.set("k3", "v3", Some(60)).await.unwrap();
    assert_eq!(cache.get("k1").await.unwrap(), Some("v1".to_string()));
    assert_eq!(cache.get("k2").await.unwrap(), Some("v2".to_string()));
    assert_eq!(cache.get("k3").await.unwrap(), Some("v3".to_string()));
}

#[tokio::test]
async fn test_cache_clear() {
    let cache = Cache::new_memory(100);
    cache.set("k1", "v1", Some(60)).await.unwrap();
    cache.set("k2", "v2", Some(60)).await.unwrap();
    cache.clear().await.unwrap();
    assert!(cache.get("k1").await.unwrap().is_none());
    assert!(cache.get("k2").await.unwrap().is_none());
}

#[tokio::test]
async fn test_cache_contains() {
    let cache = Cache::new_memory(100);
    cache.set("k1", "v1", Some(60)).await.unwrap();
    assert!(cache.contains("k1").await.unwrap());
    assert!(!cache.contains("k2").await.unwrap());
}

#[tokio::test]
async fn test_cache_stats() {
    let cache = Cache::new_memory(100);
    cache.set("k1", "v1", Some(60)).await.unwrap();
    cache.set("k2", "v2", Some(60)).await.unwrap();
    cache.get("k1").await.unwrap();
    cache.get("nonexistent").await.unwrap();
    let stats = cache.stats();
    assert_eq!(stats.len(), 2);
}

#[tokio::test]
async fn test_cache_len() {
    let cache = Cache::new_memory(100);
    assert_eq!(cache.len(), 0);
    cache.set("k1", "v1", Some(60)).await.unwrap();
    cache.set("k2", "v2", Some(60)).await.unwrap();
    assert_eq!(cache.len(), 2);
}

#[tokio::test]
async fn test_cache_is_empty() {
    let cache = Cache::new_memory(100);
    assert!(cache.is_empty());
    cache.set("k1", "v1", Some(60)).await.unwrap();
    assert!(!cache.is_empty());
}

// ============================================================================
// MockCacheService trait implementation tests
// ============================================================================

#[tokio::test]
async fn test_mock_cache_service_get_set() {
    let cache = MockCache::new();
    limiteron::cache_service::CacheService::set(&cache, "key1", "val1", Some(60))
        .await
        .unwrap();
    let result = limiteron::cache_service::CacheService::get(&cache, "key1")
        .await
        .unwrap();
    assert_eq!(result, Some("val1".to_string()));
}

#[tokio::test]
async fn test_mock_cache_service_delete() {
    let cache = MockCache::new();
    limiteron::cache_service::CacheService::set(&cache, "key1", "val1", Some(60))
        .await
        .unwrap();
    limiteron::cache_service::CacheService::delete(&cache, "key1")
        .await
        .unwrap();
    let result = limiteron::cache_service::CacheService::get(&cache, "key1")
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_mock_cache_service_set_with_ttl() {
    let cache = MockCache::new();
    limiteron::cache_service::CacheService::set_with_ttl(
        &cache,
        "key1",
        "val1",
        Duration::from_secs(300),
    )
    .await
    .unwrap();
    let result = limiteron::cache_service::CacheService::get(&cache, "key1")
        .await
        .unwrap();
    assert_eq!(result, Some("val1".to_string()));
}
