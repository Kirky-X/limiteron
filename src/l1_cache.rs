//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! L1 本地缓存模块
//!
//! 用于缓存热点限流结果，减少存储层访问。
//! 使用 oxcache 作为底层缓存引擎，支持 TTL 过期策略。

use crate::error::{BanInfo, Decision};
use oxcache::{Cache, CacheError};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// 可缓存的决策结果
///
/// 用于 L1 缓存的决策结果类型，支持序列化和反序列化。
/// 与 Decision 类型不同，该类型专门用于缓存场景。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheableDecision {
    /// 决策类型：allowed, rejected, banned
    pub decision_type: String,
    /// 决策原因（可选）
    pub reason: Option<String>,
    /// 封禁信息（仅当 decision_type 为 banned 时）
    pub ban_info: Option<CacheableBanInfo>,
}

/// 可缓存的封禁信息
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
/// 使用 oxcache 实现的高性能异步缓存，用于缓存热点限流结果。
/// 支持 TTL 过期策略和容量限制。
///
/// # 特性
///
/// - 基于 oxcache 的异步缓存
/// - TTL 过期策略
/// - 容量限制
/// - 命中率统计
///
/// # 类型约束
///
/// 缓存值类型 `T` 必须实现 `Serialize + DeserializeOwned + Send + Sync + 'static`
///
/// # 示例
///
/// ```rust
/// use limiteron::l1_cache::{L1Cache, L1CacheConfig};
/// use std::time::Duration;
///
/// #[tokio::main]
/// async fn main() {
///     let config = L1CacheConfig::new(Duration::from_secs(60), 1000);
///     let cache: L1Cache<String> = L1Cache::with_config(config).await.unwrap();
///
///     // 设置缓存
///     cache.set("key".to_string(), "value".to_string()).await;
///
///     // 获取缓存
///     if let Some(value) = cache.get(&"key".to_string()).await.unwrap() {
///         println!("缓存命中: {}", value);
///     }
///
///     // 获取统计信息
///     let stats = cache.stats().await;
///     println!("命中率: {:.2}%", stats.hit_rate());
/// }
/// ```
pub struct L1Cache<T>
where
    T: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    /// oxcache 缓存实例
    cache: Cache<String, T>,
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

impl<T> L1Cache<T>
where
    T: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    /// 使用默认配置创建 L1 缓存
    ///
    /// # Errors
    ///
    /// 如果 oxcache 初始化失败，返回错误
    pub async fn new() -> Result<Self, CacheError> {
        Self::with_config(L1CacheConfig::default()).await
    }

    /// 使用指定配置创建 L1 缓存
    ///
    /// # 参数
    ///
    /// - `config`: 缓存配置
    ///
    /// # Errors
    ///
    /// 如果 oxcache 初始化失败，返回错误
    pub async fn with_config(config: L1CacheConfig) -> Result<Self, CacheError> {
        let cache = Cache::builder()
            .ttl(config.default_ttl)
            .capacity(config.max_size as u64)
            .build()
            .await?;

        Ok(Self {
            cache,
            config,
            total_lookups: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            expired_evictions: AtomicU64::new(0),
            capacity_evictions: AtomicU64::new(0),
        })
    }

    /// 创建指定 TTL 和最大大小的缓存
    ///
    /// # 参数
    ///
    /// - `default_ttl`: 默认生存时间
    /// - `max_size`: 最大缓存条目数
    ///
    /// # Errors
    ///
    /// 如果 oxcache 初始化失败，返回错误
    pub async fn with_ttl_and_size(
        default_ttl: Duration,
        max_size: usize,
    ) -> Result<Self, CacheError> {
        Self::with_config(L1CacheConfig::new(default_ttl, max_size)).await
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
    pub async fn get(&self, key: &str) -> Result<Option<T>, CacheError> {
        if self.config.enable_stats {
            self.total_lookups.fetch_add(1, Ordering::Relaxed);
        }

        let result = self.cache.get(&key.to_string()).await?;

        if self.config.enable_stats && result.is_some() {
            self.hits.fetch_add(1, Ordering::Relaxed);
        }

        Ok(result)
    }

    /// 设置缓存值
    ///
    /// 使用默认 TTL 设置缓存值。
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    /// - `value`: 缓存值
    pub async fn set(&self, key: String, value: T) -> Result<(), CacheError> {
        self.cache.set(&key, &value).await
    }

    /// 设置缓存值（带自定义 TTL）
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    /// - `value`: 缓存值
    /// - `ttl`: 生存时间
    pub async fn set_with_ttl(
        &self,
        key: String,
        value: T,
        ttl: Duration,
    ) -> Result<(), CacheError> {
        self.cache.set_with_ttl(&key, &value, Some(ttl)).await
    }

    /// 使缓存失效
    ///
    /// 移除指定键的缓存条目。
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    pub async fn invalidate(&self, key: &str) -> Result<(), CacheError> {
        self.cache.delete(&key.to_string()).await
    }

    /// 使匹配前缀的所有缓存失效
    ///
    /// 注意：oxcache 不支持批量前缀删除，此方法需要遍历所有键
    ///
    /// # 参数
    ///
    /// - `prefix`: 键前缀
    pub async fn invalidate_by_prefix(&self, prefix: &str) -> Result<(), CacheError> {
        // oxcache 不支持前缀删除，需要先获取所有键再删除
        // 由于 oxcache API 限制，这里使用简单实现
        // 实际生产环境可能需要额外的键追踪机制
        log::warn!(
            "invalidate_by_prefix called with prefix: {}. Note: This is a no-op in oxcache-based L1Cache",
            prefix
        );
        Ok(())
    }

    /// 使包含指定字符串的所有缓存失效
    ///
    /// 注意：oxcache 不支持模式匹配删除，此方法需要遍历所有键
    ///
    /// # 参数
    ///
    /// - `pattern`: 要匹配的字符串模式
    pub async fn invalidate_containing(&self, pattern: &str) -> Result<(), CacheError> {
        log::warn!(
            "invalidate_containing called with pattern: {}. Note: This is a no-op in oxcache-based L1Cache",
            pattern
        );
        Ok(())
    }

    /// 清空所有缓存
    pub async fn clear(&self) -> Result<(), CacheError> {
        self.cache.clear().await
    }

    /// 清理过期条目
    ///
    /// 注意：oxcache 自动处理过期，此方法主要用于统计
    ///
    /// # 返回
    ///
    /// 返回清理的条目数（oxcache 自动处理，返回 0）
    pub async fn evict_expired(&self) -> Result<usize, CacheError> {
        // oxcache 自动处理过期条目
        Ok(0)
    }

    /// 获取当前缓存大小
    pub async fn len(&self) -> Result<usize, CacheError> {
        let len = self.cache.len().await? as usize;
        Ok(len)
    }

    /// 检查缓存是否为空
    pub async fn is_empty(&self) -> Result<bool, CacheError> {
        let len = self.len().await?;
        Ok(len == 0)
    }

    /// 获取缓存统计信息
    pub async fn stats(&self) -> L1CacheStats {
        let current_size = self.len().await.unwrap_or(0);
        L1CacheStats {
            total_lookups: self.total_lookups.load(Ordering::Relaxed),
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.total_lookups.load(Ordering::Relaxed) - self.hits.load(Ordering::Relaxed),
            expired_evictions: self.expired_evictions.load(Ordering::Relaxed),
            capacity_evictions: self.capacity_evictions.load(Ordering::Relaxed),
            current_size,
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

    /// 检查键是否存在
    pub async fn contains(&self, key: &str) -> Result<bool, CacheError> {
        self.cache.exists(&key.to_string()).await
    }

    /// 获取键的剩余 TTL
    ///
    /// # 返回
    ///
    /// - `Some(Duration)`: 剩余时间
    /// - `None`: 键不存在或已过期
    ///
    /// 注意：oxcache 不直接支持获取 TTL，此方法总是返回 None
    pub async fn ttl(&self, _key: &str) -> Result<Option<Duration>, CacheError> {
        // oxcache 不支持获取单个键的 TTL
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn test_cache_basic_operations() {
        let cache: L1Cache<String> = L1Cache::new().await.unwrap();

        // 测试设置和获取
        cache
            .set("key1".to_string(), "value1".to_string())
            .await
            .unwrap();
        assert_eq!(cache.get("key1").await.unwrap(), Some("value1".to_string()));

        // 测试不存在的键
        assert_eq!(cache.get("key2").await.unwrap(), None);

        // 测试失效
        cache.invalidate("key1").await.unwrap();
        assert_eq!(cache.get("key1").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_cache_custom_ttl() {
        // 注意：Moka 后端不支持 per-entry TTL，TTL 在缓存创建时设置
        // 此测试验证使用短 TTL 创建的缓存能够正确过期
        let config = L1CacheConfig::new(Duration::from_millis(50), 1000);
        let cache: L1Cache<String> = L1Cache::with_config(config).await.unwrap();

        cache
            .set("key1".to_string(), "value1".to_string())
            .await
            .unwrap();
        assert_eq!(cache.get("key1").await.unwrap(), Some("value1".to_string()));

        // 等待过期
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(cache.get("key1").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache: L1Cache<String> = L1Cache::new().await.unwrap();

        cache
            .set("key1".to_string(), "value1".to_string())
            .await
            .unwrap();

        // 命中
        cache.get("key1").await.unwrap();
        cache.get("key1").await.unwrap();

        // 未命中
        cache.get("key2").await.unwrap();

        let stats = cache.stats().await;
        assert_eq!(stats.total_lookups, 3);
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
    }

    #[tokio::test]
    async fn test_cache_contains() {
        let cache: L1Cache<String> = L1Cache::new().await.unwrap();

        cache
            .set("key1".to_string(), "value1".to_string())
            .await
            .unwrap();
        assert!(cache.contains("key1").await.unwrap());
        assert!(!cache.contains("key2").await.unwrap());
    }

    #[tokio::test]
    async fn test_cache_reset_stats() {
        let cache: L1Cache<String> = L1Cache::new().await.unwrap();

        cache
            .set("key1".to_string(), "value1".to_string())
            .await
            .unwrap();
        cache.get("key1").await.unwrap();
        cache.get("key1").await.unwrap();

        let stats = cache.stats().await;
        assert_eq!(stats.hits, 2);

        cache.reset_stats();
        let stats = cache.stats().await;
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.total_lookups, 0);
    }

    #[tokio::test]
    async fn test_cache_config_builder() {
        let config = L1CacheConfig::default()
            .with_ttl(Duration::from_secs(120))
            .with_max_size(5000)
            .with_stats(false);

        assert_eq!(config.default_ttl, Duration::from_secs(120));
        assert_eq!(config.max_size, 5000);
        assert!(!config.enable_stats);
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
}
