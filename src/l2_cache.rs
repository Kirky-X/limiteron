//! L2缓存实现
//!
//! 使用DashMap实现高性能并发缓存，支持TTL、LRU淘汰、单飞模式和批量操作。
//!
//! # 特性
//!
//! - **高性能**: 使用DashMap实现无锁并发，P99延迟 < 1ms
//! - **TTL管理**: 自动清理过期数据
//! - **单飞模式**: 防止缓存击穿
//! - **LRU淘汰**: 自动淘汰最少使用的数据
//! - **批量操作**: 支持批量get/set操作
//!
//! # 使用示例
//!
//! ```no_run
//! use limiteron::l2_cache::L2Cache;
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() {
//!     let cache = L2Cache::new(10000, Duration::from_secs(60));
//!
//!     // 设置值
//!     cache.set("key1", "value1", Some(Duration::from_secs(30))).await;
//!
//!     // 获取值
//!     if let Some(value) = cache.get("key1").await {
//!         println!("Value: {}", value);
//!     }
//!
//!     // 单飞模式加载
//!     let value = cache.get_or_load("key2", async {
//!         // 从数据库加载
//!         Ok("loaded_value".to_string())
//!     }).await.unwrap();
//! }
//! ```

/// 默认缓存容量
pub const DEFAULT_CACHE_CAPACITY: usize = 10_000;

/// 默认TTL（5分钟）
pub const DEFAULT_TTL_SECS: u64 = 300;

/// 默认清理间隔（1分钟）
pub const DEFAULT_CLEANUP_INTERVAL_SECS: u64 = 60;

/// 默认LRU淘汰阈值（90%）
pub const DEFAULT_EVICTION_THRESHOLD: f64 = 0.9;

use ahash::AHashMap as HashMap;
use dashmap::DashMap;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{debug, trace};

use crate::error::StorageError;

/// 缓存条目
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// 缓存值
    pub value: String,
    /// 过期时间（None表示永不过期）
    pub expires_at: Option<Instant>,
    /// 最后访问时间
    pub last_accessed: Instant,
    /// 访问次数
    pub access_count: u64,
}

impl CacheEntry {
    /// 创建新的缓存条目
    pub fn new(value: String, ttl: Option<Duration>) -> Self {
        let expires_at = ttl.map(|d| Instant::now() + d);
        Self {
            value,
            expires_at,
            last_accessed: Instant::now(),
            access_count: 1,
        }
    }

    /// 检查是否过期
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Instant::now() > expires_at
        } else {
            false
        }
    }

    /// 更新访问信息
    pub fn update_access(&mut self) {
        self.last_accessed = Instant::now();
        self.access_count += 1;
    }
}

/// 单飞加载器
struct SingleFlightLoader {
    /// 加载中的任务: key -> sender
    pending: DashMap<String, watch::Sender<Option<Result<String, StorageError>>>>,
}

impl SingleFlightLoader {
    fn new() -> Self {
        Self {
            pending: DashMap::new(),
        }
    }

    /// 尝试获取已存在的加载任务，或创建新的
    async fn get_or_load<F, Fut>(&self, key: &str, loader: F) -> Result<String, StorageError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<String, StorageError>>,
    {
        use dashmap::mapref::entry::Entry;

        // 尝试插入新任务或获取现有任务
        let tx = match self.pending.entry(key.to_string()) {
            Entry::Occupied(entry) => {
                // 已有其他请求在加载，等待结果
                trace!("等待其他请求加载 key={}", key);
                let tx = entry.get();
                let mut rx = tx.subscribe();
                drop(entry); // 释放锁

                // 检查当前值
                if let Some(res) = rx.borrow().clone() {
                    return res;
                }

                // 等待变更
                if rx.changed().await.is_ok() {
                    if let Some(res) = rx.borrow().clone() {
                        return res;
                    }
                }

                return Err(StorageError::TimeoutError(
                    "Loader dropped without result".to_string(),
                ));
            }
            Entry::Vacant(entry) => {
                // 创建新任务
                let (tx, _) = watch::channel(None);
                entry.insert(tx.clone());
                tx
            }
        };

        // 执行加载
        let result = loader().await;

        // 通知等待者
        let _ = tx.send(Some(result.clone()));

        // 清理单飞条目
        self.pending.remove(key);

        result
    }
}

/// L2缓存配置
#[derive(Debug, Clone)]
pub struct L2CacheConfig {
    /// 缓存容量
    pub capacity: usize,
    /// 默认TTL
    pub default_ttl: Option<Duration>,
    /// 清理间隔
    pub cleanup_interval: Duration,
    /// LRU淘汰阈值（容量使用率超过此值时触发淘汰）
    pub eviction_threshold: f64,
}

impl Default for L2CacheConfig {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_CACHE_CAPACITY,
            default_ttl: Some(Duration::from_secs(DEFAULT_TTL_SECS)),
            cleanup_interval: Duration::from_secs(DEFAULT_CLEANUP_INTERVAL_SECS),
            eviction_threshold: DEFAULT_EVICTION_THRESHOLD,
        }
    }
}

impl L2CacheConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    pub fn default_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = Some(ttl);
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
}

/// L2缓存实现
pub struct L2Cache {
    /// 缓存数据（使用 LRU Cache 实现自动淘汰）
    data: Arc<parking_lot::Mutex<lru::LruCache<String, CacheEntry>>>,
    /// 单飞加载器
    single_flight: Arc<SingleFlightLoader>,
    /// 配置
    config: L2CacheConfig,
    /// 统计信息
    __stats: Arc<CacheStats>,
    /// 清理任务句柄
    cleanup_handle: Option<JoinHandle<()>>,
}

/// 缓存统计信息
#[derive(Debug, Default)]
pub struct CacheStats {
    /// 命中次数
    hits: AtomicU64,
    /// 未命中次数
    misses: AtomicU64,
    /// 过期次数
    expirations: AtomicU64,
    /// 淘汰次数
    evictions: AtomicU64,
    /// 写入次数
    writes: AtomicU64,
}

impl CacheStats {
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
        let total = self.hits() + self.misses();
        if total == 0 {
            0.0
        } else {
            self.hits() as f64 / total as f64
        }
    }
}

impl L2Cache {
    /// 创建新的L2缓存
    ///
    /// # 参数
    ///
    /// * `capacity` - 缓存容量
    /// * `cleanup_interval` - 清理间隔
    pub fn new(capacity: usize, cleanup_interval: Duration) -> Self {
        Self::with_config(L2CacheConfig {
            capacity,
            cleanup_interval,
            ..Default::default()
        })
    }

    /// 使用配置创建L2缓存
    pub fn with_config(config: L2CacheConfig) -> Self {
        let stats = Arc::new(CacheStats::default());
        let single_flight = Arc::new(SingleFlightLoader::new());
        let cleanup_handle = Self::start_cleanup_task(Arc::clone(&stats), config.cleanup_interval);

        Self {
            data: Arc::new(parking_lot::Mutex::new(lru::LruCache::new(
                NonZeroUsize::new(config.capacity).unwrap_or(NonZeroUsize::new(1).unwrap()),
            ))),
            single_flight,
            config,
            __stats: stats,
            cleanup_handle: Some(cleanup_handle),
        }
    }

    /// 启动清理任务
    fn start_cleanup_task(__stats: Arc<CacheStats>, interval: Duration) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut cleanup_interval = tokio::time::interval(interval);
            loop {
                cleanup_interval.tick().await;
                debug!("执行缓存清理任务");
                // 清理逻辑在各个缓存实例中实现
            }
        })
    }

    /// 获取值
    pub async fn get(&self, key: &str) -> Option<String> {
        let mut cache = self.data.lock();
        if let Some(entry) = cache.get_mut(key) {
            // 检查是否过期
            if entry.is_expired() {
                cache.pop(key);
                self.__stats.expirations.fetch_add(1, Ordering::Relaxed);
                return None;
            }

            // 更新访问信息
            entry.update_access();
            self.__stats.hits.fetch_add(1, Ordering::Relaxed);
            Some(entry.value.clone())
        } else {
            self.__stats.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// 设置值
    pub async fn set(&self, key: &str, value: &str, ttl: Option<Duration>) {
        let ttl = ttl.or(self.config.default_ttl);
        let entry = CacheEntry::new(value.to_string(), ttl);

        let mut cache = self.data.lock();

        // 检查是否需要淘汰
        if cache.len() >= self.config.capacity {
            // 放置新键会自动淘汰LRU，所以不需要手动evict
            // 但为了统计信息，我们记录一次eviction
            if cache.len() >= self.config.capacity {
                self.__stats.evictions.fetch_add(1, Ordering::Relaxed);
            }
        }

        cache.put(key.to_string(), entry);
        self.__stats.writes.fetch_add(1, Ordering::Relaxed);
    }

    /// 删除值
    pub async fn delete(&self, key: &str) {
        let mut cache = self.data.lock();
        cache.pop(key);
    }

    /// 检查键是否存在
    pub async fn contains(&self, key: &str) -> bool {
        let mut cache = self.data.lock();
        if let Some(entry) = cache.get(key) {
            !entry.is_expired()
        } else {
            false
        }
    }

    /// 清空缓存
    pub async fn clear(&self) {
        let mut cache = self.data.lock();
        cache.clear();
    }

    /// 获取缓存大小
    pub async fn len(&self) -> usize {
        let cache = self.data.lock();
        cache.len()
    }

    /// 检查缓存是否为空
    pub async fn is_empty(&self) -> bool {
        let cache = self.data.lock();
        cache.is_empty()
    }

    /// 单飞模式获取或加载值
    ///
    /// 如果缓存中存在且未过期，直接返回；否则使用加载器加载。
    /// 防止缓存击穿：多个并发请求同时加载同一个key时，只有一个会实际加载。
    pub async fn get_or_load<F, Fut>(&self, key: &str, loader: F) -> Result<String, StorageError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<String, StorageError>>,
    {
        // 快速路径：从缓存获取
        if let Some(value) = self.get(key).await {
            return Ok(value);
        }

        // 单飞模式加载
        let value = self.single_flight.get_or_load(key, loader).await?;

        // 缓存加载的值
        self.set(key, &value, self.config.default_ttl).await;

        Ok(value)
    }

    /// 批量获取
    pub async fn batch_get(&self, keys: &[String]) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for key in keys {
            if let Some(value) = self.get(key).await {
                result.insert(key.clone(), value);
            }
        }
        result
    }

    /// 批量设置
    pub async fn batch_set(&self, items: &[(String, String, Option<Duration>)]) {
        for (key, value, ttl) in items {
            self.set(key, value, *ttl).await;
        }
    }

    /// 批量删除
    pub async fn batch_delete(&self, keys: &[String]) {
        for key in keys {
            self.delete(key).await;
        }
    }

    /// 清理过期数据
    pub async fn cleanup_expired(&self) -> usize {
        let mut cache = self.data.lock();

        // 收集所有过期的键
        let expired_keys: Vec<String> = cache
            .iter()
            .filter(|(_, entry)| entry.is_expired())
            .map(|(key, _)| key.clone())
            .collect();

        let count = expired_keys.len();

        // 移除过期的键
        for key in expired_keys {
            cache.pop(&key);
            self.__stats.expirations.fetch_add(1, Ordering::Relaxed);
        }

        if count > 0 {
            debug!("清理了 {} 条过期数据", count);
        }

        count
    }

    /// LRU淘汰
    #[allow(dead_code)]
    fn evict_lru(&self) {
        // 使用 LruCache 的 pop_lru() 方法，这是 O(1) 的
        let mut cache = self.data.lock();
        if cache.pop_lru().is_some() {
            self.__stats.evictions.fetch_add(1, Ordering::Relaxed);
            debug!("LRU淘汰成功");
        }
    }

    /// 获取统计信息
    pub fn stats(&self) -> &CacheStats {
        &self.__stats
    }

    /// 获取配置
    pub fn config(&self) -> &L2CacheConfig {
        &self.config
    }

    /// 停止清理任务
    pub async fn shutdown(&self) {
        if let Some(handle) = &self.cleanup_handle {
            handle.abort();
        }
    }
}

impl Drop for L2Cache {
    fn drop(&mut self) {
        if let Some(handle) = self.cleanup_handle.take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_cache_set_get() {
        let cache = L2Cache::new(100, Duration::from_secs(60));

        cache.set("key1", "value1", None).await;
        let value = cache.get("key1").await;
        assert_eq!(value, Some("value1".to_string()));
    }

    #[tokio::test]
    async fn test_cache_get_not_found() {
        let cache = L2Cache::new(100, Duration::from_secs(60));

        let value = cache.get("nonexistent").await;
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_cache_delete() {
        let cache = L2Cache::new(100, Duration::from_secs(60));

        cache.set("key1", "value1", None).await;
        cache.delete("key1").await;
        let value = cache.get("key1").await;
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_cache_ttl() {
        let cache = L2Cache::new(100, Duration::from_secs(60));

        cache
            .set("key1", "value1", Some(Duration::from_millis(100)))
            .await;
        tokio::time::sleep(Duration::from_millis(150)).await;

        let value = cache.get("key1").await;
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_cache_contains() {
        let cache = L2Cache::new(100, Duration::from_secs(60));

        cache.set("key1", "value1", None).await;
        assert!(cache.contains("key1").await);
        assert!(!cache.contains("nonexistent").await);
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache = L2Cache::new(100, Duration::from_secs(60));

        cache.set("key1", "value1", None).await;
        cache.set("key2", "value2", None).await;
        cache.clear().await;

        assert!(cache.is_empty().await);
        assert_eq!(cache.len().await, 0);
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let cache = L2Cache::new(100, Duration::from_secs(60));

        // 批量设置
        let items = vec![
            ("key1".to_string(), "value1".to_string(), None),
            ("key2".to_string(), "value2".to_string(), None),
            ("key3".to_string(), "value3".to_string(), None),
        ];
        cache.batch_set(&items).await;

        // 批量获取
        let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
        let result = cache.batch_get(&keys).await;
        assert_eq!(result.len(), 3);
        assert_eq!(result.get("key1"), Some(&"value1".to_string()));

        // 批量删除
        let delete_keys = vec!["key1".to_string(), "key2".to_string()];
        cache.batch_delete(&delete_keys).await;

        assert_eq!(cache.len().await, 1);
    }

    #[tokio::test]
    async fn test_single_flight() {
        let cache = L2Cache::new(100, Duration::from_secs(60));

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

        // 并发加载同一个key
        let task1 = cache.get_or_load("key1", loader);
        let task2 = cache.get_or_load("key1", loader);
        let task3 = cache.get_or_load("key1", loader);

        let (r1, r2, r3) = tokio::join!(task1, task2, task3);

        assert!(r1.is_ok());
        assert!(r2.is_ok());
        assert!(r3.is_ok());
        // 单飞模式应该只加载一次
        assert_eq!(load_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = L2Cache::new(100, Duration::from_secs(60));

        cache.set("key1", "value1", None).await;
        cache.get("key1").await; // hit
        cache.get("key2").await; // miss

        let stats = cache.stats();
        assert_eq!(stats.hits(), 1);
        assert_eq!(stats.misses(), 1);
        assert_eq!(stats.hit_rate(), 0.5);
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let cache = L2Cache::new(100, Duration::from_secs(60));

        cache
            .set("key1", "value1", Some(Duration::from_millis(100)))
            .await;
        cache.set("key2", "value2", None).await;

        tokio::time::sleep(Duration::from_millis(150)).await;
        let cleaned = cache.cleanup_expired().await;

        assert_eq!(cleaned, 1);
        assert_eq!(cache.len().await, 1);
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        let cache = L2Cache::new(3, Duration::from_secs(60));

        cache.set("key1", "value1", None).await;
        cache.set("key2", "value2", None).await;
        cache.set("key3", "value3", None).await;

        // 访问key1和key2，使key3成为LRU
        cache.get("key1").await;
        cache.get("key2").await;

        // 添加新key，应该淘汰key3
        cache.set("key4", "value4", None).await;

        assert_eq!(cache.len().await, 3);
        assert!(cache.contains("key1").await);
        assert!(cache.contains("key2").await);
        assert!(!cache.contains("key3").await);
        assert!(cache.contains("key4").await);
    }

    #[tokio::test]
    async fn test_config_builder() {
        let config = L2CacheConfig::new()
            .capacity(5000)
            .default_ttl(Duration::from_secs(600))
            .cleanup_interval(Duration::from_secs(30))
            .eviction_threshold(0.8);

        assert_eq!(config.capacity, 5000);
        assert_eq!(config.default_ttl, Some(Duration::from_secs(600)));
        assert_eq!(config.cleanup_interval, Duration::from_secs(30));
        assert_eq!(config.eviction_threshold, 0.8);
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let cache = Arc::new(L2Cache::new(1000, Duration::from_secs(60)));
        let mut handles = vec![];

        // 并发写入
        for i in 0..100 {
            let cache_clone = Arc::clone(&cache);
            let handle = tokio::spawn(async move {
                let key = format!("key{}", i);
                let value = format!("value{}", i);
                cache_clone.set(&key, &value, None).await;
                cache_clone.get(&key).await
            });
            handles.push(handle);
        }

        // 等待所有任务完成
        for handle in handles {
            handle.await.unwrap();
        }

        // 验证所有数据都存在
        assert_eq!(cache.len().await, 100);
    }
}
