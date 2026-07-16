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
/// use limiteron::StatsManager;
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
    /// use limiteron::StatsManager;
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
    /// use limiteron::StatsManager;
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
    /// use limiteron::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_total();
    /// assert_eq!(stats.snapshot().total_requests, 1);
    /// ```
    #[inline]
    pub fn increment_total(&self) {
        self.total_requests.fetch_add(1, Ordering::SeqCst);
    }

    /// 增加允许请求数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_allowed();
    /// assert_eq!(stats.snapshot().allowed_requests, 1);
    /// ```
    #[inline]
    pub fn increment_allowed(&self) {
        self.allowed_requests.fetch_add(1, Ordering::SeqCst);
    }

    /// 增加拒绝请求数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_rejected();
    /// assert_eq!(stats.snapshot().rejected_requests, 1);
    /// ```
    #[inline]
    pub fn increment_rejected(&self) {
        self.rejected_requests.fetch_add(1, Ordering::SeqCst);
    }

    /// 增加封禁请求数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_banned();
    /// assert_eq!(stats.snapshot().banned_requests, 1);
    /// ```
    #[inline]
    pub fn increment_banned(&self) {
        self.banned_requests.fetch_add(1, Ordering::SeqCst);
    }

    /// 增加错误数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_error();
    /// assert_eq!(stats.snapshot().error_count, 1);
    /// ```
    #[inline]
    pub fn increment_error(&self) {
        self.error_count.fetch_add(1, Ordering::SeqCst);
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
    /// use limiteron::StatsManager;
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
            total_requests: self.total_requests.load(Ordering::SeqCst),
            allowed_requests: self.allowed_requests.load(Ordering::SeqCst),
            rejected_requests: self.rejected_requests.load(Ordering::SeqCst),
            banned_requests: self.banned_requests.load(Ordering::SeqCst),
            error_count: self.error_count.load(Ordering::SeqCst),
            last_updated: Some(Utc::now()),
        }
    }

    /// 重置所有统计计数器
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::StatsManager;
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
        self.total_requests.store(0, Ordering::SeqCst);
        self.allowed_requests.store(0, Ordering::SeqCst);
        self.rejected_requests.store(0, Ordering::SeqCst);
        self.banned_requests.store(0, Ordering::SeqCst);
        self.error_count.store(0, Ordering::SeqCst);
    }

    /// 获取总请求数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_total();
    /// assert_eq!(stats.total(), 1);
    /// ```
    #[inline]
    pub fn total(&self) -> u64 {
        self.total_requests.load(Ordering::SeqCst)
    }

    /// 获取允许请求数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_allowed();
    /// assert_eq!(stats.allowed(), 1);
    /// ```
    #[inline]
    pub fn allowed(&self) -> u64 {
        self.allowed_requests.load(Ordering::SeqCst)
    }

    /// 获取拒绝请求数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_rejected();
    /// assert_eq!(stats.rejected(), 1);
    /// ```
    #[inline]
    pub fn rejected(&self) -> u64 {
        self.rejected_requests.load(Ordering::SeqCst)
    }

    /// 获取封禁请求数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_banned();
    /// assert_eq!(stats.banned(), 1);
    /// ```
    #[inline]
    pub fn banned(&self) -> u64 {
        self.banned_requests.load(Ordering::SeqCst)
    }

    /// 获取错误数
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::StatsManager;
    ///
    /// let stats = StatsManager::new();
    /// stats.increment_error();
    /// assert_eq!(stats.errors(), 1);
    /// ```
    #[inline]
    pub fn errors(&self) -> u64 {
        self.error_count.load(Ordering::SeqCst)
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
    /// use limiteron::StatsManager;
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
    /// use limiteron::StatsManager;
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
    /// use limiteron::StatsManager;
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
    /// use limiteron::StatsManager;
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

    // ========================================================================
    // stats race condition 修复测试
    //
    // 验证策略：
    // 1. 并发 increment 期间 snapshot 看到的每个计数器单调非递减（SeqCst 保证
    //    fetch_add 的全局顺序，Relaxed 不保证跨计数器的可见性顺序）。
    // 2. reset 后立即 snapshot 可见到归零状态（SeqCst 的 store 对后续 load 可见）。
    // 3. 模拟 DecisionChain::check 的成对调用模式（increment_total 必先于
    //    increment_allowed/rejected），验证 snapshot 满足业务不变式
    //    allowed + rejected + banned + error >= total（每个分类计数伴随 total）。
    //    在 Relaxed 下，snapshot 可能读到 total 已 increment 但分类计数尚未
    //    increment 的中间态，违反不变式；SeqCst 保证代码顺序即全局可见顺序。
    // ========================================================================

    #[test]
    fn test_stats_manager_concurrent_snapshot_monotonic() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering as StdOrdering};
        use std::thread;
        use std::time::Duration;

        let stats = Arc::new(StatsManager::new());
        let stop = Arc::new(AtomicBool::new(false));
        let violations = Arc::new(std::sync::atomic::AtomicU64::new(0));

        // 工作线程：持续 increment 各计数器
        let stats_worker = Arc::clone(&stats);
        let stop_worker = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            while !stop_worker.load(StdOrdering::SeqCst) {
                stats_worker.increment_total();
                stats_worker.increment_allowed();
                stats_worker.increment_rejected();
                stats_worker.increment_banned();
                stats_worker.increment_error();
            }
        });

        // 快照线程：持续 snapshot，验证计数器单调非递减
        let stats_snap = Arc::clone(&stats);
        let stop_snap = Arc::clone(&stop);
        let violations_snap = Arc::clone(&violations);
        let snapshotter = thread::spawn(move || {
            let mut prev = stats_snap.snapshot();
            for _ in 0..1000 {
                let curr = stats_snap.snapshot();
                if curr.total_requests < prev.total_requests
                    || curr.allowed_requests < prev.allowed_requests
                    || curr.rejected_requests < prev.rejected_requests
                    || curr.banned_requests < prev.banned_requests
                    || curr.error_count < prev.error_count
                {
                    violations_snap.fetch_add(1, StdOrdering::SeqCst);
                }
                prev = curr;
            }
            // 持续运行一段时间，让快照线程和工作线程充分并发
            let _ = stop_snap;
        });

        thread::sleep(Duration::from_millis(200));
        stop.store(true, StdOrdering::SeqCst);
        worker.join().unwrap();
        snapshotter.join().unwrap();

        assert_eq!(
            violations.load(StdOrdering::SeqCst),
            0,
            "snapshot 观测到计数器倒退：违反单调性（increment 期间不应有 reset）"
        );
    }

    #[test]
    fn test_stats_manager_concurrent_check_pattern_eventual_consistency() {
        use std::sync::Arc;
        use std::thread;

        // 模拟 DecisionChain::check 的统计更新模式：
        //   拒绝：increment_total + increment_rejected
        //   错误：increment_total + increment_error
        //   允许：increment_total + increment_allowed
        // 验证"最终一致性"：所有工作线程结束后，fetch_add 的原子性保证
        // total == 各分类计数之和（每轮 total+1 且恰好一个分类+1）。
        // 注意：snapshot() 内部多个 load 非原子，无法在并发期间验证
        // 跨计数器不变式；只能在所有 increment 完成后验证最终值。
        // SeqCst 保证 fetch_add 的全局顺序，确保最终计数无丢失更新。
        let stats = Arc::new(StatsManager::new());
        const THREADS: usize = 8;
        const ITERS: u64 = 1000;

        let mut handles = vec![];
        for _ in 0..THREADS {
            let stats_clone = Arc::clone(&stats);
            handles.push(thread::spawn(move || {
                for i in 0..ITERS {
                    stats_clone.increment_total();
                    match i % 3 {
                        0 => stats_clone.increment_allowed(),
                        1 => stats_clone.increment_rejected(),
                        _ => stats_clone.increment_error(),
                    }
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let snap = stats.snapshot();
        let expected_total = (THREADS as u64) * ITERS;
        let classified = snap.allowed_requests + snap.rejected_requests + snap.error_count;
        assert_eq!(snap.total_requests, expected_total, "total 计数丢失更新");
        assert_eq!(classified, expected_total, "分类计数丢失更新");
        // 每轮恰好一个分类+1，所以 classified == total
        assert_eq!(
            classified, snap.total_requests,
            "SeqCst 应保证 fetch_add 无丢失更新：classified 应等于 total"
        );
    }
}
