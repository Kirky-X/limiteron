//! L1缓存模块集成测试

use limiteron::l1_cache::{L1Cache, L1CacheConfig, RateLimitCacheKey};
use std::time::Duration;

#[tokio::test]
async fn test_l1_cache_basic() {
    let cache: L1Cache<bool> = L1Cache::new();
    let key = RateLimitCacheKey::user_rate_limit("user1", "rule1");
    cache.set(key.clone(), false);
    let result = cache.get(&key);
    assert!(result.is_some());
}

#[tokio::test]
async fn test_l1_cache_miss() {
    let cache: L1Cache<bool> = L1Cache::new();
    let key = RateLimitCacheKey::user_rate_limit("user1", "rule1");
    let result = cache.get(&key);
    assert!(result.is_none());
}

#[tokio::test]
async fn test_l1_cache_delete() {
    let cache: L1Cache<bool> = L1Cache::new();
    let key = RateLimitCacheKey::user_rate_limit("user1", "rule1");
    cache.set(key.clone(), false);
    cache.invalidate(&key);
    assert!(cache.get(&key).is_none());
}

#[tokio::test]
async fn test_rate_limit_cache_key() {
    let key = RateLimitCacheKey::user_rate_limit("user", "rule");
    assert!(key.contains("user"));
    assert!(key.contains("rule"));
}

#[tokio::test]
async fn test_l1_cache_with_config() {
    let config = L1CacheConfig::new(Duration::from_secs(60), 1000);
    let cache: L1Cache<String> = L1Cache::with_config(config);
    let key = RateLimitCacheKey::ip_rate_limit("192.168.1.1", "rule1");
    cache.set(key.clone(), "allowed".to_string());
    assert!(cache.get(&key).is_some());
}

#[tokio::test]
async fn test_l1_cache_clear() {
    let cache: L1Cache<i32> = L1Cache::new();
    cache.set("key1".to_string(), 1);
    cache.set("key2".to_string(), 2);
    assert_eq!(cache.len(), 2);
    cache.clear();
    assert!(cache.is_empty());
}

#[tokio::test]
async fn test_l1_cache_stats() {
    let cache: L1Cache<bool> = L1Cache::new();
    cache.set("key1".to_string(), true);
    let _ = cache.get("key1"); // hit
    let _ = cache.get("missing"); // miss
    let stats = cache.stats();
    assert_eq!(stats.total_lookups, 2);
}
