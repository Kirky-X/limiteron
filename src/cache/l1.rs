//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! L1缓存实现 - 基于 oxcache 库
//!
//! 使用 oxcache 的高性能内存缓存作为 L1 缓存层。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ahash::AHashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::constants::{
    DEFAULT_L1_CACHE_CAPACITY, DEFAULT_L1_CACHE_CLEANUP_INTERVAL_SECS,
    DEFAULT_L1_CACHE_LRU_THRESHOLD, DEFAULT_L1_CACHE_TTL_SECS,
};
use crate::error::StorageError;
use oxcache::Cache;

/// 默认缓存容量
pub const DEFAULT_CACHE_CAPACITY: usize = DEFAULT_L1_CACHE_CAPACITY;

/// 默认TTL（5分钟）
pub const DEFAULT_TTL_SECS: u64 = DEFAULT_L1_CACHE_TTL_SECS;

/// 默认清理间隔（1分钟）
pub const DEFAULT_CLEANUP_INTERVAL_SECS: u64 = DEFAULT_L1_CACHE_CLEANUP_INTERVAL_SECS;

/// 默认LRU淘汰阈值（90%）
pub const DEFAULT_EVICTION_THRESHOLD: f64 = DEFAULT_L1_CACHE_LRU_THRESHOLD;

// String already implements Cacheable via oxcache's blanket impl

// 缓存条目（用于内部统计）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub value: String,
    pub expires_at: Option<u64>, // Unix timestamp
}

/// L2缓存配置
#[derive(Debug, Clone)]
pub struct L1CacheConfig {
    pub capacity: usize,
    pub default_ttl: Option<Duration>,
    pub cleanup_interval: Duration,
    pub eviction_threshold: f64,
}

impl Default for L1CacheConfig {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_CACHE_CAPACITY,
            default_ttl: Some(Duration::from_secs(DEFAULT_TTL_SECS)),
            cleanup_interval: Duration::from_secs(DEFAULT_CLEANUP_INTERVAL_SECS),
            eviction_threshold: DEFAULT_EVICTION_THRESHOLD,
        }
    }
}

impl L1CacheConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn capacity(mut self, capacity: usize) -> Self {
        if capacity == 0 {
            warn!("缓存容量设置为0，将使用最小值1");
            self.capacity = 1;
        } else {
            self.capacity = capacity;
        }
        self
    }

    pub fn default_ttl(mut self, ttl: Duration) -> Self {
        if ttl.as_secs() == 0 {
            self.default_ttl = Some(Duration::from_secs(DEFAULT_TTL_SECS));
        } else {
            self.default_ttl = Some(ttl);
        }
        self
    }

    pub fn cleanup_interval(mut self, interval: Duration) -> Self {
        self.cleanup_interval = interval;
        self
    }

    pub fn eviction_threshold(mut self, threshold: f64) -> Self {
        self.eviction_threshold = threshold;
        self
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.capacity == 0 {
            Err("缓存容量不能为0".to_string())
        } else {
            Ok(())
        }
    }
}

/// 缓存统计信息
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    hits: Arc<AtomicU64>,
    misses: Arc<AtomicU64>,
    expirations: Arc<AtomicU64>,
    evictions: Arc<AtomicU64>,
    writes: Arc<AtomicU64>,
}

impl CacheStats {
    pub fn new() -> Self {
        Self {
            hits: Arc::new(AtomicU64::new(0)),
            misses: Arc::new(AtomicU64::new(0)),
            expirations: Arc::new(AtomicU64::new(0)),
            evictions: Arc::new(AtomicU64::new(0)),
            writes: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_expiration(&self) {
        self.expirations.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_eviction(&self) {
        self.evictions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_write(&self) {
        self.writes.fetch_add(1, Ordering::Relaxed);
    }

    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    pub fn expirations(&self) -> u64 {
        self.expirations.load(Ordering::Relaxed)
    }

    pub fn evictions(&self) -> u64 {
        self.evictions.load(Ordering::Relaxed)
    }

    pub fn writes(&self) -> u64 {
        self.writes.load(Ordering::Relaxed)
    }

    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
}

/// L2缓存实现 - 基于 oxcache
pub struct L1Cache {
    cache: Arc<Cache<String, String>>,
    config: L1CacheConfig,
    stats: Arc<CacheStats>,
}

impl L1Cache {
    /// 创建新的L2缓存
    pub async fn new(capacity: usize, default_ttl: Duration) -> Self {
        Self::with_config(
            L1CacheConfig::new()
                .capacity(capacity)
                .default_ttl(default_ttl),
        )
        .await
    }

    /// 使用配置创建L2缓存
    pub async fn with_config(config: L1CacheConfig) -> Self {
        if let Err(e) = config.validate() {
            panic!("L1Cache配置无效: {}", e);
        }

        // 使用 oxcache 创建内存缓存
        let cache: Cache<String, String> = Cache::builder()
            .capacity(config.capacity as u64)
            .build()
            .await
            .expect("Failed to create oxcache cache");

        let stats = Arc::new(CacheStats::new());

        debug!(
            "L1Cache 创建成功 (oxcache): capacity={}, default_ttl={:?}",
            config.capacity, config.default_ttl
        );

        Self {
            cache: Arc::new(cache),
            config,
            stats,
        }
    }

    pub async fn get(&self, key: &str) -> Option<String> {
        match self.cache.get(&key.to_string()).await {
            Ok(Some(value)) => {
                self.stats.record_hit();
                Some(value)
            }
            Ok(None) => {
                self.stats.record_miss();
                None
            }
            Err(_) => {
                self.stats.record_miss();
                None
            }
        }
    }

    pub async fn set(&self, key: &str, value: &str, ttl: Option<Duration>) {
        match self
            .cache
            .set_with_ttl(&key.to_string(), &value.to_string(), ttl)
            .await
        {
            Ok(_) => {
                self.stats.record_write();
            }
            Err(e) => {
                tracing::error!(error = ?e, "Failed to set cache value");
            }
        }
    }

    pub async fn delete(&self, key: &str) {
        if let Err(e) = self.cache.delete(&key.to_string()).await {
            tracing::error!(error = ?e, "Failed to delete cache key");
        }
    }

    pub async fn contains(&self, key: &str) -> bool {
        self.cache.exists(&key.to_string()).await.unwrap_or(false)
    }

    pub async fn clear(&self) {
        if let Err(e) = self.cache.clear().await {
            tracing::error!(error = ?e, "Failed to clear cache");
        }
    }

    pub async fn len(&self) -> usize {
        match self.cache.stats().await {
            Ok(stats) => stats.get("size").and_then(|s| s.parse().ok()).unwrap_or(0),
            Err(_) => 0,
        }
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    pub async fn get_or_load<F, Fut>(&self, key: &str, loader: F) -> Result<String, StorageError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<String, StorageError>>,
    {
        if let Some(value) = self.get(key).await {
            return Ok(value);
        }

        let value = loader().await?;
        self.set(key, &value, self.config.default_ttl).await;
        Ok(value)
    }

    pub async fn batch_get(&self, keys: &[String]) -> AHashMap<String, String> {
        match self.cache.get_many(keys.iter()).await {
            Ok(result) => result.into_iter().collect(),
            Err(_) => AHashMap::new(),
        }
    }

    pub async fn batch_set(&self, items: &[(String, String, Option<Duration>)]) {
        for (key, value, ttl) in items {
            self.set(key, value, *ttl).await;
        }
    }

    pub async fn batch_delete(&self, keys: &[String]) {
        if let Err(e) = self.cache.delete_many(keys.iter()).await {
            tracing::error!(error = ?e, "Failed to batch delete cache keys");
        }
    }

    pub async fn cleanup_expired(&self) -> usize {
        // oxcache 自动处理过期条目，这里返回 0 表示不需要手动清理
        0
    }

    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    pub fn config(&self) -> &L1CacheConfig {
        &self.config
    }

    pub async fn shutdown(&self) {
        if let Err(e) = self.cache.shutdown().await {
            tracing::error!(error = ?e, "Failed to shutdown cache");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_cache_set_get() {
        let cache = L1Cache::new(100, Duration::from_secs(60)).await;
        cache.set("key1", "value1", None).await;
        let value = cache.get("key1").await;
        assert_eq!(value, Some("value1".to_string()));
    }

    #[tokio::test]
    async fn test_cache_ttl() {
        let cache = L1Cache::new(100, Duration::from_secs(60)).await;
        cache
            .set("key1", "value1", Some(Duration::from_millis(100)))
            .await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        let value = cache.get("key1").await;
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_single_flight() {
        let cache = L1Cache::new(100, Duration::from_secs(60)).await;
        let load_count = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let load_count_clone = load_count.clone();

        let loader = || {
            let load_count = load_count_clone.clone();
            async move {
                load_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok("loaded_value".to_string())
            }
        };

        let task1 = cache.get_or_load("key1", loader);
        let task2 = cache.get_or_load("key1", loader);
        let task3 = cache.get_or_load("key1", loader);

        let (r1, r2, r3) = tokio::join!(task1, task2, task3);
        assert!(r1.is_ok() && r2.is_ok() && r3.is_ok());
        assert_eq!(load_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_contains() {
        let cache = L1Cache::new(100, Duration::from_secs(60)).await;
        cache.set("key1", "value1", None).await;
        assert!(cache.contains("key1").await);
        assert!(!cache.contains("key2").await);
    }

    #[tokio::test]
    async fn test_clear() {
        let cache = L1Cache::new(100, Duration::from_secs(60)).await;
        cache.set("key1", "value1", None).await;
        cache.clear().await;
        assert!(!cache.contains("key1").await);
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let cache = L1Cache::new(100, Duration::from_secs(60)).await;

        let items = vec![
            ("key1".to_string(), "value1".to_string(), None),
            ("key2".to_string(), "value2".to_string(), None),
        ];
        cache.batch_set(&items).await;

        let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
        let result = cache.batch_get(&keys).await;

        assert_eq!(result.len(), 2);
        assert_eq!(result.get("key1"), Some(&"value1".to_string()));
        assert_eq!(result.get("key2"), Some(&"value2".to_string()));
    }
}
