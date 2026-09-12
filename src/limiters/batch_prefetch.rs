// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 批量令牌预取器
//!
//! 「批量令牌预取」= 一次调用为 N 个 key 各预留一批令牌（每 key 一次
//! 原子 `allow(cost)`，而非 N 次单令牌往返），供客户端在突发前一次性
//! 预订预算（Admin API `POST /api/v1/tokens/prefetch`）。
//!
//! MVP 口径：
//! - 每 key 一个 [`TokenBucketLimiter`]（capacity = 预取量，refill 1:1/秒，
//!   即预取预算按秒窗口恢复）；同 key 后续预取量变化时按新容量重建桶
//!   （last-config-wins，不 panic——预取是控制面操作，容量即语义）；
//! - 决策热路径不经过本组件（控制面专用），无性能回退风险。

use std::sync::Arc;

use dashmap::DashMap;

use super::token_bucket::TokenBucketLimiter;
use super::traits::Limiter;

/// 单 key 预取结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct PrefetchResult {
    /// 请求的 key
    pub key: String,
    /// 请求预取的令牌数
    pub requested: u64,
    /// 是否全部授予（true = 预算充足并已扣除）
    pub granted: bool,
}

/// 批量令牌预取器
///
/// 内部按 key 维护令牌桶；`prefetch` / `prefetch_batch` 是唯一写入口。
#[derive(Default)]
pub struct BatchTokenPrefetcher {
    buckets: DashMap<String, Arc<TokenBucketLimiter>>,
}

impl BatchTokenPrefetcher {
    /// 创建空预取器
    pub fn new() -> Self {
        Self::default()
    }

    /// 为单个 key 预取 `tokens` 个令牌（一次原子消费）。
    ///
    /// `tokens == 0` 视为非法（与 `Limiter::allow(0)` 校验语义一致），返回
    /// `granted = false` 而非 Err，便于批量场景逐项报告。
    pub async fn prefetch(&self, key: &str, tokens: u64) -> PrefetchResult {
        let granted = if tokens == 0 {
            false
        } else {
            let limiter = self.bucket_for(key, tokens);
            limiter.allow(tokens).await.unwrap_or(false)
        };
        PrefetchResult {
            key: key.to_string(),
            requested: tokens,
            granted,
        }
    }

    /// 批量预取：N 个 (key, tokens) 各执行一次原子预留。
    ///
    /// 返回顺序与输入一致；单 key 失败不影响其余 key。
    pub async fn prefetch_batch(&self, items: &[(String, u64)]) -> Vec<PrefetchResult> {
        let mut results = Vec::with_capacity(items.len());
        for (key, tokens) in items {
            results.push(self.prefetch(key, *tokens).await);
        }
        results
    }

    /// 当前跟踪的 key 数（诊断/指标）
    pub fn tracked_keys(&self) -> usize {
        self.buckets.len()
    }

    /// 取（或按新容量重建）key 对应的令牌桶
    fn bucket_for(&self, key: &str, tokens: u64) -> Arc<TokenBucketLimiter> {
        if let Some(existing) = self.buckets.get(key) {
            if existing.capacity() == tokens {
                return existing.value().clone();
            }
        }
        // capacity = tokens, refill_rate = tokens（预取预算按秒恢复，1:1）
        let limiter = Arc::new(TokenBucketLimiter::new(tokens, tokens));
        self.buckets.insert(key.to_string(), limiter.clone());
        limiter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单 key 首次预取：预算充足 → granted
    #[tokio::test]
    async fn test_t613_prefetch_grants_when_budget_available() {
        let prefetcher = BatchTokenPrefetcher::new();
        let r = prefetcher.prefetch("client-a", 10).await;
        assert!(r.granted);
        assert_eq!(r.requested, 10);
        assert_eq!(prefetcher.tracked_keys(), 1);
    }

    /// 桶耗尽后同 key 再预取 → 拒绝（令牌不凭空产生）
    #[tokio::test]
    async fn test_t613_prefetch_rejects_when_budget_exhausted() {
        let prefetcher = BatchTokenPrefetcher::new();
        assert!(prefetcher.prefetch("client-a", 5).await.granted);
        // 桶容量 5 已耗尽；立即再取 5 → 拒绝
        let r = prefetcher.prefetch("client-a", 5).await;
        assert!(!r.granted, "exhausted budget must reject");
    }

    /// 预取量变化 → 按新容量重建桶（last-config-wins，不 panic）
    #[tokio::test]
    async fn test_t613_prefetch_capacity_change_rebuilds_bucket() {
        let prefetcher = BatchTokenPrefetcher::new();
        assert!(prefetcher.prefetch("client-b", 3).await.granted);
        // 不同容量 → 重建为 capacity=20 的桶
        let r = prefetcher.prefetch("client-b", 20).await;
        assert!(r.granted, "rebuilt bucket has fresh budget");
    }

    /// 批量预取：顺序保持、逐项独立
    #[tokio::test]
    async fn test_t613_prefetch_batch_orders_and_isolates() {
        let prefetcher = BatchTokenPrefetcher::new();
        let items = vec![
            ("k1".to_string(), 5),
            ("k2".to_string(), 5),
            ("k3".to_string(), 5),
        ];
        let results = prefetcher.prefetch_batch(&items).await;
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.granted));
        assert_eq!(results[1].key, "k2");
    }

    /// tokens=0 → 逐项报告 granted=false（不 panic、不影响其余项）
    #[tokio::test]
    async fn test_t613_prefetch_zero_tokens_reported_not_fatal() {
        let prefetcher = BatchTokenPrefetcher::new();
        let items = vec![("bad".to_string(), 0), ("good".to_string(), 2)];
        let results = prefetcher.prefetch_batch(&items).await;
        assert!(!results[0].granted);
        assert!(results[1].granted);
    }

    /// key 隔离：不同 key 互不共享预算
    #[tokio::test]
    async fn test_t613_prefetch_keys_are_isolated() {
        let prefetcher = BatchTokenPrefetcher::new();
        assert!(prefetcher.prefetch("iso-1", 2).await.granted);
        // iso-1 预算耗尽 → 拒绝
        assert!(!prefetcher.prefetch("iso-1", 2).await.granted);
        // iso-2 不受 iso-1 耗尽影响
        let r = prefetcher.prefetch("iso-2", 2).await;
        assert!(r.granted, "sibling key must be isolated");
    }
}
