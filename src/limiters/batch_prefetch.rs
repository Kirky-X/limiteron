// Copyright (c) 2026 Kirky.X🌠
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
use std::time::Instant;

use dashmap::DashMap;

use super::token_bucket::TokenBucketLimiter;
use super::traits::Limiter;

/// 淘汰后回落的条目数目标值（非硬上限，见 [`BatchTokenPrefetcher`]）
const MAX_PREFETCH_ENTRIES: usize = 10_000;
/// 触发淘汰的硬阈值（条目数达到此值即淘汰最旧项，回落至 [`MAX_PREFETCH_ENTRIES`]）
const PREFETCH_CLEANUP_THRESHOLD: usize = 11_000;
/// 单 key 长度上限（字节）——超长 key 无业务意义且放大单条占用
const MAX_PREFETCH_KEY_LEN: usize = 256;

/// 单 key 预取结果
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PrefetchResult {
    /// 请求的 key
    pub key: String,
    /// 请求预取的令牌数
    pub requested: u64,
    /// 是否全部授予（true = 预算充足并已扣除）
    pub granted: bool,
}

/// 预取桶条目：令牌桶 + 创建时刻
///
/// 创建时刻供容量超限时的最旧淘汰使用（无访问时间戳：预取是控制面
/// 低频操作，创建序近似访问序）。
struct PrefetchBucket {
    /// 该 key 的令牌桶
    limiter: Arc<TokenBucketLimiter>,
    /// 桶创建时刻
    created_at: Instant,
}

/// 批量令牌预取器
///
/// 内部按 key 维护令牌桶；`prefetch` / `prefetch_batch` 是唯一写入口。
///
/// 桶表有界：条目数达 `PREFETCH_CLEANUP_THRESHOLD`（11000）时淘汰最旧条目
/// 并回落至 `MAX_PREFETCH_ENTRIES`（10000）；两值之间（10000~11000）为高水位，
/// 非硬上限——管理 API 的批量预取可由请求方逐请求注入全新 key，条目永不回收
/// 会单调增长直至 OOM。
#[derive(Default)]
pub struct BatchTokenPrefetcher {
    buckets: DashMap<String, PrefetchBucket>,
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
        } else if key.len() > MAX_PREFETCH_KEY_LEN {
            log::warn!(
                target: "limiteron",
                "prefetch rejected: key exceeds {MAX_PREFETCH_KEY_LEN} bytes ({} bytes)",
                key.len()
            );
            false
        } else {
            let limiter = self.bucket_for(key, tokens);
            // 校验类错误（如 cost 超 MAX_COST）与预算耗尽同样落为
            // granted=false（批量逐项报告契约），但至少留下诊断日志
            limiter.allow(tokens).await.unwrap_or_else(|e| {
                log::warn!(
                    target: "limiteron",
                    "prefetch allow failed for key '{}': {e}",
                    crate::logging::redact_basic(Some(key))
                );
                false
            })
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
        if let Some(existing) = self.buckets.get(key)
            && existing.limiter.capacity() == tokens
        {
            return existing.limiter.clone();
        }
        // capacity = tokens, refill_rate = tokens（预取预算按秒恢复，1:1）
        let limiter = Arc::new(TokenBucketLimiter::new(tokens, tokens));
        self.buckets.insert(
            key.to_string(),
            PrefetchBucket {
                limiter: limiter.clone(),
                created_at: Instant::now(),
            },
        );
        self.evict_over_capacity();
        limiter
    }

    /// 桶表超限时淘汰最旧条目（防无界增长致 OOM）
    ///
    /// 只收集创建时刻（16 字节/条）定位淘汰水位，不克隆全量 key——满表
    /// （1.1 万条 × key 上限 256 字节）下全量克隆需数 MB 临时内存。
    /// 水位用 O(n) 选择（`select_nth_unstable_by_key`）而非全排序，
    /// 随后只克隆并删除水位以下的 key。
    fn evict_over_capacity(&self) {
        if self.buckets.len() < PREFETCH_CLEANUP_THRESHOLD {
            return;
        }
        let len = self.buckets.len();
        if len <= MAX_PREFETCH_ENTRIES {
            return;
        }
        let to_remove = len - MAX_PREFETCH_ENTRIES;
        let mut created: Vec<Instant> = self.buckets.iter().map(|e| e.value().created_at).collect();
        // 第 to_remove 小的创建时刻即淘汰水位（并列时刻一并淘汰）
        let (_, pivot, _) = created.select_nth_unstable_by_key(to_remove - 1, |t| *t);
        let cutoff = *pivot;
        let stale: Vec<String> = self
            .buckets
            .iter()
            .filter(|e| e.value().created_at <= cutoff)
            .map(|e| e.key().clone())
            .collect();
        for key in stale {
            self.buckets.remove(&key);
        }
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

    /// 桶表有界：高水位为 PREFETCH_CLEANUP_THRESHOLD，超出后淘汰最旧条目
    /// （OOM DoS 防护）
    #[tokio::test]
    async fn test_t613_prefetch_entries_capped() {
        let prefetcher = BatchTokenPrefetcher::new();
        for i in 0..(PREFETCH_CLEANUP_THRESHOLD + 100) {
            let _ = prefetcher.prefetch(&format!("cap-{i}"), 1).await;
        }
        assert!(
            prefetcher.tracked_keys() <= PREFETCH_CLEANUP_THRESHOLD,
            "prefetch bucket count must stay bounded, got: {}",
            prefetcher.tracked_keys()
        );
    }

    /// 桶表不是硬上限 [`MAX_PREFETCH_ENTRIES`]：两次淘汰之间条目数可越过该值，
    /// 达 [`PREFETCH_CLEANUP_THRESHOLD`] 时才淘汰最旧条目并回落至上限
    #[tokio::test]
    async fn test_prefetch_entries_fall_back_to_max_after_cleanup() {
        let prefetcher = BatchTokenPrefetcher::new();
        let mut prev = prefetcher.tracked_keys();
        let mut cleanups = 0usize;
        let mut exceeded_max = false;
        for i in 0..(PREFETCH_CLEANUP_THRESHOLD * 3) {
            let _ = prefetcher.prefetch(&format!("fb-{i}"), 1).await;
            let now = prefetcher.tracked_keys();
            assert!(
                now <= PREFETCH_CLEANUP_THRESHOLD,
                "条目数不得越过高水位 PREFETCH_CLEANUP_THRESHOLD，got: {now}"
            );
            if now < prev + 1 {
                // 本次插入伴随淘汰：条目数未按 +1 增长
                cleanups += 1;
                assert!(
                    now <= MAX_PREFETCH_ENTRIES,
                    "淘汰后应回落至 MAX_PREFETCH_ENTRIES 以内，got: {now}"
                );
            } else if now > MAX_PREFETCH_ENTRIES {
                exceeded_max = true;
            }
            prev = now;
            if cleanups >= 2 && exceeded_max {
                break;
            }
        }
        assert!(
            exceeded_max,
            "两次淘汰之间条目数应可越过 MAX_PREFETCH_ENTRIES（非硬上限）"
        );
        assert!(
            cleanups >= 2,
            "每次越过 PREFETCH_CLEANUP_THRESHOLD 都应淘汰最旧条目并回落，observed cleanups: {cleanups}"
        );
    }

    /// 超长 key 被拒绝：不建桶（不放大单条占用）
    #[tokio::test]
    async fn test_t613_prefetch_rejects_overlong_key() {
        let prefetcher = BatchTokenPrefetcher::new();
        let key = "k".repeat(MAX_PREFETCH_KEY_LEN + 1);
        let r = prefetcher.prefetch(&key, 5).await;
        assert!(!r.granted, "overlong key must not be granted");
        assert_eq!(prefetcher.tracked_keys(), 0);
    }
}
