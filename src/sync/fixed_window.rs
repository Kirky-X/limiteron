// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 同步固定窗口限流器（per-identifier）
//!
//! 窗口内前 `limit` 次放行，后续请求拒绝至窗口滚动；被拒绝的请求不消耗预算
//! （计数不增长），与常见固定窗口语义一致。每个标识（`String` 键）独立计数，
//! 与单实例无键的 [`FixedWindowLimiter`](crate::limiters::FixedWindowLimiter)
//! 互补：后者限"全局速率"，本类型限"每个客户端的速率"。
//!
//! 标识表内存有界：追踪标识数达到 `max_tracked`（默认 65536）时，先清除已
//! 过期条目，仍满则淘汰最早开窗的条目（FIFO），防止海量伪造标识撑爆内存。
//!
//! # 示例
//!
//! ```rust
//! use limiteron::sync::SyncFixedWindowLimiter;
//! use std::time::Duration;
//!
//! let limiter = SyncFixedWindowLimiter::new(2, Duration::from_secs(60));
//! assert!(limiter.check("client-a").is_ok());
//! assert!(limiter.check("client-a").is_ok());
//! // 窗口内超限拒绝，拒绝不消耗预算
//! assert!(limiter.check("client-a").is_err());
//! // 独立标识互不影响
//! assert!(limiter.check("client-b").is_ok());
//! ```

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::clock::{Clock, SystemClock};
use parking_lot::Mutex;

/// 默认追踪标识上限。
const DEFAULT_MAX_TRACKED: usize = 65_536;

/// 限流拒绝详情。
///
/// 携带窗口配置供调用方生成 `Retry-After` 等响应元数据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitRejection {
    /// 窗口内允许的最大请求数。
    pub limit: u64,
    /// 窗口长度。
    pub window: Duration,
}

impl fmt::Display for RateLimitRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "rate limit exceeded: {} requests per {}s window",
            self.limit,
            self.window.as_secs()
        )
    }
}

impl std::error::Error for RateLimitRejection {}

/// 单标识的窗口计数。
#[derive(Clone, Copy, Debug)]
struct WindowEntry {
    window_start: Instant,
    count: u64,
}

/// 同步固定窗口限流器（per-identifier）。
///
/// 线程安全（内部 `parking_lot::Mutex`）。时间经泛型 [`Clock`] 注入：
/// 生产用默认 [`SystemClock`]，测试注入 `MockClock`
/// 手动推进。
///
/// # 示例
///
/// ```rust
/// use limiteron::sync::SyncFixedWindowLimiter;
/// use std::time::Duration;
///
/// let limiter = SyncFixedWindowLimiter::new(1, Duration::from_secs(60));
/// assert!(limiter.check("a").is_ok());
/// assert!(limiter.check("a").is_err(), "窗口内超限拒绝");
/// assert!(limiter.check("b").is_ok(), "独立标识互不影响");
/// // 窗口滚动后计数重置（时间推进见单元测试）
/// ```
pub struct SyncFixedWindowLimiter {
    limit: u64,
    window: Duration,
    max_tracked: usize,
    clock: Arc<dyn Clock>,
    state: Mutex<HashMap<String, WindowEntry>>,
}

impl fmt::Debug for SyncFixedWindowLimiter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncFixedWindowLimiter")
            .field("limit", &self.limit)
            .field("window", &self.window)
            .field("max_tracked", &self.max_tracked)
            .finish_non_exhaustive()
    }
}

impl SyncFixedWindowLimiter {
    /// 创建限流器：`limit` 为窗口内最大请求数（小于 1 时抬升为 1），
    /// `window` 为窗口长度（零时长抬升为 1 纳秒）。
    pub fn new(limit: u64, window: Duration) -> Self {
        Self::with_clock(Arc::new(SystemClock), limit, window)
    }

    /// 以注入时钟创建限流器（测试用）。
    ///
    /// 时钟为 `Arc<dyn Clock>` 共享实例：`MockClock` 的 `Clone` 是深拷贝，
    /// 须以 `Arc<MockClock>` 注入，`advance` 才能被限流器观测到。
    pub fn with_clock(clock: Arc<dyn Clock>, limit: u64, window: Duration) -> Self {
        Self {
            limit: limit.max(1),
            window: if window.is_zero() {
                Duration::from_nanos(1)
            } else {
                window
            },
            max_tracked: DEFAULT_MAX_TRACKED,
            clock,
            state: Mutex::new(HashMap::new()),
        }
    }

    /// 设置追踪标识上限（标识表达到上限时清除过期条目，仍满则淘汰最早开窗条目）。
    pub fn with_max_tracked(mut self, max_tracked: usize) -> Self {
        self.max_tracked = max_tracked.max(1);
        self
    }

    /// 判定当前时刻标识是否放行（[`admit_at`](Self::admit_at) 的时钟注入版）。
    pub fn check(&self, identifier: &str) -> Result<(), RateLimitRejection> {
        self.admit_at(identifier, self.clock.now())
    }

    /// 判定指定时刻标识是否放行：窗口滚动则重置计数，预算内放行并消耗一次，
    /// 超限拒绝（计数不增长）。
    pub fn admit_at(&self, identifier: &str, now: Instant) -> Result<(), RateLimitRejection> {
        let rejection = RateLimitRejection {
            limit: self.limit,
            window: self.window,
        };
        let mut state = self.state.lock();
        let entry = match state.get_mut(identifier) {
            Some(entry) => entry,
            None => {
                Self::make_room(&mut state, now, self.window, self.max_tracked);
                state.entry(identifier.to_string()).or_insert(WindowEntry {
                    window_start: now,
                    count: 0,
                })
            }
        };
        if now.duration_since(entry.window_start) >= self.window {
            *entry = WindowEntry {
                window_start: now,
                count: 0,
            };
        }
        if entry.count >= self.limit {
            return Err(rejection);
        }
        entry.count += 1;
        Ok(())
    }

    /// 标识表满员时腾位：清除已过窗口的僵尸条目；仍满则淘汰最早开窗的条目。
    ///
    /// 仅在插入新标识前调用，已存在条目的热路径（`get_mut` 命中）零开销。
    fn make_room(
        state: &mut HashMap<String, WindowEntry>,
        now: Instant,
        window: Duration,
        max_tracked: usize,
    ) {
        if state.len() < max_tracked {
            return;
        }
        state.retain(|_, entry| now.duration_since(entry.window_start) < window);
        if state.len() >= max_tracked {
            // 全部条目仍在窗口内：淘汰最早开窗者（近似 FIFO）
            let oldest = state
                .iter()
                .min_by_key(|(_, entry)| entry.window_start)
                .map(|(key, _)| key.clone());
            if let Some(key) = oldest {
                state.remove(&key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn test_admit_within_limit_and_reject_beyond() {
        let limiter = SyncFixedWindowLimiter::new(2, Duration::from_secs(60));
        assert!(limiter.check("ip1").is_ok());
        assert!(limiter.check("ip1").is_ok());
        let err = limiter.check("ip1").unwrap_err();
        assert_eq!(
            err,
            RateLimitRejection {
                limit: 2,
                window: Duration::from_secs(60)
            }
        );
        // 拒绝不消耗预算：计数停在 2（后续仍拒绝，而非预算翻倍）
        assert!(limiter.check("ip1").is_err());
        // 独立标识互不影响
        assert!(limiter.check("ip2").is_ok());
    }

    #[test]
    fn test_window_roll_resets_count() {
        let clock = Arc::new(MockClock::with_instant(t0(), 0));
        let limiter = SyncFixedWindowLimiter::with_clock(clock.clone(), 1, Duration::from_secs(60));
        assert!(limiter.check("ip1").is_ok());
        assert!(limiter.check("ip1").is_err());
        clock.advance(Duration::from_secs(61));
        assert!(limiter.check("ip1").is_ok(), "窗口滚动后应重新放行");
    }

    #[test]
    fn test_expired_entries_purged_when_full() {
        let clock = Arc::new(MockClock::with_instant(t0(), 0));
        let limiter = SyncFixedWindowLimiter::with_clock(clock.clone(), 2, Duration::from_secs(60))
            .with_max_tracked(2);
        assert!(limiter.check("a").is_ok());
        assert!(limiter.check("b").is_ok());
        // 窗口过期后 a/b 成僵尸条目：新标识 c 触发清空过期者
        clock.advance(Duration::from_secs(61));
        assert!(limiter.check("c").is_ok());
        assert!(limiter.check("d").is_ok(), "过期条目被清除后仍有空位");
        // 过期清除不触碰窗口内活跃条目
        assert!(limiter.check("c").is_ok());
        assert!(limiter.check("d").is_ok());
        assert!(limiter.check("c").is_err(), "c 的预算已被消耗 2 次");
    }

    #[test]
    fn test_oldest_entry_evicted_when_full_in_window() {
        let clock = Arc::new(MockClock::with_instant(t0(), 0));
        let limiter =
            SyncFixedWindowLimiter::with_clock(clock.clone(), 10, Duration::from_secs(60))
                .with_max_tracked(2);
        assert!(limiter.check("a").is_ok());
        clock.advance(Duration::from_secs(1));
        assert!(limiter.check("b").is_ok());
        // 全部条目仍在窗口内：淘汰最早开窗的 a
        clock.advance(Duration::from_secs(1));
        assert!(limiter.check("c").is_ok());
        assert_eq!(limiter.state.lock().len(), 2, "标识表保持有界");
        // a 被淘汰：以新窗口重新计数放行
        assert!(limiter.check("a").is_ok());
    }

    #[test]
    fn test_limit_below_one_raised_to_one() {
        let limiter = SyncFixedWindowLimiter::new(0, Duration::from_secs(60));
        assert!(limiter.check("ip1").is_ok());
        assert!(limiter.check("ip1").is_err());
    }

    #[test]
    fn test_rejection_display() {
        let err = RateLimitRejection {
            limit: 120,
            window: Duration::from_secs(60),
        };
        assert_eq!(
            err.to_string(),
            "rate limit exceeded: 120 requests per 60s window"
        );
    }
}
