// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! inklog structured logging integration for limiteron.
//!
//! Enable via the `inklog` cargo feature. Provides [`init_inklog_logger`]
//! which initializes inklog's `LoggerManager`, installing a global `tracing`
//! subscriber and a `log` crate bridge. As long as the manager is alive, all
//! existing `tracing::`/`log::` macro calls in limiteron route through
//! inklog's structured sinks (console, file, database).
//!
//! When the `inklog` feature is **disabled**, limiteron retains its original
//! `log`/`tracing` behavior unchanged.
//!
//! # Example
//!
//! ```rust,no_run
//! # #[cfg(feature = "inklog")]
//! # {
//! use limiteron::integrations::inklog::init_inklog_logger;
//!
//! # tokio_test::block_on(async {
//! let _manager = init_inklog_logger().await.expect("init inklog");
//! log::info!("routed through inklog");
//! # });
//! # }
//! ```

/// Re-export inklog core types for direct access.
pub use ::inklog::{InklogConfig, InklogError, LoggerManager};

/// Initialize inklog as the global structured logging backend.
///
/// Creates a `LoggerManager` with default config. Equivalent to
/// `init_inklog_logger_with_config(InklogConfig::default())`.
///
/// # Errors
///
/// Returns `Err(InklogError)` if the `LoggerManager` fails to construct.
pub async fn init_inklog_logger() -> Result<LoggerManager, InklogError> {
    init_inklog_logger_with_config(InklogConfig::default()).await
}

/// Initialize inklog with a custom configuration.
///
/// Accepts an [`InklogConfig`] so callers can control log level, output sinks
/// (console / file / database), and per-crate target levels. The returned
/// manager must be kept alive for the duration of the application.
///
/// # Audit event bridge
///
/// When the `inklog` feature is enabled, all `log::info!` / `tracing::info!`
/// calls inside limiteron — including the audit logger’s `write_batch` — are
/// automatically routed through inklog’s structured sinks. HMAC signing stays
/// in the limiteron application layer (audit-log feature) and is unaffected.
///
/// # Errors
///
/// Returns `Err(InklogError)` if the `LoggerManager` fails to construct.
///
/// # Example
///
/// ```rust,no_run
/// # #[cfg(feature = "inklog")]
/// # {
/// use limiteron::integrations::inklog::{init_inklog_logger_with_config, InklogConfig};
///
/// # tokio_test::block_on(async {
/// let mut config = InklogConfig::default();
/// config.global.level = "debug".to_string();
/// let _manager = init_inklog_logger_with_config(config).await.expect("init");
/// # });
/// # }
/// ```
pub async fn init_inklog_logger_with_config(
    config: InklogConfig,
) -> Result<LoggerManager, InklogError> {
    LoggerManager::with_config(config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R-inklog-001: `init_inklog_logger()` returns a `LoggerManager`.
    #[tokio::test]
    #[serial_test::serial]
    async fn init_inklog_logger_returns_manager() {
        let result = init_inklog_logger().await;
        assert!(result.is_ok(), "init_inklog_logger should return Ok");
    }

    /// R-inklog-001: `log`/`tracing` macros do not panic after inklog init.
    #[tokio::test]
    #[serial_test::serial]
    async fn log_macros_survive_after_inklog_init() {
        let _manager = init_inklog_logger()
            .await
            .expect("init_inklog_logger should succeed");
        log::info!("inklog bridge active");
        log::warn!("inklog bridge warn");
        tracing::info!(target: "limiteron", "tracing via inklog");
    }

    /// R-inklog-001: `init_inklog_logger()` is idempotent (repeat calls don't panic).
    #[tokio::test]
    #[serial_test::serial]
    async fn init_inklog_logger_is_idempotent() {
        let _first = init_inklog_logger()
            .await
            .expect("first init should succeed");
        let second = init_inklog_logger().await;
        assert!(
            second.is_ok(),
            "second init should still return Ok (install failure downgraded to warn)"
        );
    }

    /// `init_inklog_logger_with_config` accepts custom InklogConfig.
    #[tokio::test]
    #[serial_test::serial]
    async fn init_inklog_logger_with_config_works() {
        let config = InklogConfig::default();
        let result = init_inklog_logger_with_config(config).await;
        assert!(
            result.is_ok(),
            "init_inklog_logger_with_config should return Ok"
        );
    }
}

// ============================================================================
// inklog `SinkRateLimit` 端口实现（分层铁律：inklog 定义端口 +
// NoOp 默认，limiteron 作为上层提供真实 token 预算实现）
// ============================================================================

use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::Mutex;

/// 每目标（target）同步令牌桶：支持按流逝时间补充与写失败归还预算。
#[derive(Debug)]
struct SinkBucket {
    capacity: u64,
    refill_per_sec: f64,
    state: Mutex<SinkBucketState>,
}

#[derive(Debug)]
struct SinkBucketState {
    tokens: f64,
    last_refill: std::time::Instant,
}

impl SinkBucket {
    fn new(capacity: u64, refill_per_sec: f64) -> Self {
        Self {
            capacity,
            refill_per_sec,
            state: Mutex::new(SinkBucketState {
                tokens: capacity as f64,
                last_refill: std::time::Instant::now(),
            }),
        }
    }

    fn refill(&self) -> f64 {
        let mut st = self.state.lock();
        let elapsed = st.last_refill.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            st.tokens = (st.tokens + elapsed * self.refill_per_sec).min(self.capacity as f64);
            st.last_refill = std::time::Instant::now();
        }
        st.tokens
    }

    /// 同步 CAS 式扣减（端口决策点在 sink 写入前同步执行，不可 await）
    fn try_acquire(&self) -> bool {
        let mut tokens = self.refill();
        if tokens < 1.0 {
            return false;
        }
        tokens -= 1.0;
        self.state.lock().tokens = tokens;
        true
    }

    /// 写失败归还预算（下次重试有额度）
    fn refund(&self) {
        let mut st = self.state.lock();
        st.tokens = (st.tokens + 1.0).min(self.capacity as f64);
    }

    fn available(&self) -> u64 {
        self.refill() as u64
    }
}

/// limiteron 实现的 inklog [`SinkRateLimit`] 端口：按 target 分桶的令牌预算限流。
///
/// 策略（防护性默认，与 inklog 端口契约对齐）：
/// - **按 target 分桶**：日志洪水通常来自单个失控模块，预算按 target 隔离；
/// - **ERROR 直通**：严重日志不参与限流（排障关键时刻不可丢）；
/// - **写失败归还**：`report(Failed)` 归还预算（不因下游故障白扣额度）。
///
/// 注入形态（分层铁律：limiteron → inklog 合法上层依赖）：
///
/// ```rust,ignore
/// let limiter = Arc::new(LimiteronSinkRateLimit::new(100, 100.0));
/// let sink = RateLimitedSink::new(inner_sink, limiter);
/// inklog::LoggerManager::builder().add_sink(Arc::new(sink));
/// ```
pub struct LimiteronSinkRateLimit {
    capacity: u64,
    refill_per_sec: f64,
    buckets: DashMap<String, Arc<SinkBucket>>,
}

impl LimiteronSinkRateLimit {
    /// 以每 target 预算容量与补充速率创建
    pub fn new(capacity: u64, refill_per_sec: f64) -> Self {
        Self {
            capacity,
            refill_per_sec,
            buckets: DashMap::new(),
        }
    }

    /// 当前跟踪的 target 数（诊断）
    pub fn tracked_targets(&self) -> usize {
        self.buckets.len()
    }

    fn bucket_for(&self, target: &str) -> Arc<SinkBucket> {
        if let Some(b) = self.buckets.get(target) {
            return b.value().clone();
        }
        let bucket = Arc::new(SinkBucket::new(self.capacity, self.refill_per_sec));
        self.buckets.insert(target.to_string(), bucket.clone());
        bucket
    }

    /// 查询某 target 剩余预算（非消费）
    pub fn available(&self, target: &str) -> u64 {
        self.buckets
            .get(target)
            .map(|b| b.value().available())
            .unwrap_or(self.capacity)
    }
}

impl ::inklog::SinkRateLimit for LimiteronSinkRateLimit {
    fn try_acquire(&self, record: &::inklog::LogRecord) -> bool {
        // ERROR 直通：严重级别不受预算约束（防护性默认）
        if record.level == "ERROR" {
            return true;
        }
        self.bucket_for(&record.target).try_acquire()
    }

    fn report(&self, record: &::inklog::LogRecord, outcome: ::inklog::SinkWriteOutcome) {
        if outcome == ::inklog::SinkWriteOutcome::Failed {
            self.bucket_for(&record.target).refund();
        }
    }

    fn name(&self) -> &str {
        "limiteron-sink-rate-limit"
    }
}

#[cfg(test)]
mod sink_rate_limit_tests {
    use super::*;
    use ::inklog::{SinkRateLimit, SinkWriteOutcome};

    fn record(level: &str, target: &str) -> ::inklog::LogRecord {
        let mut r =
            ::inklog::LogRecord::new(tracing::Level::INFO, target.to_string(), "msg".to_string());
        r.level = level.to_string();
        r
    }

    /// 预算内放行、耗尽拒绝（按 target 消费）
    #[test]
    fn test_t617_sink_rate_limit_budget_enforced() {
        let lim = LimiteronSinkRateLimit::new(3, 0.0);
        assert!(lim.try_acquire(&record("INFO", "app::a")));
        assert!(lim.try_acquire(&record("INFO", "app::a")));
        assert!(lim.try_acquire(&record("INFO", "app::a")));
        assert!(
            !lim.try_acquire(&record("INFO", "app::a")),
            "预算耗尽必须拒绝"
        );
        assert_eq!(lim.available("app::a"), 0);
    }

    /// ERROR 级别直通（不消费预算、不受耗尽影响）
    #[test]
    fn test_t617_sink_rate_limit_error_bypasses() {
        let lim = LimiteronSinkRateLimit::new(1, 0.0);
        assert!(lim.try_acquire(&record("WARN", "app::b")));
        assert!(!lim.try_acquire(&record("WARN", "app::b")));
        for _ in 0..5 {
            assert!(
                lim.try_acquire(&record("ERROR", "app::b")),
                "ERROR 必须直通"
            );
        }
        assert_eq!(lim.available("app::b"), 0, "ERROR 直通不消费预算");
    }

    /// 按 target 隔离：不同模块预算互不影响
    #[test]
    fn test_t617_sink_rate_limit_targets_isolated() {
        let lim = LimiteronSinkRateLimit::new(2, 0.0);
        assert!(lim.try_acquire(&record("INFO", "mod::x")));
        assert!(lim.try_acquire(&record("INFO", "mod::x")));
        assert!(!lim.try_acquire(&record("INFO", "mod::x")));
        assert!(
            lim.try_acquire(&record("INFO", "mod::y")),
            "兄弟 target 不受影响"
        );
        assert_eq!(lim.tracked_targets(), 2);
    }

    /// 写失败归还预算（端口 report 契约）
    #[test]
    fn test_t617_sink_rate_limit_failed_report_refunds() {
        let lim = LimiteronSinkRateLimit::new(1, 0.0);
        let rec = record("INFO", "app::c");
        assert!(lim.try_acquire(&rec));
        assert!(!lim.try_acquire(&rec));
        lim.report(&rec, SinkWriteOutcome::Failed);
        assert_eq!(lim.available("app::c"), 1, "写失败必须归还预算");
        assert!(lim.try_acquire(&rec));
        // Written/Rejected 报告不改变预算
        lim.report(&rec, SinkWriteOutcome::Written);
        assert_eq!(lim.available("app::c"), 0);
    }

    /// 端口命名（诊断/指标标签契约）
    #[test]
    fn test_t617_sink_rate_limit_name() {
        let lim = LimiteronSinkRateLimit::new(10, 10.0);
        assert_eq!(SinkRateLimit::name(&lim), "limiteron-sink-rate-limit");
    }

    /// 时间补充：耗尽后按 refill_per_sec 恢复（上限容量）
    #[test]
    fn test_t617_sink_rate_limit_refills_over_time() {
        let lim = LimiteronSinkRateLimit::new(100, 100.0);
        let rec = record("INFO", "app::d");
        for _ in 0..100 {
            assert!(lim.try_acquire(&rec));
        }
        assert!(!lim.try_acquire(&rec));
        std::thread::sleep(std::time::Duration::from_millis(120));
        assert!(
            lim.available("app::d") >= 5,
            "120ms @ 100/s 应至少恢复 ~12 个预算（时钟偏移留裕量）"
        );
        assert!(lim.try_acquire(&rec));
    }
}
