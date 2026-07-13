// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 分布式限流器模块
//!
//! 提供基于内存的分布式限流器实现，支持原子计数操作。
//! 用于进程内分布式兼容测试，也可作为分布式 DAO 的参考实现。

use super::traits::{DistributedLimiter, Limiter};
use crate::error::LimiteronError;
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 带过期时间的计数器条目
#[derive(Clone)]
struct TtlEntry {
    count: u64,
    expires_at: Instant,
}

/// 内存分布式限流器
///
/// 基于 DashMap 实现的进程内分布式限流器，提供原子计数操作。
/// 适用于单实例部署或测试环境，也可作为分布式 DAO 的参考实现。
///
/// # 特性
/// - 原子递增（incr）
/// - 带 TTL 的原子递增（incr_with_ttl）
/// - 计数查询（get_count）
/// - 计数重置（reset）
///
/// # 示例
///
/// ```rust
/// use limiteron::limiters::{DistributedLimiter, InMemoryDistributedLimiter};
///
/// #[tokio::main]
/// async fn main() {
///     let limiter = InMemoryDistributedLimiter::new();
///     let count = limiter.incr("user:123", 1).await.unwrap();
///     assert_eq!(count, 1);
/// }
/// ```
pub struct InMemoryDistributedLimiter {
    /// 永久计数器（无 TTL）
    counters: Arc<DashMap<String, u64>>,
    /// 带 TTL 的计数器
    ttl_counters: Arc<DashMap<String, TtlEntry>>,
}

impl InMemoryDistributedLimiter {
    /// 创建新的内存分布式限流器
    pub fn new() -> Self {
        Self {
            counters: Arc::new(DashMap::new()),
            ttl_counters: Arc::new(DashMap::new()),
        }
    }

    /// 清理过期的 TTL 计数器
    fn cleanup_expired(&self) {
        let now = Instant::now();
        self.ttl_counters.retain(|_, entry| entry.expires_at > now);
    }
}

impl Default for InMemoryDistributedLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Limiter for InMemoryDistributedLimiter {
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError> {
        // 使用固定键 "_global" 进行计数，兼容 Limiter trait 接口
        // 真正的分布式限流应通过 incr + get_count + 阈值判断实现
        self.incr("_global", cost).await?;
        Ok(true)
    }
}

#[async_trait]
impl DistributedLimiter for InMemoryDistributedLimiter {
    async fn incr(&self, key: &str, amount: u64) -> Result<u64, LimiteronError> {
        if key.is_empty() {
            return Err(LimiteronError::ConfigError(
                "Key cannot be empty".to_string(),
            ));
        }

        let new_count = self
            .counters
            .entry(key.to_string())
            .and_modify(|c| *c = c.saturating_add(amount))
            .or_insert(amount);

        Ok(*new_count)
    }

    async fn incr_with_ttl(
        &self,
        key: &str,
        amount: u64,
        ttl: Duration,
    ) -> Result<u64, LimiteronError> {
        if key.is_empty() {
            return Err(LimiteronError::ConfigError(
                "Key cannot be empty".to_string(),
            ));
        }

        let now = Instant::now();
        let expires_at = now + ttl;

        // 清理过期条目
        self.cleanup_expired();

        let new_count = self
            .ttl_counters
            .entry(key.to_string())
            .and_modify(|entry| {
                if entry.expires_at > now {
                    // 未过期，累加
                    entry.count = entry.count.saturating_add(amount);
                    entry.expires_at = expires_at;
                } else {
                    // 已过期，重置
                    entry.count = amount;
                    entry.expires_at = expires_at;
                }
            })
            .or_insert(TtlEntry {
                count: amount,
                expires_at,
            });

        Ok(new_count.count)
    }

    async fn get_count(&self, key: &str) -> Result<u64, LimiteronError> {
        // 先检查 TTL 计数器
        if let Some(entry) = self.ttl_counters.get(key) {
            if entry.expires_at > Instant::now() {
                return Ok(entry.count);
            }
        }

        // 再检查永久计数器
        if let Some(count) = self.counters.get(key) {
            return Ok(*count);
        }

        Ok(0)
    }

    async fn reset(&self, key: &str) -> Result<(), LimiteronError> {
        self.counters.remove(key);
        self.ttl_counters.remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_incr_new_key() {
        let limiter = InMemoryDistributedLimiter::new();
        let count = limiter.incr("user:1", 1).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_incr_existing_key() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter.incr("user:1", 5).await.unwrap();
        let count = limiter.incr("user:1", 3).await.unwrap();
        assert_eq!(count, 8);
    }

    #[tokio::test]
    async fn test_incr_empty_key() {
        let limiter = InMemoryDistributedLimiter::new();
        let result = limiter.incr("", 1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_incr_saturating() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter.incr("user:1", u64::MAX).await.unwrap();
        let count = limiter.incr("user:1", 1).await.unwrap();
        assert_eq!(count, u64::MAX);
    }

    #[tokio::test]
    async fn test_incr_with_ttl_new_key() {
        let limiter = InMemoryDistributedLimiter::new();
        let count = limiter
            .incr_with_ttl("user:1", 1, Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_incr_with_ttl_expired() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter
            .incr_with_ttl("user:1", 5, Duration::from_millis(1))
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(10)).await;

        // 过期后再次递增应重置为新值
        let count = limiter
            .incr_with_ttl("user:1", 3, Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_get_count_nonexistent() {
        let limiter = InMemoryDistributedLimiter::new();
        let count = limiter.get_count("nonexistent").await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_get_count_after_incr() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter.incr("user:1", 10).await.unwrap();
        let count = limiter.get_count("user:1").await.unwrap();
        assert_eq!(count, 10);
    }

    #[tokio::test]
    async fn test_get_count_after_ttl_expire() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter
            .incr_with_ttl("user:1", 5, Duration::from_millis(1))
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(10)).await;

        let count = limiter.get_count("user:1").await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_reset() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter.incr("user:1", 10).await.unwrap();
        limiter.reset("user:1").await.unwrap();
        let count = limiter.get_count("user:1").await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_reset_ttl_counter() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter
            .incr_with_ttl("user:1", 10, Duration::from_secs(60))
            .await
            .unwrap();
        limiter.reset("user:1").await.unwrap();
        let count = limiter.get_count("user:1").await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_concurrent_incr() {
        let limiter = Arc::new(InMemoryDistributedLimiter::new());
        let mut handles = vec![];

        for _ in 0..10 {
            let limiter_clone = Arc::clone(&limiter);
            handles.push(tokio::spawn(async move {
                limiter_clone.incr("concurrent", 1).await.unwrap();
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        let count = limiter.get_count("concurrent").await.unwrap();
        assert_eq!(count, 10);
    }

    #[tokio::test]
    async fn test_limiter_trait_compatibility() {
        // InMemoryDistributedLimiter 同时实现 Limiter + DistributedLimiter
        let limiter = InMemoryDistributedLimiter::new();
        let allowed = limiter.allow(1).await.unwrap();
        assert!(allowed);
        let count = limiter.incr("test", 1).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_incr_with_ttl_accumulate() {
        let limiter = InMemoryDistributedLimiter::new();
        let c1 = limiter
            .incr_with_ttl("user:1", 3, Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(c1, 3);
        let c2 = limiter
            .incr_with_ttl("user:1", 5, Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(c2, 8);
    }

    #[tokio::test]
    async fn test_different_keys_isolated() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter.incr("user:1", 10).await.unwrap();
        limiter.incr("user:2", 20).await.unwrap();
        assert_eq!(limiter.get_count("user:1").await.unwrap(), 10);
        assert_eq!(limiter.get_count("user:2").await.unwrap(), 20);
    }

    #[tokio::test]
    async fn test_reset_nonexistent_key() {
        let limiter = InMemoryDistributedLimiter::new();
        // 重置不存在的键不应报错
        let result = limiter.reset("nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_incr_with_ttl_empty_key() {
        let limiter = InMemoryDistributedLimiter::new();
        let result = limiter.incr_with_ttl("", 1, Duration::from_secs(60)).await;
        assert!(result.is_err());
    }
}
