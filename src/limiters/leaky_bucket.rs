// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 漏桶限流器模块
//!
//! 漏桶算法的**水位计型（meter）**实现：请求以任意速率入桶（水位上升），
//! 桶以恒定速率漏出（水位下降），桶满即拒。判定语义与令牌桶严格对偶
//! （水位 = 容量 − 令牌数），选型差异仅在记账方向与快照口径（漏桶的
//! `remaining` 报告剩余可入桶空间、`reset` 报告漏空时间）；不做请求
//! 排队/延迟放行——需要流出时间点整形的场景应在服务层排队。
//!
//! 并发模型：状态为 `parking_lot::Mutex` 单点串行（水位与记账时间须
//! 配对更新，无锁化需双字 CAS）。低争用场景开销可忽略；单实例被大量
//! worker 共享且判定为热路径瓶颈时，优先选无锁的
//! [`crate::limiters::TokenBucketLimiter`]（判定语义对偶）。

use super::traits::{Limiter, RateLimitSnapshot, validate_cost};
use crate::clock::{Clock, SystemClock};
use crate::constants::{MAX_TOKEN_BUCKET_CAPACITY, MAX_TOKEN_BUCKET_REFILL_RATE};
use crate::error::LimiteronError;
use async_trait::async_trait;
use parking_lot::Mutex;
use std::sync::Arc;

/// 漏桶限流器
///
/// 请求到达时向桶中注入 `cost` 单位水量，水位超过桶容量即拒绝；
/// 水位以 `leak_rate`（单位/秒）恒定漏出，将突发输入整形成恒定输出。
///
/// # 特性
/// - 漏出积分守恒：亚单位时间积分滞留下次漏出累计（与令牌桶补充对称），
///   低速率场景有效漏出速率不低于配置值
/// - 漏空时积压积分随漏空丢弃：桶已空则未来漏出只取决于未来流入，
///   不得凭历史时间积分「欠漏」出额外空间
/// - [`Limiter::peek`]/[`Limiter::remaining`] 纯读虚拟推演，零副作用
pub struct LeakyBucketLimiter {
    /// 桶容量（突发上限）
    capacity: u64,
    /// 漏出速率（单位/秒）
    leak_rate: u64,
    /// 时钟实例
    clock: Arc<dyn Clock>,
    /// 桶状态（水位 + 上次漏出时间，锁保护下配对更新）
    state: Mutex<LeakyState>,
}

#[derive(Debug)]
struct LeakyState {
    /// 当前水位
    water: u64,
    /// 上次漏出记账时间（纳秒时间戳）
    last_leak_nanos: u64,
}

impl LeakyBucketLimiter {
    /// 创建漏桶限流器（系统时钟）
    ///
    /// # 参数
    /// * `capacity` - 桶容量（突发上限）
    /// * `leak_rate` - 漏出速率（单位/秒）
    pub fn new(capacity: u64, leak_rate: u64) -> Result<Self, LimiteronError> {
        Self::with_clock(capacity, leak_rate, Arc::new(SystemClock))
    }

    /// 以自定义时钟创建（测试注入用）
    pub fn with_clock(
        capacity: u64,
        leak_rate: u64,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, LimiteronError> {
        if capacity == 0 {
            return Err(LimiteronError::ConfigError(
                "leaky bucket capacity must be non-zero".to_string(),
            ));
        }
        if leak_rate == 0 {
            return Err(LimiteronError::ConfigError(
                "leaky bucket leak_rate must be non-zero".to_string(),
            ));
        }
        // 上限与工厂校验（LimiterFactory::validate_config）同源，防止绕过
        // 工厂直构超大桶导致 u128→u64 记账中间量失真（防御纵深）
        if capacity > MAX_TOKEN_BUCKET_CAPACITY {
            return Err(LimiteronError::ConfigError(format!(
                "leaky bucket capacity must not exceed {MAX_TOKEN_BUCKET_CAPACITY}"
            )));
        }
        if leak_rate > MAX_TOKEN_BUCKET_REFILL_RATE {
            return Err(LimiteronError::ConfigError(format!(
                "leaky bucket leak_rate must not exceed {MAX_TOKEN_BUCKET_REFILL_RATE}"
            )));
        }
        let now = clock.unix_timestamp_nanos();
        Ok(Self {
            capacity,
            leak_rate,
            clock,
            state: Mutex::new(LeakyState {
                water: 0,
                last_leak_nanos: now,
            }),
        })
    }

    /// 获取桶容量
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// 获取漏出速率
    pub fn leak_rate(&self) -> u64 {
        self.leak_rate
    }

    /// 当前水位（纯读：按恒定漏出虚拟推演至当前时刻，不改记账状态）
    pub async fn water_level(&self) -> u64 {
        let state = self.state.lock();
        self.virtual_water(&state)
    }

    /// 虚拟漏出推演：从记账状态推算当前真实水位（不写回）
    fn virtual_water(&self, state: &LeakyState) -> u64 {
        let now = self.clock.unix_timestamp_nanos();
        let elapsed = now.saturating_sub(state.last_leak_nanos);
        let leaked = ((elapsed as u128) * (self.leak_rate as u128) / 1_000_000_000u128)
            .try_into()
            .unwrap_or(u64::MAX);
        state.water.saturating_sub(leaked)
    }

    /// 真实漏出：按恒定速率把水位降至当前时刻
    ///
    /// 漏出记账与令牌桶补充对称：整数额漏出消费掉的时间
    /// （`leaked × 1e9 / rate`）推进 `last_leak_nanos`，亚单位时间积分
    /// 滞留下次累计；漏空（`leaked ≥ water`）时记账时间直接推到 now——
    /// 桶已空，多余时间积分随漏空丢弃，不参与后续记账。
    fn leak(&self, state: &mut LeakyState) {
        let now = self.clock.unix_timestamp_nanos();
        let elapsed = now.saturating_sub(state.last_leak_nanos);
        if elapsed == 0 {
            return;
        }
        let leaked = ((elapsed as u128) * (self.leak_rate as u128) / 1_000_000_000u128)
            .try_into()
            .unwrap_or(u64::MAX);
        if leaked == 0 {
            return;
        }
        if leaked >= state.water {
            state.water = 0;
            state.last_leak_nanos = now;
        } else {
            state.water -= leaked;
            let consumed: u64 = (leaked as u128)
                .checked_mul(1_000_000_000u128)
                .map(|v| v / (self.leak_rate as u128))
                .and_then(|v| u64::try_from(v).ok())
                .unwrap_or(u64::MAX);
            state.last_leak_nanos += consumed.min(elapsed);
        }
    }
}

#[async_trait]
impl Limiter for LeakyBucketLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError> {
        validate_cost(cost)?;
        let mut state = self.state.lock();
        self.leak(&mut state);
        // 桶满即拒：注入 cost 后水位不得超过容量
        if cost > self.capacity.saturating_sub(state.water) {
            return Ok(false);
        }
        state.water += cost;
        Ok(true)
    }

    /// 非消费预检：虚拟漏出推演后读取剩余可入桶空间，不扣减
    async fn peek(&self, cost: u64) -> Result<RateLimitSnapshot, LimiteronError> {
        validate_cost(cost)?;
        let state = self.state.lock();
        let water = self.virtual_water(&state);
        Ok(self.snapshot(water))
    }

    /// 剩余额度查询（非消费）
    async fn remaining(&self) -> Result<RateLimitSnapshot, LimiteronError> {
        let state = self.state.lock();
        let water = self.virtual_water(&state);
        Ok(self.snapshot(water))
    }
}

impl LeakyBucketLimiter {
    /// 以水位渲染标准限流头快照（limit=容量，remaining=剩余可入桶空间）
    fn snapshot(&self, water: u64) -> RateLimitSnapshot {
        // 桶漏空所需时间 = 水位 / 漏出速率（向上取整秒）
        let reset_secs = water.div_ceil(self.leak_rate);
        RateLimitSnapshot {
            limit: self.capacity,
            remaining: self.capacity.saturating_sub(water),
            reset_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;
    use std::time::Duration;

    #[tokio::test]
    async fn test_leaky_bucket_basic_allow_and_reject() {
        let limiter = LeakyBucketLimiter::new(10, 5).unwrap();

        // 空桶初始水位 0，容量内注入全部成功
        for _ in 0..10 {
            assert!(limiter.allow(1).await.unwrap());
        }
        assert_eq!(limiter.water_level().await, 10);

        // 桶满后再注入即拒
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_leaky_bucket_cost_exceeding_capacity_always_rejected() {
        let limiter = LeakyBucketLimiter::new(10, 5).unwrap();

        // 单笔注入超过桶容量，即使空桶也必须拒绝（漏桶硬顶）
        assert!(!limiter.allow(11).await.unwrap());
        assert_eq!(limiter.water_level().await, 0, "拒绝的注入不得改变水位");
    }

    #[tokio::test]
    async fn test_leaky_bucket_constant_leak_with_mock_clock() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter = LeakyBucketLimiter::with_clock(10, 5, clock).unwrap();

        assert!(limiter.allow(10).await.unwrap());
        assert_eq!(limiter.water_level().await, 10);

        // 前进 1s @5/s → 漏出 5
        mock_clock.advance(Duration::from_secs(1));
        assert_eq!(limiter.water_level().await, 5);

        // 漏出后腾出空间，可再注入
        assert!(limiter.allow(5).await.unwrap());
        assert_eq!(limiter.water_level().await, 10);
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_leaky_bucket_drain_frees_space_for_burst() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter = LeakyBucketLimiter::with_clock(10, 10, clock).unwrap();

        assert!(limiter.allow(10).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());

        // 完整漏空一周期后应恢复整桶容量（突发整形的周期性放行）
        mock_clock.advance(Duration::from_secs(1));
        for _ in 0..10 {
            assert!(limiter.allow(1).await.unwrap());
        }
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_leaky_bucket_leak_capped_at_empty_no_negative_accumulation() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter = LeakyBucketLimiter::with_clock(10, 100, clock).unwrap();

        // 只注入 1 单位，随后远超漏空所需时间
        assert!(limiter.allow(1).await.unwrap());
        mock_clock.advance(Duration::from_secs(100));
        assert_eq!(limiter.water_level().await, 0, "漏出受水位封顶，不得为负");

        // 漏空后不得凭历史积压时间「找回」突放空间：立即注入恰容量即满
        assert!(limiter.allow(10).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_leaky_bucket_fraction_conservation() {
        // 漏出积分守恒回归：rate=3/s，每 ~500ms 触发一次漏出，每周期应漏
        // 1.5 个单位。修复前每周期 floor(1.5)=1 且记账时间推到 now，
        // 0.5 单位时间积分永久丢失 → 10 周期只漏 ~10 个；
        // 修复后积分滞留按 1,2,1,2 交替累计 → ~15 个。
        // （真实时钟 + 抖动鲁棒区间。）
        let limiter = LeakyBucketLimiter::new(100, 3).unwrap();

        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            assert!(limiter.allow(1).await.unwrap(), "低速率漏桶应持续放行");
        }

        let water = limiter.water_level().await;
        assert!(
            water <= 5,
            "10×500ms@3/s 积分守恒应漏出 ~15 单位(毛注入 10-漏出 ~15≤0)，实际积压 {water}"
        );
    }

    #[tokio::test]
    async fn test_leaky_bucket_peek_remaining_pure() {
        // peek/remaining 零副作用契约：纯读虚拟推演，绝不推进记账/改水位
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter = LeakyBucketLimiter::with_clock(10, 5, clock).unwrap();

        assert!(limiter.allow(10).await.unwrap());
        mock_clock.advance(Duration::from_secs(2));

        let snap = limiter.remaining().await.unwrap();
        assert_eq!(snap.limit, 10);
        assert_eq!(
            snap.remaining, 10,
            "10@5/s 漏 10，虚拟推演后桶已空，剩余可入桶空间应为满容量"
        );

        // 真实状态必须原封不动：水位记账仍在 2s 前，直接再推演应同值
        for _ in 0..100 {
            let _ = limiter.peek(1).await.unwrap();
        }
        assert_eq!(limiter.water_level().await, 0);

        // peek 报告的剩余空间随后 allow 真实可得（peek 与 allow 一致性）
        assert!(limiter.allow(10).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_leaky_bucket_snapshot_reset_secs() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter = LeakyBucketLimiter::with_clock(10, 5, clock).unwrap();

        // 空桶：reset 为 0（已可用）
        let snap = limiter.remaining().await.unwrap();
        assert_eq!(snap.reset_secs, 0);

        // 注入 10：漏空需 10/5 = 2s
        assert!(limiter.allow(10).await.unwrap());
        let snap = limiter.remaining().await.unwrap();
        assert_eq!(snap.remaining, 0);
        assert_eq!(snap.reset_secs, 2);
    }

    #[tokio::test]
    async fn test_leaky_bucket_zero_cost_rejected() {
        let limiter = LeakyBucketLimiter::new(10, 5).unwrap();
        assert!(limiter.allow(0).await.is_err());
        assert!(limiter.peek(0).await.is_err());
    }

    #[tokio::test]
    async fn test_leaky_bucket_cost_exceeds_max_rejected() {
        let limiter = LeakyBucketLimiter::new(10, 5).unwrap();
        assert!(limiter.allow(1_000_001).await.is_err());
    }

    #[tokio::test]
    async fn test_leaky_bucket_invalid_config_rejected() {
        assert!(LeakyBucketLimiter::new(0, 5).is_err(), "容量 0 应拒绝");
        assert!(LeakyBucketLimiter::new(10, 0).is_err(), "漏速 0 应拒绝");
    }

    #[tokio::test]
    async fn test_leaky_bucket_constructor_enforces_ceiling() {
        // 直构路径与工厂校验同上限（防御纵深）：超大桶会让记账中间量
        // u128→u64 转换失真，构造期即拒绝
        assert!(
            LeakyBucketLimiter::new(MAX_TOKEN_BUCKET_CAPACITY + 1, 5).is_err(),
            "超上限容量应拒绝"
        );
        assert!(
            LeakyBucketLimiter::new(10, MAX_TOKEN_BUCKET_REFILL_RATE + 1).is_err(),
            "超上限漏速应拒绝"
        );
        assert!(
            LeakyBucketLimiter::new(MAX_TOKEN_BUCKET_CAPACITY, MAX_TOKEN_BUCKET_REFILL_RATE)
                .is_ok()
        );
    }

    #[tokio::test]
    async fn test_leaky_bucket_concurrent_allow_respects_capacity() {
        // 并发正确性：多任务争用同一把状态锁时，注入总量不得超过容量。
        // MockClock 冻结时间（真实时钟下高漏速会在测试期间漏出水量，
        // 放行数将高于容量），使「放行数 == 容量」成为确定断言
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter = Arc::new(LeakyBucketLimiter::with_clock(100, 1_000_000, clock).unwrap());
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
            "并发注入总量必须恰好等于桶容量"
        );
        assert_eq!(limiter.water_level().await, 100);
    }

    #[test]
    fn test_leaky_bucket_accessors() {
        let limiter = LeakyBucketLimiter::new(500, 25).unwrap();
        assert_eq!(limiter.capacity(), 500);
        assert_eq!(limiter.leak_rate(), 25);
    }

    #[tokio::test]
    async fn test_leaky_bucket_check_default_impl_maps_rejection() {
        use crate::limiters::Limiter;
        let limiter = LeakyBucketLimiter::new(1, 1).unwrap();
        assert!(limiter.check("any_key").await.is_ok());
        // 桶满后 check() 须将 Ok(false) 映射为 Err（不得静默吞掉）
        let result = limiter.check("any_key").await;
        assert!(result.is_err(), "桶满后 check 应返回 Err");
    }
}
