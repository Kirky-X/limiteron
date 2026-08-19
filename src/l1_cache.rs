// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! L1 本地缓存模块
//!
//! 用于缓存热点限流结果，减少存储层访问。
//! 使用 oxcache 作为底层缓存引擎，支持 TTL 过期策略。

use crate::error::{BanInfo, Decision, RateLimitMetadata, RejectionMetadata};
use oxcache::{Cache, OxCacheError};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// 可缓存的决策结果
///
/// 用于 L1 缓存的决策结果类型，支持序列化和反序列化。
/// 与 Decision 类型不同，该类型专门用于缓存场景。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CacheableDecision {
    /// 决策类型：allowed, rejected, banned
    pub decision_type: String,
    /// 决策原因（可选）
    pub reason: Option<String>,
    /// 封禁信息（仅当 decision_type 为 banned 时）
    pub ban_info: Option<CacheableBanInfo>,
}

/// 可缓存的封禁信息
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CacheableBanInfo {
    /// 封禁原因
    pub reason: String,
    /// 封禁到期时间（ISO 8601 格式）
    pub banned_until: String,
    /// 封禁次数
    pub ban_times: u32,
}

impl CacheableDecision {
    /// 创建允许决策（仅测试用）
    #[cfg(test)]
    pub fn allowed() -> Self {
        Self {
            decision_type: "allowed".to_string(),
            reason: None,
            ban_info: None,
        }
    }

    /// 创建拒绝决策（仅测试用）
    #[cfg(test)]
    pub fn rejected(reason: impl Into<String>) -> Self {
        Self {
            decision_type: "rejected".to_string(),
            reason: Some(reason.into()),
            ban_info: None,
        }
    }

    /// 创建封禁决策
    pub fn banned(ban_info: &BanInfo) -> Self {
        Self {
            decision_type: "banned".to_string(),
            reason: Some(ban_info.reason().to_string()),
            ban_info: Some(CacheableBanInfo {
                reason: ban_info.reason().to_string(),
                banned_until: ban_info.banned_until().to_rfc3339(),
                ban_times: ban_info.ban_times(),
            }),
        }
    }

    /// 从 Decision 转换
    pub fn from_decision(decision: &Decision) -> Self {
        match decision {
            Decision::Allowed(metadata) => Self {
                decision_type: "allowed".to_string(),
                reason: if metadata.policy.is_empty() {
                    None
                } else {
                    Some(metadata.policy.clone())
                },
                ban_info: None,
            },
            Decision::Rejected(metadata) => Self {
                decision_type: "rejected".to_string(),
                reason: Some(metadata.reason.clone()),
                ban_info: None,
            },
            Decision::Banned(info) => Self::banned(info),
        }
    }

    /// 转换为 Decision
    pub fn to_decision(&self) -> Decision {
        match self.decision_type.as_str() {
            "allowed" => Decision::Allowed(RateLimitMetadata {
                limit: 0,
                remaining: 0,
                reset_at: 0,
                retry_after: None,
                policy: self.reason.clone().unwrap_or_default(),
            }),
            "rejected" => Decision::Rejected(RejectionMetadata {
                reason: self.reason.clone().unwrap_or_default(),
                retry_after: 0,
                limit: 0,
                reset_at: 0,
            }),
            "banned" => {
                if let Some(info) = &self.ban_info {
                    Decision::Banned(BanInfo::new(
                        info.reason.clone(),
                        chrono::DateTime::parse_from_rfc3339(&info.banned_until)
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .unwrap_or_else(|_| chrono::Utc::now()),
                        info.ban_times,
                    ))
                } else {
                    Decision::Banned(BanInfo::new("unknown".to_string(), chrono::Utc::now(), 0))
                }
            }
            _ => Decision::allowed_default(),
        }
    }

    /// 检查是否为允许决策（仅测试用）
    #[cfg(test)]
    pub fn is_allowed(&self) -> bool {
        self.decision_type == "allowed"
    }

    /// 检查是否为拒绝决策（仅测试用）
    #[cfg(test)]
    pub fn is_rejected(&self) -> bool {
        self.decision_type == "rejected"
    }

    /// 检查是否为封禁决策（仅测试用）
    #[cfg(test)]
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

    /// 生成用户限流缓存键（带命名空间）
    ///
    /// # 参数
    ///
    /// - `namespace`: 租户命名空间前缀
    /// - `user_id`: 用户 ID
    /// - `rule_id`: 规则 ID
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::RateLimitCacheKey;
    ///
    /// let key = RateLimitCacheKey::user_rate_limit_with_ns("tenant:acme:env:prod", "user123", "rule1");
    /// assert_eq!(key, "tenant:acme:env:prod:rl:user:user123:rule1");
    /// ```
    pub fn user_rate_limit_with_ns(namespace: &str, user_id: &str, rule_id: &str) -> String {
        format!("{}:rl:user:{}:{}", namespace, user_id, rule_id)
    }

    /// 生成 IP 限流缓存键（带命名空间）
    ///
    /// # 参数
    ///
    /// - `namespace`: 租户命名空间前缀
    /// - `ip`: IP 地址
    /// - `rule_id`: 规则 ID
    pub fn ip_rate_limit_with_ns(namespace: &str, ip: &str, rule_id: &str) -> String {
        format!("{}:rl:ip:{}:{}", namespace, ip, rule_id)
    }

    /// 生成 API Key 限流缓存键（带命名空间）
    ///
    /// # 参数
    ///
    /// - `namespace`: 租户命名空间前缀
    /// - `api_key`: API Key
    /// - `rule_id`: 规则 ID
    pub fn api_key_rate_limit_with_ns(namespace: &str, api_key: &str, rule_id: &str) -> String {
        format!("{}:rl:apikey:{}:{}", namespace, api_key, rule_id)
    }

    /// 生成通用限流缓存键（带命名空间）
    ///
    /// # 参数
    ///
    /// - `namespace`: 租户命名空间前缀
    /// - `identifier`: 标识符
    /// - `rule_id`: 规则 ID
    pub fn generic_with_ns(namespace: &str, identifier: &str, rule_id: &str) -> String {
        format!("{}:rl:generic:{}:{}", namespace, identifier, rule_id)
    }

    /// 生成封禁检查缓存键（带命名空间）
    ///
    /// # 参数
    ///
    /// - `namespace`: 租户命名空间前缀
    /// - `identifier`: 标识符
    pub fn ban_check_with_ns(namespace: &str, identifier: &str) -> String {
        format!("{}:ban:{}", namespace, identifier)
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

/// 孤岛模式降级策略
///
/// 当存储层（L2/L3）不可用时，L1 缓存进入孤岛模式，使用此策略决定如何处理请求。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IslandFallbackStrategy {
    /// 保守策略：拒绝所有请求，避免过载
    RejectAll,
    /// 宽松策略：允许所有请求通过，可能超限
    AllowAll,
    /// 本地降级：使用 L1 缓存中的历史决策（如果存在）
    #[default]
    LocalDecision,
    /// 配额限制：使用预设的保守配额继续限流
    ConservativeQuota {
        /// 保守配额的请求限制
        max_requests: u32,
        /// 时间窗口（秒）
        window_secs: u64,
    },
}

/// 孤岛模式配置
///
/// 配置 L1 缓存在存储层故障时的行为。
#[derive(Debug, Clone)]
pub struct IslandModeConfig {
    /// 是否启用孤岛模式
    pub enabled: bool,
    /// 降级策略
    pub fallback_strategy: IslandFallbackStrategy,
    /// 孤岛模式下的 TTL（通常更长，因为无法从存储层刷新）
    pub island_ttl: Duration,
    /// 是否在存储层恢复后自动退出孤岛模式
    pub auto_exit_on_recovery: bool,
}

impl Default for IslandModeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            fallback_strategy: IslandFallbackStrategy::default(),
            island_ttl: Duration::from_secs(300), // 5 分钟
            auto_exit_on_recovery: true,
        }
    }
}

impl IslandModeConfig {
    /// 创建新的孤岛模式配置
    pub fn new(fallback_strategy: IslandFallbackStrategy) -> Self {
        Self {
            enabled: true,
            fallback_strategy,
            ..Default::default()
        }
    }

    /// 设置降级策略
    pub fn with_fallback_strategy(mut self, strategy: IslandFallbackStrategy) -> Self {
        self.fallback_strategy = strategy;
        self
    }

    /// 设置孤岛模式下的 TTL
    pub fn with_island_ttl(mut self, ttl: Duration) -> Self {
        self.island_ttl = ttl;
        self
    }

    /// 设置是否在存储层恢复后自动退出孤岛模式
    pub fn with_auto_exit_on_recovery(mut self, auto_exit: bool) -> Self {
        self.auto_exit_on_recovery = auto_exit;
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
/// use limiteron::{L1Cache, L1CacheConfig};
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
    cache: Arc<Cache<String, T>>,
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
    /// 孤岛模式配置
    island_config: Arc<RwLock<Option<IslandModeConfig>>>,
    /// 是否处于孤岛模式 (0 = false, 1 = true)
    is_island_mode: AtomicU64,
}

impl<T> Clone for L1Cache<T>
where
    T: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
            config: self.config.clone(),
            total_lookups: AtomicU64::new(
                self.total_lookups
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            hits: AtomicU64::new(self.hits.load(std::sync::atomic::Ordering::Relaxed)),
            expired_evictions: AtomicU64::new(
                self.expired_evictions
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            capacity_evictions: AtomicU64::new(
                self.capacity_evictions
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            island_config: Arc::clone(&self.island_config),
            is_island_mode: AtomicU64::new(
                self.is_island_mode
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
        }
    }
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
    pub async fn new() -> Result<Self, OxCacheError> {
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
    pub async fn with_config(config: L1CacheConfig) -> Result<Self, OxCacheError> {
        let cache = Cache::builder()
            .ttl(config.default_ttl)
            .capacity(config.max_size as u64)
            .build()
            .await?;

        Ok(Self {
            cache: Arc::new(cache),
            config,
            total_lookups: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            expired_evictions: AtomicU64::new(0),
            capacity_evictions: AtomicU64::new(0),
            island_config: Arc::new(RwLock::new(None)),
            is_island_mode: AtomicU64::new(0),
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
    ) -> Result<Self, OxCacheError> {
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
    pub async fn get(&self, key: &str) -> Result<Option<T>, OxCacheError> {
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
    pub async fn set(&self, key: String, value: T) -> Result<(), OxCacheError> {
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
    ) -> Result<(), OxCacheError> {
        self.cache.set_with_ttl(&key, &value, Some(ttl)).await
    }

    /// 使缓存失效
    ///
    /// 移除指定键的缓存条目。
    ///
    /// # 参数
    ///
    /// - `key`: 缓存键
    pub async fn invalidate(&self, key: &str) -> Result<(), OxCacheError> {
        self.cache.delete(&key.to_string()).await
    }

    /// 使匹配前缀的所有缓存失效
    ///
    /// 通过 oxcache 的 `keys(pattern)` 枚举所有匹配键并逐个删除。
    ///
    /// # 参数
    ///
    /// - `prefix`: 键前缀
    pub async fn invalidate_by_prefix(&self, prefix: &str) -> Result<(), OxCacheError> {
        let keys = self.cache.keys(&format!("{}*", prefix)).await?;
        for key in keys {
            self.cache.delete(&key).await?;
        }
        Ok(())
    }

    /// 使包含指定字符串的所有缓存失效
    ///
    /// 通过 oxcache 的 `keys("*")` 枚举全部键，过滤包含 pattern 的项并删除。
    ///
    /// # 参数
    ///
    /// - `pattern`: 要匹配的字符串模式
    pub async fn invalidate_containing(&self, pattern: &str) -> Result<(), OxCacheError> {
        let keys = self.cache.keys("*").await?;
        for key in keys {
            if key.contains(pattern) {
                self.cache.delete(&key).await?;
            }
        }
        Ok(())
    }

    /// 清空所有缓存
    pub async fn clear(&self) -> Result<(), OxCacheError> {
        self.cache.clear().await
    }

    /// 清理过期条目
    ///
    /// 注意：oxcache 自动处理过期，此方法主要用于统计
    ///
    /// # 返回
    ///
    /// 返回清理的条目数（oxcache 自动处理，返回 0）
    pub async fn evict_expired(&self) -> Result<usize, OxCacheError> {
        // oxcache 自动处理过期条目
        Ok(0)
    }

    /// 获取当前缓存大小
    pub async fn len(&self) -> Result<usize, OxCacheError> {
        let len = self.cache.len().await? as usize;
        Ok(len)
    }

    /// 检查缓存是否为空
    pub async fn is_empty(&self) -> Result<bool, OxCacheError> {
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
    pub async fn contains(&self, key: &str) -> Result<bool, OxCacheError> {
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
    pub async fn ttl(&self, _key: &str) -> Result<Option<Duration>, OxCacheError> {
        // oxcache 不支持获取单个键的 TTL
        Ok(None)
    }

    // ==================== 孤岛模式方法 ====================

    /// 启用孤岛模式
    ///
    /// 当存储层（L2/L3）故障时调用，L1 缓存进入独立运行模式。
    ///
    /// # 参数
    /// - `config`: 孤岛模式配置
    pub fn enable_island_mode(&self, config: IslandModeConfig) {
        let was_island = self.is_island_mode.swap(1, Ordering::AcqRel);
        if was_island == 0 {
            log::warn!(
                target: "l1_cache",
                "L1 缓存进入孤岛模式: strategy={:?}",
                config.fallback_strategy
            );
        }
        let mut island_config = self.island_config.write();
        *island_config = Some(config);
    }

    /// 禁用孤岛模式
    ///
    /// 当存储层恢复后调用，L1 缓存恢复正常模式。
    pub fn disable_island_mode(&self) {
        let was_island = self.is_island_mode.swap(0, Ordering::AcqRel);
        if was_island == 1 {
            log::info!(target: "l1_cache", "L1 缓存退出孤岛模式");
        }
        let mut island_config = self.island_config.write();
        *island_config = None;
    }

    /// 检查是否处于孤岛模式
    pub fn is_island_mode(&self) -> bool {
        self.is_island_mode.load(Ordering::Acquire) == 1
    }

    /// 获取孤岛模式配置
    pub fn island_config(&self) -> Option<IslandModeConfig> {
        self.island_config.read().clone()
    }

    /// 获取孤岛模式统计信息
    ///
    /// 扩展标准统计信息，包含孤岛模式状态。
    #[cfg(test)]
    pub(crate) async fn island_stats(&self) -> L1CacheStats {
        let mut stats = self.stats().await;
        if self.is_island_mode() {
            stats.current_size += 1; // 标记位：使用 current_size+1 表示孤岛模式
        }
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        use crate::error::{BanInfo, Decision, RateLimitMetadata};

        // 测试允许决策
        let allowed = CacheableDecision::allowed();
        assert!(allowed.is_allowed());
        let decision = allowed.to_decision();
        assert!(matches!(decision, Decision::Allowed(_)));

        // 测试拒绝决策
        let rejected = CacheableDecision::rejected("rate limit exceeded");
        assert!(rejected.is_rejected());
        let decision = rejected.to_decision();
        if let Decision::Rejected(metadata) = decision {
            assert_eq!(metadata.reason, "rate limit exceeded");
        } else {
            panic!("Expected Rejected decision");
        }

        // 测试封禁决策
        let ban_info = BanInfo::new("spam".to_string(), chrono::Utc::now(), 3);
        let banned = CacheableDecision::banned(&ban_info);
        assert!(banned.is_banned());

        // 测试从 Decision 转换
        let metadata = RateLimitMetadata {
            limit: 100,
            remaining: 50,
            reset_at: 1234567890,
            retry_after: None,
            policy: "test".to_string(),
        };
        let decision = Decision::Allowed(metadata);
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

    #[test]
    fn test_cacheable_decision_from_decision_rejected() {
        let metadata = RejectionMetadata {
            reason: "too many requests".to_string(),
            retry_after: 30,
            limit: 100,
            reset_at: 1234567890,
        };
        let decision = Decision::Rejected(metadata);
        let cacheable = CacheableDecision::from_decision(&decision);
        assert!(cacheable.is_rejected());
        assert_eq!(cacheable.reason, Some("too many requests".to_string()));
    }

    #[test]
    fn test_cacheable_decision_from_decision_banned() {
        let ban_info = BanInfo::new("spam".to_string(), chrono::Utc::now(), 3);
        let decision = Decision::Banned(ban_info);
        let cacheable = CacheableDecision::from_decision(&decision);
        assert!(cacheable.is_banned());
        let cached_ban = cacheable.ban_info.unwrap();
        assert_eq!(cached_ban.reason, "spam");
        assert_eq!(cached_ban.ban_times, 3);
    }

    #[test]
    fn test_cacheable_decision_from_decision_allowed_empty_policy() {
        let metadata = RateLimitMetadata {
            limit: 100,
            remaining: 50,
            reset_at: 1234567890,
            retry_after: None,
            policy: String::new(),
        };
        let decision = Decision::Allowed(metadata);
        let cacheable = CacheableDecision::from_decision(&decision);
        assert!(cacheable.is_allowed());
        assert_eq!(cacheable.reason, None);
    }

    #[test]
    fn test_cacheable_decision_to_decision_unknown() {
        let cd = CacheableDecision {
            decision_type: "unknown".to_string(),
            reason: None,
            ban_info: None,
        };
        let decision = cd.to_decision();
        assert!(matches!(decision, Decision::Allowed(_)));
    }

    #[test]
    fn test_cacheable_decision_to_decision_banned_no_baninfo() {
        let cd = CacheableDecision {
            decision_type: "banned".to_string(),
            reason: Some("test".to_string()),
            ban_info: None,
        };
        let decision = cd.to_decision();
        assert!(matches!(decision, Decision::Banned(_)));
    }

    #[test]
    fn test_cacheable_decision_to_decision_banned_with_baninfo() {
        // 覆盖 ban_info = Some(...) 路径（lines 116-121）
        let cd = CacheableDecision {
            decision_type: "banned".to_string(),
            reason: Some("banned".to_string()),
            ban_info: Some(CacheableBanInfo {
                reason: "policy violation".to_string(),
                banned_until: "2026-12-31T23:59:59Z".to_string(),
                ban_times: 5,
            }),
        };
        let decision = cd.to_decision();
        match decision {
            Decision::Banned(info) => {
                assert_eq!(info.reason(), "policy violation");
                assert_eq!(info.ban_times(), 5);
            }
            _ => panic!("expected Decision::Banned"),
        }
    }

    #[test]
    fn test_cacheable_decision_to_decision_banned_invalid_date() {
        // 当日期解析失败时，应回退到当前时间（unwrap_or_else 分支）
        let cd = CacheableDecision {
            decision_type: "banned".to_string(),
            reason: Some("banned".to_string()),
            ban_info: Some(CacheableBanInfo {
                reason: "bad date".to_string(),
                banned_until: "not-a-date".to_string(),
                ban_times: 1,
            }),
        };
        let decision = cd.to_decision();
        assert!(matches!(decision, Decision::Banned(_)));
    }

    #[test]
    fn test_rate_limit_cache_key_with_namespace() {
        let ns = "tenant:acme:env:prod";
        assert_eq!(
            RateLimitCacheKey::user_rate_limit_with_ns(ns, "user123", "rule1"),
            "tenant:acme:env:prod:rl:user:user123:rule1"
        );
        assert_eq!(
            RateLimitCacheKey::ip_rate_limit_with_ns(ns, "10.0.0.1", "rule2"),
            "tenant:acme:env:prod:rl:ip:10.0.0.1:rule2"
        );
        assert_eq!(
            RateLimitCacheKey::api_key_rate_limit_with_ns(ns, "key123", "rule3"),
            "tenant:acme:env:prod:rl:apikey:key123:rule3"
        );
        assert_eq!(
            RateLimitCacheKey::generic_with_ns(ns, "ident", "rule4"),
            "tenant:acme:env:prod:rl:generic:ident:rule4"
        );
        assert_eq!(
            RateLimitCacheKey::ban_check_with_ns(ns, "user123"),
            "tenant:acme:env:prod:ban:user123"
        );
    }

    #[test]
    fn test_cache_stats_hit_rate_empty() {
        let stats = L1CacheStats::default();
        assert_eq!(stats.hit_rate(), 0.0);
        assert_eq!(stats.miss_rate(), 100.0);
    }

    #[test]
    fn test_cache_stats_methods() {
        let stats = L1CacheStats {
            total_lookups: 100,
            hits: 75,
            misses: 25,
            ..Default::default()
        };
        assert_eq!(stats.hit_rate(), 75.0);
        assert_eq!(stats.miss_rate(), 25.0);
    }

    #[test]
    fn test_cache_stats_debug() {
        let stats = L1CacheStats::default();
        let debug = format!("{:?}", stats);
        assert!(debug.starts_with("L1CacheStats {"));
        assert!(debug.contains("total_lookups"));
    }

    #[test]
    fn test_cache_config_new() {
        let config = L1CacheConfig::new(Duration::from_secs(30), 500);
        assert_eq!(config.default_ttl, Duration::from_secs(30));
        assert_eq!(config.max_size, 500);
        assert!(config.enable_stats);
    }

    #[test]
    fn test_island_fallback_strategy_default() {
        assert_eq!(
            IslandFallbackStrategy::default(),
            IslandFallbackStrategy::LocalDecision
        );
    }

    #[test]
    fn test_island_mode_config_default_and_builder() {
        let default = IslandModeConfig::default();
        assert!(!default.enabled);
        assert_eq!(
            default.fallback_strategy,
            IslandFallbackStrategy::LocalDecision
        );
        assert_eq!(default.island_ttl, Duration::from_secs(300));
        assert!(default.auto_exit_on_recovery);

        let built = IslandModeConfig::default()
            .with_fallback_strategy(IslandFallbackStrategy::AllowAll)
            .with_island_ttl(Duration::from_secs(600))
            .with_auto_exit_on_recovery(false);
        assert_eq!(built.fallback_strategy, IslandFallbackStrategy::AllowAll);
        assert_eq!(built.island_ttl, Duration::from_secs(600));
        assert!(!built.auto_exit_on_recovery);
    }

    #[test]
    fn test_island_mode_config_new() {
        let config = IslandModeConfig::new(IslandFallbackStrategy::RejectAll);
        assert!(config.enabled);
        assert_eq!(config.fallback_strategy, IslandFallbackStrategy::RejectAll);

        let config2 = IslandModeConfig::new(IslandFallbackStrategy::ConservativeQuota {
            max_requests: 100,
            window_secs: 60,
        });
        assert!(config2.enabled);
        assert!(matches!(
            config2.fallback_strategy,
            IslandFallbackStrategy::ConservativeQuota { .. }
        ));
    }

    #[tokio::test]
    async fn test_cache_with_ttl_and_size() {
        let cache: L1Cache<String> = L1Cache::with_ttl_and_size(Duration::from_secs(60), 100)
            .await
            .unwrap();
        cache.set("k".to_string(), "v".to_string()).await.unwrap();
        assert_eq!(cache.get("k").await.unwrap(), Some("v".to_string()));
    }

    #[tokio::test]
    async fn test_cache_set_with_ttl() {
        let cache: L1Cache<String> = L1Cache::new().await.unwrap();
        cache
            .set_with_ttl("k".to_string(), "v".to_string(), Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(cache.get("k").await.unwrap(), Some("v".to_string()));
    }

    #[tokio::test]
    async fn test_cache_clear_and_len() {
        let cache: L1Cache<String> = L1Cache::new().await.unwrap();
        let _ = cache.len().await.unwrap();
        let _ = cache.is_empty().await.unwrap();

        cache.set("k1".to_string(), "v1".to_string()).await.unwrap();
        cache.set("k2".to_string(), "v2".to_string()).await.unwrap();

        // Verify items exist before clear
        assert_eq!(cache.get("k1").await.unwrap(), Some("v1".to_string()));
        assert_eq!(cache.get("k2").await.unwrap(), Some("v2".to_string()));

        cache.clear().await.unwrap();

        // Verify items are gone after clear
        assert_eq!(cache.get("k1").await.unwrap(), None);
        assert_eq!(cache.get("k2").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_cache_evict_expired() {
        let cache: L1Cache<String> = L1Cache::new().await.unwrap();
        assert_eq!(cache.evict_expired().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_cache_ttl_method() {
        let cache: L1Cache<String> = L1Cache::new().await.unwrap();
        cache.set("k".to_string(), "v".to_string()).await.unwrap();
        assert!(cache.ttl("k").await.unwrap().is_none());
        assert!(cache.ttl("nonexistent").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_cache_invalidate_prefix_and_containing() {
        let cache: L1Cache<String> = L1Cache::new().await.unwrap();
        cache.invalidate_by_prefix("test:").await.unwrap();
        cache.invalidate_containing("pattern").await.unwrap();
    }

    #[tokio::test]
    async fn test_cache_empty_key() {
        let cache: L1Cache<String> = L1Cache::new().await.unwrap();
        cache
            .set(String::new(), "empty_val".to_string())
            .await
            .unwrap();
        assert_eq!(cache.get("").await.unwrap(), Some("empty_val".to_string()));
        cache.invalidate("").await.unwrap();
        assert_eq!(cache.get("").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_cache_stats_disabled() {
        let config = L1CacheConfig::new(Duration::from_secs(60), 1000).with_stats(false);
        let cache: L1Cache<String> = L1Cache::with_config(config).await.unwrap();

        cache.set("k".to_string(), "v".to_string()).await.unwrap();
        let _ = cache.get("k").await.unwrap();
        let _ = cache.get("nonexistent").await.unwrap();

        let stats = cache.stats().await;
        assert_eq!(stats.total_lookups, 0);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }

    #[tokio::test]
    async fn test_cache_island_mode() {
        let cache: L1Cache<String> = L1Cache::new().await.unwrap();

        assert!(!cache.is_island_mode());
        assert!(cache.island_config().is_none());

        let island_cfg = IslandModeConfig::new(IslandFallbackStrategy::RejectAll);
        cache.enable_island_mode(island_cfg);

        assert!(cache.is_island_mode());
        let cfg = cache.island_config().unwrap();
        assert_eq!(cfg.fallback_strategy, IslandFallbackStrategy::RejectAll);

        // Re-enable should not panic
        cache.enable_island_mode(IslandModeConfig::new(IslandFallbackStrategy::AllowAll));

        cache.disable_island_mode();
        assert!(!cache.is_island_mode());
        assert!(cache.island_config().is_none());

        // Double disable should not panic
        cache.disable_island_mode();
    }

    #[tokio::test]
    async fn test_cache_island_stats() {
        let cache: L1Cache<String> = L1Cache::new().await.unwrap();
        let normal_stats = cache.island_stats().await;
        assert_eq!(normal_stats.total_lookups, 0);

        cache.enable_island_mode(IslandModeConfig::new(IslandFallbackStrategy::LocalDecision));
        let island_stats = cache.island_stats().await;
        assert_eq!(island_stats.current_size, normal_stats.current_size + 1);

        cache.disable_island_mode();
    }

    #[tokio::test]
    async fn test_cache_clone() {
        let cache: L1Cache<String> = L1Cache::new().await.unwrap();
        cache.set("k".to_string(), "v".to_string()).await.unwrap();
        let _ = cache.get("k").await.unwrap();

        let cloned = cache.clone();
        // Cloned instance shares the same underlying cache data
        assert_eq!(cloned.get("k").await.unwrap(), Some("v".to_string()));

        // Both instances can access shared data
        assert_eq!(cache.get("k").await.unwrap(), Some("v".to_string()));
    }

    #[tokio::test]
    async fn test_cache_concurrent_access() {
        let cache: L1Cache<String> = L1Cache::new().await.unwrap();
        let mut handles = Vec::new();

        for i in 0..10 {
            let c = cache.clone();
            handles.push(tokio::spawn(async move {
                let key = format!("concurrent_key_{}", i);
                let val = format!("concurrent_val_{}", i);
                c.set(key.clone(), val.clone()).await.unwrap();
                let result = c.get(&key).await.unwrap();
                assert_eq!(result, Some(val));
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all data is accessible from the original instance
        for i in 0..10 {
            let key = format!("concurrent_key_{}", i);
            assert_eq!(
                cache.get(&key).await.unwrap(),
                Some(format!("concurrent_val_{}", i))
            );
        }
    }
}
