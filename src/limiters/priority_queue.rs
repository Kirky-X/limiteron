// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 优先级队列限流器（feature `priority-queue`）
//!
//! 按优先级调度的配额分配：总配额按权重为各优先级档位划分保留额，
//! 高优先级可借用更低档位的未用预留（严格优先级调度），低档位只能
//! 消费自身保留额——兼顾「高优先保畅」与「每档保底带宽」。
//!
//! # 调度规则（确定性，按序判定）
//!
//! 1. 全局硬顶：本窗口累计消费 + cost ≤ 总配额，否则拒绝；
//! 2. 保内放行：请求档位自身用量 + cost ≤ 该档保留额，则放行；
//! 3. 借用放行：超出保留额的部分 ≤ 更低档位的未用预留合计，则放行
//!    （只能向更低档位借，不得侵占更高档位的保证带宽）。
//!
//! # Example
//!
//! ```rust
//! use limiteron::limiters::priority_queue::{PriorityQueueConfig, PriorityQueueLimiter};
//! use std::time::Duration;
//!
//! let limiter = PriorityQueueLimiter::new(PriorityQueueConfig {
//!     window: Duration::from_secs(1),
//!     total_per_window: 100,
//!     level_weights: vec![5, 3, 2],
//!     default_priority: 2,
//! })
//! .unwrap();
//!
//! // 0 为最高优先级：可借用更低档位未用预留
//! # tokio::runtime::Runtime::new().unwrap().block_on(async {
//! assert!(limiter.allow_with_priority(0, 50).await.unwrap());
//! # });
//! ```

use crate::clock::{Clock, SystemClock};
use crate::error::LimiteronError;
use crate::limiters::traits::{Limiter, RateLimitSnapshot, validate_cost};
use async_trait::async_trait;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

/// 优先级队列配置
#[derive(Debug, Clone)]
pub struct PriorityQueueConfig {
    /// 配额窗口长度
    pub window: Duration,
    /// 每窗口总配额
    pub total_per_window: u64,
    /// 各优先级档位权重（下标 0 为最高优先级）；保留额 = 总配额 × 权重占比，
    /// 整除余数从最高档位起依次补 1
    pub level_weights: Vec<u64>,
    /// [`Limiter::allow`]（无优先级参数）使用的默认档位
    pub default_priority: usize,
}

impl PriorityQueueConfig {
    /// 校验配置合法性
    pub fn validate(&self) -> Result<(), LimiteronError> {
        if self.window.is_zero() {
            return Err(LimiteronError::ConfigError(
                "priority-queue window must be non-zero".to_string(),
            ));
        }
        if self.total_per_window == 0 {
            return Err(LimiteronError::ConfigError(
                "priority-queue total_per_window must be non-zero".to_string(),
            ));
        }
        if self.level_weights.is_empty() {
            return Err(LimiteronError::ConfigError(
                "priority-queue level_weights must not be empty".to_string(),
            ));
        }
        if self.level_weights.contains(&0) {
            return Err(LimiteronError::ConfigError(
                "priority-queue level_weights must not contain zero".to_string(),
            ));
        }
        if self.default_priority >= self.level_weights.len() {
            return Err(LimiteronError::ConfigError(format!(
                "priority-queue default_priority {} out of range (levels: {})",
                self.default_priority,
                self.level_weights.len()
            )));
        }
        Ok(())
    }
}

#[derive(Debug)]
struct WindowState {
    window_start: u64,
    used: Vec<u64>,
}

/// 优先级队列限流器
pub struct PriorityQueueLimiter {
    total: u64,
    window_secs: u64,
    default_priority: usize,
    reserved: Vec<u64>,
    clock: Arc<dyn Clock>,
    state: Mutex<WindowState>,
}

impl PriorityQueueLimiter {
    /// 创建优先级队列限流器（系统时钟）
    pub fn new(config: PriorityQueueConfig) -> Result<Self, LimiteronError> {
        Self::with_clock(config, Arc::new(SystemClock))
    }

    /// 以自定义时钟创建（测试注入用）
    pub fn with_clock(
        config: PriorityQueueConfig,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, LimiteronError> {
        config.validate()?;
        let weights_sum: u64 = config.level_weights.iter().sum();
        let mut reserved: Vec<u64> = config
            .level_weights
            .iter()
            .map(|w| config.total_per_window * w / weights_sum)
            .collect();
        // 整除余数从最高档位起依次补 1（确定性分配）
        let mut remainder = config.total_per_window - reserved.iter().sum::<u64>();
        for slot in reserved.iter_mut() {
            if remainder == 0 {
                break;
            }
            *slot += 1;
            remainder -= 1;
        }
        let window_start = clock.unix_timestamp();
        Ok(Self {
            total: reserved.iter().sum(),
            window_secs: config.window.as_secs().max(1),
            default_priority: config.default_priority,
            reserved,
            clock,
            state: Mutex::new(WindowState {
                window_start,
                used: vec![0; config.level_weights.len()],
            }),
        })
    }

    /// 档位数
    pub fn level_count(&self) -> usize {
        self.reserved.len()
    }

    /// 当前窗口各档位已消费量（观测用，非消费）
    pub async fn level_usage(&self) -> Vec<u64> {
        let mut state = self.state.lock();
        self.advance_window(&mut state);
        state.used.clone()
    }

    /// 按优先级档位申请配额（0 为最高优先级）
    pub async fn allow_with_priority(
        &self,
        priority: usize,
        cost: u64,
    ) -> Result<bool, LimiteronError> {
        let _cost = validate_cost(cost)?;
        if priority >= self.reserved.len() {
            return Err(LimiteronError::ConfigError(format!(
                "priority {} out of range (levels: {})",
                priority,
                self.reserved.len()
            )));
        }
        let mut state = self.state.lock();
        self.advance_window(&mut state);
        // 规则 1：全局硬顶——本窗口累计消费不得超过总配额
        let total_used: u64 = state.used.iter().sum();
        if total_used > self.total.saturating_sub(cost) {
            return Ok(false);
        }
        // 规则 2：保内放行——自身档位保留额内直接消费
        let level_used = state.used[priority];
        if level_used <= self.reserved[priority].saturating_sub(cost) {
            state.used[priority] += cost;
            return Ok(true);
        }
        // 规则 3：借用放行——超出保留额的部分只能向更低档位的未用预留借
        // （不得侵占更高档位的保证带宽；无更低档位时可借量为 0）
        let borrowable: u64 = self.reserved[priority + 1..]
            .iter()
            .zip(state.used[priority + 1..].iter())
            .map(|(reserved, used)| reserved.saturating_sub(*used))
            .sum();
        let overage = level_used + cost - self.reserved[priority];
        if overage <= borrowable {
            state.used[priority] += cost;
            return Ok(true);
        }
        Ok(false)
    }

    fn advance_window(&self, state: &mut WindowState) {
        let now = self.clock.unix_timestamp();
        if now >= state.window_start.saturating_add(self.window_secs) {
            state.window_start = now;
            state.used.iter_mut().for_each(|u| *u = 0);
        }
    }
}

#[async_trait]
impl Limiter for PriorityQueueLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError> {
        self.allow_with_priority(self.default_priority, cost).await
    }

    async fn peek(&self, cost: u64) -> Result<RateLimitSnapshot, LimiteronError> {
        let _cost = validate_cost(cost)?;
        let mut state = self.state.lock();
        self.advance_window(&mut state);
        let used: u64 = state.used.iter().sum();
        Ok(RateLimitSnapshot {
            limit: self.total,
            remaining: self.total.saturating_sub(used),
            reset_secs: state
                .window_start
                .saturating_add(self.window_secs)
                .saturating_sub(self.clock.unix_timestamp()),
        })
    }

    async fn remaining(&self) -> Result<RateLimitSnapshot, LimiteronError> {
        self.peek(1).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;

    fn config(total: u64, weights: Vec<u64>) -> PriorityQueueConfig {
        PriorityQueueConfig {
            window: Duration::from_secs(1),
            total_per_window: total,
            level_weights: weights,
            default_priority: 2,
        }
    }

    #[tokio::test]
    async fn test_admits_within_reserved_band() {
        let limiter =
            PriorityQueueLimiter::new(config(100, vec![5, 3, 2])).expect("config should be valid");
        assert!(limiter.allow_with_priority(1, 30).await.unwrap());
        // 最低档位用满自身保留额后，中间档位超保留额的部分借无可借
        assert!(limiter.allow_with_priority(2, 20).await.unwrap());
        assert!(!limiter.allow_with_priority(1, 1).await.unwrap());
    }

    #[tokio::test]
    async fn test_high_priority_borrows_from_lower_levels() {
        let limiter =
            PriorityQueueLimiter::new(config(100, vec![5, 3, 2])).expect("config should be valid");
        assert!(limiter.allow_with_priority(0, 50).await.unwrap());
        // 超出自身保留额 50 的部分向更低档位借用
        assert!(limiter.allow_with_priority(0, 50).await.unwrap());
        // 全局硬顶
        assert!(!limiter.allow_with_priority(0, 1).await.unwrap());
    }

    #[tokio::test]
    async fn test_low_priority_cannot_borrow_from_higher_levels() {
        let limiter =
            PriorityQueueLimiter::new(config(100, vec![5, 3, 2])).expect("config should be valid");
        // 更高档位已消费 90，最低档保内 20 仍可用
        assert!(limiter.allow_with_priority(0, 90).await.unwrap());
        assert!(limiter.allow_with_priority(2, 10).await.unwrap());
        // 最低档超保留额后无处可借（无更低档位）
        assert!(!limiter.allow_with_priority(2, 1).await.unwrap());
    }

    #[tokio::test]
    async fn test_window_reset_restores_quota() {
        let clock = Arc::new(MockClock::new());
        let limiter = PriorityQueueLimiter::with_clock(config(100, vec![5, 3, 2]), clock.clone())
            .expect("config should be valid");
        assert!(limiter.allow_with_priority(0, 100).await.unwrap());
        assert!(!limiter.allow_with_priority(0, 1).await.unwrap());
        clock.advance(Duration::from_secs(2));
        assert!(limiter.allow_with_priority(0, 100).await.unwrap());
    }

    #[tokio::test]
    async fn test_default_priority_drives_limiter_allow() {
        let mut cfg = config(100, vec![5, 3, 2]);
        cfg.default_priority = 0;
        let limiter = PriorityQueueLimiter::new(cfg).expect("config should be valid");
        // 默认档 = 最高优先级：可消费全部配额
        assert!(limiter.allow(60).await.unwrap());
        assert!(limiter.allow(40).await.unwrap());
    }

    #[tokio::test]
    async fn test_peek_does_not_consume() {
        let limiter =
            PriorityQueueLimiter::new(config(100, vec![5, 3, 2])).expect("config should be valid");
        assert!(limiter.allow_with_priority(0, 40).await.unwrap());
        let snapshot = limiter.peek(1).await.unwrap();
        assert_eq!(snapshot.limit, 100);
        assert_eq!(snapshot.remaining, 60);
        let again = limiter.remaining().await.unwrap();
        assert_eq!(again.remaining, 60);
    }

    #[tokio::test]
    async fn test_out_of_range_priority_rejected() {
        let limiter =
            PriorityQueueLimiter::new(config(100, vec![5, 3, 2])).expect("config should be valid");
        assert!(limiter.allow_with_priority(3, 1).await.is_err());
    }

    #[test]
    fn test_invalid_configs_rejected() {
        assert!(PriorityQueueLimiter::new(config(100, vec![])).is_err());
        assert!(PriorityQueueLimiter::new(config(0, vec![1, 1])).is_err());
        let mut cfg = config(100, vec![5, 3, 2]);
        cfg.default_priority = 3;
        assert!(PriorityQueueLimiter::new(cfg).is_err());
    }

    #[tokio::test]
    async fn test_level_usage_observable() {
        let limiter =
            PriorityQueueLimiter::new(config(100, vec![5, 3, 2])).expect("config should be valid");
        assert!(limiter.allow_with_priority(1, 10).await.unwrap());
        assert_eq!(limiter.level_count(), 3);
        assert_eq!(limiter.level_usage().await, vec![0, 10, 0]);
    }
}
