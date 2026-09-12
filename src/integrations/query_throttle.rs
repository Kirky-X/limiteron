// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! dbnexus 查询限流端口（QueryThrottle）语义文档化 + limiteron 实现。
//!
//! # 端口语义（design D3 跨项目端口边：dbnexus 定义 → limiteron 实现）
//!
//! `QueryThrottle` 是 dbnexus 侧的查询限流端口：DB 池在执行查询前向
//! limiteron 申请查询预算，防止慢查询风暴拖垮共享存储。端口契约：
//!
//! 1. **按分片（shard）分桶**：预算按物理存储实例隔离，`shard_id` 即
//!    分片标识（与 dbnexus `ShardRouter` 的 shard key 同源）；
//! 2. **非消费预检**：`remaining(shard)` 返回标准限流头数据
//!    （limit/remaining/reset），绝不扣减预算；
//! 3. **全有或全无消费**：`acquire(shard, cost)` 要么原子扣减 `cost`
//!    并返回 `true`，要么 `false`（零副作用，调用方可退避重试）；
//! 4. **预算语义**：令牌桶 capacity = 每分片并发查询预算，refill 1:1/秒
//!    （预算按秒窗口恢复）。
//!
//! # 注入方式（当 dbnexus 侧端口 trait 落地后）
//!
//! dbnexus 的 `DbPool` 持有 `Arc<dyn QueryThrottle>`（`dbnexus` 侧
//! 交付 trait 定义）；落地前 limiteron 先以本模块固定语义，
//! trait 对齐即插即用：
//!
//! ```rust,ignore
//! // dbnexus 侧（未来）：
//! pool.with_query_throttle(Arc::new(LimiteronQueryThrottle::new(100)));
//! // 执行路径：pool.execute 查询前 acquire(shard_id, 1)
//! ```
//!
//! # Example
//!
//! ```
//! use limiteron::integrations::query_throttle::{LimiteronQueryThrottle, QueryThrottle};
//!
//! # tokio_test::block_on(async {
//! let throttle = LimiteronQueryThrottle::new(10);
//! assert!(throttle.acquire("shard-0", 5).await.unwrap());
//! assert!(!throttle.acquire("shard-0", 6).await.unwrap(), "预算 10 已用 5，缺口 6 → 拒绝");
//! let snap = throttle.remaining("shard-0").await.unwrap();
//! assert_eq!(snap.remaining, 5);
//! # });
//! ```

use dashmap::DashMap;
use std::sync::Arc;

use crate::error::LimiteronError;
use crate::limiters::{Limiter, RateLimitSnapshot, TokenBucketLimiter};

/// 查询限流端口（语义见模块文档）
#[async_trait::async_trait]
pub trait QueryThrottle: Send + Sync {
    /// 原子申请 `cost` 个查询预算；`true` = 获得并扣减
    async fn acquire(&self, shard_id: &str, cost: u64) -> Result<bool, LimiteronError>;

    /// 非消费预检：分片剩余预算与标准限流头数据
    async fn remaining(&self, shard_id: &str) -> Result<RateLimitSnapshot, LimiteronError>;
}

/// limiteron 实现的 dbnexus 查询限流：按 shard 的令牌桶预算
pub struct LimiteronQueryThrottle {
    budget: u64,
    buckets: DashMap<String, Arc<TokenBucketLimiter>>,
}

impl LimiteronQueryThrottle {
    /// 以每分片查询预算创建（refill 1:1/秒，预算按秒窗口恢复）
    pub fn new(budget: u64) -> Self {
        Self {
            budget,
            buckets: DashMap::new(),
        }
    }

    /// 每分片预算
    pub fn budget(&self) -> u64 {
        self.budget
    }

    /// 当前跟踪的分片数（诊断）
    pub fn tracked_shards(&self) -> usize {
        self.buckets.len()
    }

    fn bucket_for(&self, shard_id: &str) -> Arc<TokenBucketLimiter> {
        if let Some(b) = self.buckets.get(shard_id) {
            if b.capacity() == self.budget {
                return b.value().clone();
            }
        }
        // budget 变化 → last-config-wins 重建（控制面操作，容量即语义）
        let bucket = Arc::new(TokenBucketLimiter::new(self.budget, self.budget));
        self.buckets.insert(shard_id.to_string(), bucket.clone());
        bucket
    }
}

#[async_trait::async_trait]
impl QueryThrottle for LimiteronQueryThrottle {
    async fn acquire(&self, shard_id: &str, cost: u64) -> Result<bool, LimiteronError> {
        if cost == 0 {
            return Err(LimiteronError::ConfigError(
                "QueryThrottle cost cannot be zero".to_string(),
            ));
        }
        self.bucket_for(shard_id).allow(cost).await
    }

    async fn remaining(&self, shard_id: &str) -> Result<RateLimitSnapshot, LimiteronError> {
        self.bucket_for(shard_id).remaining().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 预算内放行并扣减
    #[tokio::test]
    async fn test_t617_query_throttle_acquire_within_budget() {
        let throttle = LimiteronQueryThrottle::new(10);
        assert!(throttle.acquire("s0", 5).await.unwrap());
        assert_eq!(throttle.remaining("s0").await.unwrap().remaining, 5);
        assert_eq!(throttle.tracked_shards(), 1);
    }

    /// 预算不足 → 全有或全无拒绝（零副作用）
    #[tokio::test]
    async fn test_t617_query_throttle_all_or_nothing() {
        let throttle = LimiteronQueryThrottle::new(10);
        assert!(throttle.acquire("s0", 6).await.unwrap());
        assert!(
            !throttle.acquire("s0", 6).await.unwrap(),
            "缺口 6 > 剩余 4 → 拒绝"
        );
        assert_eq!(
            throttle.remaining("s0").await.unwrap().remaining,
            4,
            "拒绝不得扣减预算"
        );
        // 恰好剩余额度 → 放行
        assert!(throttle.acquire("s0", 4).await.unwrap());
        assert_eq!(throttle.remaining("s0").await.unwrap().remaining, 0);
    }

    /// 分片隔离：shard 间预算互不影响
    #[tokio::test]
    async fn test_t617_query_throttle_shards_isolated() {
        let throttle = LimiteronQueryThrottle::new(5);
        assert!(throttle.acquire("db-a", 5).await.unwrap());
        assert!(!throttle.acquire("db-a", 1).await.unwrap());
        assert!(
            throttle.acquire("db-b", 5).await.unwrap(),
            "兄弟分片预算独立"
        );
        assert_eq!(throttle.tracked_shards(), 2);
    }

    /// remaining 不消费：peek 后预算不变（端口非消费预检语义）
    #[tokio::test]
    async fn test_t617_query_throttle_remaining_is_non_consuming() {
        let throttle = LimiteronQueryThrottle::new(20);
        assert!(throttle.acquire("s1", 8).await.unwrap());
        let snap1 = throttle.remaining("s1").await.unwrap();
        assert_eq!(snap1.remaining, 12);
        assert_eq!(snap1.limit, 20);
        let snap2 = throttle.remaining("s1").await.unwrap();
        assert_eq!(snap1, snap2, "连续 peek 不得扣减预算");
    }

    /// cost=0 → ConfigError（显性失败）
    #[tokio::test]
    async fn test_t617_query_throttle_zero_cost_rejected() {
        let throttle = LimiteronQueryThrottle::new(10);
        assert!(throttle.acquire("s0", 0).await.is_err());
    }
}
