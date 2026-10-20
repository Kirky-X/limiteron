// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
// MemoryCache - In-memory cache implementation of CacheService
//
// Implements the CacheService trait with an in-memory backend backed by
// oxcache `Cache` (moka TTL backend). The hand-written DashMap entry
// store and second-resolution expiry are gone: per-entry TTL is handled
// natively by the backend.

use crate::cache::CacheService;
use crate::error::StorageError;
use async_trait::async_trait;
use oxcache::Cache;
use oxcache::error::OxCacheError;
use std::sync::Arc;
use std::time::Duration;

/// In-memory cache implementation backed by oxcache `Cache`
///
/// TTL 语义：per-entry TTL 由 oxcache 后端（moka）原生承载——
/// `set(key, value, None)` 走构建期默认 TTL（`CacheBuilder::ttl`）；
/// `set_with_ttl` 接受任意精度 `Duration`，亚秒 TTL 精确过期
/// （旧实现的秒级时间戳 clamp `max(1)` 已随 `CacheEntry` 删除，
/// 行为由"至少存活 1 秒"收紧为"精确亚秒过期"）。
pub struct MemoryCache {
    inner: Arc<Cache<String, String>>,
}

impl MemoryCache {
    /// Create a new MemoryCache with default TTL
    ///
    /// `Some(ttl)` 经 `CacheBuilder::ttl` 设为构建期默认 TTL；
    /// `None` 不设默认 TTL（条目不过期）。
    pub fn new(default_ttl: Option<u64>) -> Self {
        let mut builder = Cache::builder();
        if let Some(ttl) = default_ttl {
            builder = builder.ttl(Duration::from_secs(ttl));
        }
        // 默认 Moka 路径在当前 feature 链（core → ... → memory）下必然
        // 可用；失败仅发生于 memory feature 缺失的非法组合
        let cache = builder
            .build_sync()
            .unwrap_or_else(|e| panic!("oxcache Cache build failed: {e}"));
        Self {
            inner: Arc::new(cache),
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
}

/// OxCacheError → StorageError 映射（与 crate 内先例 src/cache/storage.rs 一致）
fn map_error(e: OxCacheError) -> StorageError {
    match e {
        OxCacheError::Connection(_) => StorageError::ConnectionError(e.to_string()),
        OxCacheError::Timeout(_) => StorageError::ConnectionError(e.to_string()),
        _ => StorageError::QueryError(e.to_string()),
    }
}

#[async_trait]
impl CacheService for MemoryCache {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        self.inner.get(&key.to_string()).await.map_err(map_error)
    }

    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError> {
        // None → builder 默认 TTL（design D4）
        self.inner
            .set_with_ttl(
                &key.to_string(),
                &value.to_string(),
                ttl.map(Duration::from_secs),
            )
            .await
            .map_err(map_error)
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.inner.delete(&key.to_string()).await.map_err(map_error)
    }

    async fn set_with_ttl(
        &self,
        key: &str,
        value: &str,
        ttl: Duration,
    ) -> Result<(), StorageError> {
        // 亚秒 TTL 由后端真实承载：直接传 Duration，删除旧 clamp
        self.inner
            .set_with_ttl(&key.to_string(), &value.to_string(), Some(ttl))
            .await
            .map_err(map_error)
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
    use tokio::time::{Duration, sleep};

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

        cache
            .set("expiring_key", "expiring_value", None)
            .await
            .unwrap();
        let result = cache.get("expiring_key").await.unwrap();
        assert_eq!(result, Some("expiring_value".to_string()));

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
        // 500ms 到期前存活（400ms 探针，收紧"在此之前存活"断言）
        sleep(Duration::from_millis(400)).await;
        let result = cache.get("specific_key").await.unwrap();
        assert_eq!(result, Some("specific_value".to_string()));

        // 500ms 后精确过期：oxcache 后端真实承载亚秒 TTL。旧实现把亚秒
        // clamp 到 1 秒且用秒级时间戳判定（实际过期时刻散布于 (0,1]），
        // 700ms 探针对旧实现为概率性判别；迁移后本断言确定性成立。
        sleep(Duration::from_millis(300)).await;
        let result = cache.get("specific_key").await.unwrap();
        assert_eq!(
            result, None,
            "a 500ms TTL must expire precisely after 700ms"
        );
    }

    /// Default 300s 默认 TTL 的全时长锚（R-memory-cache-002）：
    /// 墙钟 300s 无法进常规单测，按需运行 `cargo test -- --ignored`。
    #[tokio::test]
    #[ignore = "wall-clock 301s; run explicitly for the full-duration default TTL anchor"]
    async fn test_default_ttl_300s_full_duration() {
        let cache = MemoryCache::default();

        cache.set("k", "v", None).await.unwrap();
        // 300 秒窗口内存活（299s 探针）
        sleep(Duration::from_secs(299)).await;
        assert_eq!(
            cache.get("k").await.unwrap(),
            Some("v".to_string()),
            "Default 300s TTL entry must stay alive at 299s"
        );
        // 超过 300 秒后过期（累计 302s）
        sleep(Duration::from_secs(3)).await;
        assert_eq!(
            cache.get("k").await.unwrap(),
            None,
            "Default 300s TTL entry must expire at 302s"
        );
    }

    #[tokio::test]
    async fn test_memory_cache_no_expiry() {
        let cache = MemoryCache::no_expiry();

        cache
            .set("permanent_key", "permanent_value", None)
            .await
            .unwrap();
        let result = cache.get("permanent_key").await.unwrap();
        assert_eq!(result, Some("permanent_value".to_string()));

        // Even after some time, the value should still exist
        tokio::time::sleep(Duration::from_millis(100)).await;
        let result = cache.get("permanent_key").await.unwrap();
        assert_eq!(result, Some("permanent_value".to_string()));
    }

    #[tokio::test]
    async fn test_default_ttl_keeps_entry_alive() {
        // Default 300s 默认 TTL 锚：条目在探测窗内存活。墙钟 300s
        // 无法在单测等待；同一 builder TTL 路径的过期行为由
        // test_memory_cache_with_ttl（1s）与亚秒精确过期测试覆盖。
        let cache = MemoryCache::default();

        cache.set("k", "v", None).await.unwrap();
        sleep(Duration::from_millis(100)).await;
        assert_eq!(cache.get("k").await.unwrap(), Some("v".to_string()));
    }

    #[tokio::test]
    async fn test_per_op_ttl_overrides_long_default() {
        // per-op TTL 覆盖默认值：300s 默认下 set(Some(1)) 仍按 1s 过期
        let cache = MemoryCache::new(Some(300));

        cache.set("k", "v", Some(1)).await.unwrap();
        let result = cache.get("k").await.unwrap();
        assert_eq!(result, Some("v".to_string()));

        sleep(Duration::from_secs(2)).await;
        let result = cache.get("k").await.unwrap();
        assert_eq!(result, None);
    }
}
