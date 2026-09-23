// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 令牌桶限流器模块
//!
//! 使用令牌桶算法实现速率限制。

use super::traits::{Limiter, RateLimitSnapshot, validate_cost};
use crate::clock::{Clock, SystemClock};
use crate::error::LimiteronError;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// 令牌桶限流器
///
/// 使用令牌桶算法实现速率限制，令牌以恒定速率补充到桶中，
/// 请求到达时从桶中获取令牌，如果令牌不足则拒绝请求。
///
/// # 特性
/// - 使用 AtomicU64 实现令牌计数
/// - 使用 AtomicU64 实现最后补充时间（携带亚令牌时间积分，补充速率守恒）
/// - 使用 CAS (Compare-And-Swap) 循环确保原子性
/// - 使用 SeqCst 内存序确保并发安全
///
/// # 示例
/// ```rust
/// use limiteron::limiters::TokenBucketLimiter;
/// use limiteron::limiters::Limiter;
///
/// #[tokio::main]
/// async fn main() {
///     // 创建容量为 100，补充速率为 10 令牌/秒的令牌桶
///     let limiter = TokenBucketLimiter::new(100, 10);
///
///     // 尝试消费 10 个令牌
///     let allowed = limiter.allow(10).await.unwrap();
///     assert!(allowed);
/// }
/// ```
pub struct TokenBucketLimiter {
    /// 桶的最大容量
    capacity: u64,
    /// 当前令牌数（使用原子操作）
    tokens: AtomicU64,
    /// 令牌补充速率（令牌/秒）
    refill_rate: u64,
    /// 最后补充时间（纳秒时间戳）
    last_refill: AtomicU64,
    /// 时钟实例
    clock: Arc<dyn Clock>,
}

impl TokenBucketLimiter {
    /// Creates a new token bucket limiter.
    ///
    /// # Arguments
    /// * `capacity` - Maximum tokens in the bucket
    /// * `refill_rate` - Tokens added per second
    ///
    /// # Examples
    /// ```rust
    /// use limiteron::limiters::TokenBucketLimiter;
    ///
    /// let limiter = TokenBucketLimiter::new(100, 10);
    /// ```
    pub fn new(capacity: u64, refill_rate: u64) -> Self {
        Self::with_clock(capacity, refill_rate, Arc::new(SystemClock))
    }

    /// Creates a new token bucket limiter with a custom clock.
    ///
    /// # Arguments
    /// * `capacity` - Maximum tokens in the bucket
    /// * `refill_rate` - Tokens added per second
    /// * `clock` - Clock implementation for time injection (useful for testing)
    pub fn with_clock(capacity: u64, refill_rate: u64, clock: Arc<dyn Clock>) -> Self {
        let now_nanos = clock.unix_timestamp_nanos();

        Self {
            capacity,
            tokens: AtomicU64::new(capacity),
            refill_rate,
            last_refill: AtomicU64::new(now_nanos),
            clock,
        }
    }

    /// 获取桶容量
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// 获取补充速率
    pub fn refill_rate(&self) -> u64 {
        self.refill_rate
    }

    /// 获取当前令牌数
    pub fn tokens(&self) -> u64 {
        self.tokens.load(Ordering::SeqCst)
    }

    /// 补充令牌
    ///
    /// 时间积分携带：`last_refill` 只前推被整数额令牌真正消费掉的时间
    /// （`actual × 1e9 / refill_rate`），亚令牌时间积分滞留给下次累计。
    /// 历史教训：曾把补充量 `floor(elapsed × rate)` 记账后直接将
    /// `last_refill` 推到 now——每次补充事件丢失至多 1 个令牌的时间积分，
    /// 低速率场景下有效补充速率系统性低于配置值（如 1/s 每 1.5s 一请求
    /// 时实际只有 2/3 速率）。
    /// 补充量受容量封顶时，未消费的时间积分随封顶丢弃（令牌桶语义）。
    fn refill_tokens(&self) {
        let now = self.clock.unix_timestamp_nanos();

        loop {
            let last = self.last_refill.load(Ordering::SeqCst);
            let elapsed_nanos = now.saturating_sub(last);

            // 至少经过1毫秒才补充
            if elapsed_nanos < 1_000_000 {
                break;
            }

            // 应得令牌数（u128 中间量防 elapsed×rate 溢出）
            let earned = ((elapsed_nanos as u128) * (self.refill_rate as u128)) / 1_000_000_000u128;
            let tokens_to_add = u64::try_from(earned).unwrap_or(u64::MAX);

            if tokens_to_add == 0 {
                break;
            }

            // 独占 [last, now] 窗口的补充权
            if self
                .last_refill
                .compare_exchange(last, now, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                // 补充令牌（受容量封顶），并记录实际补充数额
                #[allow(unused_assignments)]
                let mut actual = 0u64;
                loop {
                    let current = self.tokens.load(Ordering::SeqCst);
                    let headroom = self.capacity.saturating_sub(current);
                    actual = tokens_to_add.min(headroom);
                    let new_tokens = current.saturating_add(actual);

                    if self
                        .tokens
                        .compare_exchange(current, new_tokens, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        break;
                    }
                }

                // last_refill 回退到"被整数额令牌消费掉的时间"末端：
                // 未封顶 → consumed = actual×1e9/rate（≤ elapsed），亚令牌
                // 积分滞留；封顶（actual < earned）→ 积分丢弃，驻留 now。
                let consumed_nanos = if actual < tokens_to_add {
                    elapsed_nanos
                } else {
                    let c = ((actual as u128) * 1_000_000_000u128) / (self.refill_rate as u128);
                    (c as u64).min(elapsed_nanos)
                };
                let settled_last = last.saturating_add(consumed_nanos);
                loop {
                    let cur = self.last_refill.load(Ordering::SeqCst);
                    if cur == now
                        && self
                            .last_refill
                            .compare_exchange(cur, settled_last, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                    {
                        break;
                    }
                    if cur != now {
                        // 已被后续补充窗口接管（其读到 elapsed≈0 会自动跳过），
                        // 不覆盖他人的推进
                        break;
                    }
                }
                break;
            }
        }
    }
}

#[async_trait]
impl Limiter for TokenBucketLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError> {
        validate_cost(cost)?;

        // 先补充令牌
        self.refill_tokens();

        loop {
            let current = self.tokens.load(Ordering::SeqCst);

            // 检查令牌是否足够
            if current < cost {
                return Ok(false);
            }

            // 尝试消费令牌
            if self
                .tokens
                .compare_exchange(current, current - cost, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Ok(true);
            }
        }
    }

    /// 非消费预检：先补充令牌后读取余额，不扣减
    async fn peek(&self, cost: u64) -> Result<RateLimitSnapshot, LimiteronError> {
        validate_cost(cost)?;
        Ok(self.current_snapshot())
    }

    /// 剩余额度查询（非消费）
    async fn remaining(&self) -> Result<RateLimitSnapshot, LimiteronError> {
        Ok(self.current_snapshot())
    }
}

impl TokenBucketLimiter {
    /// 读取当前快照（不扣减、不写回；虚拟计算与真实补充完全同构）
    ///
    /// 历史教训：曾直接调用 `refill_tokens()`（真实补充）后读余额——
    /// peek/remaining 会推进 `last_refill` 并写入 `tokens`，违反
    /// `Limiter::peek` 的零副作用契约。现改为纯读虚拟推演：
    /// `min(应得积分, 容量空位)` 与真实补充的封顶语义逐位一致。
    fn current_snapshot(&self) -> RateLimitSnapshot {
        let now = self.clock.unix_timestamp_nanos();
        let last = self.last_refill.load(Ordering::SeqCst);
        let tokens = self.tokens.load(Ordering::SeqCst);

        let elapsed_nanos = now.saturating_sub(last);
        let tokens = if elapsed_nanos >= 1_000_000 && self.refill_rate > 0 {
            let earned = ((elapsed_nanos as u128) * (self.refill_rate as u128)) / 1_000_000_000u128;
            let earned = u64::try_from(earned).unwrap_or(u64::MAX);
            let virtual_added = earned.min(self.capacity.saturating_sub(tokens)) as u128;
            tokens.saturating_add(virtual_added as u64)
        } else {
            tokens
        };

        // 桶未满时，补满所需时间 = 缺口 / 补充速率（向上取整秒）
        let missing = self.capacity.saturating_sub(tokens);
        let reset_secs = if missing == 0 || self.refill_rate == 0 {
            0
        } else {
            missing.div_ceil(self.refill_rate)
        };
        RateLimitSnapshot {
            limit: self.capacity,
            remaining: tokens,
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
    async fn test_token_bucket_basic() {
        let limiter = TokenBucketLimiter::new(100, 10);

        // 初始应该有 100 个令牌
        assert_eq!(limiter.tokens(), 100);

        // 消费 10 个令牌应该成功
        assert!(limiter.allow(10).await.unwrap());

        // 剩余 90 个令牌
        assert_eq!(limiter.tokens(), 90);
    }

    #[tokio::test]
    async fn test_token_bucket_exceed_capacity() {
        let limiter = TokenBucketLimiter::new(10, 10);

        // 消费 10 个令牌应该成功
        assert!(limiter.allow(10).await.unwrap());

        // 再消费 1 个应该失败
        assert!(!limiter.allow(1).await.unwrap());
    }

    #[tokio::test]
    async fn test_token_bucket_refill() {
        let limiter = TokenBucketLimiter::new(10, 1000); // 1000 tokens/sec

        // 消费所有令牌
        assert!(limiter.allow(10).await.unwrap());
        assert_eq!(limiter.tokens(), 0);

        // 等待一小段时间
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // 触发补充
        let _ = limiter.allow(1).await;

        // 应该有补充的令牌
        let tokens = limiter.tokens();
        assert!(tokens > 0, "Expected tokens > 0, got {}", tokens);
    }

    #[tokio::test]
    async fn test_token_bucket_zero_cost() {
        let limiter = TokenBucketLimiter::new(100, 10);
        let result = limiter.allow(0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_token_bucket_exceed_max_cost() {
        let limiter = TokenBucketLimiter::new(100, 10);
        let result = limiter.allow(1_000_001).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_token_bucket_with_mock_clock() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter = TokenBucketLimiter::with_clock(10, 100, clock);

        // 消费所有令牌
        assert!(limiter.allow(10).await.unwrap());
        assert_eq!(limiter.tokens(), 0);

        // 时间前进 1 秒
        mock_clock.advance(Duration::from_secs(1));

        // 触发补充,应该补充 100 个令牌(但受容量限制为 10)
        let _ = limiter.allow(1).await;
        let tokens = limiter.tokens();
        assert!(
            tokens > 0,
            "Expected tokens > 0 after time advance, got {}",
            tokens
        );
    }

    #[test]
    fn test_token_bucket_accessors() {
        let limiter = TokenBucketLimiter::new(500, 25);
        assert_eq!(limiter.capacity(), 500);
        assert_eq!(limiter.refill_rate(), 25);
        assert_eq!(limiter.tokens(), 500);
    }

    #[tokio::test]
    async fn test_token_bucket_check_default_impl() {
        use crate::limiters::Limiter;
        let limiter = TokenBucketLimiter::new(100, 10);
        // check() default impl calls allow(1)
        assert!(limiter.check("any_key").await.is_ok());
        assert_eq!(limiter.tokens(), 99);
    }

    #[tokio::test]
    async fn test_token_bucket_refill_capped_at_capacity() {
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter = TokenBucketLimiter::with_clock(10, 1000, clock);

        // 消费所有令牌
        assert!(limiter.allow(10).await.unwrap());
        assert_eq!(limiter.tokens(), 0);

        // 时间前进 100 秒，应该补充很多令牌，但受容量限制
        mock_clock.advance(Duration::from_secs(100));
        let _ = limiter.allow(1).await;

        // 容量限制为 10，所以最多 10 个令牌（减去刚才消费的 1）
        let tokens = limiter.tokens();
        assert!(
            tokens <= 10,
            "Expected tokens <= capacity (10), got {}",
            tokens
        );
    }

    #[tokio::test]
    async fn test_token_bucket_refill_fraction_conservation() {
        // 积分守恒回归：rate=3/s，每 ~500ms 触发一次补充，每周期应得 1.5 个
        // 令牌积分。修复前每周期 floor(1.5)=1 且 last_refill 推到 now，
        // 0.5 个令牌的积分永久丢失 → 10 周期只得 ~10-10(消费)=0 个；
        // 修复后积分滞留按 1,2,1,2 交替累积 → ~15-10=5 个。
        // （真实时钟 + 抖动鲁棒区间：old 实现即使每周期多记 1 个也够不到 4。）
        let limiter = TokenBucketLimiter::new(100, 3);
        assert!(limiter.allow(100).await.unwrap());
        assert_eq!(limiter.tokens(), 0);

        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let _ = limiter.allow(1).await; // 触发补充并消费 1 个
        }

        let tokens = limiter.tokens();
        assert!(
            (4..=6).contains(&tokens),
            "10×500ms@3/s 积分守恒应累积 ~5 个令牌(毛积累 15-消费 10),实际 {tokens}"
        );
    }

    #[tokio::test]
    async fn test_token_bucket_refill_cap_discards_fraction() {
        // 容量封顶时时间积分随封顶丢弃（令牌桶语义），不得在封顶后
        // 一次性"找回"积压积分造成突发放行。
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter = TokenBucketLimiter::with_clock(10, 1, clock);

        assert!(limiter.allow(10).await.unwrap());
        assert_eq!(limiter.tokens(), 0);

        // 前进 100s：应得 100 个积分，但只补到容量 10
        mock_clock.advance(Duration::from_secs(100));
        let _ = limiter.allow(1).await; // 补到容量 10，消 1
        assert_eq!(limiter.tokens(), 9);

        // +2s：只剩 1 个空位，应得 2 个只补得进 1，消 1
        mock_clock.advance(Duration::from_secs(2));
        let _ = limiter.allow(1).await;
        assert_eq!(limiter.tokens(), 9);

        // 再 +2s：桶始终贴着容量运行，积压积分不得"找回"造成突放
        mock_clock.advance(Duration::from_secs(2));
        let _ = limiter.allow(1).await;
        assert_eq!(limiter.tokens(), 9, "封顶后的积压积分应已丢弃");
    }

    #[tokio::test]
    async fn test_token_bucket_peek_remaining_pure() {
        // peek/remaining 零副作用契约（traits.rs）：不消费也绝不变更状态。
        // 修复前 current_snapshot 调用真实 refill_tokens——推进 last_refill
        // 并写入 tokens；纯实现只做虚拟推演。
        let mock_clock = Arc::new(MockClock::new());
        let clock: Arc<dyn Clock> = mock_clock.clone();
        let limiter = TokenBucketLimiter::with_clock(10, 5, clock);

        assert!(limiter.allow(10).await.unwrap());
        assert_eq!(limiter.tokens(), 0);

        // 前进 10s：快照应报告虚拟补充后的余额（封顶 10）
        mock_clock.advance(Duration::from_secs(10));
        let snap = limiter.remaining().await.unwrap();
        assert_eq!(snap.remaining, 10);
        // 但桶的真实状态必须原封不动（修复前此处 tokens 已被真实补充为 10）
        assert_eq!(limiter.tokens(), 0, "remaining() 不得修改桶状态");

        // 连续 peek ×100 状态仍不变
        for _ in 0..100 {
            let _ = limiter.peek(1).await.unwrap();
        }
        assert_eq!(limiter.tokens(), 0, "peek() 不得修改桶状态");

        // 纯粹性下的行为一致性：peek 报告的余额，随后 allow 真实可得
        assert!(limiter.allow(10).await.unwrap());
    }
}
