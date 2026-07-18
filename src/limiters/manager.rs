// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 全局限流器管理器
//!
//! 提供 `LimiterManager` 全局单例，按 key 缓存限流器实例，
//! 供 `#[flow_control]` 宏生成的代码使用。
//!
//! # 设计
//!
//! - rate limiter: 使用 `TokenBucketLimiter`，capacity=amount，refill_rate 根据 unit 计算
//! - quota limiter: 使用 `QuotaLimiter`（需要 `quota-control` feature），配置 `QuotaConfig`
//! - concurrency limiter: 使用 `ConcurrencyLimiter`，max_concurrent
//!
//! # 线程安全
//!
//! - `DashMap` 提供高并发读写
//! - `Arc` 共享限流器实例
//! - 全局单例通过 `std::sync::LazyLock` 实现（Rust 1.80+）
//! - 访问时间使用 `AtomicU64`（纳秒时间戳），快速路径无锁更新（audit-C-001）
//! - cleanup 用 `AtomicBool` 限制并发，避免同步阻塞请求（audit-H-002）
//!
//! # 限制
//!
//! - 限流器缓存通过 LRU 淘汰管理容量（`MAX_LIMITER_ENTRIES` / `CLEANUP_THRESHOLD`）
//! - `rate="100/m"` 等 unit > 1s 的配置，refill_rate 会向下取整为 u64
//!   （如 100/60 = 1，可能导致精度损失）
//! - 参数一致性校验基于 (capacity, refill_rate) / (max, period) / (max_concurrent)
//!   元组，**不校验 `unit_secs` 字段**：refill_rate 相同但 unit_secs 不同时
//!   （如 amount=100, unit_secs=60 vs amount=100, unit_secs=120，但后者
//!   refill_rate = 100/120 = 1，与前者 100/60 = 1 在 refill_rate 维度上无法区分）
//!   不会触发 panic。这是已知限制，未来需要时可改为缓存 (amount, unit_secs) 元组作为指纹。
//! - `rate_limiters` 与 `rate_access_times` 在并发场景下可能出现短暂不一致
//!   （如 cleanup 期间）。这是 DashMap 多 shard 的固有问题，cleanup 使用 `retain`
//!   原子过滤以最小化窗口（audit-L-001 已缓解）。

use crate::limiters::{ConcurrencyLimiter, TokenBucketLimiter};
use dashmap::DashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "quota-control")]
use crate::limiters::QuotaLimiter;
#[cfg(feature = "quota-control")]
use crate::quota::{AlertConfig, QuotaConfig, QuotaType};

/// Limiter 缓存最大条目数（LRU 上限）
const MAX_LIMITER_ENTRIES: usize = 100_000;
/// 触发 LRU 清理的阈值（超过此值启动清理）
const CLEANUP_THRESHOLD: usize = 110_000;
/// 清理时移除的旧条目比例（10%）
const CLEANUP_RATIO: f64 = 0.1;

/// 获取当前 Unix 纳秒时间戳
///
/// 用作 `AtomicU64` 访问时间存储，避免快速路径中的写锁（audit-C-001）。
/// 返回 `u64`，时钟回退或溢出时返回 0（保守值，cleanup 时被当作"最旧"淘汰）。
fn now_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// 脱敏 key 用于 panic 消息（audit-H-001）
///
/// 避免将用户标识符（user_id / api_key 等）原文泄露到日志或 panic 输出。
/// - 长度 ≤ 16：仅暴露字符数（如 `<8 chars>`）
/// - 长度 > 16：暴露前 8 个字符 + 总长度（如 `user1234...(32 chars)`）
fn redact_key(key: &str) -> String {
    if key.len() > 16 {
        // 取前 8 个 char（避免 UTF-8 边界切片 panic）
        let prefix: String = key.chars().take(8).collect();
        format!("{}...({} chars)", prefix, key.len())
    } else {
        format!("<{} chars>", key.len())
    }
}

/// 通用 LRU 清理逻辑（audit-M-001 DRY + audit-H-001 不持锁分配 + audit-M-004 不全排序）
///
/// - 收集 (key, access_time) 到 Vec（持读锁，仅 map 不 collect 大数据）
/// - 用 `select_nth_unstable_by_key` 找到第 `to_remove` 个最旧的（O(n) 平均，无需全排序）
/// - 收集待移除 key 到 `HashSet`
/// - 用 `retain` 原子过滤两个 DashMap（持写锁，但只一次，不在迭代中 remove）
fn cleanup_lru<L>(
    limiters: &DashMap<String, Arc<L>>,
    access_times: &DashMap<String, AtomicU64>,
    max_entries: usize,
) {
    let target_count = (max_entries as f64 * (1.0 - CLEANUP_RATIO)) as usize;

    // 收集 (key, time) 到 Vec，仅持读锁
    let mut entries: Vec<(String, u64)> = access_times
        .iter()
        .map(|e| (e.key().clone(), e.value().load(Ordering::Relaxed)))
        .collect();

    if entries.len() <= target_count {
        return;
    }

    let to_remove = entries.len() - target_count;

    // select_nth_unstable_by_key：O(n) 平均找到第 (to_remove-1) 小的元素，
    // 同时将 Vec 划分为 [0..to_remove) 最旧 + [to_remove..) 较新（无需全排序）
    // index 为 to_remove - 1（0-based），划分后前 to_remove 个即待淘汰
    entries.select_nth_unstable_by_key(to_remove - 1, |e| e.1);

    let to_remove_keys: HashSet<String> = entries
        .into_iter()
        .take(to_remove)
        .map(|(k, _)| k)
        .collect();

    // 用 retain 一次性过滤（持写锁，但只一次，避免迭代中 remove 的死锁/不一致）
    limiters.retain(|k, _| !to_remove_keys.contains(k));
    access_times.retain(|k, _| !to_remove_keys.contains(k));
}

/// 全局限流器管理器
///
/// 按 key 缓存限流器实例，避免每次调用都创建新实例。
/// 使用 `DashMap` 提供高并发读写。
///
/// # 示例
///
/// ```rust
/// use limiteron::limiters::manager::GLOBAL_LIMITER_MANAGER;
/// use limiteron::limiters::Limiter;
///
/// #[tokio::main]
/// async fn main() {
///     let limiter = GLOBAL_LIMITER_MANAGER.get_rate_limiter("user:123", 100, 1);
///     let allowed = limiter.allow(1).await.unwrap();
///     assert!(allowed);
/// }
/// ```
pub struct LimiterManager {
    /// Rate limiters 缓存（key -> TokenBucketLimiter）
    rate_limiters: DashMap<String, Arc<TokenBucketLimiter>>,
    /// Rate limiters 最后访问时间（key -> AtomicU64 纳秒时间戳），用于 LRU 淘汰
    ///
    /// 使用 `AtomicU64` 而非 `Instant`，快速路径无锁更新（audit-C-001）。
    rate_access_times: DashMap<String, AtomicU64>,
    /// Quota limiters 缓存（key -> QuotaLimiter）
    #[cfg(feature = "quota-control")]
    quota_limiters: DashMap<String, Arc<QuotaLimiter>>,
    /// Quota limiters 最后访问时间（key -> AtomicU64 纳秒时间戳）
    #[cfg(feature = "quota-control")]
    quota_access_times: DashMap<String, AtomicU64>,
    /// Concurrency limiters 缓存（key -> ConcurrencyLimiter）
    concurrency_limiters: DashMap<String, Arc<ConcurrencyLimiter>>,
    /// Concurrency limiters 最后访问时间（key -> AtomicU64 纳秒时间戳）
    concurrency_access_times: DashMap<String, AtomicU64>,
    /// Rate limiter cleanup 并发限制（audit-H-002：避免同步清理阻塞请求）
    rate_cleanup_in_progress: AtomicBool,
    /// Quota limiter cleanup 并发限制
    #[cfg(feature = "quota-control")]
    quota_cleanup_in_progress: AtomicBool,
    /// Concurrency limiter cleanup 并发限制
    concurrency_cleanup_in_progress: AtomicBool,
}

impl LimiterManager {
    /// 创建新的 `LimiterManager`
    ///
    /// 通常不需要手动调用，使用 `GLOBAL_LIMITER_MANAGER` 全局单例即可。
    /// 测试中可创建独立实例以避免全局状态污染。
    pub fn new() -> Self {
        Self {
            rate_limiters: DashMap::new(),
            rate_access_times: DashMap::new(),
            #[cfg(feature = "quota-control")]
            quota_limiters: DashMap::new(),
            #[cfg(feature = "quota-control")]
            quota_access_times: DashMap::new(),
            concurrency_limiters: DashMap::new(),
            concurrency_access_times: DashMap::new(),
            rate_cleanup_in_progress: AtomicBool::new(false),
            #[cfg(feature = "quota-control")]
            quota_cleanup_in_progress: AtomicBool::new(false),
            concurrency_cleanup_in_progress: AtomicBool::new(false),
        }
    }

    /// 获取或创建 rate limiter
    ///
    /// # 参数
    /// - `key`: 限流器 key（用于隔离不同用户/API/函数）
    /// - `amount`: 请求限额（如 `100/s` 中的 100）
    /// - `unit_secs`: 时间窗口秒数（1=s, 60=m, 3600=h）
    ///
    /// # 返回
    /// `Arc<TokenBucketLimiter>`，可跨线程共享
    ///
    /// # 语义
    /// - capacity = amount（桶容量 = 限额，允许突发 amount 个请求）
    /// - refill_rate = max(1, amount / unit_secs)（每秒补充速率）
    ///
    /// # Panic
    ///
    /// 同 key 但参数（capacity / refill_rate）不一致时 panic（Rule 12：失败必须显性化）。
    /// 这是设计决策：参数不一致是代码 bug，应在开发阶段发现。
    /// 生产环境 panic 会导致当前请求失败（500），但不会崩溃整个进程。
    /// panic 消息中 key 已脱敏（audit-H-001），避免泄露用户标识符到日志。
    ///
    /// # 限制
    ///
    /// 参数一致性仅基于 `(capacity, refill_rate)`，**不校验 `unit_secs`**：
    /// refill_rate 相同但 unit_secs 不同时不会 panic（如 100/60 与 100/120 在
    /// refill_rate 维度上均为 1，无法区分）。详见模块文档 `# 限制` 章节。
    ///
    /// # 示例
    /// - `rate="100/s"` → capacity=100, refill_rate=100
    /// - `rate="100/m"` → capacity=100, refill_rate=max(1, 100/60)=1
    /// - `rate="100/h"` → capacity=100, refill_rate=max(1, 100/3600)=1
    pub fn get_rate_limiter(
        &self,
        key: &str,
        amount: u64,
        unit_secs: u64,
    ) -> Arc<TokenBucketLimiter> {
        let key = key.to_string(); // audit-L-001：缓存一次，避免慢路径多次 to_string
        let refill_rate = if unit_secs > 0 {
            (amount / unit_secs).max(1)
        } else {
            amount
        };

        // 快速路径：get() 读锁，避免每次都走 entry() 写锁
        if let Some(existing) = self.rate_limiters.get(&key) {
            let existing_limiter = existing.value();
            // 参数一致性校验（Rule 12：失败必须显性化）
            // audit-H-001：panic 消息中 key 已脱敏
            assert!(
                existing_limiter.capacity() == amount
                    && existing_limiter.refill_rate() == refill_rate,
                "LimiterManager: rate limiter key '{}' already exists with different params (existing: capacity={}, refill_rate={}; new: capacity={}, refill_rate={})",
                redact_key(&key),
                existing_limiter.capacity(),
                existing_limiter.refill_rate(),
                amount,
                refill_rate
            );
            // 更新访问时间（LRU 用）
            // audit-C-001：用 AtomicU64::store 无锁更新，避免 insert 写锁 + 堆分配
            if let Some(t) = self.rate_access_times.get(&key) {
                t.store(now_nanos(), Ordering::Relaxed);
            } else {
                // 极端情况：limiters 中有但 access_times 中无（如并发 cleanup 短暂删除）
                // 用 entry() 补回，确保一致性
                self.rate_access_times
                    .entry(key.clone())
                    .or_insert_with(|| AtomicU64::new(now_nanos()));
            }
            return existing_limiter.clone();
        }

        // 慢路径：entry().or_insert_with().clone() 模式
        // audit-H-002：返回 DashMap 中的 Arc 而非本地创建的，避免并发场景下限流绕过
        let limiter = self
            .rate_limiters
            .entry(key.clone())
            .or_insert_with(|| Arc::new(TokenBucketLimiter::new(amount, refill_rate)))
            .clone();
        self.rate_access_times
            .entry(key.clone())
            .or_insert_with(|| AtomicU64::new(now_nanos()));

        // LRU 检查：超过阈值则触发 cleanup
        // audit-H-002：用 AtomicBool CAS 限制并发 cleanup，避免同步阻塞请求
        if self.rate_limiters.len() > CLEANUP_THRESHOLD {
            if self
                .rate_cleanup_in_progress
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.cleanup_rate_limiters();
                self.rate_cleanup_in_progress
                    .store(false, Ordering::Release);
            }
        }
        limiter
    }

    /// 获取或创建 quota limiter
    ///
    /// # 参数
    /// - `key`: 限流器 key
    /// - `period`: 配额周期（如 3600 秒 = 1 小时）
    /// - `max`: 配额上限
    ///
    /// # 返回
    /// `Arc<QuotaLimiter>`，可跨线程共享
    ///
    /// # Panic
    ///
    /// 同 key 但参数（max / period）不一致时 panic（Rule 12：失败必须显性化）。
    /// panic 消息中 key 已脱敏（audit-H-001）。
    #[cfg(feature = "quota-control")]
    pub fn get_quota_limiter(
        &self,
        key: &str,
        period: std::time::Duration,
        max: u64,
    ) -> Arc<QuotaLimiter> {
        let key = key.to_string(); // audit-L-001：缓存一次

        // 快速路径：get() 读锁
        if let Some(existing) = self.quota_limiters.get(&key) {
            let existing_limiter = existing.value();
            // 参数一致性校验（Rule 12：失败必须显性化）
            // audit-H-001：panic 消息中 key 已脱敏
            assert!(
                existing_limiter.max() == max && existing_limiter.period() == period,
                "LimiterManager: quota limiter key '{}' already exists with different params (existing: max={}, period={:?}; new: max={}, period={:?})",
                redact_key(&key),
                existing_limiter.max(),
                existing_limiter.period(),
                max,
                period
            );
            // 更新访问时间（audit-C-001：无锁 store）
            if let Some(t) = self.quota_access_times.get(&key) {
                t.store(now_nanos(), Ordering::Relaxed);
            } else {
                self.quota_access_times
                    .entry(key.clone())
                    .or_insert_with(|| AtomicU64::new(now_nanos()));
            }
            return existing_limiter.clone();
        }

        // 慢路径：entry().or_insert_with().clone()（audit-H-002）
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: max,
            window_size: period.as_secs(),
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig::default(),
        };
        let limiter = self
            .quota_limiters
            .entry(key.clone())
            .or_insert_with(|| Arc::new(QuotaLimiter::new(config)))
            .clone();
        self.quota_access_times
            .entry(key.clone())
            .or_insert_with(|| AtomicU64::new(now_nanos()));

        // LRU 检查（audit-H-002：AtomicBool CAS 限制并发）
        if self.quota_limiters.len() > CLEANUP_THRESHOLD {
            if self
                .quota_cleanup_in_progress
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.cleanup_quota_limiters();
                self.quota_cleanup_in_progress
                    .store(false, Ordering::Release);
            }
        }
        limiter
    }

    /// 获取或创建 concurrency limiter
    ///
    /// # 参数
    /// - `key`: 限流器 key
    /// - `max_concurrent`: 最大并发数
    ///
    /// # 返回
    /// `Arc<ConcurrencyLimiter>`，可跨线程共享
    ///
    /// # Panic
    ///
    /// 同 key 但参数（max_concurrent）不一致时 panic（Rule 12：失败必须显性化）。
    /// panic 消息中 key 已脱敏（audit-H-001）。
    pub fn get_concurrency_limiter(
        &self,
        key: &str,
        max_concurrent: u64,
    ) -> Arc<ConcurrencyLimiter> {
        let key = key.to_string(); // audit-L-001：缓存一次

        // 快速路径：get() 读锁
        if let Some(existing) = self.concurrency_limiters.get(&key) {
            let existing_limiter = existing.value();
            // 参数一致性校验（Rule 12：失败必须显性化）
            // audit-H-001：panic 消息中 key 已脱敏
            assert!(
                existing_limiter.max_concurrent() == max_concurrent,
                "LimiterManager: concurrency limiter key '{}' already exists with different params (existing: max_concurrent={}; new: max_concurrent={})",
                redact_key(&key),
                existing_limiter.max_concurrent(),
                max_concurrent
            );
            // 更新访问时间（audit-C-001：无锁 store）
            if let Some(t) = self.concurrency_access_times.get(&key) {
                t.store(now_nanos(), Ordering::Relaxed);
            } else {
                self.concurrency_access_times
                    .entry(key.clone())
                    .or_insert_with(|| AtomicU64::new(now_nanos()));
            }
            return existing_limiter.clone();
        }

        // 慢路径：entry().or_insert_with().clone()（audit-H-002）
        let limiter = self
            .concurrency_limiters
            .entry(key.clone())
            .or_insert_with(|| Arc::new(ConcurrencyLimiter::new(max_concurrent)))
            .clone();
        self.concurrency_access_times
            .entry(key.clone())
            .or_insert_with(|| AtomicU64::new(now_nanos()));

        // LRU 检查（audit-H-002：AtomicBool CAS 限制并发）
        if self.concurrency_limiters.len() > CLEANUP_THRESHOLD {
            if self
                .concurrency_cleanup_in_progress
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.cleanup_concurrency_limiters();
                self.concurrency_cleanup_in_progress
                    .store(false, Ordering::Release);
            }
        }
        limiter
    }

    /// 清空所有缓存的限流器（仅供测试使用）
    ///
    /// 生产代码不应调用此方法——`LimiterManager` 设计为单例累积缓存，
    /// 由 LRU 机制（`cleanup_*_limiters`）自动管理容量。
    /// 仅在单元测试中重置全局状态时使用，避免测试间相互污染。
    #[cfg(test)]
    pub fn clear_for_test(&self) {
        self.rate_limiters.clear();
        self.rate_access_times.clear();
        #[cfg(feature = "quota-control")]
        self.quota_limiters.clear();
        #[cfg(feature = "quota-control")]
        self.quota_access_times.clear();
        self.concurrency_limiters.clear();
        self.concurrency_access_times.clear();
    }

    /// LRU 清理：按访问时间淘汰最旧的 (1 - CLEANUP_RATIO) 比例外的条目
    ///
    /// 当 `rate_limiters.len()` 超过 `CLEANUP_THRESHOLD` 时被调用，
    /// 委托给 `cleanup_lru` 泛型函数（audit-M-001 DRY）。
    fn cleanup_rate_limiters(&self) {
        self.cleanup_rate_limiters_to(MAX_LIMITER_ENTRIES);
    }

    /// LRU 清理的可注入版本（`max_entries` 用于测试时调小阈值）
    ///
    /// 计算 `target_count = max_entries * (1 - CLEANUP_RATIO)`，
    /// 当 `entries.len() > target_count` 时移除最旧的条目。
    fn cleanup_rate_limiters_to(&self, max_entries: usize) {
        cleanup_lru(&self.rate_limiters, &self.rate_access_times, max_entries);
    }

    /// LRU 清理 quota limiters（cfg-gated 同 quota_limiters 字段）
    #[cfg(feature = "quota-control")]
    fn cleanup_quota_limiters(&self) {
        self.cleanup_quota_limiters_to(MAX_LIMITER_ENTRIES);
    }

    #[cfg(feature = "quota-control")]
    fn cleanup_quota_limiters_to(&self, max_entries: usize) {
        cleanup_lru(&self.quota_limiters, &self.quota_access_times, max_entries);
    }

    /// LRU 清理 concurrency limiters
    fn cleanup_concurrency_limiters(&self) {
        self.cleanup_concurrency_limiters_to(MAX_LIMITER_ENTRIES);
    }

    fn cleanup_concurrency_limiters_to(&self, max_entries: usize) {
        cleanup_lru(
            &self.concurrency_limiters,
            &self.concurrency_access_times,
            max_entries,
        );
    }

    /// 获取 rate limiter 缓存数量（audit-M-003：cfg-gate 仅测试可用）
    #[cfg(test)]
    pub fn rate_limiter_count(&self) -> usize {
        self.rate_limiters.len()
    }

    /// 获取 quota limiter 缓存数量（audit-M-003：cfg-gate 仅测试可用）
    #[cfg(feature = "quota-control")]
    #[cfg(test)]
    pub fn quota_limiter_count(&self) -> usize {
        self.quota_limiters.len()
    }

    /// 获取 concurrency limiter 缓存数量（audit-M-003：cfg-gate 仅测试可用）
    #[cfg(test)]
    pub fn concurrency_limiter_count(&self) -> usize {
        self.concurrency_limiters.len()
    }
}

impl Default for LimiterManager {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for LimiterManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LimiterManager")
            .field("rate_limiters_count", &self.rate_limiters.len())
            .field(
                "quota_limiters_count",
                #[cfg(feature = "quota-control")]
                &self.quota_limiters.len(),
                #[cfg(not(feature = "quota-control"))]
                &0usize,
            )
            .field(
                "concurrency_limiters_count",
                &self.concurrency_limiters.len(),
            )
            .finish()
    }
}

/// 全局限流器管理器单例
///
/// 通过 `std::sync::LazyLock` 实现线程安全的延迟初始化。
/// 首次访问时创建 `LimiterManager` 实例。
///
/// # 使用
///
/// ```rust
/// use limiteron::limiters::manager::GLOBAL_LIMITER_MANAGER;
/// use limiteron::limiters::Limiter;
///
/// # #[tokio::main]
/// # async fn main() {
/// let limiter = GLOBAL_LIMITER_MANAGER.get_rate_limiter("user:123", 100, 1);
/// assert!(limiter.allow(1).await.unwrap());
/// # }
/// ```
pub static GLOBAL_LIMITER_MANAGER: LazyLock<LimiterManager> = LazyLock::new(LimiterManager::new);

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limiters::Limiter;

    #[test]
    fn test_limiter_manager_new() {
        let manager = LimiterManager::new();
        assert_eq!(manager.rate_limiter_count(), 0);
        assert_eq!(manager.concurrency_limiter_count(), 0);
        #[cfg(feature = "quota-control")]
        assert_eq!(manager.quota_limiter_count(), 0);
    }

    #[test]
    fn test_limiter_manager_default() {
        let manager = LimiterManager::default();
        assert_eq!(manager.rate_limiter_count(), 0);
    }

    #[test]
    fn test_get_rate_limiter_caches_by_key() {
        let manager = LimiterManager::new();
        let l1 = manager.get_rate_limiter("key1", 100, 1);
        let l2 = manager.get_rate_limiter("key1", 100, 1);
        // 同 key 应返回同一实例（Arc 指针相等）
        assert!(Arc::ptr_eq(&l1, &l2));
        assert_eq!(manager.rate_limiter_count(), 1);

        // 不同 key 应返回不同实例
        let l3 = manager.get_rate_limiter("key2", 100, 1);
        assert!(!Arc::ptr_eq(&l1, &l3));
        assert_eq!(manager.rate_limiter_count(), 2);
    }

    #[tokio::test]
    async fn test_rate_limiter_allow_within_limit() {
        let manager = LimiterManager::new();
        // capacity=10, refill_rate=10/s
        let limiter = manager.get_rate_limiter("test_allow", 10, 1);
        for _ in 0..10 {
            assert!(limiter.allow(1).await.unwrap(), "应允许 10 个请求");
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_refill_rate_calculation() {
        let manager = LimiterManager::new();
        // rate="100/s" → capacity=100, refill_rate=100
        let limiter_s = manager.get_rate_limiter("per_sec", 100, 1);
        // capacity=100，应允许 100 个请求
        for _ in 0..100 {
            assert!(limiter_s.allow(1).await.unwrap());
        }

        // rate="100/m" → capacity=100, refill_rate=max(1, 100/60)=1
        let limiter_m = manager.get_rate_limiter("per_min", 100, 60);
        // capacity=100，应允许 100 个请求
        for _ in 0..100 {
            assert!(limiter_m.allow(1).await.unwrap());
        }

        // rate="100/h" → capacity=100, refill_rate=max(1, 100/3600)=1
        let limiter_h = manager.get_rate_limiter("per_hour", 100, 3600);
        for _ in 0..100 {
            assert!(limiter_h.allow(1).await.unwrap());
        }
    }

    #[test]
    fn test_get_rate_limiter_zero_unit_secs_uses_amount_as_refill() {
        let manager = LimiterManager::new();
        // unit_secs=0 → refill_rate=amount（fallback）
        let limiter = manager.get_rate_limiter("zero_unit", 50, 0);
        // 验证 limiter 可用
        assert_eq!(manager.rate_limiter_count(), 1);
        // 不验证具体行为，因为 unit_secs=0 是异常输入
        let _ = limiter;
    }

    #[cfg(feature = "quota-control")]
    #[test]
    fn test_get_quota_limiter_caches_by_key() {
        let manager = LimiterManager::new();
        let l1 = manager.get_quota_limiter("qkey1", std::time::Duration::from_secs(3600), 1000);
        let l2 = manager.get_quota_limiter("qkey1", std::time::Duration::from_secs(3600), 1000);
        assert!(Arc::ptr_eq(&l1, &l2));
        assert_eq!(manager.quota_limiter_count(), 1);
    }

    #[cfg(feature = "quota-control")]
    #[tokio::test]
    async fn test_quota_limiter_check_consumes_quota() {
        use crate::limiters::Limiter;
        let manager = LimiterManager::new();
        let limiter =
            manager.get_quota_limiter("quota_test", std::time::Duration::from_secs(3600), 3);

        // 前 3 个请求应成功
        for i in 0..3 {
            assert!(
                limiter.check("quota_test").await.is_ok(),
                "请求 {} 应成功",
                i
            );
        }
        // 第 4 个请求应失败（QuotaExceeded）
        assert!(
            limiter.check("quota_test").await.is_err(),
            "第 4 个请求应失败"
        );
    }

    #[test]
    fn test_get_concurrency_limiter_caches_by_key() {
        let manager = LimiterManager::new();
        let l1 = manager.get_concurrency_limiter("ckey1", 10);
        let l2 = manager.get_concurrency_limiter("ckey1", 10);
        assert!(Arc::ptr_eq(&l1, &l2));
        assert_eq!(manager.concurrency_limiter_count(), 1);
    }

    #[tokio::test]
    async fn test_concurrency_limiter_acquire_and_release() {
        let manager = LimiterManager::new();
        let limiter = manager.get_concurrency_limiter("conc_test_release", 2);

        // 占用全部 2 个 slot
        let p1 = limiter.acquire(1).await.unwrap();
        let p2 = limiter.acquire(1).await.unwrap();

        // 第 3 次 acquire 应阻塞（因已满），用 timeout 验证不会立即返回
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), limiter.acquire(1)).await;
        assert!(
            result.is_err(),
            "3rd acquire should block (timeout expected)"
        );

        // 释放 p1 后，第 3 次 acquire 应立即成功
        drop(p1);
        let p3 = limiter
            .acquire(1)
            .await
            .expect("acquire after release should succeed");
        drop(p2);
        drop(p3);
    }

    #[test]
    fn test_clear_empties_all_caches() {
        let manager = LimiterManager::new();
        manager.get_rate_limiter("r1", 100, 1);
        manager.get_concurrency_limiter("c1", 10);
        #[cfg(feature = "quota-control")]
        manager.get_quota_limiter("q1", std::time::Duration::from_secs(3600), 100);

        assert_eq!(manager.rate_limiter_count(), 1);
        assert_eq!(manager.concurrency_limiter_count(), 1);
        #[cfg(feature = "quota-control")]
        assert_eq!(manager.quota_limiter_count(), 1);

        manager.clear_for_test();

        assert_eq!(manager.rate_limiter_count(), 0);
        assert_eq!(manager.concurrency_limiter_count(), 0);
        #[cfg(feature = "quota-control")]
        assert_eq!(manager.quota_limiter_count(), 0);
    }

    #[test]
    fn test_debug_format() {
        let manager = LimiterManager::new();
        manager.get_rate_limiter("debug_test", 100, 1);
        let debug_str = format!("{:?}", manager);
        assert!(debug_str.contains("LimiterManager"));
        assert!(debug_str.contains("rate_limiters_count"));
    }

    #[test]
    fn test_global_limiter_manager_is_accessible() {
        // 验证全局单例可访问
        let limiter = GLOBAL_LIMITER_MANAGER.get_rate_limiter("global_test", 1, 1);
        assert_eq!(GLOBAL_LIMITER_MANAGER.rate_limiter_count(), 1);
        // 清理全局状态（避免影响其他测试）
        GLOBAL_LIMITER_MANAGER.clear_for_test();
        assert_eq!(GLOBAL_LIMITER_MANAGER.rate_limiter_count(), 0);
        let _ = limiter;
    }

    #[tokio::test]
    async fn test_global_limiter_manager_rate_limiter_works() {
        GLOBAL_LIMITER_MANAGER.clear_for_test();
        let limiter = GLOBAL_LIMITER_MANAGER.get_rate_limiter("global_rate_test", 5, 1);
        // capacity=5, 应允许 5 个请求
        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }
        GLOBAL_LIMITER_MANAGER.clear_for_test();
    }

    // ========================================================================
    // T008: get_rate_limiter 同 key 不同参数 panic
    // ========================================================================

    #[test]
    #[should_panic(expected = "rate limiter key '")]
    fn test_get_rate_limiter_param_mismatch_panics() {
        let manager = LimiterManager::new();
        // 首次创建：capacity=100, refill_rate=100
        let _ = manager.get_rate_limiter("mismatch_rate", 100, 1);
        // 同 key 不同参数（capacity 不同）应 panic
        let _ = manager.get_rate_limiter("mismatch_rate", 200, 1);
    }

    // ========================================================================
    // T009: get_quota_limiter / get_concurrency_limiter 同 key 不同参数 panic
    // ========================================================================

    #[cfg(feature = "quota-control")]
    #[test]
    #[should_panic(expected = "quota limiter key '")]
    fn test_get_quota_limiter_param_mismatch_panics() {
        let manager = LimiterManager::new();
        let _ =
            manager.get_quota_limiter("mismatch_quota", std::time::Duration::from_secs(3600), 100);
        // 同 key 不同 max 应 panic
        let _ =
            manager.get_quota_limiter("mismatch_quota", std::time::Duration::from_secs(3600), 200);
    }

    #[test]
    #[should_panic(expected = "concurrency limiter key '")]
    fn test_get_concurrency_limiter_param_mismatch_panics() {
        let manager = LimiterManager::new();
        let _ = manager.get_concurrency_limiter("mismatch_conc", 10);
        // 同 key 不同 max_concurrent 应 panic
        let _ = manager.get_concurrency_limiter("mismatch_conc", 20);
    }

    // ========================================================================
    // T010: LRU 淘汰 + 访问时间更新
    // ========================================================================

    #[test]
    fn test_lru_eviction_when_capacity_exceeded() {
        // 验证 LRU 淘汰：用 cleanup_rate_limiters_to(10) 注入小阈值
        // MAX=10, CLEANUP_RATIO=0.1 → target=9
        // 填充 11 条 entry 后调用 cleanup，应移除最旧的 2 条，剩 9 条
        let manager = LimiterManager::new();

        // 填充 11 条 entry，每条 sleep 保证访问时间不同（用于 LRU 排序）
        for i in 0..11 {
            manager.get_rate_limiter(&format!("lru_key_{:02}", i), 100, 1);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(manager.rate_limiter_count(), 11);
        assert_eq!(manager.rate_access_times.len(), 11);

        // 触发清理（注入 max_entries=10）
        manager.cleanup_rate_limiters_to(10);

        // 验证清理后只剩 9 条
        assert_eq!(
            manager.rate_limiter_count(),
            9,
            "cleanup should leave 9 entries (target = 10 * 0.9)"
        );
        assert_eq!(manager.rate_access_times.len(), 9);

        // 验证最旧的 2 条被淘汰（lru_key_00 和 lru_key_01）
        assert!(
            !manager.rate_limiters.contains_key("lru_key_00"),
            "lru_key_00 (oldest) should be evicted"
        );
        assert!(
            !manager.rate_limiters.contains_key("lru_key_01"),
            "lru_key_01 (2nd oldest) should be evicted"
        );

        // 验证最新的 2 条仍存在
        assert!(
            manager.rate_limiters.contains_key("lru_key_10"),
            "lru_key_10 (newest) should remain"
        );
        assert!(
            manager.rate_limiters.contains_key("lru_key_09"),
            "lru_key_09 should remain"
        );

        // 不变式：rate_limiters 和 rate_access_times 长度一致
        assert_eq!(
            manager.rate_limiters.len(),
            manager.rate_access_times.len(),
            "rate_limiters and rate_access_times must stay consistent after cleanup"
        );
    }

    #[test]
    fn test_lru_cleanup_noop_when_below_target() {
        // 边界条件：entries 数量 < target_count 时 cleanup 是 no-op
        let manager = LimiterManager::new();
        manager.get_rate_limiter("below_1", 100, 1);
        manager.get_rate_limiter("below_2", 100, 1);
        manager.get_rate_limiter("below_3", 100, 1);

        // 调用 cleanup（即使 max_entries=10, target=9, entries=3 < 9，应不清理）
        manager.cleanup_rate_limiters_to(10);

        assert_eq!(manager.rate_limiter_count(), 3, "no-op when below target");
        assert_eq!(manager.rate_access_times.len(), 3);
    }

    #[test]
    fn test_lru_last_access_updated_on_get() {
        // 验证每次 get_* 时 access_time 被更新
        let manager = LimiterManager::new();

        // 首次 get，记录初始访问时间
        manager.get_rate_limiter("lru_update_test", 100, 1);
        let first_access = manager
            .rate_access_times
            .get("lru_update_test")
            .expect("first access_time should be recorded")
            .load(Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(5));

        // 再次 get 同 key，应更新访问时间
        manager.get_rate_limiter("lru_update_test", 100, 1);
        let second_access = manager
            .rate_access_times
            .get("lru_update_test")
            .expect("second access_time should be recorded")
            .load(Ordering::Relaxed);

        assert!(
            second_access > first_access,
            "access_time should be updated on get: first={}, second={}",
            first_access,
            second_access
        );

        // 验证 entries 数量仍为 1（同 key 不应新增）
        assert_eq!(manager.rate_limiter_count(), 1);
        assert_eq!(manager.rate_access_times.len(), 1);
    }

    // ========================================================================
    // audit-macro-followup 修复9 (M-006): 并发场景 LRU 一致性测试
    // ========================================================================

    #[test]
    fn test_concurrent_get_rate_limiter_consistency() {
        // audit-M-006：并发场景下多线程 get 同 key 应返回同一 Arc 实例
        // 验证 entry().or_insert_with().clone() 模式不会让并发线程绕过限流
        let manager = LimiterManager::new();
        let key = "concurrent_test_key";

        // 10 线程并发 get 同 key（用 std::thread::scope 避免 &'static 约束）
        std::thread::scope(|s| {
            let mut handles = vec![];
            for _ in 0..10 {
                handles.push(s.spawn(|| manager.get_rate_limiter(key, 100, 1)));
            }

            let limiters: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
            // 所有线程应返回同一 Arc 实例（audit-H-002 修复后）
            for l in &limiters[1..] {
                assert!(
                    Arc::ptr_eq(&limiters[0], l),
                    "all threads should get same Arc (audit-H-002); got different Arc"
                );
            }
        });

        // 验证 cache 中只有 1 条 entry
        assert_eq!(
            manager.rate_limiter_count(),
            1,
            "concurrent get should not create duplicate entries"
        );
        assert_eq!(manager.rate_access_times.len(), 1);
    }

    #[cfg(feature = "quota-control")]
    #[test]
    fn test_concurrent_get_quota_limiter_consistency() {
        let manager = LimiterManager::new();
        let key = "concurrent_quota_key";

        std::thread::scope(|s| {
            let mut handles = vec![];
            for _ in 0..10 {
                handles.push(s.spawn(|| {
                    manager.get_quota_limiter(key, std::time::Duration::from_secs(3600), 1000)
                }));
            }

            let limiters: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
            for l in &limiters[1..] {
                assert!(
                    Arc::ptr_eq(&limiters[0], l),
                    "all threads should get same Arc (audit-H-002); got different Arc"
                );
            }
        });

        assert_eq!(
            manager.quota_limiter_count(),
            1,
            "concurrent get should not create duplicate entries"
        );
        assert_eq!(manager.quota_access_times.len(), 1);
    }

    #[test]
    fn test_concurrent_get_concurrency_limiter_consistency() {
        let manager = LimiterManager::new();
        let key = "concurrent_conc_key";

        std::thread::scope(|s| {
            let mut handles = vec![];
            for _ in 0..10 {
                handles.push(s.spawn(|| manager.get_concurrency_limiter(key, 10)));
            }

            let limiters: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
            for l in &limiters[1..] {
                assert!(
                    Arc::ptr_eq(&limiters[0], l),
                    "all threads should get same Arc (audit-H-002); got different Arc"
                );
            }
        });

        assert_eq!(
            manager.concurrency_limiter_count(),
            1,
            "concurrent get should not create duplicate entries"
        );
        assert_eq!(manager.concurrency_access_times.len(), 1);
    }

    // ========================================================================
    // audit-macro-followup 修复10 (M-007): quota / concurrency cleanup 测试
    // ========================================================================

    #[cfg(feature = "quota-control")]
    #[test]
    fn test_lru_eviction_quota() {
        // 验证 quota limiter 的 LRU 淘汰（与 rate limiter 对称）
        let manager = LimiterManager::new();

        for i in 0..11 {
            manager.get_quota_limiter(
                &format!("quota_lru_{:02}", i),
                std::time::Duration::from_secs(3600),
                1000,
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(manager.quota_limiter_count(), 11);

        manager.cleanup_quota_limiters_to(10);

        // MAX=10, target=9, 应淘汰最旧的 2 条
        assert_eq!(
            manager.quota_limiter_count(),
            9,
            "cleanup should leave 9 entries"
        );
        assert_eq!(manager.quota_access_times.len(), 9);

        assert!(
            !manager.quota_limiters.contains_key("quota_lru_00"),
            "quota_lru_00 (oldest) should be evicted"
        );
        assert!(
            !manager.quota_limiters.contains_key("quota_lru_01"),
            "quota_lru_01 (2nd oldest) should be evicted"
        );
        assert!(
            manager.quota_limiters.contains_key("quota_lru_10"),
            "quota_lru_10 (newest) should remain"
        );

        // 不变式：limiters 和 access_times 长度一致
        assert_eq!(
            manager.quota_limiters.len(),
            manager.quota_access_times.len(),
            "quota limiters and access_times must stay consistent after cleanup"
        );
    }

    #[test]
    fn test_lru_eviction_concurrency() {
        // 验证 concurrency limiter 的 LRU 淘汰（与 rate limiter 对称）
        let manager = LimiterManager::new();

        for i in 0..11 {
            manager.get_concurrency_limiter(&format!("conc_lru_{:02}", i), 10);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(manager.concurrency_limiter_count(), 11);

        manager.cleanup_concurrency_limiters_to(10);

        assert_eq!(
            manager.concurrency_limiter_count(),
            9,
            "cleanup should leave 9 entries"
        );
        assert_eq!(manager.concurrency_access_times.len(), 9);

        assert!(
            !manager.concurrency_limiters.contains_key("conc_lru_00"),
            "conc_lru_00 (oldest) should be evicted"
        );
        assert!(
            !manager.concurrency_limiters.contains_key("conc_lru_01"),
            "conc_lru_01 (2nd oldest) should be evicted"
        );
        assert!(
            manager.concurrency_limiters.contains_key("conc_lru_10"),
            "conc_lru_10 (newest) should remain"
        );

        assert_eq!(
            manager.concurrency_limiters.len(),
            manager.concurrency_access_times.len(),
            "concurrency limiters and access_times must stay consistent after cleanup"
        );
    }

    // ========================================================================
    // audit-macro-followup 修复2 (H-001): redact_key 单元测试
    // ========================================================================

    #[test]
    fn test_redact_key_short() {
        // 短 key（≤ 16）仅暴露字符数，不暴露原文
        let redacted = redact_key("short_key");
        assert_eq!(redacted, "<9 chars>");
        // 不应包含原文
        assert!(!redacted.contains("short_key"));
    }

    #[test]
    fn test_redact_key_long() {
        // 长 key（> 16）暴露前 8 字符 + 总长度
        // "user_id_123456789_long_key" 长度 = 8 + 9 + 9 = 26
        let redacted = redact_key("user_id_123456789_long_key");
        assert_eq!(redacted, "user_id_...(26 chars)");
        // 不应包含完整原文
        assert!(!redacted.contains("user_id_123456789_long_key"));
    }

    #[test]
    fn test_redact_key_unicode_safe() {
        // Unicode 字符边界安全：取前 8 个 char（不是 8 字节）
        let redacted = redact_key("中文用户_1234567890_extra_long");
        // 应该正常返回，不 panic
        assert!(redacted.contains("..."));
        assert!(redacted.contains("chars"));
    }

    #[test]
    fn test_redact_key_boundary() {
        // 边界：恰好 16 字符
        let key_16 = "0123456789abcdef"; // 16 chars
        assert_eq!(redact_key(key_16), "<16 chars>");
        // 17 字符
        let key_17 = "0123456789abcdefg"; // 17 chars
        let redacted = redact_key(key_17);
        assert!(redacted.contains("...(17 chars)"));
        assert!(redacted.contains("01234567")); // 前 8 字符
    }
}
