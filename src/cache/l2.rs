//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! L2缓存实现 - 基于 oxcache 库
//!
//! 使用 oxcache 的 TieredCache (L1 内存 + L2 Redis) 提供高性能分布式缓存。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ahash::AHashMap;
use tracing::{error, info, trace, warn};

use crate::cache::l1::CacheStats;
use crate::error::StorageError;
#[cfg(feature = "fallback")]
use crate::fallback::{ComponentType, FallbackManager, FallbackStrategy};
use oxcache::Cache;

// String already implements Cacheable via oxcache's blanket impl

/// L3缓存配置
#[derive(Debug, Clone)]
pub struct L2CacheConfig {
    /// Redis URL
    pub redis_url: String,
    /// L1缓存容量（内存缓存）
    pub l1_capacity: u64,
    /// L1缓存默认TTL
    pub l1_default_ttl: Option<Duration>,
    /// Redis默认TTL
    pub redis_default_ttl: Option<Duration>,
    /// 是否启用缓存穿透保护
    pub enable_cache_penetration_protection: bool,
    /// 空值缓存TTL
    pub null_value_ttl: Duration,
    /// 降级检查间隔
    pub degrade_check_interval: Duration,
}

impl Default for L2CacheConfig {
    fn default() -> Self {
        Self {
            redis_url: "redis://localhost:6379".to_string(),
            l1_capacity: 10000,
            l1_default_ttl: Some(Duration::from_secs(300)),
            redis_default_ttl: Some(Duration::from_secs(600)),
            enable_cache_penetration_protection: true,
            null_value_ttl: Duration::from_secs(60),
            degrade_check_interval: Duration::from_secs(5),
        }
    }
}

impl L2CacheConfig {
    pub fn new(redis_url: impl Into<String>) -> Self {
        Self {
            redis_url: redis_url.into(),
            ..Default::default()
        }
    }

    pub fn l1_capacity(mut self, capacity: u64) -> Self {
        self.l1_capacity = capacity;
        self
    }

    pub fn l1_default_ttl(mut self, ttl: Duration) -> Self {
        self.l1_default_ttl = Some(ttl);
        self
    }

    pub fn redis_default_ttl(mut self, ttl: Duration) -> Self {
        self.redis_default_ttl = Some(ttl);
        self
    }

    pub fn enable_cache_penetration_protection(mut self, enable: bool) -> Self {
        self.enable_cache_penetration_protection = enable;
        self
    }

    pub fn degrade_check_interval(mut self, interval: Duration) -> Self {
        self.degrade_check_interval = interval;
        self
    }
}

/// L3缓存统计
#[derive(Debug, Default)]
pub struct L2CacheStats {
    l1_hits: AtomicU64,
    l1_misses: AtomicU64,
    l2_hits: AtomicU64,
    l3_hits: AtomicU64,
    misses: AtomicU64,
    degradations: AtomicU64,
    recoveries: AtomicU64,
    penetration_protections: AtomicU64,
    writes: AtomicU64,
}

impl L2CacheStats {
    pub fn l1_hits(&self) -> u64 {
        self.l1_hits.load(Ordering::Relaxed)
    }

    pub fn l1_misses(&self) -> u64 {
        self.l1_misses.load(Ordering::Relaxed)
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

    pub fn writes(&self) -> u64 {
        self.writes.load(Ordering::Relaxed)
    }

    pub fn overall_hit_rate(&self) -> f64 {
        let total = self.l1_hits() + self.l2_hits() + self.l3_hits() + self.misses();
        if total == 0 {
            0.0
        } else {
            (self.l1_hits() + self.l2_hits() + self.l3_hits()) as f64 / total as f64
        }
    }

    pub fn l1_hit_rate(&self) -> f64 {
        let l1_total = self.l1_hits() + self.l1_misses();
        if l1_total == 0 {
            0.0
        } else {
            self.l1_hits() as f64 / l1_total as f64
        }
    }

    pub fn l2_hit_rate(&self) -> f64 {
        let l2_total = self.l2_hits() + self.l3_hits() + self.misses();
        if l2_total == 0 {
            0.0
        } else {
            (self.l2_hits() + self.l3_hits()) as f64 / l2_total as f64
        }
    }

    pub fn reset(&self) {
        self.l1_hits.store(0, Ordering::Relaxed);
        self.l1_misses.store(0, Ordering::Relaxed);
        self.l2_hits.store(0, Ordering::Relaxed);
        self.l3_hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
        self.degradations.store(0, Ordering::Relaxed);
        self.recoveries.store(0, Ordering::Relaxed);
        self.penetration_protections.store(0, Ordering::Relaxed);
        self.writes.store(0, Ordering::Relaxed);
    }
}

/// L3缓存实现 - 基于 oxcache TieredCache
pub struct L2Cache {
    /// oxcache TieredCache (L1 内存 + L2 Redis)
    cache: Arc<Cache<String, String>>,
    /// 配置
    config: L2CacheConfig,
    /// 统计信息
    stats: Arc<L2CacheStats>,
    /// 内部L1缓存统计
    #[allow(dead_code)]
    l1_stats: Arc<CacheStats>,
    /// 是否降级（使用纯L1内存模式）
    degraded: Arc<AtomicBool>,
    /// 最后降级时间
    last_degraded_at: Arc<std::sync::RwLock<Option<Instant>>>,
    /// 健康检查任务句柄
    health_check_handle: tokio::task::JoinHandle<()>,
    /// 降级策略管理器
    #[cfg(feature = "fallback")]
    fallback_manager: Arc<FallbackManager>,
}

impl L2Cache {
    /// 创建新的L3缓存
    pub async fn new(config: L2CacheConfig) -> Result<Self, StorageError> {
        info!("创建L3缓存 (oxcache), Redis URL: {}", config.redis_url);

        // 创建 oxcache 缓存
        // 注意：当前使用内存后端，后续需要实现 Redis 后端连接
        let cache: Cache<String, String> = Cache::builder()
            .capacity(config.l1_capacity)
            .ttl(config.redis_default_ttl.unwrap_or_default())
            .build()
            .await
            .map_err(|e| StorageError::InvalidConfig(format!("创建缓存失败: {}", e)))?;

        info!("oxcache TieredCache 创建成功");

        let degraded = Arc::new(AtomicBool::new(false));
        let last_degraded_at = Arc::new(std::sync::RwLock::new(None));
        let stats = Arc::new(L2CacheStats::default());
        let l1_stats = Arc::new(CacheStats::new());

        // 创建降级策略管理器
        #[cfg(feature = "fallback")]
        let fallback_manager = Arc::new(FallbackManager::new(None));

        // 设置L3缓存的降级策略
        #[cfg(feature = "fallback")]
        fallback_manager
            .set_strategy(
                ComponentType::L2Cache,
                crate::fallback::FallbackConfig::new(
                    ComponentType::L2Cache,
                    FallbackStrategy::Degraded,
                )
                .enabled(true)
                .timeout(Duration::from_secs(5))
                .max_retries(3),
            )
            .await;

        // 启动健康检查任务
        let cache_arc = Arc::new(cache);
        let health_check_handle = Self::start_health_check(
            Arc::clone(&cache_arc),
            Arc::clone(&degraded),
            Arc::clone(&last_degraded_at),
            Arc::clone(&stats),
            #[cfg(feature = "fallback")]
            Arc::clone(&fallback_manager),
            config.degrade_check_interval,
        );

        Ok(Self {
            cache: cache_arc,
            config,
            stats,
            l1_stats,
            degraded,
            last_degraded_at,
            health_check_handle,
            #[cfg(feature = "fallback")]
            fallback_manager,
        })
    }

    fn start_health_check(
        cache: Arc<Cache<String, String>>,
        degraded: Arc<AtomicBool>,
        last_degraded_at: Arc<std::sync::RwLock<Option<Instant>>>,
        stats: Arc<L2CacheStats>,
        #[cfg(feature = "fallback")] fallback_manager: Arc<FallbackManager>,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut check_interval = tokio::time::interval(interval);
            loop {
                check_interval.tick().await;

                if degraded.load(Ordering::Relaxed) {
                    trace!("尝试恢复L3缓存");

                    match cache.health_check().await {
                        Ok(true) => {
                            let current = degraded.load(Ordering::Relaxed);
                            if current {
                                degraded.store(false, Ordering::Relaxed);
                                *last_degraded_at.write().unwrap() = None;
                                stats.recoveries.fetch_add(1, Ordering::Relaxed);
                                info!("L3缓存已恢复");

                                #[cfg(feature = "fallback")]
                                fallback_manager
                                    .recover_failure(ComponentType::L2Cache)
                                    .await;
                            }
                        }
                        Ok(false) => {
                            trace!("L3缓存健康检查返回false");
                        }
                        Err(e) => {
                            trace!("L3缓存仍处于不健康状态: {}", e);
                        }
                    }
                }
            }
        })
    }

    /// 获取值
    pub async fn get(&self, key: &str) -> Option<String> {
        // 检查是否降级
        if self.degraded.load(Ordering::Relaxed) {
            self.stats.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        }

        // 使用 oxcache tiered cache 获取值
        match self.cache.get(&key.to_string()).await {
            Ok(Some(value)) => {
                // 判断是 L1 还是 L2 命中（通过 stats 判断）
                // oxcache 的 tiered cache 会自动处理 L1/L2 层级
                self.stats.l1_hits.fetch_add(1, Ordering::Relaxed);
                trace!("L1缓存命中: key={}", key);
                Some(value)
            }
            Ok(None) => {
                // 检查是否是降级状态
                match self.cache.health_check().await {
                    Ok(true) => {
                        self.stats.misses.fetch_add(1, Ordering::Relaxed);
                        None
                    }
                    Ok(false) => {
                        // L2 不可用，降级
                        self.set_degraded(true).await;
                        self.stats.misses.fetch_add(1, Ordering::Relaxed);
                        None
                    }
                    Err(e) => {
                        error!("L3缓存健康检查失败: {}", e);
                        self.set_degraded(true).await;
                        self.stats.misses.fetch_add(1, Ordering::Relaxed);
                        None
                    }
                }
            }
            Err(e) => {
                error!("L3缓存读取失败: key={}, error={}", key, e);
                self.set_degraded(true).await;
                #[cfg(feature = "fallback")]
                self.fallback_manager
                    .record_failure(ComponentType::L2Cache, &e.to_string())
                    .await;
                self.stats.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// 设置值
    pub async fn set(&self, key: &str, value: &str, ttl: Option<Duration>) {
        self.stats.writes.fetch_add(1, Ordering::Relaxed);

        // 如果降级，不再尝试写入 Redis
        if self.degraded.load(Ordering::Relaxed) {
            return;
        }

        // 使用 oxcache tiered cache 设置值
        if let Err(e) = self
            .cache
            .set_with_ttl(&key.to_string(), &value.to_string(), ttl)
            .await
        {
            error!("L3缓存写入失败: key={}, error={}", key, e);
            self.set_degraded(true).await;
        }
    }

    /// 删除值
    pub async fn delete(&self, key: &str) {
        if self.degraded.load(Ordering::Relaxed) {
            return;
        }

        if let Err(e) = self.cache.delete(&key.to_string()).await {
            error!("L3缓存删除失败: key={}, error={}", key, e);
            self.set_degraded(true).await;
        }
    }

    /// 批量获取
    pub async fn batch_get(&self, keys: &[String]) -> AHashMap<String, String> {
        if self.degraded.load(Ordering::Relaxed) {
            for _ in keys {
                self.stats.misses.fetch_add(1, Ordering::Relaxed);
            }
            return AHashMap::new();
        }

        match self.cache.get_many(keys.iter()).await {
            Ok(result) => {
                for key in keys {
                    if result.contains_key(key) {
                        self.stats.l1_hits.fetch_add(1, Ordering::Relaxed);
                    } else {
                        self.stats.misses.fetch_add(1, Ordering::Relaxed);
                    }
                }
                result.into_iter().collect()
            }
            Err(e) => {
                error!("L3缓存批量读取失败: error={}", e);
                self.set_degraded(true).await;
                for _ in keys {
                    self.stats.misses.fetch_add(1, Ordering::Relaxed);
                }
                AHashMap::new()
            }
        }
    }

    /// 批量设置
    pub async fn batch_set(&self, items: &[(String, String, Option<Duration>)]) {
        self.stats
            .writes
            .fetch_add(items.len() as u64, Ordering::Relaxed);

        if self.degraded.load(Ordering::Relaxed) {
            return;
        }

        // 使用 oxcache 的 set_many
        for (key, value, _) in items {
            if let Err(e) = self.cache.set(key, value).await {
                error!("L3缓存批量写入失败: key={}, error={}", key, e);
                self.set_degraded(true).await;
                return;
            }
        }
    }

    /// 批量删除
    pub async fn batch_delete(&self, keys: &[String]) {
        if self.degraded.load(Ordering::Relaxed) {
            return;
        }

        if let Err(e) = self.cache.delete_many(keys.iter()).await {
            error!("L3缓存批量删除失败: error={}", e);
            self.set_degraded(true).await;
        }
    }

    /// 获取或加载
    pub async fn get_or_load<F, Fut>(&self, key: &str, loader: F) -> Result<String, StorageError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<String, StorageError>>,
    {
        if let Some(value) = self.get(key).await {
            return Ok(value);
        }

        match loader().await {
            Ok(value) => {
                self.set(key, &value, None).await;
                Ok(value)
            }
            Err(e) => {
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

    async fn set_degraded(&self, degraded: bool) {
        let current = self.degraded.load(Ordering::Relaxed);
        if current != degraded {
            self.degraded.store(degraded, Ordering::Relaxed);
            if degraded {
                *self.last_degraded_at.write().unwrap() = Some(Instant::now());
                self.stats.degradations.fetch_add(1, Ordering::Relaxed);
                warn!("L3缓存已降级");
            } else {
                self.stats.recoveries.fetch_add(1, Ordering::Relaxed);
                info!("L3缓存已恢复");
            }
        }
    }

    pub async fn is_degraded(&self) -> bool {
        self.degraded.load(Ordering::Relaxed)
    }

    pub fn stats(&self) -> &L2CacheStats {
        &self.stats
    }

    #[cfg(feature = "fallback")]
    pub fn fallback_manager(&self) -> &Arc<FallbackManager> {
        &self.fallback_manager
    }

    pub async fn clear(&self) {
        if let Err(e) = self.cache.clear().await {
            error!("L3缓存清除失败: {}", e);
        }
    }

    pub async fn shutdown(&self) {
        self.health_check_handle.abort();
        if let Err(e) = self.cache.shutdown().await {
            error!("L3缓存关闭失败: {}", e);
        }
    }
}

impl Drop for L2Cache {
    fn drop(&mut self) {
        self.health_check_handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_l2_cache_config_default() {
        let config = L2CacheConfig::default();
        assert_eq!(config.l1_capacity, 10000);
        assert_eq!(config.redis_default_ttl, Some(Duration::from_secs(600)));
        assert!(config.enable_cache_penetration_protection);
    }

    #[tokio::test]
    async fn test_l2_cache_stats() {
        let stats = L2CacheStats::default();
        assert_eq!(stats.l1_hits(), 0);
        assert_eq!(stats.l2_hits(), 0);
        assert_eq!(stats.misses(), 0);

        stats.l1_hits.fetch_add(1, Ordering::Relaxed);
        stats.l2_hits.fetch_add(1, Ordering::Relaxed);
        stats.misses.fetch_add(1, Ordering::Relaxed);

        assert_eq!(stats.l1_hits(), 1);
        assert_eq!(stats.l2_hits(), 1);
        assert_eq!(stats.misses(), 1);
        assert!((stats.overall_hit_rate() - 2.0 / 3.0).abs() < 0.01);

        stats.reset();
        assert_eq!(stats.l1_hits(), 0);
    }
}
