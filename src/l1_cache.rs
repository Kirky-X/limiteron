//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! L1 本地缓存模块
//!
//! 用于缓存热点限流结果，减少存储层访问。
//! 采用 DashMap 实现无锁并发缓存，支持 TTL 过期策略。

use crate::error::{BanInfo, Decision};
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// 缓存条目
///
/// 存储缓存值及其过期时间。
#[derive(Debug, Clone)]
struct CacheEntry<T> {
    /// 缓存值
    value: T,
    /// 过期时间点
    expires_at: Instant,
}

impl<T> CacheEntry<T> {
    /// 创建新的缓存条目
    fn new(value: T, ttl: Duration) -> Self {
        Self {
            value,
            expires_at: Instant::now() + ttl,
        }
    }

    /// 检查是否已过期
    fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}

/// 可缓存的决策结果
///
/// 用于 L1 缓存的决策结果类型，支持序列化和反序列化。
/// 与 Decision 类型不同，该类型专门用于缓存场景。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CacheableDecision {
    /// 决策类型：allowed, rejected, banned
    pub decision_type: String,
    /// 决策原因（可选）
    pub reason: Option<String>,
    /// 封禁信息（仅当 decision_type 为 banned 时）
    pub ban_info: Option<CacheableBanInfo>,
}

/// 可缓存的封禁信息
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CacheableBanInfo {
    /// 封禁原因
    pub reason: String,
    /// 封禁到期时间（ISO 8601 格式）
    pub banned_until: String,
    /// 封禁次数
    pub ban_times: u32,
}

impl CacheableDecision {
    /// 创建允许决策
    pub fn allowed() -> Self {
        Self {
            decision_type: "allowed".to_string(),
            reason: None,
            ban_info: None,
        }
    }

    /// 创建拒绝决策
    pub fn rejected(reason: impl Into<String>) -> Self {
        Self {
            decision_type: "rejected".to_string(),
            reason: Some(reason.into()),
            ban_info: None,
        }
    }

    /// 创建封禁决策
    pub fn banned(ban_info: BanInfo) -> Self {
        Self {
            decision_type: "banned".to_string(),
            reason: Some(ban_info.reason.clone()),
            ban_info: Some(CacheableBanInfo {
                reason: ban_info.reason,
                banned_until: ban_info.banned_until.to_rfc3339(),
                ban_times: ban_info.ban_times,
            }),
        }
    }

    /// 从 Decision 转换
    pub fn from_decision(decision: &Decision) -> Self {
        match decision {
            Decision::Allowed(reason) => Self {
                decision_type: "allowed".to_string(),
                reason: reason.clone(),
                ban_info: None,
            },
            Decision::Rejected(reason) => Self {
                decision_type: "rejected".to_string(),
                reason: Some(reason.clone()),
                ban_info: None,
            },
            Decision::Banned(info) => Self::banned(info.clone()),
        }
    }

    /// 转换为 Decision
    pub fn to_decision(&self) -> Decision {
        match self.decision_type.as_str() {
            "allowed" => Decision::Allowed(self.reason.clone()),
            "rejected" => Decision::Rejected(self.reason.clone().unwrap_or_default()),
            "banned" => {
                if let Some(info) = &self.ban_info {
                    Decision::Banned(BanInfo {
                        reason: info.reason.clone(),
                        banned_until: chrono::DateTime::parse_from_rfc3339(&info.banned_until)
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .unwrap_or_else(|_| chrono::Utc::now()),
                        ban_times: info.ban_times,
                    })
                } else {
                    Decision::Banned(BanInfo {
                        reason: "unknown".to_string(),
                        banned_until: chrono::Utc::now(),
                        ban_times: 0,
                    })
                }
            }
            _ => Decision::Allowed(None),
        }
    }

    /// 检查是否为允许决策
    pub fn is_allowed(&self) -> bool {
        self.decision_type == "allowed"
    }

    /// 检查是否为拒绝决策
    pub fn is_rejected(&self) -> bool {
        self.decision_type == "rejected"
    }

    /// 检查是否为封禁决策
    pub fn is_banned(&self) -> bool {
        self.decision_type == "banned"
    }
}

/// 限流缓存键生成器
///
/// 用于生成限流场景下的缓存键。
pub struct RateLimitCacheKey;

impl RateLimitCacheKey {
    /// 生成用户限流缓存键
    pub fn user_rate_limit(user_id: &str, rule_id: &str) -> String {
        format!("rl:user:{}:{}", user_id, rule_id)
    }

    /// 生成 IP 限流缓存键
    pub fn ip_rate_limit(ip: &str, rule_id: &str) -> String {
        format!("rl:ip:{}:{}", ip, rule_id)
    }

    /// 生成 API Key 限流缓存键
    pub fn api_key_rate_limit(api_key: &str, rule_id: &str) -> String {
        format!("rl:apikey:{}:{}", api_key, rule_id)
    }

    /// 生成通用限流缓存键
    pub fn generic(identifier: &str, rule_id: &str) -> String {
        format!("rl:generic:{}:{}", identifier, rule_id)
    }

    /// 生成封禁检查缓存键
    pub fn ban_check(identifier: &str) -> String {
        format!("ban:{}", identifier)
    }
}

/// L1 缓存统计信息
///
/// 记录缓存的命中、未命中、驱逐等统计信息。
#[derive(Debug, Clone, Default)]
pub struct L1CacheStats {
    /// 总查询次数
    pub total_lookups: u64,
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 过期驱逐次数
    pub expired_evictions: u64,
    /// 容量驱逐次数
    pub capacity_evictions: u64,
    /// 当前缓存大小
    pub current_size: usize,
    /// 最大缓存大小
    pub max_size: usize,
}

impl L1CacheStats {
    /// 计算缓存命中率
    ///
    /// # 返回
    ///
    /// 返回命中率百分比（0.0 - 100.0）
    pub fn hit_rate(&self) -> f64 {
        if self.total_lookups == 0 {
            return 0.0;
        }
        (self.hits as f64 / self.total_lookups as f64) * 100.0
    }

    /// 计算未命中率
    ///
    /// # 返回
    ///
    /// 返回未命中率百分比（0.0 - 100.0）
    pub fn miss_rate(&self) -> f64 {
        100.0 - self.hit_rate()
    }
}

/// L1 本地缓存配置
///
/// 用于配置 L1 缓存的行为参数。
#[derive(Debug, Clone)]
pub struct L1CacheConfig {
    /// 默认 TTL（生存时间）
    pub default_ttl: Duration,
    /// 最大缓存条目数
    pub max_size: usize,
    /// 是否启用统计
    pub enable_stats: bool,
}

impl Default for L1CacheConfig {
    fn default() -> Self {
        Self {
            default_ttl: Duration::from_secs(60),
            max_size: 10_000,
            enable_stats: true,
        }
    }
}

impl L1CacheConfig {
    /// 创建新的配置
    pub fn new(default_ttl: Duration, max_size: usize) -> Self {
        Self {
            default_ttl,
            max_size,
            enable_stats: true,
        }
    }

    /// 设置默认 TTL
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = ttl;
        self
    }

    /// 设置最大缓存大小
    pub fn with_max_size(mut self, max_size: usize) -> Self {
        self.max_size = max_size;
        self
    }

    /// 设置是否启用统计
    pub fn with_stats(mut self, enable: bool) -> Self {
        self.enable_stats = enable;
        self
    }
}

/// L1 本地缓存
///
/// 使用 DashMap 实现的高性能并发缓存，用于缓存热点限流结果。
/// 支持 TTL 过期策略和容量限制。
///
/// # 特性
///
/// - 无锁并发读写（基于 DashMap）
/// - TTL 过期策略
/// - 容量限制与 LRU 风格驱逐
/// - 命中率统计
///
/// # 示例
///
/// ```rust
/// use limiteron::l1_cache::{L1Cache, L1CacheConfig};
/// use std::time::Duration;
///
/// let config = L1CacheConfig::new(Duration::from_secs(60), 1000);
/// let cache: L1Cache<String> = L1Cache::with_config(config);
///
/// // 设置缓存
/// cache.set("key".to_string(), "value".to_string());
///
/// // 获取缓存
/// if let Some(value) = cache.get(&"key".to_string()) {
///     println!("缓存命中: {}", value);
/// }
///
/// // 获取统计信息
/// let stats = cache.stats();
/// println!("命中率: {:.2}%", stats.hit_rate());
/// ```
pub struct L1Cache<T> {
    /// 缓存存储
    cache: DashMap<String, CacheEntry<T>>,
    /// 配置
    config: L1CacheConfig,
    /// 统计：总查询次数
    total_lookups: AtomicU64,
    /// 统计：命中次数
    hits: AtomicU64,
    /// 统计：过期驱逐次数
    expired_evictions: AtomicU64,
    /// 统计：容量驱逐次数
    capacity_evictions: AtomicU64,
}

impl<T: Clone + Send + Sync + 'static> L1Cache<T> {
    /// 使用默认配置创建 L1 缓存
    pub fn new() -> Self {
        Self::with_config(L1CacheConfig::default())
    }

    /// 使用指定配置创建 L1 缓存
    ///
    /// # 参数
    ///
    /// - `config`: 缓存配置
    pub fn with_config(config: L1CacheConfig) -> Self {
        Self {
            cache: DashMap::with_capacity(config.max_size),
            config,
            total_lookups: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            expired_evictions: AtomicU64::new(0),
            capacity_evictions: AtomicU64::new(0),
        }
    }

    /// 创建指定 TTL 和最大大小的缓存
    ///
    /// # 参数
    ///
    /// - `default_ttl`: 默认生存时间
    /// - `max_size`: 最大缓存条目数
    pub fn with_ttl_and_size(default_ttl: Duration, max_size: usize) -> Self {
        Self::with_config(L1CacheConfig::new(default_ttl, max_size))
    }

    /// 获取缓存值
    ///
    /// 如果缓存不存在或已过期，返回 None。
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    ///
    /// # 返回
    ///
    /// 返回缓存的值（如果存在且未过期）
    pub fn get(&self, key: &str) -> Option<T> {
        if self.config.enable_stats {
            self.total_lookups.fetch_add(1, Ordering::Relaxed);
        }

        let result = self.cache.get(key).and_then(|entry| {
            if entry.is_expired() {
                // 过期条目，返回 None
                None
            } else {
                Some(entry.value.clone())
            }
        });

        if self.config.enable_stats && result.is_some() {
            self.hits.fetch_add(1, Ordering::Relaxed);
        }

        result
    }

    /// 设置缓存值
    ///
    /// 使用默认 TTL 设置缓存值。如果缓存已满，会驱逐部分条目。
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    /// - `value`: 缓存值
    pub fn set(&self, key: String, value: T) {
        self.set_with_ttl(key, value, self.config.default_ttl)
    }

    /// 设置缓存值（带自定义 TTL）
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    /// - `value`: 缓存值
    /// - `ttl`: 生存时间
    pub fn set_with_ttl(&self, key: String, value: T, ttl: Duration) {
        // 检查容量限制
        if self.cache.len() >= self.config.max_size {
            self.evict_expired();

            // 如果仍然超过容量，进行容量驱逐
            if self.cache.len() >= self.config.max_size {
                self.evict_for_capacity();
            }
        }

        let entry = CacheEntry::new(value, ttl);
        self.cache.insert(key, entry);
    }

    /// 使缓存失效
    ///
    /// 移除指定键的缓存条目。
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    pub fn invalidate(&self, key: &str) {
        self.cache.remove(key);
    }

    /// 使匹配前缀的所有缓存失效
    ///
    /// # 参数
    ///
    /// - `prefix`: 键前缀
    pub fn invalidate_by_prefix(&self, prefix: &str) {
        self.cache.retain(|k, _| !k.starts_with(prefix));
    }

    /// 使包含指定字符串的所有缓存失效
    ///
    /// # 参数
    ///
    /// - `pattern`: 要匹配的字符串模式
    pub fn invalidate_containing(&self, pattern: &str) {
        self.cache.retain(|k, _| !k.contains(pattern));
    }

    /// 清空所有缓存
    pub fn clear(&self) {
        self.cache.clear();
    }

    /// 清理过期条目
    ///
    /// 遍历缓存并移除所有过期的条目。
    ///
    /// # 返回
    ///
    /// 返回清理的条目数
    pub fn evict_expired(&self) -> usize {
        let mut evicted = 0;
        self.cache.retain(|_, v| {
            let should_retain = !v.is_expired();
            if !should_retain {
                evicted += 1;
            }
            should_retain
        });

        if evicted > 0 && self.config.enable_stats {
            self.expired_evictions
                .fetch_add(evicted as u64, Ordering::Relaxed);
        }

        evicted
    }

    /// 为容量进行驱逐
    ///
    /// 当缓存达到容量限制时，驱逐部分条目以腾出空间。
    /// 采用简单的随机驱逐策略（DashMap 的 retain 实现）。
    fn evict_for_capacity(&self) {
        let target_size = (self.config.max_size as f64 * 0.8) as usize;
        let current_size = self.cache.len();

        if current_size <= target_size {
            return;
        }

        let to_evict = current_size - target_size;
        let mut evicted = 0;

        // 首先驱逐过期条目
        evicted += self.evict_expired();

        // 如果还需要驱逐，按访问顺序驱逐（简化实现：随机驱逐）
        if evicted < to_evict {
            let remaining_to_evict = to_evict - evicted;
            let mut count = 0;

            self.cache.retain(|_, _| {
                if count < remaining_to_evict {
                    count += 1;
                    false // 移除
                } else {
                    true // 保留
                }
            });

            evicted += count;
        }

        if evicted > 0 && self.config.enable_stats {
            self.capacity_evictions
                .fetch_add(evicted as u64, Ordering::Relaxed);
        }
    }

    /// 获取当前缓存大小
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// 检查缓存是否为空
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// 获取缓存统计信息
    pub fn stats(&self) -> L1CacheStats {
        L1CacheStats {
            total_lookups: self.total_lookups.load(Ordering::Relaxed),
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.total_lookups.load(Ordering::Relaxed) - self.hits.load(Ordering::Relaxed),
            expired_evictions: self.expired_evictions.load(Ordering::Relaxed),
            capacity_evictions: self.capacity_evictions.load(Ordering::Relaxed),
            current_size: self.cache.len(),
            max_size: self.config.max_size,
        }
    }

    /// 重置统计信息
    pub fn reset_stats(&self) {
        self.total_lookups.store(0, Ordering::Relaxed);
        self.hits.store(0, Ordering::Relaxed);
        self.expired_evictions.store(0, Ordering::Relaxed);
        self.capacity_evictions.store(0, Ordering::Relaxed);
    }

    /// 检查键是否存在且未过期
    pub fn contains(&self, key: &str) -> bool {
        self.cache
            .get(key)
            .map(|e| !e.is_expired())
            .unwrap_or(false)
    }

    /// 获取键的剩余 TTL
    ///
    /// # 返回
    ///
    /// - `Some(Duration)`: 剩余时间
    /// - `None`: 键不存在或已过期
    pub fn ttl(&self, key: &str) -> Option<Duration> {
        self.cache.get(key).and_then(|entry| {
            if entry.is_expired() {
                None
            } else {
                Some(entry.expires_at.duration_since(Instant::now()))
            }
        })
    }
}

impl<T: Clone + Send + Sync + 'static> Default for L1Cache<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_cache_basic_operations() {
        let cache: L1Cache<String> = L1Cache::new();

        // 测试设置和获取
        cache.set("key1".to_string(), "value1".to_string());
        assert_eq!(cache.get("key1"), Some("value1".to_string()));

        // 测试不存在的键
        assert_eq!(cache.get("key2"), None);

        // 测试失效
        cache.invalidate("key1");
        assert_eq!(cache.get("key1"), None);
    }

    #[test]
    fn test_cache_ttl() {
        let cache: L1Cache<String> = L1Cache::with_ttl_and_size(Duration::from_millis(50), 100);

        cache.set("key1".to_string(), "value1".to_string());
        assert_eq!(cache.get("key1"), Some("value1".to_string()));

        // 等待过期
        thread::sleep(Duration::from_millis(60));
        assert_eq!(cache.get("key1"), None);
    }

    #[test]
    fn test_cache_custom_ttl() {
        let cache: L1Cache<String> = L1Cache::new();

        cache.set_with_ttl(
            "key1".to_string(),
            "value1".to_string(),
            Duration::from_millis(50),
        );
        assert_eq!(cache.get("key1"), Some("value1".to_string()));

        thread::sleep(Duration::from_millis(60));
        assert_eq!(cache.get("key1"), None);
    }

    #[test]
    fn test_cache_stats() {
        let cache: L1Cache<String> = L1Cache::new();

        cache.set("key1".to_string(), "value1".to_string());

        // 命中
        cache.get("key1");
        cache.get("key1");

        // 未命中
        cache.get("key2");

        let stats = cache.stats();
        assert_eq!(stats.total_lookups, 3);
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate() - 66.67).abs() < 0.1);
    }

    #[test]
    fn test_cache_capacity_eviction() {
        let cache: L1Cache<String> = L1Cache::with_ttl_and_size(Duration::from_secs(60), 5);

        // 填充缓存
        for i in 0..5 {
            cache.set(format!("key{}", i), format!("value{}", i));
        }

        assert_eq!(cache.len(), 5);

        // 添加第6个条目，触发驱逐
        cache.set("key5".to_string(), "value5".to_string());

        // 缓存大小应该被限制
        assert!(cache.len() <= 5);
    }

    #[test]
    fn test_cache_evict_expired() {
        let cache: L1Cache<String> = L1Cache::with_ttl_and_size(Duration::from_millis(50), 100);

        cache.set("key1".to_string(), "value1".to_string());
        cache.set_with_ttl(
            "key2".to_string(),
            "value2".to_string(),
            Duration::from_secs(60),
        );

        thread::sleep(Duration::from_millis(60));

        let evicted = cache.evict_expired();
        assert_eq!(evicted, 1);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get("key2"), Some("value2".to_string()));
    }

    #[test]
    fn test_cache_invalidate_by_prefix() {
        let cache: L1Cache<String> = L1Cache::new();

        cache.set("user:1".to_string(), "value1".to_string());
        cache.set("user:2".to_string(), "value2".to_string());
        cache.set("ip:1".to_string(), "value3".to_string());

        cache.invalidate_by_prefix("user:");

        assert_eq!(cache.get("user:1"), None);
        assert_eq!(cache.get("user:2"), None);
        assert_eq!(cache.get("ip:1"), Some("value3".to_string()));
    }

    #[test]
    fn test_cache_concurrent_access() {
        let cache: Arc<L1Cache<String>> = Arc::new(L1Cache::new());
        let mut handles = vec![];

        // 并发写入
        for i in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for j in 0..100 {
                    cache_clone.set(format!("key-{}-{}", i, j), format!("value-{}-{}", i, j));
                }
            });
            handles.push(handle);
        }

        // 并发读取
        for i in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for j in 0..100 {
                    cache_clone.get(&format!("key-{}-{}", i, j));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // 验证统计
        let stats = cache.stats();
        assert!(stats.total_lookups > 0);
    }

    #[test]
    fn test_cache_ttl_method() {
        let cache: L1Cache<String> = L1Cache::with_ttl_and_size(Duration::from_secs(60), 100);

        cache.set("key1".to_string(), "value1".to_string());

        let ttl = cache.ttl("key1").unwrap();
        assert!(ttl <= Duration::from_secs(60));
        assert!(ttl > Duration::from_secs(58));

        // 不存在的键
        assert!(cache.ttl("key2").is_none());
    }

    #[test]
    fn test_cache_contains() {
        let cache: L1Cache<String> = L1Cache::with_ttl_and_size(Duration::from_millis(50), 100);

        cache.set("key1".to_string(), "value1".to_string());
        assert!(cache.contains("key1"));
        assert!(!cache.contains("key2"));

        thread::sleep(Duration::from_millis(60));
        assert!(!cache.contains("key1"));
    }

    #[test]
    fn test_cache_reset_stats() {
        let cache: L1Cache<String> = L1Cache::new();

        cache.set("key1".to_string(), "value1".to_string());
        cache.get("key1");
        cache.get("key1");

        let stats = cache.stats();
        assert_eq!(stats.hits, 2);

        cache.reset_stats();
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.total_lookups, 0);
    }

    #[test]
    fn test_cache_config_builder() {
        let config = L1CacheConfig::default()
            .with_ttl(Duration::from_secs(120))
            .with_max_size(5000)
            .with_stats(false);

        assert_eq!(config.default_ttl, Duration::from_secs(120));
        assert_eq!(config.max_size, 5000);
        assert!(!config.enable_stats);
    }

    #[test]
    fn test_cache_stats_disabled() {
        let config = L1CacheConfig::default().with_stats(false);
        let cache: L1Cache<String> = L1Cache::with_config(config);

        cache.set("key1".to_string(), "value1".to_string());
        cache.get("key1");
        cache.get("key2");

        let stats = cache.stats();
        // 统计被禁用，计数应该为 0
        assert_eq!(stats.total_lookups, 0);
        assert_eq!(stats.hits, 0);
    }

    // ==================== 缓存命中率测试 ====================

    /// 模拟热点场景下的缓存命中率测试
    ///
    /// 测试场景：80% 的请求访问 20% 的热点键
    #[test]
    fn test_cache_hit_rate_hotspot() {
        let cache: L1Cache<String> = L1Cache::with_ttl_and_size(Duration::from_secs(60), 1000);

        // 模拟热点键（20% 的键）
        let hot_keys: Vec<String> = (0..20).map(|i| format!("hot_key_{}", i)).collect();

        // 模拟冷键（80% 的键）
        let cold_keys: Vec<String> = (0..80).map(|i| format!("cold_key_{}", i)).collect();

        // 初始化所有键
        for key in hot_keys.iter().chain(cold_keys.iter()) {
            cache.set(key.clone(), format!("value_{}", key));
        }

        // 模拟 1000 次访问：80% 访问热点键，20% 访问冷键
        for _ in 0..800 {
            // 热点键访问
            for key in &hot_keys {
                cache.get(key);
            }
        }

        for _ in 0..200 {
            // 冷键访问
            for key in &cold_keys {
                cache.get(key);
            }
        }

        let stats = cache.stats();

        // 验证命中率
        // 热点键访问次数：800 * 20 = 16000 次（全部命中）
        // 冷键访问次数：200 * 80 = 16000 次（全部命中）
        // 总访问次数：32000 次
        // 由于所有键都已缓存，命中率应该接近 100%
        assert!(
            stats.hit_rate() > 99.0,
            "命中率应该接近 100%，实际为 {:.2}%",
            stats.hit_rate()
        );
        assert_eq!(stats.total_lookups, 32000);
    }

    /// 测试缓存未命中场景
    #[test]
    fn test_cache_miss_rate() {
        let cache: L1Cache<String> = L1Cache::new();

        // 只设置少量键
        cache.set("key1".to_string(), "value1".to_string());
        cache.set("key2".to_string(), "value2".to_string());

        // 访问已存在的键（命中）
        for _ in 0..10 {
            cache.get("key1");
            cache.get("key2");
        }

        // 访问不存在的键（未命中）
        for i in 0..80 {
            cache.get(&format!("nonexistent_{}", i));
        }

        let stats = cache.stats();

        // 总访问次数：20（命中） + 80（未命中） = 100
        assert_eq!(stats.total_lookups, 100);
        assert_eq!(stats.hits, 20);
        assert_eq!(stats.misses, 80);
        assert!((stats.hit_rate() - 20.0).abs() < 0.1);
    }

    /// 测试高并发场景下的缓存命中率
    #[test]
    fn test_cache_hit_rate_concurrent() {
        let cache: Arc<L1Cache<String>> =
            Arc::new(L1Cache::with_ttl_and_size(Duration::from_secs(60), 10000));

        // 预填充热点键
        for i in 0..100 {
            cache.set(format!("hot_{}", i), format!("value_{}", i));
        }

        let mut handles = vec![];

        // 并发访问热点键
        for _ in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for _ in 0..1000 {
                    // 每个线程访问相同的热点键
                    for i in 0..100 {
                        cache_clone.get(&format!("hot_{}", i));
                    }
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let stats = cache.stats();

        // 10 个线程 * 1000 次 * 100 个键 = 1,000,000 次访问
        // 所有访问都应该命中
        assert_eq!(stats.total_lookups, 1_000_000);
        assert!(stats.hit_rate() > 99.0, "并发场景下命中率应该接近 100%");
    }

    /// 测试缓存过期对命中率的影响
    #[test]
    fn test_cache_hit_rate_with_expiration() {
        let cache: L1Cache<String> = L1Cache::with_ttl_and_size(Duration::from_millis(50), 100);

        // 设置键
        cache.set("key1".to_string(), "value1".to_string());

        // 第一次访问（命中）
        assert!(cache.get("key1").is_some());

        // 等待过期
        thread::sleep(Duration::from_millis(60));

        // 第二次访问（未命中，因为过期）
        assert!(cache.get("key1").is_none());

        // 重新设置
        cache.set("key1".to_string(), "value2".to_string());

        // 第三次访问（命中）
        assert!(cache.get("key1").is_some());

        let stats = cache.stats();
        assert_eq!(stats.total_lookups, 3);
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
    }

    /// 测试缓存容量限制对命中率的影响
    #[test]
    fn test_cache_hit_rate_with_capacity_limit() {
        // 创建容量为 10 的缓存
        let cache: L1Cache<String> = L1Cache::with_ttl_and_size(Duration::from_secs(60), 10);

        // 填充缓存
        for i in 0..10 {
            cache.set(format!("key_{}", i), format!("value_{}", i));
        }

        // 访问前 5 个键（应该命中）
        for i in 0..5 {
            assert!(cache.get(&format!("key_{}", i)).is_some());
        }

        // 添加新键，触发驱逐
        for i in 10..15 {
            cache.set(format!("key_{}", i), format!("value_{}", i));
        }

        // 验证缓存已满（容量上限，eviction 后约 80% ~ 100%）
        assert!(
            cache.len() >= 8 && cache.len() <= 10,
            "expected cache len between 8 and 10, got {}",
            cache.len()
        );

        // 验证 evicted 条目 > 0（说明发生了容量驱逐）
        let stats = cache.stats();
        assert!(
            stats.capacity_evictions > 0,
            "expected capacity evictions > 0, got {}",
            stats.capacity_evictions
        );

        // 验证总访问数正确（5次读取前5键 = 5，不包括 set 操作）
        assert_eq!(stats.total_lookups, 5);
    }

    /// 测试 CacheableDecision 的转换
    #[test]
    fn test_cacheable_decision_conversion() {
        use crate::error::{BanInfo, Decision};

        // 测试允许决策
        let allowed = CacheableDecision::allowed();
        assert!(allowed.is_allowed());
        assert_eq!(allowed.to_decision(), Decision::Allowed(None));

        // 测试拒绝决策
        let rejected = CacheableDecision::rejected("rate limit exceeded");
        assert!(rejected.is_rejected());
        assert_eq!(
            rejected.to_decision(),
            Decision::Rejected("rate limit exceeded".to_string())
        );

        // 测试封禁决策
        let ban_info = BanInfo {
            reason: "spam".to_string(),
            banned_until: chrono::Utc::now(),
            ban_times: 3,
        };
        let banned = CacheableDecision::banned(ban_info.clone());
        assert!(banned.is_banned());

        // 测试从 Decision 转换
        let decision = Decision::Allowed(Some("test".to_string()));
        let cacheable = CacheableDecision::from_decision(&decision);
        assert!(cacheable.is_allowed());
        assert_eq!(cacheable.reason, Some("test".to_string()));
    }

    /// 测试缓存键生成器
    #[test]
    fn test_rate_limit_cache_key() {
        // 用户限流键
        let user_key = RateLimitCacheKey::user_rate_limit("user123", "rule1");
        assert_eq!(user_key, "rl:user:user123:rule1");

        // IP 限流键
        let ip_key = RateLimitCacheKey::ip_rate_limit("192.168.1.1", "rule2");
        assert_eq!(ip_key, "rl:ip:192.168.1.1:rule2");

        // API Key 限流键
        let api_key = RateLimitCacheKey::api_key_rate_limit("api123", "rule3");
        assert_eq!(api_key, "rl:apikey:api123:rule3");

        // 通用限流键
        let generic_key = RateLimitCacheKey::generic("identifier", "rule4");
        assert_eq!(generic_key, "rl:generic:identifier:rule4");

        // 封禁检查键
        let ban_key = RateLimitCacheKey::ban_check("user123");
        assert_eq!(ban_key, "ban:user123");
    }

    /// 测试按包含字符串失效缓存
    #[test]
    fn test_cache_invalidate_containing() {
        let cache: L1Cache<String> = L1Cache::new();

        cache.set("rl:user:123:rule1".to_string(), "value1".to_string());
        cache.set("rl:user:123:rule2".to_string(), "value2".to_string());
        cache.set("rl:user:456:rule1".to_string(), "value3".to_string());
        cache.set("rl:ip:192.168.1.1:rule1".to_string(), "value4".to_string());

        // 使包含 ":rule1" 的所有缓存失效
        cache.invalidate_containing(":rule1");

        assert!(cache.get("rl:user:123:rule1").is_none());
        assert!(cache.get("rl:user:456:rule1").is_none());
        assert!(cache.get("rl:ip:192.168.1.1:rule1").is_none());
        assert_eq!(cache.get("rl:user:123:rule2"), Some("value2".to_string()));
    }

    /// 压力测试：高负载下的缓存命中率
    #[test]
    fn test_cache_hit_rate_stress() {
        let cache: Arc<L1Cache<String>> =
            Arc::new(L1Cache::with_ttl_and_size(Duration::from_secs(60), 10000));

        // 预填充 1000 个键
        for i in 0..1000 {
            cache.set(format!("key_{}", i), format!("value_{}", i));
        }

        let mut handles = vec![];

        // 启动 50 个并发线程
        for thread_id in 0..50 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                let mut local_hits = 0;
                let mut local_misses = 0;

                // 每个线程执行 1000 次操作
                // 使用确定性模式：基于线程 ID 和迭代次数选择键
                for iter in 0..1000 {
                    // 使用简单的哈希模式选择键
                    let key_idx = (thread_id * 1000 + iter) % 1000;
                    if cache_clone.get(&format!("key_{}", key_idx)).is_some() {
                        local_hits += 1;
                    } else {
                        local_misses += 1;
                    }
                }

                (local_hits, local_misses)
            });
            handles.push(handle);
        }

        let mut total_hits = 0;
        let mut total_misses = 0;

        for handle in handles {
            let (hits, misses) = handle.join().unwrap();
            total_hits += hits;
            total_misses += misses;
        }

        let stats = cache.stats();

        // 验证统计正确性
        assert_eq!(stats.hits, total_hits);
        assert_eq!(stats.misses, total_misses);

        // 命中率应该很高（> 95%）
        let hit_rate = stats.hit_rate();
        assert!(
            hit_rate > 95.0,
            "压力测试下命中率应该 > 95%，实际为 {:.2}%",
            hit_rate
        );
    }
}
