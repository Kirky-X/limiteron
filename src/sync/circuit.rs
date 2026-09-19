// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 同步三态熔断器
//!
//! 状态机与 async 版 [`CircuitBreaker`](crate::circuit::CircuitBreaker) 一致：
//!
//! ```text
//! Closed --连续失败达阈值--> Open --冷却到期（惰性）--> HalfOpen --探针成功--> Closed
//!                                                              └--探针失败--> Open（重置冷却）
//! ```
//!
//! 与 async 版的差异：
//! - 纯同步（`parking_lot::Mutex` + `Instant`），无 tokio 依赖；
//! - 半开探针准入内建于状态机：`Open` 冷却到期后首个 [`admit`](SyncCircuitBreaker::admit)
//!   调用原子转 `HalfOpen` 并成为唯一探针，探针结算前其余调用被拒绝
//!   （async 版经 `half_open_max_calls` 原子放行多个探针）；
//! - 失败判定即闭包返回的 `Result`：`Err` 一律计入失败，无错误分类器。
//!
//! # 示例
//!
//! ```rust
//! use limiteron::sync::{CircuitCallError, SyncCircuitBreaker};
//! use std::time::Duration;
//!
//! // 连续失败 2 次熔断，冷却 30 秒
//! let breaker = SyncCircuitBreaker::new(2, Duration::from_secs(30));
//!
//! // 放行并透传结果
//! assert_eq!(breaker.call(|| Ok::<i32, String>(42)).unwrap(), 42);
//!
//! // 前两次失败被放行执行，第二次失败后熔断打开
//! let _ = breaker.call(|| Err::<(), _>("boom"));
//! let _ = breaker.call(|| Err::<(), _>("boom"));
//!
//! // 熔断打开：闭包不再执行（快速失败）
//! match breaker.call(|| Err::<(), _>("boom")) {
//!     Err(CircuitCallError::Open) => { /* 拒绝 */ }
//!     _ => unreachable!(),
//! }
//! ```

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::clock::{Clock, SystemClock};
use parking_lot::Mutex;

/// 熔断器包装的调用结果。
///
/// - [`CircuitCallError::Open`]：熔断打开，闭包未执行即被拒绝（快速失败）；
/// - [`CircuitCallError::Inner`]：调用被放行但自身失败，透传原始错误。
#[derive(Debug, PartialEq, Eq)]
pub enum CircuitCallError<E> {
    /// 熔断打开：闭包未执行。
    Open,
    /// 调用被放行但失败：透传原始错误。
    Inner(E),
}

impl<E> CircuitCallError<E> {
    /// 熔断是否处于打开拒绝态（`true` 表示闭包未执行）。
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open)
    }

    /// 取透传的原始错误（Open 态返回 `None`）。
    pub fn into_inner(self) -> Option<E> {
        match self {
            Self::Open => None,
            Self::Inner(e) => Some(e),
        }
    }
}

impl<E: fmt::Display> fmt::Display for CircuitCallError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => write!(f, "circuit breaker is open"),
            Self::Inner(e) => write!(f, "{e}"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for CircuitCallError<E> {}

/// 熔断器内部状态。
#[derive(Debug)]
enum State {
    /// 关闭（正常放行）：记录连续失败计数。
    Closed { failures: u32 },
    /// 打开（快速失败）：记录打开时刻，冷却到期后惰性转半开。
    Open { opened_at: Instant },
    /// 半开（探针在途）：放行的单个探针调用成败决定回闭合或重新打开。
    HalfOpen,
}

/// 同步三态熔断器。
///
/// 线程安全（内部 `parking_lot::Mutex`）。时间经泛型 [`Clock`] 注入：
/// 生产用默认 [`SystemClock`]，测试注入 `MockClock`
/// 手动推进，不依赖真实睡眠。
///
/// # 示例
///
/// ```rust
/// use limiteron::sync::SyncCircuitBreaker;
/// use std::time::Duration;
///
/// let breaker = SyncCircuitBreaker::new(1, Duration::from_secs(10));
/// assert!(breaker.call(|| Err::<(), _>("x")).is_err()); // 失败达阈值，转打开
/// assert!(breaker.call(|| Ok::<(), ()>(())).unwrap_err().is_open()); // 快速失败
/// // 冷却到期后 admit() 惰性转半开并放行探针（时间推进见单元测试）
/// ```
pub struct SyncCircuitBreaker {
    threshold: u32,
    cooldown: Duration,
    clock: Arc<dyn Clock>,
    state: Mutex<State>,
}

impl fmt::Debug for SyncCircuitBreaker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncCircuitBreaker")
            .field("threshold", &self.threshold)
            .field("cooldown", &self.cooldown)
            .finish_non_exhaustive()
    }
}

impl SyncCircuitBreaker {
    /// 创建熔断器：`threshold` 为连续失败阈值（小于 1 时抬升为 1），
    /// `cooldown` 为打开态冷却时长。
    pub fn new(threshold: u32, cooldown: Duration) -> Self {
        Self::with_clock(Arc::new(SystemClock), threshold, cooldown)
    }

    /// 以注入时钟创建熔断器（测试用）。
    ///
    /// 时钟为 `Arc<dyn Clock>` 共享实例：`MockClock` 的 `Clone` 是深拷贝，
    /// 须先 `as_arc()` 再注入，`advance` 才能被熔断器观测到。
    pub fn with_clock(clock: Arc<dyn Clock>, threshold: u32, cooldown: Duration) -> Self {
        Self {
            threshold: threshold.max(1),
            cooldown,
            clock,
            state: Mutex::new(State::Closed { failures: 0 }),
        }
    }

    /// 经熔断器执行调用：拒绝时返回 [`CircuitCallError::Open`]，闭包不执行。
    ///
    /// 放行的调用成功记 [`record_success`](Self::record_success)、失败记
    /// [`record_failure`](Self::record_failure)。
    pub fn call<T, E>(&self, f: impl FnOnce() -> Result<T, E>) -> Result<T, CircuitCallError<E>> {
        if !self.admit() {
            return Err(CircuitCallError::Open);
        }
        match f() {
            Ok(value) => {
                self.record_success();
                Ok(value)
            }
            Err(err) => {
                self.record_failure();
                Err(CircuitCallError::Inner(err))
            }
        }
    }

    /// 判定当前时刻是否放行（[`admit_at`](Self::admit_at) 的时钟注入版）。
    pub fn admit(&self) -> bool {
        self.admit_at(self.clock.now())
    }

    /// 判定指定时刻是否放行；`Open` 冷却到期时惰性转 `HalfOpen`（本次调用即探针）。
    ///
    /// `HalfOpen` 态返回 `false`：探针在途期间拒绝后续调用，直到探针经
    /// [`record_success`](Self::record_success) / [`record_failure`](Self::record_failure)
    /// 结算。
    pub fn admit_at(&self, now: Instant) -> bool {
        let mut state = self.state.lock();
        match &*state {
            State::Closed { .. } => true,
            State::Open { opened_at } => {
                if now.duration_since(*opened_at) >= self.cooldown {
                    *state = State::HalfOpen;
                    true
                } else {
                    false
                }
            }
            // 探针在途：拒绝并发探测
            State::HalfOpen => false,
        }
    }

    /// 记录成功：任意状态回闭合，失败计数清零。
    pub fn record_success(&self) {
        let mut state = self.state.lock();
        *state = State::Closed { failures: 0 };
    }

    /// 记录失败（[`record_failure_at`](Self::record_failure_at) 的时钟注入版）。
    pub fn record_failure(&self) {
        self.record_failure_at(self.clock.now());
    }

    /// 记录指定时刻的失败：闭合态累加计数（达阈值转打开）；半开探针失败立即
    /// 重新打开并重置冷却。
    ///
    /// `Open` 态经 [`call`](Self::call) 不可达（拒绝不计失败、不刷新冷却）；
    /// 此分支覆盖手工调用路径，统一转 `Open`。
    pub fn record_failure_at(&self, now: Instant) {
        let mut state = self.state.lock();
        match &*state {
            State::Closed { failures } => {
                let failures = *failures + 1;
                *state = if failures >= self.threshold {
                    State::Open { opened_at: now }
                } else {
                    State::Closed { failures }
                };
            }
            State::HalfOpen | State::Open { .. } => *state = State::Open { opened_at: now },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;

    /// 基准时刻（MockClock 的固定起点，保证时间运算精确）。
    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn test_call_passthrough_success_and_error() {
        let b = SyncCircuitBreaker::new(3, Duration::from_secs(30));
        assert_eq!(b.call(|| Ok::<i32, &str>(42)), Ok(42));
        assert_eq!(
            b.call(|| Err::<i32, &str>("x")),
            Err(CircuitCallError::Inner("x"))
        );
    }

    #[test]
    fn test_threshold_failures_open_circuit_and_fast_fail() {
        let b = SyncCircuitBreaker::new(3, Duration::from_secs(30));
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = calls.clone();
        let call = || {
            let c = counter.clone();
            b.call(move || -> Result<(), &str> {
                c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err("boom")
            })
        };
        // 前 3 次真实执行并失败
        assert!(matches!(call(), Err(CircuitCallError::Inner("boom"))));
        assert!(matches!(call(), Err(CircuitCallError::Inner("boom"))));
        assert!(matches!(call(), Err(CircuitCallError::Inner("boom"))));
        // 第 4 次：熔断打开，闭包不执行（计数停在 3）
        assert!(matches!(call(), Err(CircuitCallError::Open)));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[test]
    fn test_success_resets_failure_count() {
        let clock = Arc::new(MockClock::with_instant(t0(), 0));
        let b = SyncCircuitBreaker::with_clock(clock, 3, Duration::from_secs(30));
        b.record_failure();
        b.record_failure();
        b.record_success();
        b.record_failure();
        b.record_failure();
        // 连续失败仅 2 次（< 阈值），仍闭合放行
        assert!(b.admit());
    }

    #[test]
    fn test_open_rejects_until_cooldown_then_allows_probe() {
        let clock = Arc::new(MockClock::with_instant(t0(), 0));
        let b = SyncCircuitBreaker::with_clock(clock.clone(), 1, Duration::from_secs(10));
        b.record_failure(); // 达阈值立即打开
        clock.advance(Duration::from_secs(5));
        assert!(!b.admit());
        // 冷却到期：惰性转半开并放行探针
        clock.advance(Duration::from_secs(5));
        assert!(b.admit());
    }

    #[test]
    fn test_rejections_do_not_extend_cooldown() {
        let clock = Arc::new(MockClock::with_instant(t0(), 0));
        let b = SyncCircuitBreaker::with_clock(clock.clone(), 1, Duration::from_secs(10));
        b.record_failure();
        clock.advance(Duration::from_secs(3));
        assert!(!b.admit());
        clock.advance(Duration::from_secs(3));
        assert!(!b.admit());
        // 拒绝不刷新冷却：到期时刻仍然放行
        clock.advance(Duration::from_secs(4));
        assert!(b.admit());
    }

    #[test]
    fn test_half_open_probe_success_closes() {
        let clock = Arc::new(MockClock::with_instant(t0(), 0));
        let b = SyncCircuitBreaker::with_clock(clock.clone(), 2, Duration::from_secs(10));
        b.record_failure();
        b.record_failure(); // 打开
        clock.advance(Duration::from_secs(10));
        assert!(b.admit()); // 探针放行（半开）
        b.record_success(); // 探针成功 → 闭合，计数清零
        clock.advance(Duration::from_secs(1));
        assert!(b.admit());
        b.record_failure();
        // 仅 1 次失败（< 阈值 2），仍闭合
        clock.advance(Duration::from_secs(1));
        assert!(b.admit());
    }

    #[test]
    fn test_half_open_probe_failure_reopens() {
        let clock = Arc::new(MockClock::with_instant(t0(), 0));
        let b = SyncCircuitBreaker::with_clock(clock.clone(), 1, Duration::from_secs(10));
        b.record_failure(); // 打开
        clock.advance(Duration::from_secs(10));
        assert!(b.admit()); // 探针放行（半开）
        b.record_failure(); // 探针失败 → 重新打开（新冷却）
        clock.advance(Duration::from_secs(5));
        assert!(!b.admit());
        clock.advance(Duration::from_secs(5)); // 新冷却到期再探
        assert!(b.admit());
    }

    #[test]
    fn test_half_open_rejects_concurrent_probes() {
        let clock = Arc::new(MockClock::with_instant(t0(), 0));
        let b = SyncCircuitBreaker::with_clock(clock.clone(), 1, Duration::from_secs(10));
        b.record_failure();
        clock.advance(Duration::from_secs(10));
        assert!(b.admit(), "首个到期调用成为探针");
        assert!(!b.admit(), "探针在途期间其余调用被拒绝");
        b.record_success();
        assert!(b.admit(), "探针成功闭合后恢复放行");
    }

    #[test]
    fn test_threshold_below_one_is_raised_to_one() {
        let clock = Arc::new(MockClock::with_instant(t0(), 0));
        let b = SyncCircuitBreaker::with_clock(clock, 0, Duration::from_secs(30));
        b.record_failure();
        assert!(!b.admit());
    }

    #[test]
    fn test_circuit_call_error_helpers() {
        assert!(CircuitCallError::<&str>::Open.is_open());
        assert_eq!(CircuitCallError::Inner("x").into_inner(), Some("x"));
        assert!(CircuitCallError::<&str>::Open.into_inner().is_none());
        assert_eq!(
            CircuitCallError::<&str>::Open.to_string(),
            "circuit breaker is open"
        );
        assert_eq!(CircuitCallError::Inner("boom").to_string(), "boom");
    }
}
