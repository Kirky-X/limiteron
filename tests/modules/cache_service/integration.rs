// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 缓存服务模块集成测试
//!
//! 测试缓存服务 trait 和 oxcache 集成

use limiteron::async_trait::async_trait;
use limiteron::error::StorageError;
use std::time::Duration;

// NOTE: 以下 API 在源码中不存在，相关测试已移除（v0.2.0 决策）：
// - `CacheKey::rate_limit/ban/quota/custom`（CacheKey 是 trait，非 struct，无这些方法）
// - `Cacheable` trait（cache_key/to_cache_bytes/from_cache_bytes 方法）
// - `Cache::new_memory(100)` 构造函数（oxcache 0.3.2 无此方法）
// 待 v0.2.1 决策：是补源码 API 还是保持移除。

// ============================================================================
// Mock CacheService for trait testing
// ============================================================================

struct MockCache {
    data: std::sync::Mutex<ahash::AHashMap<String, String>>,
}

impl MockCache {
    fn new() -> Self {
        Self {
            data: std::sync::Mutex::new(ahash::AHashMap::new()),
        }
    }
}

#[async_trait]
impl limiteron::cache::cache_service::CacheService for MockCache {
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
// CacheKey Tests — 已移除（CacheKey 是 trait，rate_limit/ban/quota/custom 方法不存在）
// ============================================================================

// ============================================================================
// Cacheable Tests — 已移除（Cacheable trait 不存在于源码）
// ============================================================================

// ============================================================================
// Cache (oxcache) Tests — 已移除（Cache::new_memory 不存在于 oxcache 0.3.2）
// ============================================================================

// ============================================================================
// MockCacheService trait implementation tests
// ============================================================================

#[tokio::test]
async fn test_mock_cache_service_get_set() {
    let cache = MockCache::new();
    limiteron::cache::cache_service::CacheService::set(&cache, "key1", "val1", Some(60))
        .await
        .unwrap();
    let result = limiteron::cache::cache_service::CacheService::get(&cache, "key1")
        .await
        .unwrap();
    assert_eq!(result, Some("val1".to_string()));
}

#[tokio::test]
async fn test_mock_cache_service_delete() {
    let cache = MockCache::new();
    limiteron::cache::cache_service::CacheService::set(&cache, "key1", "val1", Some(60))
        .await
        .unwrap();
    limiteron::cache::cache_service::CacheService::delete(&cache, "key1")
        .await
        .unwrap();
    let result = limiteron::cache::cache_service::CacheService::get(&cache, "key1")
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_mock_cache_service_set_with_ttl() {
    let cache = MockCache::new();
    limiteron::cache::cache_service::CacheService::set_with_ttl(
        &cache,
        "key1",
        "val1",
        Duration::from_secs(300),
    )
    .await
    .unwrap();
    let result = limiteron::cache::cache_service::CacheService::get(&cache, "key1")
        .await
        .unwrap();
    assert_eq!(result, Some("val1".to_string()));
}
