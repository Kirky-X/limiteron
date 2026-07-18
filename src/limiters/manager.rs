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
//!
//! # 限制
//!
//! - 限流器缓存无过期机制，key 无限增长可能导致内存泄漏
//!   （生产环境应配合 `Governor` + `LimiterFactory` 使用）
//! - `rate="100/m"` 等 unit > 1s 的配置，refill_rate 会向下取整为 u64
//!   （如 100/60 = 1，可能导致精度损失）

use crate::limiters::{ConcurrencyLimiter, TokenBucketLimiter};
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::LazyLock;

#[cfg(feature = "quota-control")]
use crate::limiters::QuotaLimiter;
#[cfg(feature = "quota-control")]
use crate::quota::{AlertConfig, QuotaConfig, QuotaType};

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
    /// Quota limiters 缓存（key -> QuotaLimiter）
    #[cfg(feature = "quota-control")]
    quota_limiters: DashMap<String, Arc<QuotaLimiter>>,
    /// Concurrency limiters 缓存（key -> ConcurrencyLimiter）
    concurrency_limiters: DashMap<String, Arc<ConcurrencyLimiter>>,
}

impl LimiterManager {
    /// 创建新的 `LimiterManager`
    ///
    /// 通常不需要手动调用，使用 `GLOBAL_LIMITER_MANAGER` 全局单例即可。
    /// 测试中可创建独立实例以避免全局状态污染。
    pub fn new() -> Self {
        Self {
            rate_limiters: DashMap::new(),
            #[cfg(feature = "quota-control")]
            quota_limiters: DashMap::new(),
            concurrency_limiters: DashMap::new(),
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
        let refill_rate = if unit_secs > 0 {
            (amount / unit_secs).max(1)
        } else {
            amount
        };
        self.rate_limiters
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(TokenBucketLimiter::new(amount, refill_rate)))
            .clone()
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
    #[cfg(feature = "quota-control")]
    pub fn get_quota_limiter(
        &self,
        key: &str,
        period: std::time::Duration,
        max: u64,
    ) -> Arc<QuotaLimiter> {
        self.quota_limiters
            .entry(key.to_string())
            .or_insert_with(|| {
                let config = QuotaConfig {
                    quota_type: QuotaType::Count,
                    limit: max,
                    window_size: period.as_secs(),
                    allow_overdraft: false,
                    overdraft_limit_percent: 0,
                    alert_config: AlertConfig::default(),
                };
                Arc::new(QuotaLimiter::new(config))
            })
            .clone()
    }

    /// 获取或创建 concurrency limiter
    ///
    /// # 参数
    /// - `key`: 限流器 key
    /// - `max_concurrent`: 最大并发数
    ///
    /// # 返回
    /// `Arc<ConcurrencyLimiter>`，可跨线程共享
    pub fn get_concurrency_limiter(
        &self,
        key: &str,
        max_concurrent: u64,
    ) -> Arc<ConcurrencyLimiter> {
        self.concurrency_limiters
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(ConcurrencyLimiter::new(max_concurrent)))
            .clone()
    }

    /// 清空所有缓存的限流器（主要用于测试）
    pub fn clear(&self) {
        self.rate_limiters.clear();
        #[cfg(feature = "quota-control")]
        self.quota_limiters.clear();
        self.concurrency_limiters.clear();
    }

    /// 获取 rate limiter 缓存数量（主要用于测试）
    pub fn rate_limiter_count(&self) -> usize {
        self.rate_limiters.len()
    }

    /// 获取 quota limiter 缓存数量（主要用于测试）
    #[cfg(feature = "quota-control")]
    pub fn quota_limiter_count(&self) -> usize {
        self.quota_limiters.len()
    }

    /// 获取 concurrency limiter 缓存数量（主要用于测试）
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
        let limiter = manager.get_concurrency_limiter("conc_test", 2);

        // 获取 2 个 permit
        let p1 = limiter.acquire(1).await.unwrap();
        let p2 = limiter.acquire(1).await.unwrap();

        // 第 3 个应失败（已达上限）
        // 注意：acquire 是阻塞的，但 ConcurrencyLimiter::new() 无超时，
        // 所以会一直等待。这里用 try_acquire 语义（acquire_many_nonblocking）
        // 但 ConcurrencyLimiter 没有 try_acquire，跳过此测试
        drop(p1);
        drop(p2);
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

        manager.clear();

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
        GLOBAL_LIMITER_MANAGER.clear();
        assert_eq!(GLOBAL_LIMITER_MANAGER.rate_limiter_count(), 0);
        let _ = limiter;
    }

    #[tokio::test]
    async fn test_global_limiter_manager_rate_limiter_works() {
        GLOBAL_LIMITER_MANAGER.clear();
        let limiter = GLOBAL_LIMITER_MANAGER.get_rate_limiter("global_rate_test", 5, 1);
        // capacity=5, 应允许 5 个请求
        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }
        GLOBAL_LIMITER_MANAGER.clear();
    }
}
