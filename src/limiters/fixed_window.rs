// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 固定窗口限流器模块
//!
//! 使用固定窗口算法实现速率限制。

use super::traits::{Limiter, RateLimitSnapshot, validate_cost};
use crate::clock::{Clock, SystemClock};
use crate::error::LimiteronError;
use async_trait::async_trait;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

/// 窗口状态快照：计数与窗口起点必须成对读写。
///
/// 历史教训：二者曾各自为 AtomicU64，窗口翻转分两步发布（先 CAS 起点、
/// 后清零计数），并发线程可观察到「新起点 + 旧计数」——递增被随后的清零
/// 吞并导致超发，或读到未清零的大计数导致误拒。现合并为单一临界区状态。
#[derive(Debug)]
struct WindowState {
    /// 当前窗口计数
    count: u64,
    /// 当前窗口开始时间（纳秒时间戳）
    window_start: u64,
}

/// 固定窗口限流器
///
/// 使用固定窗口算法实现速率限制，将时间划分为固定长度的窗口，
/// 每个窗口独立计数，窗口到期自动重置。
///
/// # 特性
/// - 计数与窗口起点在锁内读-判-写，窗口翻转与递增在同一临界区完成（并发安全）
/// - 窗口翻转保持构造时刻的网格对齐，不随请求漂移
/// - 窗口边界处的固有临界突刺（前后窗口各放行至多 max）为算法语义，未做平滑
///
/// # 示例
/// ```rust
/// use limiteron::limiters::{FixedWindowLimiter, Limiter};
/// use std::time::Duration;
///
/// #[tokio::main]
/// async fn main() {
///     // 创建窗口大小为 1 秒，最大请求数为 100 的固定窗口限流器
///     let limiter = FixedWindowLimiter::new(Duration::from_secs(1), 100);
///
///     // 尝试请求
///     let allowed = limiter.allow(1).await.unwrap();
///     assert!(allowed);
/// }
/// ```
pub struct FixedWindowLimiter {
    /// 窗口大小
    window_size: Duration,
    /// 窗口内最大请求数
    max_requests: u64,
    /// 计数 + 窗口起点组合状态（锁内读-判-写）
    state: Mutex<WindowState>,
    /// 时钟实例
    clock: Arc<dyn Clock>,
}

impl FixedWindowLimiter {
    /// Creates a new fixed window limiter.
    ///
    /// # Arguments
    /// * `window_size` - Fixed window duration
    /// * `max_requests` - Maximum requests per window
    ///
    /// # Examples
    /// ```rust
    /// use limiteron::limiters::FixedWindowLimiter;
    /// use std::time::Duration;
    ///
    /// let limiter = FixedWindowLimiter::new(Duration::from_secs(1), 100);
    /// ```
    pub fn new(window_size: Duration, max_requests: u64) -> Self {
        Self::with_clock(window_size, max_requests, Arc::new(SystemClock))
    }

    /// Creates a new fixed window limiter with a custom clock.
    ///
    /// # Arguments
    /// * `window_size` - Fixed window duration
    /// * `max_requests` - Maximum requests per window
    /// * `clock` - Clock implementation for time injection (useful for testing)
    pub fn with_clock(window_size: Duration, max_requests: u64, clock: Arc<dyn Clock>) -> Self {
        let now = clock.unix_timestamp_nanos();

        Self {
            window_size,
            max_requests,
            state: Mutex::new(WindowState {
                count: 0,
                window_start: now,
            }),
            clock,
        }
    }

    /// 在锁内推进过期的窗口并返回当前计数。
    ///
    /// 翻转（清零计数 + 推进起点）与读取在同一临界区完成；
    /// `windows_passed` 一次跨过多个过期窗口，保持构造时刻的网格对齐不漂移。
    fn advance_expired_window(&self, state: &mut WindowState) {
        let now = self.clock.unix_timestamp_nanos();

        // 防御：Duration::ZERO 窗口会导致除零 panic。
        // 用具名构造器传入 0 窗口时退化到 1ns（每次即被视为新窗口，不崩溃）。
        let window_size_nanos = self.window_size.as_nanos().max(1) as u64;

        let window_end = state.window_start.saturating_add(window_size_nanos);
        if now >= window_end {
            let elapsed = now.saturating_sub(state.window_start);
            let windows_passed = elapsed / window_size_nanos;
            state.window_start = state
                .window_start
                .saturating_add(windows_passed.saturating_mul(window_size_nanos));
            state.count = 0;
        }
    }

    /// 获取当前窗口的计数（仅用于测试）
    #[cfg(test)]
    fn get_count(&self) -> u64 {
        let mut state = self.state.lock();
        self.advance_expired_window(&mut state);
        state.count
    }
}

#[async_trait]
impl Limiter for FixedWindowLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError> {
        let cost = validate_cost(cost)?;
        let mut state = self.state.lock();
        self.advance_expired_window(&mut state);

        // 以减法形式比较：current 接近 u64::MAX 时加法会回绕、误放行
        if cost > self.max_requests.saturating_sub(state.count) {
            return Ok(false);
        }
        state.count += cost;
        Ok(true)
    }

    /// 非消费预检：读窗口计数，不递增
    async fn peek(&self, cost: u64) -> Result<RateLimitSnapshot, LimiteronError> {
        let cost = validate_cost(cost)?;
        let mut state = self.state.lock();
        self.advance_expired_window(&mut state);
        let remaining = self.max_requests.saturating_sub(state.count);
        let reset_secs = self.window_reset_secs_locked(&state);
        let _ = cost;
        Ok(RateLimitSnapshot {
            limit: self.max_requests,
            remaining,
            reset_secs,
        })
    }

    /// 剩余额度查询（非消费）
    async fn remaining(&self) -> Result<RateLimitSnapshot, LimiteronError> {
        let mut state = self.state.lock();
        self.advance_expired_window(&mut state);
        let reset_secs = self.window_reset_secs_locked(&state);
        Ok(RateLimitSnapshot {
            limit: self.max_requests,
            remaining: self.max_requests.saturating_sub(state.count),
            reset_secs,
        })
    }
}

impl FixedWindowLimiter {
    /// 距当前窗口翻转的秒数（向上取整，窗口过期重置后为窗口全长）。
    /// 调用方必须已持有状态锁。
    fn window_reset_secs_locked(&self, state: &WindowState) -> u64 {
        let now_ns = self.clock.unix_timestamp_nanos();
        let window_ns = self.window_size.as_nanos() as u64;
        let window_end_ns = state.window_start.saturating_add(window_ns);
        if window_end_ns <= now_ns {
            return 0;
        }
        (window_end_ns - now_ns).div_ceil(1_000_000_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[tokio::test]
    async fn test_fixed_window_basic() {
        let limiter = FixedWindowLimiter::new(Duration::from_secs(60), 10);

        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }
        assert_eq!(limiter.get_count(), 5);
    }

    #[tokio::test]
    async fn test_fixed_window_exceed() {
        let limiter = FixedWindowLimiter::new(Duration::from_secs(60), 3);

        assert!(limiter.allow(1).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_fixed_window_with_mock_clock() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter = FixedWindowLimiter::with_clock(Duration::from_secs(10), 5, clock);

        // 消费 5 个请求
        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }

        // 第 6 个应该失败
        assert!(!limiter.allow(1).await.unwrap());

        // 前进时间使窗口过期
        mock_clock.advance(Duration::from_secs(11));

        // 触发窗口重置,新的请求应该成功
        assert!(limiter.allow(1).await.unwrap());
    }

    /// 窗口翻转竞态回归：翻转瞬间的并发递增不得被清零吞并（超发），
    /// 也不得读到未清零的旧计数（误拒）。
    ///
    /// 旧实现「先 CAS 起点、后清零计数」分两步发布，64 线程同帧跨窗口时
    /// 递增会落在清零之前的窗口里被抹掉——放行 64 个但账面只剩零星计数，
    /// 随后的请求被错误放行。修复后（临界区内读-判-写）每轮账实相符。
    #[tokio::test]
    async fn test_fixed_window_no_lost_increments_across_rollover() {
        const MAX: u64 = 64;
        const THREADS: usize = 64;
        const ROUNDS: usize = 25;

        for round in 0..ROUNDS {
            let mock = Arc::new(MockClock::new());
            let clock: Arc<dyn Clock> = mock.clone();
            let limiter = Arc::new(FixedWindowLimiter::with_clock(
                Duration::from_secs(10),
                MAX,
                clock,
            ));

            // 灌满第一窗口
            for _ in 0..MAX {
                assert!(
                    limiter.allow(1).await.unwrap(),
                    "round {round} 灌满阶段误拒"
                );
            }
            assert!(!limiter.allow(1).await.unwrap());

            // 翻转窗口，64 线程同帧并发各 allow 一次
            mock.advance(Duration::from_secs(11));
            let allowed = Arc::new(AtomicU64::new(0));
            let barrier = Arc::new(tokio::sync::Barrier::new(THREADS));
            let mut handles = Vec::with_capacity(THREADS);
            for _ in 0..THREADS {
                let limiter = Arc::clone(&limiter);
                let barrier = Arc::clone(&barrier);
                let allowed = Arc::clone(&allowed);
                handles.push(tokio::spawn(async move {
                    barrier.wait().await;
                    if limiter.allow(1).await.unwrap() {
                        allowed.fetch_add(1, Ordering::SeqCst);
                    }
                }));
            }
            for h in handles {
                h.await.unwrap();
            }

            assert_eq!(
                allowed.load(Ordering::SeqCst),
                MAX,
                "round {round} 新窗口放行数偏离"
            );
            assert_eq!(
                limiter.get_count(),
                MAX,
                "round {round} 账实不符：递增被翻转清零吞并"
            );
            // 窗口已满，下一个必须拒绝（账被吞并时会错误放行）
            assert!(
                !limiter.allow(1).await.unwrap(),
                "round {round} 翻转竞态导致超发"
            );
        }
    }
}
