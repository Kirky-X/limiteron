// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Quota Limiter
//!
//! Implements a simple quota-based limiter that tracks usage per key
//! with configurable limits and time windows.

use super::traits::RateLimitSnapshot;
use crate::error::LimiteronError;
#[cfg(feature = "quota-control")]
use crate::quota::QuotaConfig;
use crate::storage::QuotaStorage;
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Quota usage record for a single key
#[derive(Debug, Clone)]
struct QuotaRecord {
    /// Current usage count
    usage: u64,
    /// Window start time
    window_start: Instant,
}

/// QuotaLimiter - A simple quota-based rate limiter
///
/// Tracks usage per identifier key within a time window.
/// When a key exceeds its quota limit, requests are rejected.
pub struct QuotaLimiter {
    /// Quota configuration
    config: QuotaConfig,
    /// Per-key usage tracking (key -> usage, window_start)
    usage: Arc<DashMap<String, QuotaRecord>>,
    /// 过期清理 single-flight 守卫
    ///
    /// `len() > 上限` 检查与 `retain()` 非原子：并发插入下若不加守卫，
    /// 每个并发调用都会执行 O(n) 全表 retain，清理本身反而成为放大器。
    /// CAS 保证同一时刻至多一个清理在执行。
    cleanup_in_progress: AtomicBool,
    /// 可选共享账本后端。
    ///
    /// 注入后 allow/check 经 `QuotaStorage::consume` 原子裁决（cache 路径
    /// 单 Lua 原子、DB 路径条件 UPDATE，跨实例正确），账本持久化于后端；
    /// 默认 `None` = 纯内存模式——单实例语义，多实例部署各自计数、
    /// 进程重启清零，语义差异须由调用方文档标注。
    storage: Option<Arc<dyn QuotaStorage>>,
    /// storage 模式的资源标识（共享账本的 resource 维度）
    resource: String,
}

/// 链式/无 key 场景（`Limiter::allow` 不提供 key）使用的匿名配额桶键。
/// 避免与真实用户键冲突的低概率前缀。
const ANONYMOUS_QUOTA_KEY: &str = "__limiteron_anonymous_quota__";

/// 每 key 用量记录的跟踪上限（高基数 key 内存约束）
const QUOTA_MAX_TRACKED_KEYS: usize = 10_000;

impl QuotaLimiter {
    /// Creates a new QuotaLimiter with the given configuration.
    ///
    /// # Arguments
    /// * `config` - Quota configuration including limit, window size, etc.
    ///
    /// # Panic
    ///
    /// `config.window_size == 0` 时 panic。
    /// `window_size = 0` 会导致 `Duration::from_secs(0)` 窗口立即过期，
    /// 每次请求都重置 usage，配额限制形同虚设——这是配置 bug，应在开发阶段发现。
    /// 用 `assert!` 而非 `Result` 以保持 API 兼容性（Rule 12：失败必须显性化）。
    ///
    /// # Examples
    /// ```rust
    /// use limiteron::limiters::QuotaLimiter;
    /// use limiteron::quota::QuotaConfig;
    /// use limiteron::quota::QuotaType;
    ///
    /// let config = QuotaConfig {
    ///     quota_type: QuotaType::Count,
    ///     limit: 1000,
    ///     window_size: 3600,
    ///     allow_overdraft: false,
    ///     overdraft_limit_percent: 20,
    ///     alert_config: Default::default(),
    /// };
    /// let limiter = QuotaLimiter::new(config);
    /// ```
    pub fn new(config: QuotaConfig) -> Self {
        assert!(
            config.window_size > 0,
            "QuotaConfig.window_size must be greater than 0 (audit-L-003); \
             window_size=0 would cause immediate window expiry, making quota useless"
        );
        Self {
            config,
            usage: Arc::new(DashMap::new()),
            cleanup_in_progress: AtomicBool::new(false),
            storage: None,
            resource: String::new(),
        }
    }

    /// Creates a storage-backed QuotaLimiter.
    ///
    /// 注入 `QuotaStorage` 后，配额裁决委托后端的原子 consume
    ///（跨实例一致、账本持久化）；`resource` 作为账本维度。
    /// 默认 `new()` 构造的纯内存模式保持单实例语义不变。
    ///
    /// # Panic
    ///
    /// 同 `new()`：`config.window_size == 0` 时 panic。
    pub fn with_storage(
        config: QuotaConfig,
        storage: Arc<dyn QuotaStorage>,
        resource: impl Into<String>,
    ) -> Self {
        assert!(
            config.window_size > 0,
            "QuotaConfig.window_size must be greater than 0 (audit-L-003); \
             window_size=0 would cause immediate window expiry, making quota useless"
        );
        Self {
            config,
            usage: Arc::new(DashMap::new()),
            cleanup_in_progress: AtomicBool::new(false),
            storage: Some(storage),
            resource: resource.into(),
        }
    }

    /// 获取配额上限
    ///
    /// # 注意
    ///
    /// 仅供 `LimiterManager` 参数一致性校验使用，不应在业务代码中调用。
    /// 业务代码应通过 `Limiter::check` 接口与 limiter 交互，而非直接读取配置。
    pub fn max(&self) -> u64 {
        self.config.limit
    }

    /// 获取配额周期
    ///
    /// # 注意
    ///
    /// 仅供 `LimiterManager` 参数一致性校验使用，不应在业务代码中调用。
    /// 业务代码应通过 `Limiter::check` 接口与 limiter 交互，而非直接读取配置。
    pub fn period(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.config.window_size)
    }

    /// 生效配额上限（含透支）
    ///
    /// 饱和算术：limit 接近 u64::MAX 时乘/加不得回绕（回绕会使 max_usage
    /// 反而变小，虽然方向偏保守，但属未定义语义）
    fn max_usage(&self) -> u64 {
        if self.config.allow_overdraft {
            let overdraft_limit = self
                .config
                .limit
                .saturating_mul(self.config.overdraft_limit_percent as u64)
                / 100;
            self.config.limit.saturating_add(overdraft_limit)
        } else {
            self.config.limit
        }
    }

    /// storage 模式裁决路径：委托 `QuotaStorage::consume` 原子扣减
    async fn consume_via_storage(
        &self,
        storage: &Arc<dyn QuotaStorage>,
        key: &str,
        cost: u64,
    ) -> Result<bool, LimiteronError> {
        // 零成本请求放行且不落账
        if cost == 0 {
            return Ok(true);
        }
        let result = storage
            .consume(
                key,
                &self.resource,
                cost,
                self.max_usage(),
                Duration::from_secs(self.config.window_size),
            )
            .await
            .map_err(LimiteronError::StorageError)?;
        if result.allowed {
            Ok(true)
        } else {
            Err(LimiteronError::QuotaExceeded(format!(
                "Quota exceeded for key '{}': storage ledger rejected (requested {})",
                key, cost
            )))
        }
    }

    /// 匿名桶只读快照（不落账、不创建记录）
    ///
    /// 链式/无 key 场景的余额查询入口：为决策链的限流头
    /// （RateLimit-Limit / Retry-After）提供真实值，避免落入 limit=0 兜底。
    fn anonymous_snapshot(&self) -> RateLimitSnapshot {
        let now = Instant::now();
        let window_duration = Duration::from_secs(self.config.window_size);
        let max_usage = self.max_usage();

        // 只读查询：记录不存在或窗口已过期时按满额报告
        let (usage, elapsed) = match self.usage.get(ANONYMOUS_QUOTA_KEY) {
            Some(rec) if now.duration_since(rec.window_start) < window_duration => {
                (rec.usage, now.duration_since(rec.window_start))
            }
            _ => (0, Duration::ZERO),
        };

        let reset_secs = if usage == 0 {
            0
        } else {
            (window_duration - elapsed.min(window_duration)).as_secs()
        };

        RateLimitSnapshot {
            limit: max_usage,
            remaining: max_usage.saturating_sub(usage),
            reset_secs,
        }
    }

    /// Checks and consumes quota for the given key.
    ///
    /// # Arguments
    /// * `key` - The identifier key (user ID, API key, etc.)
    /// * `cost` - 本次请求消耗的额度。历史教训：曾固定每次扣 1，忽略 cost
    ///   参数——Token/金额类配额语义失效。`cost=0` 放行不落账。
    ///
    /// # Returns
    /// * `Ok(true)` - Quota available, consumption successful
    /// * `Ok(false)` - Unused (保留位)
    /// * `Err(LimiteronError)` - Quota exceeded or error
    async fn check_and_consume(&self, key: &str, cost: u64) -> Result<bool, LimiteronError> {
        // 零成本请求放行且不落账
        if cost == 0 {
            return Ok(true);
        }
        let now = Instant::now();
        let window_duration = Duration::from_secs(self.config.window_size);

        // 防攻击者可控的高基数 key 无限增长（OOM DoS）——
        // 超过跟踪上限时清理已过期窗口的记录，把内存约束在 ~上限 + 单窗口新增以内。
        // single-flight：并发下仅一个调用执行 O(n) retain，避免清理本身成为热路径放大器。
        if self.usage.len() > QUOTA_MAX_TRACKED_KEYS
            && self
                .cleanup_in_progress
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
        {
            self.usage
                .retain(|_, rec| now.duration_since(rec.window_start) < window_duration);
            self.cleanup_in_progress.store(false, Ordering::Release);
        }

        let mut record = self
            .usage
            .entry(key.to_string())
            .or_insert_with(|| QuotaRecord {
                usage: 0,
                window_start: now,
            });

        // Check if window has expired
        if now.duration_since(record.window_start) >= window_duration {
            // Reset for new window
            record.usage = 0;
            record.window_start = now;
        }

        let max_usage = self.max_usage();

        // 按成本判限：usage + cost > max_usage 才拒绝（饱和减法防溢出，
        // cost ≤ 剩余额度保证后续加法不回绕）
        if cost > max_usage.saturating_sub(record.usage) {
            return Err(LimiteronError::QuotaExceeded(format!(
                "Quota exceeded for key '{}': used {}/{} (requested {})",
                key, record.usage, max_usage, cost
            )));
        }

        record.usage = record.usage.saturating_add(cost);
        Ok(true)
    }

    /// 检查并消耗单次请求配额，超限时以窗口剩余秒数上报
    ///
    /// 与 [`Limiter::check`](crate::limiters::Limiter::check) 的单请求语义一致
    /// （cost=1），差异仅在超限上报形态：`check` 返回 `Err(QuotaExceeded)`，
    /// 本方法返回 `Err(窗口剩余秒数)`，供调用方直接填充 Retry-After 类
    /// 响应头，免去解析错误串。
    ///
    /// # 剩余秒数口径
    ///
    /// `window_size - 窗口已流逝时长`，按 `Duration::as_secs()` 向下取整；
    /// 剩余不足 1 秒时报 0（与 `anonymous_snapshot` 的 reset_secs 截断口径
    /// 一致）。检查与读表之间窗口被并发翻转时取值偏保守（偏大），
    /// 不会导致提前重试。
    ///
    /// # storage 模式
    ///
    /// 共享账本不暴露窗口起点，超限与后端错误统一映射为完整 `window_size`
    /// （保守值）；后端真实故障无法经 `u64` 通道区分，需调用方监控覆盖。
    pub async fn check_retry_after(&self, key: &str) -> Result<(), u64> {
        if let Some(storage) = &self.storage {
            // 单请求语义：固定 cost=1
            return self
                .consume_via_storage(storage, key, 1)
                .await
                .map(|_| ())
                .map_err(|_| self.config.window_size);
        }
        match self.check_and_consume(key, 1).await {
            Ok(_) => Ok(()),
            Err(LimiteronError::QuotaExceeded(_)) => Err(self.window_remaining_secs(key)),
            // 当前内存模式仅产出 QuotaExceeded；兜底保守取完整窗口
            Err(_) => Err(self.config.window_size),
        }
    }

    /// key 当前窗口剩余秒数（记录不存在按满窗计算；向下取整）
    fn window_remaining_secs(&self, key: &str) -> u64 {
        let now = Instant::now();
        let window_duration = Duration::from_secs(self.config.window_size);
        let elapsed = self
            .usage
            .get(key)
            .map(|rec| now.duration_since(rec.window_start))
            .unwrap_or(Duration::ZERO);
        (window_duration - elapsed.min(window_duration)).as_secs()
    }
}

#[async_trait]
impl crate::limiters::Limiter for QuotaLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError> {
        if let Some(storage) = &self.storage {
            // allow 契约：拒绝映射为 Ok(false)，而非错误语义冒泡
            return match self
                .consume_via_storage(storage, ANONYMOUS_QUOTA_KEY, cost)
                .await
            {
                Ok(ok) => Ok(ok),
                Err(LimiteronError::QuotaExceeded(_)) => Ok(false),
                Err(e) => Err(e),
            };
        }
        // 链式/无 key 场景下无法按用户键跟踪：对内部匿名桶按 cost 消耗配额，
        // 使配额规则经决策链挂载时真实生效。
        // 超出限制映射为 Ok(false)（拒绝语义），而非错误语义。
        match self.check_and_consume(ANONYMOUS_QUOTA_KEY, cost).await {
            Ok(ok) => Ok(ok),
            Err(LimiteronError::QuotaExceeded(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn check(&self, key: &str) -> Result<(), LimiteronError> {
        if let Some(storage) = &self.storage {
            // check() 为单请求语义：固定 cost=1
            return self.consume_via_storage(storage, key, 1).await.map(|_| ());
        }
        // check() 为单请求语义：固定 cost=1
        self.check_and_consume(key, 1).await.map(|_| ())
    }

    /// 非消费预检：匿名桶余额快照（不落账）
    async fn peek(&self, _cost: u64) -> Result<RateLimitSnapshot, LimiteronError> {
        Ok(self.anonymous_snapshot())
    }

    /// 剩余额度查询（非消费）：匿名桶余额快照
    async fn remaining(&self) -> Result<RateLimitSnapshot, LimiteronError> {
        Ok(self.anonymous_snapshot())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limiters::Limiter;
    use crate::quota::QuotaType;

    fn create_test_config() -> QuotaConfig {
        QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 10,
            window_size: 60,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_quota_limiter_allows_within_limit() {
        let config = create_test_config();
        let limiter = QuotaLimiter::new(config);

        // Should allow 10 requests
        for i in 0..10 {
            let result = limiter.check("user1").await;
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }
    }

    #[tokio::test]
    async fn test_quota_limiter_rejects_over_limit() {
        let config = create_test_config();
        let limiter = QuotaLimiter::new(config);

        // Use up the quota
        for _ in 0..10 {
            let _ = limiter.check("user1").await;
        }

        // Next request should be rejected
        let result = limiter.check("user1").await;
        assert!(result.is_err());
        assert!(matches!(result, Err(LimiteronError::QuotaExceeded(_))));
    }

    #[tokio::test]
    async fn test_quota_limiter_independent_keys() {
        let config = create_test_config();
        let limiter = QuotaLimiter::new(config);

        // user1 uses 10 requests
        for _ in 0..10 {
            let _ = limiter.check("user1").await;
        }

        // user2 should still be able to make requests
        let result = limiter.check("user2").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_quota_limiter_with_overdraft() {
        let mut config = create_test_config();
        config.allow_overdraft = true;
        config.overdraft_limit_percent = 20; // 20% overdraft

        let limiter = QuotaLimiter::new(config);

        // Should allow 10 + 2 = 12 requests (10 limit + 20% overdraft)
        for i in 0..12 {
            let result = limiter.check("user1").await;
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }

        // Next request should be rejected
        let result = limiter.check("user1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_quota_limiter_allow_method() {
        // allow() 现在对匿名桶消耗配额
        // 到上限后返回 Ok(false)（拒绝语义），使链式挂载真实生效。
        let config = create_test_config(); // limit = 10
        let limiter = QuotaLimiter::new(config);

        // 前 10 次允许
        for _ in 0..10 {
            let result = limiter.allow(1).await;
            assert!(result.is_ok());
            assert!(result.unwrap());
        }

        // 第 11 次拒绝
        let result = limiter.allow(1).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_quota_limiter_cost_semantics() {
        // cost 语义回归：修复前 allow(_cost) 每次只扣 1，忽略 cost 参数
        // ——Token/金额类配额语义失效。现按 cost 扣减。
        let config = create_test_config(); // limit = 10
        let limiter = QuotaLimiter::new(config);

        assert!(limiter.allow(4).await.unwrap());
        assert!(limiter.allow(4).await.unwrap());
        // 剩 2，cost=4 超额拒绝
        assert!(!limiter.allow(4).await.unwrap());
        // 剩 2，cost=2 恰好用完
        assert!(limiter.allow(2).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_quota_limiter_zero_cost_passes_without_consuming() {
        // cost=0 放行不落账：零成本请求不占额度
        let config = create_test_config(); // limit = 10
        let limiter = QuotaLimiter::new(config);

        for _ in 0..50 {
            assert!(limiter.allow(0).await.unwrap(), "cost=0 应放行且不落账");
        }
        // 账面未被零成本请求侵蚀：正常成本额度完整
        for _ in 0..10 {
            assert!(limiter.allow(1).await.unwrap());
        }
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_quota_limiter_window_expiry_reset() {
        // 使用 1 秒窗口，等待过期后验证重置
        let mut config = create_test_config();
        config.window_size = 1; // 1 秒窗口
        config.limit = 3;

        let limiter = QuotaLimiter::new(config);

        // 用完配额
        for _ in 0..3 {
            assert!(limiter.check("user1").await.is_ok());
        }
        // 此时应被拒绝
        assert!(limiter.check("user1").await.is_err());

        // 等待窗口过期
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // 窗口重置后应再次允许
        let result = limiter.check("user1").await;
        assert!(
            result.is_ok(),
            "Request after window reset should be allowed"
        );
    }

    #[tokio::test]
    async fn test_quota_limiter_overdraft_boundary() {
        // 测试透支边界：limit + overdraft_limit 恰好用完
        let mut config = create_test_config();
        config.limit = 10;
        config.allow_overdraft = true;
        config.overdraft_limit_percent = 50; // 50% overdraft = 5 extra

        let limiter = QuotaLimiter::new(config);

        // Should allow 10 + 5 = 15 requests
        for i in 0..15 {
            let result = limiter.check("user1").await;
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }

        // 16th should be rejected
        let result = limiter.check("user1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_quota_limiter_check_propagates_error() {
        // 当 check_and_consume 返回 Err 时，check 应传播错误
        let config = create_test_config();
        let limiter = QuotaLimiter::new(config);

        // 用完配额
        for _ in 0..10 {
            let _ = limiter.check("user1").await;
        }

        // 下一个 check 应返回 QuotaExceeded 错误
        let result = limiter.check("user1").await;
        assert!(result.is_err());
        assert!(matches!(result, Err(LimiteronError::QuotaExceeded(_))));
    }

    // ========================================================================
    // audit-macro-followup 修复20 window_size=0 panic 测试
    // ========================================================================

    #[test]
    #[should_panic(expected = "QuotaConfig.window_size must be greater than 0")]
    fn test_quota_limiter_window_size_zero_panics() {
        // window_size=0 会导致窗口立即过期，配额限制失效
        // 应在 new() 阶段 panic 而非静默接受错误配置（Rule 12）
        let mut config = create_test_config();
        config.window_size = 0;
        let _ = QuotaLimiter::new(config);
    }

    /// 共享账本 mock：跨 QuotaLimiter 实例共享单一账本
    struct MockSharedLedger {
        consumed: std::sync::atomic::AtomicU64,
    }

    #[async_trait]
    impl crate::storage::QuotaStorage for MockSharedLedger {
        async fn get_quota(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<crate::storage::QuotaInfo>, crate::error::StorageError> {
            Ok(None)
        }

        async fn consume(
            &self,
            _: &str,
            _: &str,
            cost: u64,
            limit: u64,
            _: Duration,
        ) -> Result<crate::error::ConsumeResult, crate::error::StorageError> {
            use std::sync::atomic::Ordering;
            let cur = self.consumed.fetch_add(cost, Ordering::SeqCst) + cost;
            if cur <= limit {
                Ok(crate::error::ConsumeResult::allowed(cur, limit))
            } else {
                // 超限自回滚（模拟真实后端的原子拒绝）
                self.consumed.fetch_sub(cost, Ordering::SeqCst);
                Ok(crate::error::ConsumeResult::rejected(cur - cost, limit))
            }
        }

        async fn reset(
            &self,
            _: &str,
            _: &str,
            _: u64,
            _: Duration,
        ) -> Result<(), crate::error::StorageError> {
            self.consumed.store(0, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_quota_limiter_storage_backed_mode() {
        // storage-backed 模式：多个实例共享同一账本，跨实例额度可见；
        // 纯内存实例不受账本影响。
        let ledger = Arc::new(MockSharedLedger {
            consumed: std::sync::atomic::AtomicU64::new(0),
        });
        let a = QuotaLimiter::with_storage(create_test_config(), ledger.clone(), "api_calls");
        let b = QuotaLimiter::with_storage(create_test_config(), ledger.clone(), "api_calls");

        assert!(a.allow(6).await.unwrap());
        assert!(
            !b.allow(6).await.unwrap(),
            "共享账本下 B 应看到 A 的消耗(6+6>10)"
        );
        assert!(b.allow(4).await.unwrap());
        assert!(!b.allow(1).await.unwrap(), "账本已满 10/10,应拒绝");

        // 纯内存实例与账本无关
        let solo = QuotaLimiter::new(create_test_config());
        assert!(solo.allow(10).await.unwrap());
    }

    #[tokio::test]
    async fn test_quota_limiter_remaining_snapshot() {
        // 快照回归：修复前 remaining/peek 走 trait 默认 Err → 链上拒绝时
        // 限流头落入 limit=0 兜底（decision_chain 的 Retry-After 语义失真）。
        use crate::limiters::Limiter;
        let config = create_test_config(); // limit=10, window=60
        let limiter = QuotaLimiter::new(config);

        // 未消耗：报告满额
        let snap = limiter.remaining().await.unwrap();
        assert_eq!(snap.limit, 10);
        assert_eq!(snap.remaining, 10);

        // 消耗 3 后：remaining 反映真实余额，peek 不扣减
        assert!(limiter.allow(3).await.unwrap());
        let snap = limiter.remaining().await.unwrap();
        assert_eq!(snap.remaining, 7);
        assert!(snap.reset_secs <= 60);
        let peeked = limiter.peek(1).await.unwrap();
        assert_eq!(peeked.remaining, 7, "peek 不扣减");
    }

    #[tokio::test]
    async fn test_quota_limiter_window_size_one_works() {
        // 边界：window_size=1（最小合法值）应正常工作
        let mut config = create_test_config();
        config.window_size = 1;
        config.limit = 3;
        let limiter = QuotaLimiter::new(config);

        // window_size=1s，前 3 个请求应成功
        for _ in 0..3 {
            assert!(limiter.check("boundary_user").await.is_ok());
        }
        // 第 4 个应失败
        assert!(limiter.check("boundary_user").await.is_err());
    }

    // ========================================================================
    // check_retry_after：超限以窗口剩余秒数上报（Retry-After 语义）
    // ========================================================================

    #[tokio::test]
    async fn test_check_retry_after_ok_when_quota_available() {
        let config = create_test_config(); // limit=10, window=60
        let limiter = QuotaLimiter::new(config);

        assert!(
            limiter.check_retry_after("ra_user").await.is_ok(),
            "配额充足应放行"
        );
    }

    #[tokio::test]
    async fn test_check_retry_after_ok_after_window_reset() {
        // 过期分支：窗口翻转后配额重置，请求放行
        let mut config = create_test_config();
        config.window_size = 1;
        config.limit = 1;
        let limiter = QuotaLimiter::new(config);

        assert!(limiter.check_retry_after("ra_reset").await.is_ok());
        assert!(limiter.check_retry_after("ra_reset").await.is_err());
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;
        assert!(
            limiter.check_retry_after("ra_reset").await.is_ok(),
            "窗口翻转后应重置放行"
        );
    }

    #[tokio::test]
    async fn test_check_retry_after_err_reports_remaining_secs() {
        // 未过期分支：超限拒绝，Err 携带窗口剩余秒数（>0，≤完整窗口）
        let config = create_test_config(); // limit=10, window=60
        let limiter = QuotaLimiter::new(config);

        for _ in 0..10 {
            assert!(limiter.check_retry_after("ra_remaining").await.is_ok());
        }
        let remaining = limiter
            .check_retry_after("ra_remaining")
            .await
            .expect_err("配额耗尽应返回 Err");
        assert!(
            (1..=60).contains(&remaining),
            "剩余秒数应在 (0, 60] 内，实际 {remaining}"
        );
    }

    #[tokio::test]
    async fn test_check_retry_after_zero_remaining_boundary() {
        // 恰 0 秒边界：剩余不足 1 秒时向下取整为 0（与 anonymous_snapshot
        // 的 reset_secs 截断口径一致）。不依赖调度时序：sleep 后若窗口
        // 已翻转（全量并发负载下调度间隔可能超 1s），走「重置放行」
        // 分支；否则断言 Err(0) 分支。
        let mut config = create_test_config();
        config.window_size = 1;
        config.limit = 1;
        let limiter = QuotaLimiter::new(config);

        assert!(limiter.check_retry_after("ra_zero").await.is_ok());
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        match limiter.check_retry_after("ra_zero").await {
            // 窗口已翻转：重置放行，属正确语义的另一分支
            Ok(()) => {}
            Err(remaining) => assert_eq!(remaining, 0, "窗口未翻转且剩余 <1s 应截断为 0"),
        }
    }
}
