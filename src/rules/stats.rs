// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 统计管理器模块
//!
//! 提供集中化的统计信息管理，将统计逻辑从 Governor 中分离出来。
//!
//! # 功能
//!
//! - 请求计数统计（总数、允许、拒绝、封禁、错误）
//! - 统计信息快照
//! - 统计重置

use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicU64, Ordering};

/// 统计信息快照
///
/// 表示某一时刻的统计数据快照。
#[derive(Debug, Clone, Default)]
pub struct StatsSnapshot {
    /// 总请求数
    pub total_requests: u64,
    /// 允许的请求数
    pub allowed_requests: u64,
    /// 拒绝的请求数
    pub rejected_requests: u64,
    /// 封禁的请求数
    pub banned_requests: u64,
    /// 错误数
    pub error_count: u64,
    /// 最后更新时间
    pub last_updated: Option<DateTime<Utc>>,
}

/// 统计管理器
///
/// 管理请求统计信息，提供原子操作确保线程安全。
///
/// # 特性
///
/// - 使用原子计数器实现无锁统计
/// - 支持统计快照
/// - 支持统计重置
///
/// # 示例
///
/// ```rust
/// use limiteron::rules::stats::StatsManager;
///
/// let stats = StatsManager::new();
///
/// // 记录请求
/// stats.increment_total();
/// stats.increment_allowed();
///
/// // 获取统计快照
/// let snapshot = stats.snapshot();
/// assert_eq!(snapshot.total_requests, 1);
/// assert_eq!(snapshot.allowed_requests, 1);
/// ```
pub struct StatsManager {
    /// 总请求数
    total_requests: AtomicU64,
    /// 允许的请求数
    allowed_requests: AtomicU64,
    /// 拒绝的请求数
    rejected_requests: AtomicU64,
    /// 封禁的请求数
    banned_requests: AtomicU64,
    /// 错误数
    error_count: AtomicU64,
}

impl StatsManager {
    /// 创建新的统计管理器
    ///
    /// # 返回
    ///
    /// 初始化所有计数器为 0 的新统计管理器
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// ```
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            allowed_requests: AtomicU64::new(0),
            rejected_requests: AtomicU64::new(0),
            banned_requests: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
        }
    }

    /// 使用初始值创建统计管理器
    ///
    /// # 参数
    ///
    /// - `total`: 总请求数初始值
    /// - `allowed`: 允许请求数初始值
    /// - `rejected`: 拒绝请求数初始值
    /// - `banned`: 封禁请求数初始值
    /// - `error`: 错误数初始值
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::with_values(100, 80, 10, 5, 5);
    /// ```
    pub fn with_values(total: u64, allowed: u64, rejected: u64, banned: u64, error: u64) -> Self {
        Self {
            total_requests: AtomicU64::new(total),
            allowed_requests: AtomicU64::new(allowed),
            rejected_requests: AtomicU64::new(rejected),
            banned_requests: AtomicU64::new(banned),
            error_count: AtomicU64::new(error),
        }
    }

    /// 增加总请求数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_total();
    /// assert_eq!(stats.snapshot().total_requests, 1);
    /// ```
    #[inline]
    pub fn increment_total(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// 增加允许请求数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_allowed();
    /// assert_eq!(stats.snapshot().allowed_requests, 1);
    /// ```
    #[inline]
    pub fn increment_allowed(&self) {
        self.allowed_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// 增加拒绝请求数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_rejected();
    /// assert_eq!(stats.snapshot().rejected_requests, 1);
    /// ```
    #[inline]
    pub fn increment_rejected(&self) {
        self.rejected_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// 增加封禁请求数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_banned();
    /// assert_eq!(stats.snapshot().banned_requests, 1);
    /// ```
    #[inline]
    pub fn increment_banned(&self) {
        self.banned_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// 增加错误数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_error();
    /// assert_eq!(stats.snapshot().error_count, 1);
    /// ```
    #[inline]
    pub fn increment_error(&self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 获取当前统计快照
    ///
    /// # 返回
    ///
    /// 包含当前所有统计数据的快照
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_total();
    /// stats.increment_allowed();
    ///
    /// let snapshot = stats.snapshot();
    /// assert_eq!(snapshot.total_requests, 1);
    /// assert_eq!(snapshot.allowed_requests, 1);
    /// ```
    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            total_requests: self.total_requests.load(Ordering::Relaxed),
            allowed_requests: self.allowed_requests.load(Ordering::Relaxed),
            rejected_requests: self.rejected_requests.load(Ordering::Relaxed),
            banned_requests: self.banned_requests.load(Ordering::Relaxed),
            error_count: self.error_count.load(Ordering::Relaxed),
            last_updated: Some(Utc::now()),
        }
    }

    /// 重置所有统计计数器
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_total();
    /// stats.increment_allowed();
    ///
    /// stats.reset();
    ///
    /// let snapshot = stats.snapshot();
    /// assert_eq!(snapshot.total_requests, 0);
    /// assert_eq!(snapshot.allowed_requests, 0);
    /// ```
    pub fn reset(&self) {
        self.total_requests.store(0, Ordering::Relaxed);
        self.allowed_requests.store(0, Ordering::Relaxed);
        self.rejected_requests.store(0, Ordering::Relaxed);
        self.banned_requests.store(0, Ordering::Relaxed);
        self.error_count.store(0, Ordering::Relaxed);
    }

    /// 获取总请求数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_total();
    /// assert_eq!(stats.total(), 1);
    /// ```
    #[inline]
    pub fn total(&self) -> u64 {
        self.total_requests.load(Ordering::Relaxed)
    }

    /// 获取允许请求数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_allowed();
    /// assert_eq!(stats.allowed(), 1);
    /// ```
    #[inline]
    pub fn allowed(&self) -> u64 {
        self.allowed_requests.load(Ordering::Relaxed)
    }

    /// 获取拒绝请求数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_rejected();
    /// assert_eq!(stats.rejected(), 1);
    /// ```
    #[inline]
    pub fn rejected(&self) -> u64 {
        self.rejected_requests.load(Ordering::Relaxed)
    }

    /// 获取封禁请求数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_banned();
    /// assert_eq!(stats.banned(), 1);
    /// ```
    #[inline]
    pub fn banned(&self) -> u64 {
        self.banned_requests.load(Ordering::Relaxed)
    }

    /// 获取错误数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_error();
    /// assert_eq!(stats.errors(), 1);
    /// ```
    #[inline]
    pub fn errors(&self) -> u64 {
        self.error_count.load(Ordering::Relaxed)
    }

    /// 计算允许率
    ///
    /// # 返回
    ///
    /// 允许请求占总请求的比率（0.0 到 1.0）
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_total();
    /// stats.increment_total();
    /// stats.increment_allowed();
    ///
    /// let rate = stats.allow_rate();
    /// assert_eq!(rate, 0.5);
    /// ```
    pub fn allow_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        self.allowed() as f64 / total as f64
    }

    /// 计算拒绝率
    ///
    /// # 返回
    ///
    /// 拒绝请求占总请求的比率（0.0 到 1.0）
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_total();
    /// stats.increment_total();
    /// stats.increment_rejected();
    ///
    /// let rate = stats.reject_rate();
    /// assert_eq!(rate, 0.5);
    /// ```
    pub fn reject_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        self.rejected() as f64 / total as f64
    }

    /// 计算封禁率
    ///
    /// # 返回
    ///
    /// 封禁请求占总请求的比率（0.0 到 1.0）
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_total();
    /// stats.increment_total();
    /// stats.increment_banned();
    ///
    /// let rate = stats.ban_rate();
    /// assert_eq!(rate, 0.5);
    /// ```
    pub fn ban_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        self.banned() as f64 / total as f64
    }

    /// 计算错误率
    ///
    /// # 返回
    ///
    /// 错误占总请求的比率（0.0 到 1.0）
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::rules::stats::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_total();
    /// stats.increment_total();
    /// stats.increment_error();
    ///
    /// let rate = stats.error_rate();
    /// assert_eq!(rate, 0.5);
    /// ```
    pub fn error_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        self.errors() as f64 / total as f64
    }
}

impl Default for StatsManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_manager_new() {
        let stats = StatsManager::new();
        let snapshot = stats.snapshot();

        assert_eq!(snapshot.total_requests, 0);
        assert_eq!(snapshot.allowed_requests, 0);
        assert_eq!(snapshot.rejected_requests, 0);
        assert_eq!(snapshot.banned_requests, 0);
        assert_eq!(snapshot.error_count, 0);
    }

    #[test]
    fn test_stats_manager_with_values() {
        let stats = StatsManager::with_values(100, 80, 10, 5, 5);
        let snapshot = stats.snapshot();

        assert_eq!(snapshot.total_requests, 100);
        assert_eq!(snapshot.allowed_requests, 80);
        assert_eq!(snapshot.rejected_requests, 10);
        assert_eq!(snapshot.banned_requests, 5);
        assert_eq!(snapshot.error_count, 5);
    }

    #[test]
    fn test_stats_manager_increment() {
        let stats = StatsManager::new();

        stats.increment_total();
        stats.increment_allowed();
        stats.increment_rejected();
        stats.increment_banned();
        stats.increment_error();

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.total_requests, 1);
        assert_eq!(snapshot.allowed_requests, 1);
        assert_eq!(snapshot.rejected_requests, 1);
        assert_eq!(snapshot.banned_requests, 1);
        assert_eq!(snapshot.error_count, 1);
    }

    #[test]
    fn test_stats_manager_reset() {
        let stats = StatsManager::new();

        stats.increment_total();
        stats.increment_allowed();
        stats.increment_rejected();
        stats.increment_banned();
        stats.increment_error();

        stats.reset();

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.total_requests, 0);
        assert_eq!(snapshot.allowed_requests, 0);
        assert_eq!(snapshot.rejected_requests, 0);
        assert_eq!(snapshot.banned_requests, 0);
        assert_eq!(snapshot.error_count, 0);
    }

    #[test]
    fn test_stats_manager_individual_getters() {
        let stats = StatsManager::new();

        stats.increment_total();
        stats.increment_total();
        stats.increment_allowed();
        stats.increment_rejected();
        stats.increment_banned();
        stats.increment_error();

        assert_eq!(stats.total(), 2);
        assert_eq!(stats.allowed(), 1);
        assert_eq!(stats.rejected(), 1);
        assert_eq!(stats.banned(), 1);
        assert_eq!(stats.errors(), 1);
    }

    #[test]
    fn test_stats_manager_rates() {
        let stats = StatsManager::new();

        // 添加 10 个总请求
        for _ in 0..10 {
            stats.increment_total();
        }

        // 6 个允许
        for _ in 0..6 {
            stats.increment_allowed();
        }

        // 2 个拒绝
        for _ in 0..2 {
            stats.increment_rejected();
        }

        // 1 个封禁
        stats.increment_banned();

        // 1 个错误
        stats.increment_error();

        // 验证比率
        assert!((stats.allow_rate() - 0.6).abs() < 0.001);
        assert!((stats.reject_rate() - 0.2).abs() < 0.001);
        assert!((stats.ban_rate() - 0.1).abs() < 0.001);
        assert!((stats.error_rate() - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_stats_manager_rates_zero_total() {
        let stats = StatsManager::new();

        // 没有请求时，所有比率应该是 0
        assert_eq!(stats.allow_rate(), 0.0);
        assert_eq!(stats.reject_rate(), 0.0);
        assert_eq!(stats.ban_rate(), 0.0);
        assert_eq!(stats.error_rate(), 0.0);
    }

    #[test]
    fn test_stats_manager_concurrent_increment() {
        use std::sync::Arc;
        use std::thread;

        let stats = Arc::new(StatsManager::new());
        let mut handles = vec![];

        // 启动 10 个线程，每个线程增加 100 次
        for _ in 0..10 {
            let stats_clone = Arc::clone(&stats);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    stats_clone.increment_total();
                    stats_clone.increment_allowed();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // 验证最终计数
        assert_eq!(stats.total(), 1000);
        assert_eq!(stats.allowed(), 1000);
    }

    #[test]
    fn test_stats_snapshot_last_updated() {
        let stats = StatsManager::new();
        let snapshot = stats.snapshot();

        // 验证 last_updated 字段存在且是 Some
        assert!(snapshot.last_updated.is_some());
    }

    #[test]
    fn test_stats_manager_default() {
        let stats = StatsManager::default();
        let snapshot = stats.snapshot();

        assert_eq!(snapshot.total_requests, 0);
        assert_eq!(snapshot.allowed_requests, 0);
        assert_eq!(snapshot.rejected_requests, 0);
        assert_eq!(snapshot.banned_requests, 0);
        assert_eq!(snapshot.error_count, 0);
    }
}
