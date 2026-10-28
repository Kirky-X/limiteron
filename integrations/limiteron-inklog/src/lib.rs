// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! inklog sink 限流适配器：按 target 分桶的令牌预算（limiteron Clock 内核）。
//!
//! 实现 inklog 自有 [`SinkRateLimit`] 端口（R-rate-limit-ownership-002 的
//! 「增强实现住消费侧」）：
//!
//! - 按 `record.target` 分桶，各桶独立预算（容量 `capacity`，每秒补充
//!   `refill_per_sec`）；补充语义与 limiteron
//!   `TokenBucketLimiter::new(rate, rate)` 等价——纳秒时间积分携带，
//!   亚令牌积分滞留给下次累计（封顶时丢弃）；
//! - `ERROR` 级记录直通：错误日志不因限流丢失；
//! - sink 写失败（`report(_, Failed)`）归还 1 个预算令牌（容量封顶）；
//! - 桶数上限 [`DEFAULT_MAX_BUCKETS`]：target 是外部输入，无界增长即
//!   内存放大，超限的新 target 落入共享 fallback 桶。
//!
//! 装配方式见 `examples/basic_usage.rs`。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use inklog::{LogRecord, SinkRateLimit, SinkWriteOutcome};
use limiteron::Clock;

/// 默认桶数上限：超出后新 target 共享 fallback 桶。
pub const DEFAULT_MAX_BUCKETS: usize = 4096;

/// 单桶令牌预算：纳秒时间积分补充（与 limiteron `TokenBucketLimiter`
/// 同式，含亚令牌积分滞留）。
struct Bucket {
    tokens: AtomicU64,
    last_refill_nanos: AtomicU64,
}

impl Bucket {
    fn new(capacity: u64, now_nanos: u64) -> Self {
        Self {
            tokens: AtomicU64::new(capacity),
            last_refill_nanos: AtomicU64::new(now_nanos),
        }
    }

    /// 尝试扣 1 个令牌：先按经过时间补充（容量封顶），再 CAS 扣减。
    fn try_acquire(&self, capacity: u64, refill_per_sec: u64, now_nanos: u64) -> bool {
        self.refill(capacity, refill_per_sec, now_nanos);
        let mut current = self.tokens.load(Ordering::SeqCst);
        loop {
            if current == 0 {
                return false;
            }
            match self.tokens.compare_exchange_weak(
                current,
                current - 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return true,
                Err(observed) => current = observed,
            }
        }
    }

    /// 写失败归还 1 个令牌（容量封顶）。
    fn refund(&self, capacity: u64) {
        let mut current = self.tokens.load(Ordering::SeqCst);
        loop {
            if current >= capacity {
                return;
            }
            match self.tokens.compare_exchange_weak(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return,
                Err(observed) => current = observed,
            }
        }
    }

    /// 时间积分携带补充：`last_refill` 只前推被整数额令牌真正消费掉的
    /// 时间，亚令牌积分滞留给下次累计（历史教训与实现见 limiteron
    /// `token_bucket.rs` 同注——每次补充事件丢失亚令牌积分会让低速率
    /// 场景的有效补充速率系统性低于配置值）；补充量受容量封顶时，未消费
    /// 的时间积分随封顶丢弃（令牌桶语义）。
    fn refill(&self, capacity: u64, refill_per_sec: u64, now_nanos: u64) {
        if refill_per_sec == 0 {
            return;
        }
        loop {
            let last = self.last_refill_nanos.load(Ordering::SeqCst);
            let elapsed = now_nanos.saturating_sub(last);
            // 至少经过 1 毫秒才触发补充（与 limiteron 同阈值）
            if elapsed < 1_000_000 {
                return;
            }
            // u128 中间量防 elapsed × rate 溢出
            let earned = (u128::from(elapsed) * u128::from(refill_per_sec)) / 1_000_000_000u128;
            let to_add = u64::try_from(earned).unwrap_or(u64::MAX);
            if to_add == 0 {
                return;
            }
            // 独占 [last, now] 窗口的补充权；竞态失败者以新基点重算
            if self
                .last_refill_nanos
                .compare_exchange(last, now_nanos, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                continue;
            }
            let mut current = self.tokens.load(Ordering::SeqCst);
            let actual = loop {
                let headroom = capacity.saturating_sub(current);
                let candidate = to_add.min(headroom);
                if self
                    .tokens
                    .compare_exchange_weak(
                        current,
                        current.saturating_add(candidate),
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    )
                    .is_ok()
                {
                    break candidate;
                }
                current = self.tokens.load(Ordering::SeqCst);
            };
            if actual < to_add {
                // 封顶：未消费积分随封顶丢弃，基点驻留 now
                return;
            }
            // 未封顶：基点回退到被整数额令牌消费掉的时间末端，亚令牌积分滞留
            let consumed = (u128::from(actual) * 1_000_000_000u128) / u128::from(refill_per_sec);
            let credit_nanos = elapsed.saturating_sub(u64::try_from(consumed).unwrap_or(elapsed));
            self.last_refill_nanos
                .store(now_nanos.saturating_sub(credit_nanos), Ordering::SeqCst);
            return;
        }
    }
}

/// 按 target 分桶的 sink 限流适配器：[`SinkRateLimit`] 端口的
/// limiteron 侧增强实现。
pub struct TargetBucketRateLimit {
    capacity: u64,
    refill_per_sec: u64,
    max_buckets: usize,
    buckets: Mutex<HashMap<String, Arc<Bucket>>>,
    fallback: Arc<Bucket>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for TargetBucketRateLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TargetBucketRateLimit")
            .field("capacity", &self.capacity)
            .field("refill_per_sec", &self.refill_per_sec)
            .field("max_buckets", &self.max_buckets)
            .field("buckets", &self.buckets.lock().map(|b| b.len()))
            .finish_non_exhaustive()
    }
}

impl TargetBucketRateLimit {
    /// 每桶容量 `capacity`、每秒补充 `refill_per_sec` 个令牌；
    /// `new(rate, rate)` 与 limiteron `TokenBucketLimiter::new(rate, rate)`
    /// 补充语义等价（系统时钟）。
    pub fn new(capacity: u64, refill_per_sec: u64) -> Self {
        Self::with_clock(capacity, refill_per_sec, Arc::new(limiteron::SystemClock))
    }

    /// 可注入时钟变体（测试与确定性验证）。
    pub fn with_clock(capacity: u64, refill_per_sec: u64, clock: Arc<dyn Clock>) -> Self {
        let now = clock.unix_timestamp_nanos();
        Self {
            capacity,
            refill_per_sec,
            max_buckets: DEFAULT_MAX_BUCKETS,
            buckets: Mutex::new(HashMap::new()),
            fallback: Arc::new(Bucket::new(capacity, now)),
            clock,
        }
    }

    /// 覆盖桶数上限（链式）。
    #[must_use]
    pub fn with_max_buckets(mut self, max_buckets: usize) -> Self {
        self.max_buckets = max_buckets;
        self
    }

    /// 当前分桶数（诊断面；fallback 桶不计入）。
    pub fn bucket_count(&self) -> usize {
        self.buckets.lock().map_or(0, |b| b.len())
    }

    fn bucket_for(&self, target: &str) -> Arc<Bucket> {
        let Ok(mut map) = self.buckets.lock() else {
            // 锁中毒：放弃分桶，退到共享 fallback 桶（错误路径不传播 panic）
            return Arc::clone(&self.fallback);
        };
        if let Some(bucket) = map.get(target) {
            return Arc::clone(bucket);
        }
        if map.len() >= self.max_buckets {
            return Arc::clone(&self.fallback);
        }
        let bucket = Arc::new(Bucket::new(
            self.capacity,
            self.clock.unix_timestamp_nanos(),
        ));
        map.insert(target.to_string(), Arc::clone(&bucket));
        bucket
    }

    fn is_error(record: &LogRecord) -> bool {
        record.level.eq_ignore_ascii_case("ERROR")
    }
}

impl SinkRateLimit for TargetBucketRateLimit {
    fn try_acquire(&self, record: &LogRecord) -> bool {
        if Self::is_error(record) {
            return true;
        }
        let bucket = self.bucket_for(&record.target);
        bucket.try_acquire(
            self.capacity,
            self.refill_per_sec,
            self.clock.unix_timestamp_nanos(),
        )
    }

    fn report(&self, record: &LogRecord, outcome: SinkWriteOutcome) {
        if matches!(outcome, SinkWriteOutcome::Failed) && !Self::is_error(record) {
            let bucket = self.bucket_for(&record.target);
            bucket.refund(self.capacity);
        }
    }

    fn name(&self) -> &str {
        "limiteron-inklog"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use limiteron::MockClock;
    use std::time::{Duration, Instant};

    struct FixedClock(Arc<MockClock>);
    impl Clock for FixedClock {
        fn now(&self) -> Instant {
            self.0.now()
        }
        fn unix_timestamp(&self) -> u64 {
            self.0.unix_timestamp()
        }
        fn unix_timestamp_nanos(&self) -> u64 {
            self.0.unix_timestamp_nanos()
        }
    }

    fn record(target: &str, level: &str) -> LogRecord {
        LogRecord {
            timestamp: inklog::chrono::Utc::now(),
            level: level.to_string(),
            target: target.to_string(),
            message: "m".to_string(),
            fields: HashMap::new(),
            file: None,
            line: None,
            thread_id: "t".to_string(),
            trace_id: None,
            span_id: None,
        }
    }

    fn limiter(capacity: u64, refill: u64) -> (TargetBucketRateLimit, Arc<MockClock>) {
        let mock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = Arc::new(FixedClock(Arc::clone(&mock)));
        (
            TargetBucketRateLimit::with_clock(capacity, refill, clock),
            mock,
        )
    }

    /// ERROR 级记录直通：预算耗尽也放行（错误日志不丢）。
    #[test]
    fn error_level_bypasses_limit() {
        let (rl, _mock) = limiter(1, 0);
        assert!(rl.try_acquire(&record("app", "ERROR")));
        assert!(rl.try_acquire(&record("app", "ERROR")));
        assert!(rl.try_acquire(&record("app", "error")));
    }

    /// 按 target 分桶隔离：A 桶耗尽不影响 B 桶。
    #[test]
    fn per_target_isolation() {
        let (rl, _mock) = limiter(2, 0);
        assert!(rl.try_acquire(&record("a", "INFO")));
        assert!(rl.try_acquire(&record("a", "INFO")));
        assert!(!rl.try_acquire(&record("a", "INFO")));
        assert!(rl.try_acquire(&record("b", "INFO")));
    }

    /// 补充语义：耗尽后经 1 秒恢复 `refill_per_sec` 个预算。
    #[test]
    fn refill_after_one_second() {
        let (rl, mock) = limiter(10, 10);
        for _ in 0..10 {
            assert!(rl.try_acquire(&record("app", "INFO")));
        }
        assert!(!rl.try_acquire(&record("app", "INFO")));
        mock.advance(Duration::from_secs(1));
        for _ in 0..10 {
            assert!(rl.try_acquire(&record("app", "INFO")));
        }
        assert!(!rl.try_acquire(&record("app", "INFO")));
    }

    /// 写失败归还预算：Failed 回报后可再写 1 次；Written 不归还。
    #[test]
    fn failed_write_refunds_budget() {
        let (rl, _mock) = limiter(1, 0);
        assert!(rl.try_acquire(&record("app", "INFO")));
        assert!(!rl.try_acquire(&record("app", "INFO")));
        rl.report(&record("app", "INFO"), SinkWriteOutcome::Written);
        assert!(!rl.try_acquire(&record("app", "INFO")));
        rl.report(&record("app", "INFO"), SinkWriteOutcome::Failed);
        assert!(rl.try_acquire(&record("app", "INFO")));
    }

    /// 桶数上限：超限的新 target 落入共享 fallback 桶（预算与既有桶隔离）。
    #[test]
    fn overflow_targets_share_fallback_bucket() {
        let mock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = Arc::new(FixedClock(Arc::clone(&mock)));
        let rl = TargetBucketRateLimit::with_clock(2, 0, clock).with_max_buckets(1);
        assert!(rl.try_acquire(&record("a", "INFO")));
        assert_eq!(rl.bucket_count(), 1);
        // b/c 落入共享 fallback 桶（容量 2）：两笔预算各自消耗
        assert!(rl.try_acquire(&record("b", "INFO")));
        assert!(rl.try_acquire(&record("c", "INFO")));
        assert!(!rl.try_acquire(&record("d", "INFO")));
        // fallback 耗尽不干扰已建桶 a 的独立预算
        assert!(rl.try_acquire(&record("a", "INFO")));
    }

    /// 亚令牌积分滞留：1/s 速率下 0.5s 间隔的请求隔次放行（积分累计不丢）。
    #[test]
    fn sub_token_time_credit_accumulates() {
        let (rl, mock) = limiter(1, 1);
        assert!(rl.try_acquire(&record("app", "INFO")));
        // 0.5s：0.5 个积分（不足 1），仍拒绝
        mock.advance(Duration::from_millis(500));
        assert!(!rl.try_acquire(&record("app", "INFO")));
        // 再 0.5s：累计 1.0 个积分 → 放行
        mock.advance(Duration::from_millis(500));
        assert!(rl.try_acquire(&record("app", "INFO")));
        mock.advance(Duration::from_millis(500));
        assert!(!rl.try_acquire(&record("app", "INFO")));
        mock.advance(Duration::from_millis(500));
        assert!(rl.try_acquire(&record("app", "INFO")));
    }
}
