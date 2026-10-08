// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 缓存服务模块集成测试
//!
//! 测试缓存服务 trait（产品 MemoryCache 实现）

use limiteron::error::StorageError;
use std::time::Duration;

// ============================================================================
// Mock CacheService for trait testing
// ============================================================================

// ============================================================================
// CacheService trait implementation tests（产品 MemoryCache）
// ============================================================================

#[tokio::test]
async fn test_mock_cache_service_get_set() {
    let cache = limiteron::MemoryCache::new(None);
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
    let cache = limiteron::MemoryCache::new(None);
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
    let cache = limiteron::MemoryCache::new(None);
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
