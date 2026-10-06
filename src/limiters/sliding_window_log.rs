// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 滑动窗口日志限流器模块
//!
//! 以请求日志（时间戳 + 成本队列）实现**精确**滑动窗口计数：逐项记录
//! 窗口内每次放行，过期项随窗口滑动逐出——无固定窗口的边界突刺
//! （两窗交界 2× 放行），计数严格等于「最近 window 时间内的放行总量」。
//!
//! # 复杂度与内存
//!
//! - allow：过期确认（摊还 O(1)）+ 前缀弹出（摊还 O(1)）+ 追加 O(1)
//! - peek/remaining/观测读：无新增过期时 O(1)（`LogState.scanned` 的
//!   单调确认游标）；突发后空闲期的首次读付一次 O(空闲期过期数)
//!   确认成本，同一前缀绝不重复扫描
//! - 内存：每条目 16 字节（`(u64, u64)`），条目数 ≤ 窗口内放行请求数
//!   （配置上界 `crate::constants::MAX_SLIDING_LOG_REQUESTS`）。精确
//!   计数以线性内存为代价，大配额长窗口场景优先选分片滑动窗口
//!   （O(分片) 计数器）
//!
//! # 时钟回拨
//!
//! 墙钟回拨时过期判定用回拨后的 `now`（少逐出，计数偏保守、多拒不放行，
//! fail-closed）；条目时间戳对队尾取 max 保持升序不变式，杜绝乱序条目
//! 滞留导致的计数虚高。
//!
//! **回拨例外（permissive）**：单调确认游标使已确认的过期前缀「不可逆」
//! ——若某条目在时刻 T1 被任何读/判定路径确认过期，时钟回拨到 T0 < T1
//! 后该条目仍计为已过期（放行方向）。因此 peek/remaining 等观测调用在
//! 回拨场景下可能改变后续判定（比无观测时更放行）：游标缓存的是可推导
//! 信息这一性质仅在时钟单调时成立。回拨本身属尽力而为场景，该例外是
//! 单调确认换 O(1) 读路径的已知代价。

use super::traits::{Limiter, RateLimitSnapshot, validate_cost};
use crate::clock::{Clock, SystemClock};
use crate::constants::MAX_SLIDING_LOG_REQUESTS;
use crate::error::LimiteronError;
use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

/// 滑动窗口日志限流器
///
/// 每次放行向日志队列追加 `(时间戳, cost)` 条目；窗口外条目在 allow
/// 判定前弹出（惰性逐出，摊还 O(1)），队列有效成本合计不得超过
/// `max_requests`。
///
/// # 特性
/// - **精确计数**：无固定窗口边界突刺，任意时刻放行总量严格受限
/// - **内存有界**：条目数受「窗口内放行请求数」约束（配置上界另由
///   `LimiterConfig::validate` 强制）；过期条目在 allow 路径逐出，
///   空闲期已过期条目仍驻留队列（确认游标推进、容量不回收，队列
///   清空时收缩缓冲）——观测读数与判定均基于虚拟逐出后的有效值，
///   不受驻留影响
/// - [`Limiter::peek`]/[`Limiter::remaining`] 不弹出条目、不追加日志；
///   时钟单调时游标推进仅缓存可推导信息，不影响判定；**时钟回拨例外**：
///   此前在更高时刻确认的过期前缀不再计入（permissive，见模块文档
///   「时钟回拨」节）
pub struct SlidingWindowLogLimiter {
    /// 窗口内最大放行总量
    max_requests: u64,
    /// 窗口长度
    window: Duration,
    /// 时钟实例
    clock: Arc<dyn Clock>,
    /// 日志队列状态（锁保护：条目、计数与确认游标配对更新）
    state: Mutex<LogState>,
}

#[derive(Debug)]
struct LogState {
    /// 放行日志：`(纳秒时间戳, cost)`，按时间升序（回拨时对队尾取 max）
    entries: VecDeque<(u64, u64)>,
    /// 全部条目成本合计（含已确认过期、尚未弹出的前缀）
    total_count: u64,
    /// 已确认过期前缀的成本合计
    expired_cost: u64,
    /// 已确认过期的前缀长度（单调推进的确认游标）
    ///
    /// 时钟单调时条目一经确认过期永不过期回来，每个条目一生只被扫描
    /// 一次——allow 的逐出与 peek 的读数都从该游标续扫，无新增过期时为
    /// O(1)。游标推进缓存的是可推导信息（有效计数 =
    /// `total_count - expired_cost`）；时钟回拨时已确认前缀不可逆，
    /// 判定转为 permissive（见模块文档「时钟回拨」节）。
    scanned: usize,
}

impl LogState {
    /// 窗口内有效放行总量（虚拟逐出后）
    fn effective_count(&self) -> u64 {
        self.total_count - self.expired_cost
    }

    /// 有效条目数（虚拟逐出后，内存观测口径）
    fn effective_len(&self) -> usize {
        self.entries.len() - self.scanned
    }
}

impl SlidingWindowLogLimiter {
    /// 创建滑动窗口日志限流器（系统时钟）
    ///
    /// # 参数
    /// * `max_requests` - 窗口内最大放行总量
    /// * `window` - 窗口长度
    pub fn new(max_requests: u64, window: Duration) -> Result<Self, LimiteronError> {
        Self::with_clock(max_requests, window, Arc::new(SystemClock))
    }

    /// 以自定义时钟创建（测试注入用）
    pub fn with_clock(
        max_requests: u64,
        window: Duration,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, LimiteronError> {
        if max_requests == 0 {
            return Err(LimiteronError::ConfigError(
                "sliding window log max_requests must be non-zero".to_string(),
            ));
        }
        if window.is_zero() {
            return Err(LimiteronError::ConfigError(
                "sliding window log window must be non-zero".to_string(),
            ));
        }
        // 日志型窗口内存随配额线性增长（16B/条目）：上界比计数器型窗口更严，
        // 且与配置层校验（LimiterConfig::validate）同源，直构路径同样受限
        if max_requests > MAX_SLIDING_LOG_REQUESTS {
            return Err(LimiteronError::ConfigError(format!(
                "sliding window log max_requests must not exceed {MAX_SLIDING_LOG_REQUESTS}"
            )));
        }
        Ok(Self {
            max_requests,
            window,
            clock,
            state: Mutex::new(LogState {
                entries: VecDeque::new(),
                total_count: 0,
                expired_cost: 0,
                scanned: 0,
            }),
        })
    }

    /// 窗口长度
    pub fn window(&self) -> Duration {
        self.window
    }

    /// 窗口内最大放行总量
    pub fn max_requests(&self) -> u64 {
        self.max_requests
    }

    /// 当前窗口内放行总量（不改变有效限流状态）
    pub async fn window_count(&self) -> u64 {
        let now = self.clock.unix_timestamp_nanos();
        let mut state = self.state.lock();
        self.scan_expired(&mut state, now);
        state.effective_count()
    }

    /// 当前有效条目数（虚拟逐出口径，内存观测用；驻留的已过期条目不计入）
    pub async fn logged_entries(&self) -> usize {
        let now = self.clock.unix_timestamp_nanos();
        let mut state = self.state.lock();
        self.scan_expired(&mut state, now);
        state.effective_len()
    }

    /// 窗口纳秒长度（u64；构造期窗口非零，溢出截断仅出现在天文窗口）
    fn window_nanos(&self) -> u64 {
        u64::try_from(self.window.as_nanos()).unwrap_or(u64::MAX)
    }

    /// 推进过期确认游标：把 `[scanned, 首个未过期位置)` 计入过期前缀
    ///
    /// 过期判定：`ts + window ≤ now`（该单元已整体滑出计数窗口
    /// `(now - window, now]`）。游标单调、只进不退：条目一经确认过期
    /// 永不过期回来，每个条目一生只扫描一次。只更新缓存字段
    /// （`scanned`/`expired_cost`），不弹出条目、不改变有效计数与判定。
    fn scan_expired(&self, state: &mut LogState, now: u64) {
        let window_nanos = self.window_nanos();
        while state.scanned < state.entries.len() {
            let (ts, cost) = state.entries[state.scanned];
            if ts.saturating_add(window_nanos) > now {
                break;
            }
            state.expired_cost += cost;
            state.scanned += 1;
        }
    }

    /// 弹出已确认过期的前缀（仅 allow 路径调用）
    ///
    /// 队列清空时收缩缓冲，回收突发高水位内存（`VecDeque` 不会自行
    /// 缩容）。
    fn evict_confirmed(&self, state: &mut LogState) {
        while state.scanned > 0 {
            state.entries.pop_front();
            state.scanned -= 1;
        }
        state.total_count -= state.expired_cost;
        state.expired_cost = 0;
        if state.entries.is_empty() {
            state.entries.shrink_to_fit();
        }
    }
}

#[async_trait]
impl Limiter for SlidingWindowLogLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError> {
        validate_cost(cost)?;
        let mut state = self.state.lock();
        let now = self.clock.unix_timestamp_nanos();
        self.scan_expired(&mut state, now);
        self.evict_confirmed(&mut state);
        if cost > self.max_requests.saturating_sub(state.effective_count()) {
            return Ok(false);
        }
        // 回拨保序：条目时间戳对队尾取 max，维持升序不变式
        // （回拨期判定本身用回拨后的 now，少逐出、偏保守）
        let ts = state.entries.back().map_or(now, |&(last, _)| now.max(last));
        state.entries.push_back((ts, cost));
        state.total_count += cost;
        Ok(true)
    }

    /// 非消费预检：读取剩余配额，不追加日志、不弹出条目
    async fn peek(&self, cost: u64) -> Result<RateLimitSnapshot, LimiteronError> {
        validate_cost(cost)?;
        let now = self.clock.unix_timestamp_nanos();
        let mut state = self.state.lock();
        self.scan_expired(&mut state, now);
        let oldest = state.entries.get(state.scanned).map(|&(ts, _)| ts);
        Ok(self.snapshot(state.effective_count(), oldest, now))
    }

    /// 剩余额度查询（非消费）
    async fn remaining(&self) -> Result<RateLimitSnapshot, LimiteronError> {
        let now = self.clock.unix_timestamp_nanos();
        let mut state = self.state.lock();
        self.scan_expired(&mut state, now);
        let oldest = state.entries.get(state.scanned).map(|&(ts, _)| ts);
        Ok(self.snapshot(state.effective_count(), oldest, now))
    }
}

impl SlidingWindowLogLimiter {
    /// 以窗口计数渲染标准限流头快照（`now` 由调用方传入，读路径单次时钟读）
    ///
    /// reset = 最老未过期条目完全滑出窗口所需秒数（向上取整）；
    /// 窗口为空时已可用，reset = 0。
    fn snapshot(&self, count: u64, oldest: Option<u64>, now: u64) -> RateLimitSnapshot {
        let reset_secs = oldest
            .map(|ts| {
                let remain_nanos = ts.saturating_add(self.window_nanos()).saturating_sub(now);
                (remain_nanos as u128).div_ceil(1_000_000_000) as u64
            })
            .unwrap_or(0);
        RateLimitSnapshot {
            limit: self.max_requests,
            remaining: self.max_requests.saturating_sub(count),
            reset_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;

    #[tokio::test]
    async fn test_sliding_log_basic_allow_and_reject() {
        let limiter = SlidingWindowLogLimiter::new(3, Duration::from_secs(1)).unwrap();

        for _ in 0..3 {
            assert!(limiter.allow(1).await.unwrap());
        }
        assert_eq!(limiter.window_count().await, 3);
        assert!(!limiter.allow(1).await.unwrap());
        assert_eq!(limiter.window_count().await, 3, "拒绝的请求不得入日志队列");
    }

    #[tokio::test]
    async fn test_sliding_log_cost_accounting() {
        let limiter = SlidingWindowLogLimiter::new(10, Duration::from_secs(1)).unwrap();

        assert!(limiter.allow(4).await.unwrap());
        assert!(limiter.allow(5).await.unwrap());
        assert_eq!(limiter.window_count().await, 9);

        // 剩余 1：cost=2 拒、cost=1 过
        assert!(!limiter.allow(2).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());
        assert_eq!(limiter.window_count().await, 10);
    }

    #[tokio::test]
    async fn test_sliding_log_exact_boundary_slide() {
        // 精确边界回归：条目在 ts + window ≤ now 时整体滑出。
        // 1s 窗口：t=0 放 2；t=1.0s 时 t=0 恰好过期（0 + 1s ≤ 1s），
        // 修复前若用 `ts + window < now` 判定会在此刻误拒。
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter =
            SlidingWindowLogLimiter::with_clock(2, Duration::from_secs(1), clock).unwrap();

        assert!(limiter.allow(2).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());

        mock_clock.advance(Duration::from_secs(1));
        assert!(limiter.allow(2).await.unwrap(), "恰好满窗龄的条目应已滑出");
        assert_eq!(limiter.window_count().await, 2);
    }

    #[tokio::test]
    async fn test_sliding_log_partial_expiry_batch_evict() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter =
            SlidingWindowLogLimiter::with_clock(10, Duration::from_secs(1), clock).unwrap();

        // t=0 放 3 条 cost=1；t=0.6 放 1 条 cost=2
        for _ in 0..3 {
            assert!(limiter.allow(1).await.unwrap());
        }
        mock_clock.advance(Duration::from_millis(600));
        assert!(limiter.allow(2).await.unwrap());
        assert_eq!(limiter.window_count().await, 5);

        // t=1.2：t=0 三条过期（成本 3 逐出），t=0.6 一条存活
        mock_clock.advance(Duration::from_millis(600));
        assert_eq!(limiter.window_count().await, 2, "应恰好逐出 3 条过期成本");
        assert_eq!(limiter.logged_entries().await, 1);

        // 逐出后空间恢复：还可放 8
        assert!(limiter.allow(8).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_sliding_log_bounded_memory() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter =
            SlidingWindowLogLimiter::with_clock(5, Duration::from_millis(10), clock).unwrap();

        // 连续满速写入远超窗口容量：队列长度不得随总请求增长，
        // 稳定在窗口内条目数（≤ max_requests / min_cost = 5）
        for round in 0..50 {
            mock_clock.advance(Duration::from_millis(5));
            let _ = limiter.allow(1).await.unwrap();
            let len = limiter.logged_entries().await;
            assert!(
                len <= 5,
                "日志队列应受窗口约束，第 {round} 轮长度 {len} 超界"
            );
        }
    }

    #[tokio::test]
    async fn test_sliding_log_reject_path_no_repeated_rescan() {
        // 拒绝热路径复杂度回归（审查 F1/F8）：窗口填满后时间推进使全部
        // 条目过期（空闲驻留），随后洪泛拒绝请求 + 连续 remaining 读。
        // 确认游标必须单调推进：首笔读付一次确认成本后，无新增过期时
        // 游标不再移动（虚拟视图不重复扫描同一前缀），allow 的逐出一次
        // 弹清、后续拒绝 O(1)。
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter =
            SlidingWindowLogLimiter::with_clock(4, Duration::from_secs(1), clock).unwrap();

        for _ in 0..4 {
            assert!(limiter.allow(1).await.unwrap());
        }
        assert!(!limiter.allow(1).await.unwrap(), "窗口已满应拒绝");

        // 空闲期推进 2s：4 条全部过期但未弹出（驻留）
        mock_clock.advance(Duration::from_secs(2));
        {
            let mut state = limiter.state.lock();
            assert_eq!(state.entries.len(), 4, "空闲期条目驻留（惰性逐出）");
            limiter.scan_expired(&mut state, mock_clock.unix_timestamp_nanos());
            assert_eq!(state.scanned, 4, "首次读应把 4 条全部确认为过期");

            // 无新增过期时连续读：游标与过期成本严格不变（无重复扫描付费）
            for _ in 0..100 {
                limiter.scan_expired(&mut state, mock_clock.unix_timestamp_nanos());
                assert_eq!(state.scanned, 4, "无新增过期时游标不得移动");
            }
        }
        assert_eq!(
            limiter.window_count().await,
            0,
            "驻留过期条目不计入有效计数"
        );
        assert_eq!(limiter.logged_entries().await, 0);

        // 配额恢复：4 次放行弹清过期前缀并重新填满窗口（首笔 allow 一次
        // 付清弹出成本），随后洪泛全部拒绝——拒绝路径的 remaining 读
        // 无新前缀可确认，O(1)
        for _ in 0..4 {
            assert!(limiter.allow(1).await.unwrap(), "过期驻留清空后配额应恢复");
        }
        for _ in 0..10 {
            assert!(!limiter.allow(1).await.unwrap(), "窗口重新打满后应拒绝");
        }
        assert_eq!(limiter.window_count().await, 4);
    }

    #[tokio::test]
    async fn test_sliding_log_idle_evict_releases_capacity() {
        // 空闲驻留的已过期条目在下一次 allow 逐出后弹出；
        // 队列清空时收缩缓冲，回收突发高水位内存
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter =
            SlidingWindowLogLimiter::with_clock(8, Duration::from_millis(10), clock).unwrap();

        for _ in 0..8 {
            assert!(limiter.allow(1).await.unwrap());
        }
        let high_water = {
            let state = limiter.state.lock();
            state.entries.capacity()
        };
        assert!(high_water > 0, "高水位后应有已分配缓冲");

        mock_clock.advance(Duration::from_secs(1));
        assert!(limiter.allow(1).await.unwrap(), "全过期后应恢复放行");
        let state = limiter.state.lock();
        assert!(
            state.entries.capacity() < high_water,
            "队列清空后应收缩缓冲（容量 {} 未低于高水位 {high_water}）",
            state.entries.capacity()
        );
    }

    #[tokio::test]
    async fn test_sliding_log_peek_remaining_pure() {
        // peek/remaining 零副作用契约：不弹出条目、不追加日志，
        // 有效计数与判定不受读数影响
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter =
            SlidingWindowLogLimiter::with_clock(10, Duration::from_secs(1), clock).unwrap();

        assert!(limiter.allow(10).await.unwrap());
        mock_clock.advance(Duration::from_millis(999));

        let snap = limiter.peek(5).await.unwrap();
        assert_eq!(snap.limit, 10);
        assert_eq!(snap.remaining, 0, "999ms 时窗口仍满");
        assert!(!snap.allows(5));

        // 连续 peek 状态不得变化（修复前若 peek 真实逐出会改变后续判定）
        for _ in 0..100 {
            let _ = limiter.peek(1).await.unwrap();
        }
        assert_eq!(limiter.window_count().await, 10);
        {
            let state = limiter.state.lock();
            assert_eq!(state.entries.len(), 1, "peek 不得弹出条目");
        }

        // 补 1ms 跨过过期边界后，peek 报告的剩余应真实可得
        mock_clock.advance(Duration::from_millis(1));
        let snap = limiter.remaining().await.unwrap();
        assert_eq!(snap.remaining, 10);
        assert!(limiter.allow(10).await.unwrap());
    }

    #[tokio::test]
    async fn test_sliding_log_snapshot_reset_secs() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter =
            SlidingWindowLogLimiter::with_clock(10, Duration::from_secs(2), clock).unwrap();

        // 空窗口：已可用，reset = 0
        let snap = limiter.remaining().await.unwrap();
        assert_eq!(snap.reset_secs, 0);

        // t=0 放 1 条：1s 后该条滑出还需 1s（2-1）
        assert!(limiter.allow(1).await.unwrap());
        mock_clock.advance(Duration::from_secs(1));
        let snap = limiter.remaining().await.unwrap();
        assert_eq!(snap.remaining, 9);
        assert_eq!(snap.reset_secs, 1, "reset 应为最老条目完全滑出的剩余秒数");
    }

    #[tokio::test]
    async fn test_sliding_log_clock_rollback_stays_conservative() {
        // 时钟回拨：判定用回拨后的 now（少逐出、偏保守 fail-closed），
        // 条目时间戳对队尾取 max 保持升序，不产生乱序滞留与计数虚高
        let mock_clock = Arc::new(MockClock::with_instant(std::time::Instant::now(), 10));
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter =
            SlidingWindowLogLimiter::with_clock(10, Duration::from_millis(500), clock).unwrap();

        // t=10s 放 3 条
        assert!(limiter.allow(3).await.unwrap());
        // 回拨到 t=9.5s：已有条目 ts=10s 未过期（保守保留）
        mock_clock.set_time(std::time::Instant::now(), 9);
        assert_eq!(
            limiter.window_count().await,
            3,
            "回拨期条目不得被误逐出（判定用回拨后的 now）"
        );
        // 回拨期放行：新条目 ts 取 max(now, 队尾) = 10s，保序
        assert!(limiter.allow(3).await.unwrap());
        assert_eq!(limiter.window_count().await, 6);
        {
            let state = limiter.state.lock();
            let sorted = state
                .entries
                .iter()
                .zip(state.entries.iter().skip(1))
                .all(|(&(a, _), &(b, _))| a <= b);
            assert!(sorted, "条目时间戳必须保持升序不变式");
            assert_eq!(
                state.entries.back().map(|&(ts, _)| ts),
                Some(10_000_000_000),
                "回拨期新条目应对齐队尾时间戳"
            );
        }
    }

    #[tokio::test]
    async fn test_sliding_log_peek_before_rollback_is_permissive() {
        // 回拨 × 观测语义回归：单调确认游标的 permissive
        // 例外——条目在高时刻 T1 被 peek 确认过期后，时钟回拨到 T0 < T1
        // 时该条目仍计为已过期（放行）；若无那次 peek，T0 判定下未过期
        // （拒绝）。固化该偏差方向，防止未来重构无意改变语义。
        let mock_clock = Arc::new(MockClock::with_instant(std::time::Instant::now(), 10));
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter =
            SlidingWindowLogLimiter::with_clock(4, Duration::from_millis(500), clock).unwrap();

        // t=10s 放满 4 条
        for _ in 0..4 {
            assert!(limiter.allow(1).await.unwrap());
        }
        // t=10.6s：全部条目过期（ts + 0.5s ≤ 10.6s），peek 确认游标推进
        mock_clock.advance(Duration::from_millis(600));
        assert_eq!(limiter.peek(1).await.unwrap().remaining, 4);

        // 回拨到 t=10.2s（条目过期时刻 = ts + window = 10.5s：按回拨后的
        // now 判定条目未过期）——但游标已在 10.6s 确认过期，仍计为已过期
        // （permissive 放行）；无观测调用时此处应拒绝（对照组见下）
        mock_clock.set_time(std::time::Instant::now(), 10);
        mock_clock.advance(Duration::from_millis(200));
        {
            let state = limiter.state.lock();
            assert_eq!(state.scanned, 4, "游标不可逆：已确认前缀不因回拨回退");
        }
        assert!(
            limiter.allow(4).await.unwrap(),
            "已确认过期的前缀在回拨后仍计为已过期（permissive）"
        );

        // 对照组：无 peek 的同场景在回拨后保持保守判定
        let mock_clock2 = Arc::new(MockClock::with_instant(std::time::Instant::now(), 10));
        let clock2: Arc<dyn Clock> = mock_clock2.clone();
        let limiter2 =
            SlidingWindowLogLimiter::with_clock(4, Duration::from_millis(500), clock2).unwrap();
        for _ in 0..4 {
            assert!(limiter2.allow(1).await.unwrap());
        }
        mock_clock2.set_time(std::time::Instant::now(), 10);
        mock_clock2.advance(Duration::from_millis(200));
        assert!(
            !limiter2.allow(4).await.unwrap(),
            "无观测调用时回拨判定保守：条目未过期应拒绝"
        );
    }

    #[tokio::test]
    async fn test_sliding_log_zero_cost_rejected() {
        let limiter = SlidingWindowLogLimiter::new(10, Duration::from_secs(1)).unwrap();
        assert!(limiter.allow(0).await.is_err());
        assert!(limiter.peek(0).await.is_err());
    }

    #[tokio::test]
    async fn test_sliding_log_cost_exceeds_max_rejected() {
        let limiter = SlidingWindowLogLimiter::new(10, Duration::from_secs(1)).unwrap();
        assert!(limiter.allow(1_000_001).await.is_err());
    }

    #[tokio::test]
    async fn test_sliding_log_invalid_config_rejected() {
        assert!(
            SlidingWindowLogLimiter::new(0, Duration::from_secs(1)).is_err(),
            "max_requests 0 应拒绝"
        );
        assert!(
            SlidingWindowLogLimiter::new(10, Duration::ZERO).is_err(),
            "零窗口应拒绝"
        );
    }

    #[tokio::test]
    async fn test_sliding_log_accessors() {
        let limiter = SlidingWindowLogLimiter::new(42, Duration::from_secs(7)).unwrap();
        assert_eq!(limiter.max_requests(), 42);
        assert_eq!(limiter.window(), Duration::from_secs(7));
    }

    #[tokio::test]
    async fn test_sliding_log_concurrent_allow_respects_budget() {
        // 并发正确性：多任务争用同一把状态锁时，放行总量不得超过配额
        let limiter = Arc::new(SlidingWindowLogLimiter::new(100, Duration::from_secs(60)).unwrap());
        let admitted = Arc::new(std::sync::atomic::AtomicU64::new(0));

        let mut handles = Vec::new();
        for _ in 0..8 {
            let limiter = Arc::clone(&limiter);
            let admitted = Arc::clone(&admitted);
            handles.push(tokio::spawn(async move {
                for _ in 0..50 {
                    if limiter.allow(1).await.unwrap() {
                        admitted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }));
        }
        for handle in handles {
            handle.await.unwrap();
        }
        assert_eq!(
            admitted.load(std::sync::atomic::Ordering::Relaxed),
            100,
            "并发放行总量必须恰好等于配额"
        );
        assert_eq!(limiter.window_count().await, 100);
    }

    #[tokio::test]
    async fn test_sliding_log_check_default_impl_maps_rejection() {
        use crate::limiters::Limiter;
        let limiter = SlidingWindowLogLimiter::new(1, Duration::from_secs(1)).unwrap();
        assert!(limiter.check("any_key").await.is_ok());
        let result = limiter.check("any_key").await;
        assert!(result.is_err(), "窗口满后 check 应返回 Err");
    }
}
