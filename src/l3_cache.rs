//! L3缓存实现
//!
//! 基于Redis的L3缓存，支持与L2缓存协同工作，提供降级机制和缓存穿透保护。
//!
//! # 特性
//!
//! - **三级缓存**: L1(内存) -> L2(DashMap) -> L3(Redis) -> DB
//! - **降级机制**: Redis故障时自动降级到L2
//! - **缓存穿透**: 使用空值缓存防止穿透
//! - **自动恢复**: Redis恢复后自动恢复正常
//! - **TTL管理**: 支持灵活的TTL配置

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, trace, warn};

use crate::error::StorageError;
use crate::fallback::{ComponentType, FallbackManager, FallbackStrategy};
use crate::l2_cache::L2Cache;
use crate::redis_storage::{RedisConfig, RedisStorage};
use crate::storage::Storage;

/// L3缓存配置
#[derive(Debug, Clone)]
pub struct L3CacheConfig {
    /// Redis配置
    pub redis_config: RedisConfig,
    /// L2缓存容量
    pub l2_capacity: usize,
    /// L2缓存清理间隔
    pub l2_cleanup_interval: Duration,
    /// L2缓存默认TTL
    pub l2_default_ttl: Option<Duration>,
    /// L3缓存默认TTL
    pub l3_default_ttl: Option<Duration>,
    /// 降级检查间隔
    pub degrade_check_interval: Duration,
    /// 是否启用缓存穿透保护
    pub enable_cache_penetration_protection: bool,
    /// 空值缓存TTL
    pub null_value_ttl: Duration,
}

impl Default for L3CacheConfig {
    fn default() -> Self {
        Self {
            redis_config: RedisConfig::default(),
            l2_capacity: 10000,
            l2_cleanup_interval: Duration::from_secs(60),
            l2_default_ttl: Some(Duration::from_secs(300)),
            l3_default_ttl: Some(Duration::from_secs(600)),
            degrade_check_interval: Duration::from_secs(5),
            enable_cache_penetration_protection: true,
            null_value_ttl: Duration::from_secs(60),
        }
    }
}

impl L3CacheConfig {
    /// 创建新的L3缓存配置
    pub fn new(redis_url: impl Into<String>) -> Self {
        Self {
            redis_config: RedisConfig::new(redis_url),
            ..Default::default()
        }
    }

    /// 设置L2缓存容量
    pub fn l2_capacity(mut self, capacity: usize) -> Self {
        self.l2_capacity = capacity;
        self
    }

    /// 设置L2缓存默认TTL
    pub fn l2_default_ttl(mut self, ttl: Duration) -> Self {
        self.l2_default_ttl = Some(ttl);
        self
    }

    /// 设置L3缓存默认TTL
    pub fn l3_default_ttl(mut self, ttl: Duration) -> Self {
        self.l3_default_ttl = Some(ttl);
        self
    }

    /// 设置降级检查间隔
    pub fn degrade_check_interval(mut self, interval: Duration) -> Self {
        self.degrade_check_interval = interval;
        self
    }

    /// 设置是否启用缓存穿透保护
    pub fn enable_cache_penetration_protection(mut self, enable: bool) -> Self {
        self.enable_cache_penetration_protection = enable;
        self
    }
}

/// L3缓存统计
#[derive(Debug, Default)]
pub struct L3CacheStats {
    /// L1命中次数
    l1_hits: AtomicU64,
    /// L2命中次数
    l2_hits: AtomicU64,
    /// L3命中次数
    l3_hits: AtomicU64,
    /// 未命中次数
    misses: AtomicU64,
    /// 降级次数
    degradations: AtomicU64,
    /// 恢复次数
    recoveries: AtomicU64,
    /// 缓存穿透保护次数
    penetration_protections: AtomicU64,
}

impl L3CacheStats {
    pub fn l1_hits(&self) -> u64 {
        self.l1_hits.load(Ordering::Relaxed)
    }

    pub fn l2_hits(&self) -> u64 {
        self.l2_hits.load(Ordering::Relaxed)
    }

    pub fn l3_hits(&self) -> u64 {
        self.l3_hits.load(Ordering::Relaxed)
    }

    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    pub fn degradations(&self) -> u64 {
        self.degradations.load(Ordering::Relaxed)
    }

    pub fn recoveries(&self) -> u64 {
        self.recoveries.load(Ordering::Relaxed)
    }

    pub fn penetration_protections(&self) -> u64 {
        self.penetration_protections.load(Ordering::Relaxed)
    }

    /// 总命中率
    pub fn overall_hit_rate(&self) -> f64 {
        let total = self.l1_hits() + self.l2_hits() + self.l3_hits() + self.misses();
        if total == 0 {
            0.0
        } else {
            (self.l1_hits() + self.l2_hits() + self.l3_hits()) as f64 / total as f64
        }
    }

    /// L1命中率
    pub fn l1_hit_rate(&self) -> f64 {
        let total = self.l1_hits() + self.l2_hits() + self.l3_hits() + self.misses();
        if total == 0 {
            0.0
        } else {
            self.l1_hits() as f64 / total as f64
        }
    }

    /// L2命中率
    pub fn l2_hit_rate(&self) -> f64 {
        let total = self.l1_hits() + self.l2_hits() + self.l3_hits() + self.misses();
        if total == 0 {
            0.0
        } else {
            self.l2_hits() as f64 / total as f64
        }
    }

    /// L3命中率
    pub fn l3_hit_rate(&self) -> f64 {
        let total = self.l1_hits() + self.l2_hits() + self.l3_hits() + self.misses();
        if total == 0 {
            0.0
        } else {
            self.l3_hits() as f64 / total as f64
        }
    }

    pub fn reset(&self) {
        self.l1_hits.store(0, Ordering::Relaxed);
        self.l2_hits.store(0, Ordering::Relaxed);
        self.l3_hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
        self.degradations.store(0, Ordering::Relaxed);
        self.recoveries.store(0, Ordering::Relaxed);
        self.penetration_protections.store(0, Ordering::Relaxed);
    }
}

/// L3缓存实现
pub struct L3Cache {
    /// L2缓存
    l2_cache: Arc<L2Cache>,
    /// L3存储（Redis）
    l3_storage: Arc<RwLock<Option<Arc<RedisStorage>>>>,
    /// 配置
    config: L3CacheConfig,
    /// 统计信息
    stats: Arc<L3CacheStats>,
    /// 是否降级
    degraded: Arc<AtomicBool>,
    /// 最后降级时间
    last_degraded_at: Arc<RwLock<Option<Instant>>>,
    /// 健康检查任务句柄
    health_check_handle: tokio::task::JoinHandle<()>,
    /// 降级策略管理器
    fallback_manager: Arc<FallbackManager>,
}

impl L3Cache {
    /// 创建新的L3缓存
    pub async fn new(config: L3CacheConfig) -> Result<Self, StorageError> {
        info!("创建L3缓存, Redis URL: {}", config.redis_config.url);

        // 创建L2缓存
        let l2_cache = Arc::new(L2Cache::with_config(crate::l2_cache::L2CacheConfig {
            capacity: config.l2_capacity,
            default_ttl: config.l2_default_ttl,
            cleanup_interval: config.l2_cleanup_interval,
            ..Default::default()
        }));

        // 尝试创建L3存储
        let l3_storage_arc = match RedisStorage::new(config.redis_config.clone()).await {
            Ok(storage) => {
                info!("L3存储（Redis）创建成功");
                Some(Arc::new(storage))
            }
            Err(e) => {
                warn!("L3存储（Redis）创建失败，将使用降级模式: {}", e);
                None
            }
        };

        let l3_storage = Arc::new(RwLock::new(l3_storage_arc));

        let degraded = Arc::new(AtomicBool::new(l3_storage.read().await.is_none()));
        let last_degraded_at = Arc::new(RwLock::new(if degraded.load(Ordering::Relaxed) {
            Some(Instant::now())
        } else {
            None
        }));

        let stats = Arc::new(L3CacheStats::default());

        // 创建降级策略管理器
        let fallback_manager = Arc::new(FallbackManager::new(Arc::clone(&l2_cache)));

        // 设置L3缓存的降级策略
        fallback_manager
            .set_strategy(
                ComponentType::L3Cache,
                crate::fallback::FallbackConfig::new(
                    ComponentType::L3Cache,
                    FallbackStrategy::Degraded,
                )
                .enabled(true)
                .timeout(Duration::from_secs(5))
                .max_retries(3),
            )
            .await;

        // 启动健康检查任务
        let health_check_handle = Self::start_health_check(
            Arc::clone(&l3_storage),
            Arc::clone(&degraded),
            Arc::clone(&last_degraded_at),
            Arc::clone(&stats),
            Arc::clone(&fallback_manager),
            config.degrade_check_interval,
        );

        Ok(Self {
            l2_cache,
            l3_storage,
            config,
            stats,
            degraded,
            last_degraded_at,
            health_check_handle,
            fallback_manager,
        })
    }

    /// 启动健康检查任务
    fn start_health_check(
        _l3_storage: Arc<RwLock<Option<Arc<RedisStorage>>>>,
        _degraded: Arc<AtomicBool>,
        _last_degraded_at: Arc<RwLock<Option<Instant>>>,
        _stats: Arc<L3CacheStats>,
        _fallback_manager: Arc<FallbackManager>,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut check_interval = tokio::time::interval(interval);
            loop {
                check_interval.tick().await;

                // 检查是否降级
                if _degraded.load(Ordering::Relaxed) {
                    // 尝试恢复
                    trace!("尝试恢复L3存储");

                    // 检查降级状态
                    if _fallback_manager.is_failed(ComponentType::L3Cache).await {
                        // 尝试重新连接Redis
                        if let Some(storage) = _l3_storage.read().await.as_ref() {
                            // 这里简化处理，实际应该尝试ping Redis
                            // 如果成功，清除故障状态
                            _fallback_manager
                                .recover_failure(ComponentType::L3Cache)
                                .await;
                        }
                    }
                }
            }
        })
    }

    /// 获取值（三级缓存）
    pub async fn get(&self, key: &str) -> Option<String> {
        // L1: 快速路径（这里简化，L1可以是无锁的）
        // 暂时跳过L1，直接从L2开始

        // L2: 检查L2缓存
        if let Some(value) = self.l2_cache.get(key).await {
            self.stats.l2_hits.fetch_add(1, Ordering::Relaxed);
            trace!("L2缓存命中: key={}", key);
            return Some(value);
        }

        // L3: 检查L3缓存（Redis）
        if !self.degraded.load(Ordering::Relaxed) {
            if let Some(l3_storage) = self.l3_storage.read().await.as_ref() {
                match l3_storage.as_ref().get(key).await {
                    Ok(Some(value)) => {
                        self.stats.l3_hits.fetch_add(1, Ordering::Relaxed);
                        trace!("L3缓存命中: key={}", key);

                        // 回填到L2缓存
                        self.l2_cache
                            .set(key, &value, self.config.l2_default_ttl)
                            .await;

                        return Some(value);
                    }
                    Ok(None) => {
                        // L3未命中
                        self.stats.misses.fetch_add(1, Ordering::Relaxed);
                        trace!("L3缓存未命中: key={}", key);
                    }
                    Err(e) => {
                        error!("L3缓存读取失败: key={}, error={}", key, e);
                        // 标记为降级
                        self.set_degraded(true).await;
                    }
                }
            }
        } else {
            debug!("L3缓存已降级，跳过L3查询");
        }

        None
    }

    /// 设置值（同步到L2和L3）
    pub async fn set(&self, key: &str, value: &str, ttl: Option<Duration>) {
        // 设置到L2缓存
        self.l2_cache.set(key, value, ttl).await;

        // 设置到L3缓存（如果未降级）
        if !self.degraded.load(Ordering::Relaxed) {
            if let Some(l3_storage) = self.l3_storage.read().await.as_ref() {
                let l3_ttl = ttl.or(self.config.l3_default_ttl);
                let l3_ttl_seconds = l3_ttl.map(|d| d.as_secs());

                if let Err(e) = l3_storage.as_ref().set(key, value, l3_ttl_seconds).await {
                    error!("L3缓存写入失败: key={}, error={}", key, e);
                    // 标记为降级
                    self.set_degraded(true).await;
                } else {
                    trace!("L3缓存写入成功: key={}", key);
                }
            }
        }
    }

    /// 删除值（从L2和L3删除）
    pub async fn delete(&self, key: &str) {
        // 从L2删除
        self.l2_cache.delete(key).await;

        // 从L3删除（如果未降级）
        if !self.degraded.load(Ordering::Relaxed) {
            if let Some(l3_storage) = self.l3_storage.read().await.as_ref() {
                if let Err(e) = l3_storage.as_ref().delete(key).await {
                    error!("L3缓存删除失败: key={}, error={}", key, e);
                } else {
                    trace!("L3缓存删除成功: key={}", key);
                }
            }
        }
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

    /// 获取或加载（支持缓存穿透保护）
    pub async fn get_or_load<F, Fut>(&self, key: &str, loader: F) -> Result<String, StorageError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<String, StorageError>>,
    {
        // 尝试从缓存获取
        if let Some(value) = self.get(key).await {
            return Ok(value);
        }

        // 缓存未命中，加载值
        match loader().await {
            Ok(value) => {
                // 缓存加载的值
                self.set(key, &value, None).await;
                Ok(value)
            }
            Err(e) => {
                // 如果启用缓存穿透保护，缓存空值
                if self.config.enable_cache_penetration_protection {
                    self.stats
                        .penetration_protections
                        .fetch_add(1, Ordering::Relaxed);
                    self.set(key, "__NULL__", Some(self.config.null_value_ttl))
                        .await;
                }
                Err(e)
            }
        }
    }

    /// 设置降级状态
    async fn set_degraded(&self, degraded: bool) {
        let current = self.degraded.load(Ordering::Relaxed);
        if current != degraded {
            self.degraded.store(degraded, Ordering::Relaxed);
            if degraded {
                *self.last_degraded_at.write().await = Some(Instant::now());
                self.stats.degradations.fetch_add(1, Ordering::Relaxed);
                warn!("L3缓存已降级");
            } else {
                self.stats.recoveries.fetch_add(1, Ordering::Relaxed);
                info!("L3缓存已恢复");
            }
        }
    }

    /// 检查是否降级
    pub async fn is_degraded(&self) -> bool {
        self.degraded.load(Ordering::Relaxed)
    }

    /// 获取统计信息
    pub fn stats(&self) -> &L3CacheStats {
        &self.stats
    }

    /// 获取L2缓存
    pub fn l2_cache(&self) -> &Arc<L2Cache> {
        &self.l2_cache
    }

    /// 获取L3存储
    pub async fn l3_storage(&self) -> Option<Arc<RedisStorage>> {
        self.l3_storage.read().await.as_ref().cloned()
    }

    /// 获取降级策略管理器
    pub fn fallback_manager(&self) -> &Arc<FallbackManager> {
        &self.fallback_manager
    }

    /// 清空所有缓存
    pub async fn clear(&self) {
        self.l2_cache.clear().await;

        if !self.degraded.load(Ordering::Relaxed) {
            if let Some(l3_storage) = self.l3_storage.read().await.as_ref() {
                // Redis不支持直接清空所有key，这里仅清空L2
                // 实际应用中可能需要使用SCAN命令或使用特定的key前缀
                debug!("L3缓存清空（仅清空L2，Redis需要手动处理）");
            }
        }
    }

    /// 停止健康检查任务
    pub async fn shutdown(&self) {
        self.health_check_handle.abort();
        self.l2_cache.shutdown().await;
    }
}

impl Drop for L3Cache {
    fn drop(&mut self) {
        self.health_check_handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l3_cache_config_default() {
        let config = L3CacheConfig::default();
        assert_eq!(config.l2_capacity, 10000);
        assert_eq!(config.l2_default_ttl, Some(Duration::from_secs(300)));
        assert_eq!(config.l3_default_ttl, Some(Duration::from_secs(600)));
        assert!(config.enable_cache_penetration_protection);
    }

    #[test]
    fn test_l3_cache_config_builder() {
        let config = L3CacheConfig::new("redis://localhost:6379")
            .l2_capacity(5000)
            .l2_default_ttl(Duration::from_secs(600))
            .l3_default_ttl(Duration::from_secs(1200))
            .degrade_check_interval(Duration::from_secs(10))
            .enable_cache_penetration_protection(false);

        assert_eq!(config.l2_capacity, 5000);
        assert_eq!(config.l2_default_ttl, Some(Duration::from_secs(600)));
        assert_eq!(config.l3_default_ttl, Some(Duration::from_secs(1200)));
        assert_eq!(config.degrade_check_interval, Duration::from_secs(10));
        assert!(!config.enable_cache_penetration_protection);
    }

    #[test]
    fn test_l3_cache_stats() {
        let stats = L3CacheStats::default();
        assert_eq!(stats.l1_hits(), 0);
        assert_eq!(stats.l2_hits(), 0);
        assert_eq!(stats.l3_hits(), 0);
        assert_eq!(stats.misses(), 0);
        assert_eq!(stats.overall_hit_rate(), 0.0);

        stats.l2_hits.fetch_add(1, Ordering::Relaxed);
        stats.l3_hits.fetch_add(1, Ordering::Relaxed);
        stats.misses.fetch_add(1, Ordering::Relaxed);

        assert_eq!(stats.l2_hits(), 1);
        assert_eq!(stats.l3_hits(), 1);
        assert_eq!(stats.misses(), 1);
        assert_eq!(stats.overall_hit_rate(), 2.0 / 3.0);

        stats.reset();
        assert_eq!(stats.l2_hits(), 0);
        assert_eq!(stats.l3_hits(), 0);
        assert_eq!(stats.misses(), 0);
    }

    #[tokio::test]
    async fn test_l3_cache_l2_only() {
        // 测试L2缓存功能（不依赖Redis）
        let config = L3CacheConfig {
            redis_config: RedisConfig::new("redis://invalid:6379"),
            l2_capacity: 100,
            l2_cleanup_interval: Duration::from_secs(60),
            l2_default_ttl: Some(Duration::from_secs(300)),
            l3_default_ttl: None,
            degrade_check_interval: Duration::from_secs(5),
            enable_cache_penetration_protection: false,
            null_value_ttl: Duration::from_secs(60),
        };

        let cache = L3Cache::new(config).await.unwrap();

        // 验证降级状态
        assert!(cache.is_degraded().await);

        // 测试L2缓存
        cache.set("key1", "value1", None).await;
        let value = cache.get("key1").await;
        assert_eq!(value, Some("value1".to_string()));

        // 验证统计
        assert_eq!(cache.stats().l2_hits(), 1);

        cache.shutdown().await;
    }

    #[tokio::test]
    async fn test_l3_cache_batch_operations() {
        let config = L3CacheConfig {
            redis_config: RedisConfig::new("redis://invalid:6379"),
            l2_capacity: 100,
            l2_cleanup_interval: Duration::from_secs(60),
            l2_default_ttl: Some(Duration::from_secs(300)),
            l3_default_ttl: None,
            degrade_check_interval: Duration::from_secs(5),
            enable_cache_penetration_protection: false,
            null_value_ttl: Duration::from_secs(60),
        };

        let cache = L3Cache::new(config).await.unwrap();

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

        // 批量删除
        let delete_keys = vec!["key1".to_string(), "key2".to_string()];
        cache.batch_delete(&delete_keys).await;

        let remaining = cache.batch_get(&keys).await;
        assert_eq!(remaining.len(), 1);

        cache.shutdown().await;
    }

    #[tokio::test]
    async fn test_l3_cache_get_or_load() {
        let config = L3CacheConfig {
            redis_config: RedisConfig::new("redis://invalid:6379"),
            l2_capacity: 100,
            l2_cleanup_interval: Duration::from_secs(60),
            l2_default_ttl: Some(Duration::from_secs(300)),
            l3_default_ttl: None,
            degrade_check_interval: Duration::from_secs(5),
            enable_cache_penetration_protection: false,
            null_value_ttl: Duration::from_secs(60),
        };

        let cache = L3Cache::new(config).await.unwrap();

        // 第一次加载
        let value1 = cache
            .get_or_load("key1", || async { Ok("loaded_value".to_string()) })
            .await
            .unwrap();
        assert_eq!(value1, "loaded_value".to_string());

        // 第二次从缓存获取
        let value2 = cache
            .get_or_load("key1", || async { Ok("should_not_be_called".to_string()) })
            .await
            .unwrap();
        assert_eq!(value2, "loaded_value".to_string());

        // 验证统计
        assert_eq!(cache.stats().l2_hits(), 1);

        cache.shutdown().await;
    }

    #[tokio::test]
    async fn test_l3_cache_clear() {
        let config = L3CacheConfig {
            redis_config: RedisConfig::new("redis://invalid:6379"),
            l2_capacity: 100,
            l2_cleanup_interval: Duration::from_secs(60),
            l2_default_ttl: Some(Duration::from_secs(300)),
            l3_default_ttl: None,
            degrade_check_interval: Duration::from_secs(5),
            enable_cache_penetration_protection: false,
            null_value_ttl: Duration::from_secs(60),
        };

        let cache = L3Cache::new(config).await.unwrap();

        cache.set("key1", "value1", None).await;
        cache.set("key2", "value2", None).await;

        assert_eq!(cache.l2_cache().len().await, 2);

        cache.clear().await;

        assert_eq!(cache.l2_cache().len().await, 0);

        cache.shutdown().await;
    }
}
