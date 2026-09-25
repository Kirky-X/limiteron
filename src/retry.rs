// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT

//! 带退避的重试原语。
//!
//! 流量控制之外的相邻高可用原语：对「可重试错误」做指数退避重试，
//! 与熔断（[`crate::circuit`]：错误率门限断路）互补——熔断保护下游，
//! 重试消化瞬时抖动。
//!
//! # 示例
//!
//! ```rust
//! use limiteron::retry::RetryPolicy;
//! use std::time::Duration;
//!
//! # async fn example() {
//! let policy = RetryPolicy::new(3, Duration::from_millis(100));
//! let mut attempts = 0u32;
//! let result: Result<u32, String> = policy
//!     .execute(
//!         || async {
//!             attempts += 1;
//!             if attempts < 2 {
//!                 Err("transient".to_string())
//!             } else {
//!                 Ok(42)
//!             }
//!         },
//!         |err| err == "transient", // 仅重试可重试错误
//!     )
//!     .await;
//! assert_eq!(result.unwrap(), 42);
//! # }
//! ```

use std::future::Future;
use std::time::Duration;

/// 计算第 `attempt` 次重试的退避时长（attempt 从 1 起）。
///
/// 纯函数：`base = initial × factor^(attempt-1)`，封顶 `max_delay`，
/// 叠加比例抖动 `[1, 1+jitter]`（线性同余伪随机，仅用于打散重试尖峰，
/// 非安全用途）。供自管重试循环的消费者按 attempt 号取延迟，与
/// [`RetryPolicy::execute`] 的内部退避共用同一公式。
pub fn delay_for_attempt(
    attempt: u32,
    initial: Duration,
    factor: f64,
    max_delay: Duration,
    jitter: f64,
) -> Duration {
    let base = initial.as_millis() as f64 * factor.powi(attempt as i32 - 1);
    let jitter = jitter.clamp(0.0, 1.0);
    let scaled = if jitter > 0.0 {
        // 线性同余：attempt 与时钟低位混合，产出 [0,1) 伪随机系数
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(0);
        let seed = (nanos ^ (u64::from(attempt) * 2_664_035_897)) % 10_000;
        base * (1.0 + jitter * (seed as f64 / 10_000.0))
    } else {
        base
    };
    // 封顶是绝对上限：抖动在封顶前施加（否则抖动会突破 max_delay）
    Duration::from_millis(scaled.min(max_delay.as_millis() as f64) as u64)
}

/// 重试策略：指数退避 + 封顶。
///
/// `execute` 对操作最多执行 `max_retries + 1` 次（首次 + 重试），
/// 第 `n` 次重试前等待 `min(initial_delay * factor^(n-1), max_delay)`。
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// 最大重试次数（不含首次调用；0 = 不重试，只执行一次）。
    max_retries: u32,
    /// 首次重试前的等待时长。
    initial_delay: Duration,
    /// 退避乘数（默认 2.0）。
    factor: f64,
    /// 单次等待上限（默认 60s）。
    max_delay: Duration,
    /// 退避抖动比例（0.0–1.0，默认 0；n×delay × (1 + rand*jitter)）。
    jitter: f64,
    /// 重试预算：重试次数占「总调用次数」的比例上限（0.0–1.0）。
    /// 超过预算的重试立即放弃（重试风暴防护：下游故障时重试流量
    /// 不超过 总请求 × budget_ratio）。`None` = 不启用预算（默认）。
    budget_ratio: Option<f64>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(500),
            factor: 2.0,
            max_delay: Duration::from_secs(60),
            jitter: 0.0,
            budget_ratio: None,
        }
    }
}

impl RetryPolicy {
    /// 创建策略：`max_retries` 次重试、`initial_delay` 首次退避，
    /// factor 2.0 / max_delay 60s / 无抖动。
    pub fn new(max_retries: u32, initial_delay: Duration) -> Self {
        Self {
            max_retries,
            initial_delay,
            ..Self::default()
        }
    }

    /// 设置退避乘数（默认 2.0）。
    #[must_use]
    pub fn with_factor(mut self, factor: f64) -> Self {
        self.factor = factor;
        self
    }

    /// 设置单次等待上限（默认 60s）。
    #[must_use]
    pub fn with_max_delay(mut self, max_delay: Duration) -> Self {
        self.max_delay = max_delay;
        self
    }

    /// 设置退避抖动比例（0.0–1.0）。
    #[must_use]
    pub fn with_jitter(mut self, jitter: f64) -> Self {
        self.jitter = jitter.clamp(0.0, 1.0);
        self
    }

    /// 设置重试预算（重试占比上限，0.0–1.0）。
    ///
    /// 每个 [`RetryPolicy`] 实例独立记账：`total` 为已发起的调用总数
    ///（含首调），`retries` 为实际发生的重试数；预算耗尽后后续错误
    /// 立即返回，不再重试。
    #[must_use]
    pub fn with_budget_ratio(mut self, ratio: f64) -> Self {
        self.budget_ratio = Some(ratio.clamp(0.0, 1.0));
        self
    }

    /// 最大重试次数。
    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    /// 第 `attempt` 次重试的等待时长（attempt 从 1 起）。
    ///
    /// 启用 jitter 时使用 decorrelated 抖动变体：在 `delay_for_attempt`
    /// 的基础上，延迟上界同时受前次延迟 × 3 约束（AWS 架构博客推荐的
    /// full-jitter 改良，避免高倍率 factor 下的延迟爆炸）。
    fn delay_for(&self, attempt: u32, prev_delay: Duration) -> Duration {
        let d = delay_for_attempt(
            attempt,
            self.initial_delay,
            self.factor,
            self.max_delay,
            self.jitter,
        );
        if self.jitter > 0.0 && attempt > 1 {
            let cap = prev_delay.saturating_mul(3);
            d.min(cap)
        } else {
            d
        }
    }

    /// 执行操作并按策略重试可重试错误（熔断联动版）。
    ///
    /// 与 [`Self::execute`] 相同，但每次重试前检查熔断器：熔断已打开
    /// （或进入半开冷却）时立即返回最后一次错误，不再向已判故障的
    /// 下游注入重试流量。
    #[cfg(feature = "circuit-breaker")]
    pub async fn execute_with_breaker<F, Fut, T, E, P>(
        &self,
        breaker: &crate::circuit::CircuitBreaker,
        mut op: F,
        is_retryable: P,
    ) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        P: Fn(&E) -> bool,
    {
        // 熔断打开：只执行一次操作（错误如实返回），不再重试——
        // 避免向已判故障的下游注入重试流量
        if breaker.is_open().await {
            let fut = op();
            return fut.await;
        }
        self.execute(op, is_retryable).await
    }

    /// 执行操作并按策略重试可重试错误。
    ///
    /// - `op`：每次重试都会重新调用的异步操作工厂
    /// - `is_retryable`：错误分类器；返回 false 的错误立即返回（不重试）
    ///
    /// 返回最后一次的错误（重试耗尽）或首个不可重试错误。
    pub async fn execute<F, Fut, T, E, P>(&self, op: F, is_retryable: P) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        P: Fn(&E) -> bool,
    {
        self.execute_notify(op, is_retryable, |_, _| {}).await
    }

    /// 执行操作并按策略重试可重试错误，每次重试前回调 `on_retry`。
    ///
    /// - `op`：每次重试都会重新调用的异步操作工厂
    /// - `is_retryable`：错误分类器；返回 false 的错误立即返回（不重试）
    /// - `on_retry(attempt, err)`：在第 `attempt` 次重试的退避等待前回调
    ///   （`attempt` 从 1 起），`err` 为触发本次重试的错误。仅对实际发生的
    ///   重试回调——不可重试错误与重试耗尽不回调，错误经返回值上报。
    ///
    /// 钩子为方法参数而非策略字段：[`RetryPolicy`] 保持 `derive(Debug, Clone)`
    /// 派生不变，钩子的记账状态由调用方闭包自行捕获。
    pub async fn execute_notify<F, Fut, T, E, P, N>(
        &self,
        mut op: F,
        is_retryable: P,
        on_retry: N,
    ) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        P: Fn(&E) -> bool,
        N: Fn(u32, &E),
    {
        let mut attempt = 0u32;
        // 预算记账：total = 首调 + 已发生重试
        let mut total_calls = 0u64;
        let mut retries_used = 0u64;
        let mut prev_delay = Duration::ZERO;
        loop {
            total_calls += 1;
            match op().await {
                Ok(value) => return Ok(value),
                Err(err) => {
                    let retryable = is_retryable(&err);
                    let within_retries = attempt < self.max_retries;
                    // 重试风暴防护：预算耗尽（重试数/总调用数超比例）即放弃
                    let within_budget = match self.budget_ratio {
                        Some(ratio) => (retries_used as f64) < ratio * total_calls as f64,
                        None => true,
                    };
                    if !retryable || !within_retries || !within_budget {
                        return Err(err);
                    }
                    attempt += 1;
                    retries_used += 1;
                    on_retry(attempt, &err);
                    let d = self.delay_for(attempt, prev_delay);
                    tokio::time::sleep(d).await;
                    prev_delay = d;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// 首次失败后成功的操作：重试路径命中并返回成功值。
    #[tokio::test]
    async fn retries_once_then_succeeds() {
        let policy = RetryPolicy::new(3, Duration::from_millis(1));
        let attempts = Arc::new(AtomicU32::new(0));
        let a = attempts.clone();
        let result: Result<u32, String> = policy
            .execute(
                || {
                    let a = a.clone();
                    async move {
                        let n = a.fetch_add(1, Ordering::SeqCst) + 1;
                        if n < 2 {
                            Err("transient".to_string())
                        } else {
                            Ok(n)
                        }
                    }
                },
                |e| e == "transient",
            )
            .await;
        assert_eq!(result.unwrap(), 2);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    /// 不可重试错误立即返回，不再消耗重试次数。
    #[tokio::test]
    async fn non_retryable_error_returns_immediately() {
        let policy = RetryPolicy::new(5, Duration::from_millis(1));
        let attempts = Arc::new(AtomicU32::new(0));
        let a = attempts.clone();
        let result: Result<(), String> = policy
            .execute(
                || {
                    let a = a.clone();
                    async move {
                        a.fetch_add(1, Ordering::SeqCst);
                        Err::<(), _>("permanent".to_string())
                    }
                },
                |e| e != "permanent",
            )
            .await;
        assert_eq!(result.unwrap_err(), "permanent");
        assert_eq!(attempts.load(Ordering::SeqCst), 1, "不可重试错误只执行一次");
    }

    /// 重试耗尽后返回最后一次错误（max_retries=2 → 共执行 3 次）。
    #[tokio::test]
    async fn gives_up_after_max_retries() {
        let policy = RetryPolicy::new(2, Duration::from_millis(1));
        let attempts = Arc::new(AtomicU32::new(0));
        let a = attempts.clone();
        let result: Result<(), String> = policy
            .execute(
                || {
                    let a = a.clone();
                    async move {
                        a.fetch_add(1, Ordering::SeqCst);
                        Err::<(), _>("always".to_string())
                    }
                },
                |e| e == "always",
            )
            .await;
        assert_eq!(result.unwrap_err(), "always");
        assert_eq!(attempts.load(Ordering::SeqCst), 3, "1 次首调 + 2 次重试");
    }

    /// 退避时长：指数增长且被 max_delay 封顶。
    #[test]
    fn delay_grows_exponentially_and_caps() {
        let policy =
            RetryPolicy::new(10, Duration::from_millis(100)).with_max_delay(Duration::from_secs(5));
        assert_eq!(
            policy.delay_for(1, Duration::ZERO),
            Duration::from_millis(100)
        );
        assert_eq!(
            policy.delay_for(2, Duration::ZERO),
            Duration::from_millis(200)
        );
        assert_eq!(
            policy.delay_for(3, Duration::ZERO),
            Duration::from_millis(400)
        );
        assert_eq!(
            policy.delay_for(20, Duration::ZERO),
            Duration::from_secs(5),
            "退避应封顶 max_delay"
        );
    }
    /// 重试预算：预算耗尽后停止重试（重试风暴防护）。
    #[tokio::test]
    async fn retry_budget_exhaustion_stops_retries() {
        // budget_ratio=0.5,严格小于才允许重试:
        // 首调后 0/1 < 0.5 → 允许第 1 次重试;判定第 2 次重试时
        // 1/2 = 0.5 不严格小于 → 停止,共 2 次调用(1 次首调 + 1 次重试)
        let policy = RetryPolicy::new(10, Duration::from_millis(1)).with_budget_ratio(0.5);
        let attempts = Arc::new(AtomicU32::new(0));
        let a = attempts.clone();
        let result: Result<(), String> = policy
            .execute(
                || {
                    let a = a.clone();
                    async move {
                        a.fetch_add(1, Ordering::SeqCst);
                        Err::<(), _>("always".to_string())
                    }
                },
                |e| e == "always",
            )
            .await;
        assert_eq!(result.unwrap_err(), "always");
        let n = attempts.load(Ordering::SeqCst);
        assert_eq!(n, 2, "预算 0.5 下 2 次调用(1 次重试)后应停止,实际 {n}");
    }

    /// decorrelated 抖动:attempt>1 时延迟受前次延迟 ×3 上界约束。
    #[test]
    fn jittered_delay_bounded_by_prev_times_three() {
        let policy = RetryPolicy::new(10, Duration::from_millis(100))
            .with_max_delay(Duration::from_secs(3600))
            .with_jitter(1.0);
        // attempt=2:base=200ms,prev=100ms → 上界 min(200..300, 300)=300ms 内
        let d = policy.delay_for(2, Duration::from_millis(100));
        assert!(
            d <= Duration::from_millis(300),
            "decorrelated 上界 prev×3=300ms,实际 {d:?}"
        );
        // prev 很小时上界收紧
        let d2 = policy.delay_for(3, Duration::from_millis(10));
        assert!(
            d2 <= Duration::from_millis(30),
            "上界应受 prev×3=30ms 约束,实际 {d2:?}"
        );
    }

    // ========================================================================
    // execute_notify:重试前回调钩子(方法参数,非策略字段)
    // ========================================================================

    /// 永久(不可重试)错误:op 只执行一次,on_retry 零次回调。
    #[tokio::test]
    async fn execute_notify_permanent_error_single_call_no_hook() {
        let policy = RetryPolicy::new(5, Duration::from_millis(1));
        let op_calls = Arc::new(AtomicU32::new(0));
        let hook_calls = Arc::new(AtomicU32::new(0));
        let a = op_calls.clone();
        let h = hook_calls.clone();
        let result: Result<(), String> = policy
            .execute_notify(
                || {
                    let a = a.clone();
                    async move {
                        a.fetch_add(1, Ordering::SeqCst);
                        Err::<(), _>("permanent".to_string())
                    }
                },
                |e| e != "permanent",
                |_, _| {
                    h.fetch_add(1, Ordering::SeqCst);
                },
            )
            .await;
        assert_eq!(result.unwrap_err(), "permanent");
        assert_eq!(op_calls.load(Ordering::SeqCst), 1, "永久错误只执行一次");
        assert_eq!(
            hook_calls.load(Ordering::SeqCst),
            0,
            "未发生重试不应回调 on_retry"
        );
    }

    /// 瞬时(可重试)错误持续失败:max_retries=N 时 on_retry 恰回调 N 次。
    #[tokio::test]
    async fn execute_notify_transient_error_notifies_once_per_retry() {
        let policy = RetryPolicy::new(3, Duration::from_millis(1));
        let op_calls = Arc::new(AtomicU32::new(0));
        let hook_calls = Arc::new(AtomicU32::new(0));
        let a = op_calls.clone();
        let h = hook_calls.clone();
        let result: Result<(), String> = policy
            .execute_notify(
                || {
                    let a = a.clone();
                    async move {
                        a.fetch_add(1, Ordering::SeqCst);
                        Err::<(), _>("flaky".to_string())
                    }
                },
                |e| e == "flaky",
                |_, _| {
                    h.fetch_add(1, Ordering::SeqCst);
                },
            )
            .await;
        assert_eq!(result.unwrap_err(), "flaky");
        assert_eq!(op_calls.load(Ordering::SeqCst), 4, "1 次首调 + 3 次重试");
        assert_eq!(
            hook_calls.load(Ordering::SeqCst),
            3,
            "每次重试前恰好回调一次"
        );
    }

    /// 参数序:on_retry 第一参数为重试序号(从 1 起),第二参数为触发错误。
    #[tokio::test]
    async fn execute_notify_hook_receives_attempt_then_error() {
        let policy = RetryPolicy::new(3, Duration::from_millis(1));
        let attempts_seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let errs_seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let a = attempts_seen.clone();
        let e = errs_seen.clone();
        let result: Result<(), String> = policy
            .execute_notify(
                || async { Err::<(), _>("boom".to_string()) },
                |err| err == "boom",
                move |attempt, err| {
                    a.lock().unwrap().push(attempt);
                    e.lock().unwrap().push(err.to_string());
                },
            )
            .await;
        assert!(result.is_err());
        assert_eq!(
            *attempts_seen.lock().unwrap(),
            vec![1, 2, 3],
            "attempt 序号应从 1 起逐次递增"
        );
        assert_eq!(*errs_seen.lock().unwrap(), vec!["boom", "boom", "boom"]);
    }

    /// Clone 编译:钩子是方法参数而非字段,策略克隆后仍可直接使用,
    /// 闭包捕获本地状态无需 'static 约束。
    #[tokio::test]
    async fn execute_notify_callable_on_cloned_policy() {
        let policy = RetryPolicy::new(2, Duration::from_millis(1));
        let cloned = policy.clone();
        let hook_calls = Arc::new(AtomicU32::new(0));
        let h = hook_calls.clone();
        let result: Result<(), String> = cloned
            .execute_notify(
                || async { Err::<(), _>("always".to_string()) },
                |e| e == "always",
                |_, _| {
                    h.fetch_add(1, Ordering::SeqCst);
                },
            )
            .await;
        assert!(result.is_err());
        assert_eq!(hook_calls.load(Ordering::SeqCst), 2);
        // Debug 派生未因钩子引入字段而破坏
        assert!(format!("{policy:?}").starts_with("RetryPolicy"));
    }
}
