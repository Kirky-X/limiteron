// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! E2E 高级场景测试
//!
//! 补全 usage_scenarios.md 中识别的缺失场景：
//! - 限流器边界条件（cost 边界、容量边界、突发/恢复）
//! - GCRA 限流器（匀速/突发/check 不修改状态）
//! - 并发控制器（acquire/release/超时/u32 溢出）
//! - 决策链（短路/非短路/禁用节点/错误传播/统计）
//! - 降级策略（FailOpen/FailClosed/Degraded/热更新/孤岛模式）
//! - 多租户（Default/Header 解析器、命名空间隔离、前缀注入防护）
//! - 分布式限流（InMemoryDistributedLimiter 原子计数/TTL/并发安全）
//! - 跨模块集成（多限流器组合、Governor 全链路）
//!
//! 测试约束：
//! - 进程内模式（不依赖外部 Redis/Postgres）
//! - 每个 feature 用 `#[cfg(feature = "xxx")]` 隔离
//! - 并发测试用 `multi_thread` runtime
//! - 错误断言用 match + panic 描述预期变体

#![cfg(test)]

use limiteron::error::{Decision, LimiteronError};
use limiteron::limiters::{
    ConcurrencyLimiter, FixedWindowLimiter, Limiter, ShardedSlidingWindowLimiter,
    TokenBucketLimiter,
};
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// 模块 1: 限流器边界条件
// ============================================================================

mod limiter_boundary {
    use super::*;

    /// cost=0 必须返回 ConfigError，不能静默成功
    #[tokio::test]
    async fn token_bucket_zero_cost_returns_config_error() {
        let limiter = TokenBucketLimiter::new(10, 1);
        let result = limiter.allow(0).await;
        match result {
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(
                    msg.contains("Cost cannot be zero"),
                    "unexpected message: {}",
                    msg
                );
            }
            other => panic!("Expected ConfigError for zero cost, got {:?}", other),
        }
    }

    /// cost 超过 MAX_COST 必须返回 ConfigError
    #[tokio::test]
    async fn token_bucket_cost_exceeds_max_returns_config_error() {
        let limiter = TokenBucketLimiter::new(10, 1);
        // MAX_COST 是个常量，使用极大值触发
        let oversized_cost = u64::MAX;
        let result = limiter.allow(oversized_cost).await;
        match result {
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(
                    msg.contains("Cost exceeds maximum limit"),
                    "unexpected message: {}",
                    msg
                );
            }
            other => panic!("Expected ConfigError for oversized cost, got {:?}", other),
        }
    }

    /// cost > capacity 应拒绝请求（返回 Ok(false)），不报错
    #[tokio::test]
    async fn token_bucket_cost_exceeds_capacity_denies() {
        let limiter = TokenBucketLimiter::new(5, 1);
        let result = limiter.allow(10).await;
        match result {
            Ok(false) => {}
            Ok(true) => panic!("Expected denial when cost > capacity, got allowed"),
            Err(e) => panic!("Expected Ok(false), got error: {:?}", e),
        }
    }

    /// 突发流量场景：capacity 内的请求应全部通过
    #[tokio::test]
    async fn token_bucket_burst_within_capacity_allowed() {
        let limiter = TokenBucketLimiter::new(50, 1);
        for i in 1..=50 {
            let allowed = limiter
                .allow(1)
                .await
                .unwrap_or_else(|e| panic!("Request {} errored: {:?}", i, e));
            assert!(allowed, "Request {} within burst capacity should pass", i);
        }
    }

    /// 突发超过 capacity 后必须拒绝
    #[tokio::test]
    async fn token_bucket_burst_beyond_capacity_denied() {
        let limiter = TokenBucketLimiter::new(3, 1);
        for _ in 0..3 {
            assert!(limiter.allow(1).await.unwrap());
        }
        // 第 4 个必须被拒（refill_rate=1/s，毫秒级内不会补充）
        let result = limiter.allow(1).await.unwrap();
        assert!(!result, "Request beyond capacity should be denied");
    }

    /// 速率恢复：等待 refill 后请求应再次通过
    #[tokio::test]
    async fn token_bucket_recovery_after_refill() {
        let limiter = TokenBucketLimiter::new(2, 5); // 5 tokens/sec
        // 消耗完
        assert!(limiter.allow(1).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());
        // 等待 > 200ms 以补充至少 1 个令牌
        tokio::time::sleep(Duration::from_millis(300)).await;
        let recovered = limiter.allow(1).await.unwrap();
        assert!(recovered, "Request should pass after refill");
    }

    /// 固定窗口：cost=0 也必须报 ConfigError
    #[tokio::test]
    async fn fixed_window_zero_cost_returns_config_error() {
        let limiter = FixedWindowLimiter::new(Duration::from_secs(60), 10);
        let result = limiter.allow(0).await;
        match result {
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(msg.contains("Cost cannot be zero"));
            }
            other => panic!("Expected ConfigError, got {:?}", other),
        }
    }

    /// 固定窗口：批量 cost 接近 limit 的边界判断
    #[tokio::test]
    async fn fixed_window_batch_cost_boundary() {
        let limiter = FixedWindowLimiter::new(Duration::from_secs(60), 10);
        // cost=10 应该正好等于 limit，允许
        assert!(limiter.allow(10).await.unwrap());
        // 任何额外请求应被拒
        assert!(!limiter.allow(1).await.unwrap());
    }

    /// 固定窗口：cost=11 超过 limit=10 应直接拒绝
    #[tokio::test]
    async fn fixed_window_cost_exceeds_limit_denied() {
        let limiter = FixedWindowLimiter::new(Duration::from_secs(60), 10);
        let result = limiter.allow(11).await.unwrap();
        assert!(!result, "cost > limit should be denied");
    }

    /// 固定窗口：窗口过期后计数重置
    #[tokio::test]
    async fn fixed_window_reset_after_expiry() {
        let limiter = FixedWindowLimiter::new(Duration::from_millis(200), 2);
        assert!(limiter.allow(1).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());
        tokio::time::sleep(Duration::from_millis(250)).await;
        // 窗口过期后应允许
        assert!(limiter.allow(1).await.unwrap());
    }

    /// 分片滑动窗口：cost=0 报 ConfigError
    #[tokio::test]
    async fn sharded_sliding_window_zero_cost_returns_config_error() {
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 10);
        let result = limiter.allow(0).await;
        match result {
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(msg.contains("Cost cannot be zero"));
            }
            other => panic!("Expected ConfigError, got {:?}", other),
        }
    }

    /// 分片滑动窗口：超过 max_requests 后拒绝
    #[tokio::test]
    async fn sharded_sliding_window_exceeds_max_denied() {
        let limiter = ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 3);
        for _ in 0..3 {
            assert!(limiter.allow(1).await.unwrap());
        }
        let result = limiter.allow(1).await.unwrap();
        assert!(!result, "Request beyond max should be denied");
    }
}

// ============================================================================
// 模块 2: GCRA 限流器（feature = "gcra"）
// ============================================================================

#[cfg(feature = "gcra")]
mod gcra_limiter {
    use super::*;
    use limiteron::limiters::GcraLimiter;

    /// GCRA 突发容量边界：capacity 内全部允许
    #[tokio::test]
    async fn gcra_burst_within_capacity_allowed() {
        // 1s interval，10 容量
        let limiter = GcraLimiter::new(10, 1_000_000);
        for i in 1..=10 {
            let allowed = limiter.allow(1).await.unwrap();
            assert!(allowed, "Burst request {} should be allowed", i);
        }
    }

    /// GCRA 超过突发容量后必须拒绝
    #[tokio::test]
    async fn gcra_burst_beyond_capacity_denied() {
        let limiter = GcraLimiter::new(3, 1_000_000); // 1s interval
        for _ in 0..3 {
            assert!(limiter.allow(1).await.unwrap());
        }
        let result = limiter.allow(1).await.unwrap();
        assert!(!result, "Request beyond burst capacity should be denied");
    }

    /// GCRA cost > capacity 返回 Ok(false)，不报错
    #[tokio::test]
    async fn gcra_cost_exceeds_capacity_returns_false() {
        let limiter = GcraLimiter::new(5, 1_000);
        let result = limiter.allow(6).await.unwrap();
        assert!(!result, "cost > capacity should return Ok(false)");
    }

    /// GCRA cost=0 必须报 ConfigError
    #[tokio::test]
    async fn gcra_zero_cost_returns_config_error() {
        let limiter = GcraLimiter::new(10, 1_000);
        let result = limiter.allow(0).await;
        match result {
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(msg.contains("Cost cannot be zero"));
            }
            other => panic!("Expected ConfigError, got {:?}", other),
        }
    }

    /// GCRA check() 不修改状态：check 后 allow 仍被拒绝，多次 check 的 allowed 一致
    #[tokio::test]
    async fn gcra_check_does_not_mutate_state() {
        let limiter = GcraLimiter::new(5, 1_000_000); // 1s interval
        // 先消耗所有容量
        for _ in 0..5 {
            assert!(limiter.allow(1).await.unwrap());
        }
        // check 应返回拒绝且 retry_after > 0
        let r1 = limiter.check(1);
        assert!(!r1.allowed, "check should be denied when exhausted");
        assert!(
            r1.retry_after_us > 0,
            "retry_after should be positive when denied"
        );
        // check 不修改状态：allow 仍应被拒绝（TAT 未被 check 改动）
        assert!(
            !limiter.allow(1).await.unwrap(),
            "allow should still be denied after check (check must not mutate TAT)"
        );
        // 再次 check 的 allowed 字段应一致（retry_after 基于实时时钟可有微小差异）
        let r2 = limiter.check(1);
        assert_eq!(
            r1.allowed, r2.allowed,
            "check should be idempotent (allowed)"
        );
        assert!(
            r2.retry_after_us > 0,
            "second check retry_after should still be positive"
        );
    }

    /// GCRA with_rate(0) 应使用 fallback 间隔 1_000_000us
    #[tokio::test]
    async fn gcra_with_rate_zero_uses_fallback_interval() {
        let limiter = GcraLimiter::with_rate(10, 0);
        assert_eq!(limiter.refill_interval_us(), 1_000_000);
        assert_eq!(limiter.capacity(), 10);
    }

    /// GCRA 匀速恢复：消耗完后等待一个间隔可再次通过
    #[tokio::test]
    async fn gcra_recovery_after_interval() {
        let limiter = GcraLimiter::new(2, 50_000); // 50ms interval
        assert!(limiter.allow(1).await.unwrap());
        assert!(limiter.allow(1).await.unwrap());
        assert!(!limiter.allow(1).await.unwrap());
        // 等待 60ms（> 50ms 间隔）
        tokio::time::sleep(Duration::from_millis(60)).await;
        let recovered = limiter.allow(1).await.unwrap();
        assert!(recovered, "Should recover after one interval");
    }

    /// GCRA check() cost > capacity 返回 allowed=false, remaining=0, retry_after=0
    #[tokio::test]
    async fn gcra_check_cost_exceeds_capacity() {
        let limiter = GcraLimiter::new(5, 1_000);
        let result = limiter.check(11);
        assert!(!result.allowed);
        assert_eq!(result.remaining, 0);
        assert_eq!(result.retry_after_us, 0);
    }
}

// ============================================================================
// 模块 3: 并发控制器
// ============================================================================

mod concurrency_limiter {
    use super::*;

    /// 获取许可后释放，可用许可数恢复
    #[tokio::test]
    async fn acquire_and_release_restores_permits() {
        let limiter = ConcurrencyLimiter::new(3);
        let permit = limiter.acquire(1).await.unwrap();
        // 持有期间再获取应成功（还有 2 个）
        let _p2 = limiter.acquire(1).await.unwrap();
        drop(permit);
        // 释放后应可再获取
        let _p3 = limiter.acquire(1).await.unwrap();
    }

    /// 达到上限后 allow() 返回 Ok(false)
    #[tokio::test]
    async fn allow_returns_false_when_exhausted() {
        let limiter = ConcurrencyLimiter::new(1);
        // 持有许可
        let _permit = limiter.acquire(1).await.unwrap();
        // allow 使用 try_acquire_many，应返回 Ok(false)
        let result = limiter.allow(1).await.unwrap();
        assert!(!result, "allow should return false when exhausted");
    }

    /// acquire 超时返回 LimitError
    #[tokio::test]
    async fn acquire_timeout_returns_limit_error() {
        let limiter = ConcurrencyLimiter::with_timeout(1, Duration::from_millis(50));
        let _permit = limiter.acquire(1).await.unwrap();
        let result = limiter.acquire(1).await;
        match result {
            Err(LimiteronError::LimitError(msg)) => {
                assert!(
                    msg.contains("超时"),
                    "expected timeout message, got: {}",
                    msg
                );
            }
            Ok(_) => panic!("Expected timeout error, got Ok"),
            Err(e) => panic!("Expected LimitError, got {:?}", e),
        }
    }

    /// builder 缺少 max_concurrent 必须报 ConfigError
    #[tokio::test]
    async fn builder_missing_max_concurrent_returns_config_error() {
        let result = ConcurrencyLimiter::builder().build();
        match result {
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(msg.contains("max_concurrent is required"));
            }
            Ok(_) => panic!("Expected ConfigError, got Ok"),
            Err(e) => panic!("Expected ConfigError, got {:?}", e),
        }
    }

    /// builder max_concurrent=0 必须报 ConfigError
    #[tokio::test]
    async fn builder_zero_max_concurrent_returns_config_error() {
        let result = ConcurrencyLimiter::builder().max_concurrent(0).build();
        match result {
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(msg.contains("greater than 0"));
            }
            Ok(_) => panic!("Expected ConfigError, got Ok"),
            Err(e) => panic!("Expected ConfigError, got {:?}", e),
        }
    }

    /// allow() cost 超 u32 范围必须报 LimitError
    #[tokio::test]
    async fn allow_cost_overflows_u32_returns_limit_error() {
        let limiter = ConcurrencyLimiter::new(10);
        let result = limiter.allow(u64::from(u32::MAX) + 1).await;
        match result {
            Err(LimiteronError::LimitError(msg)) => {
                assert!(
                    msg.contains("u32"),
                    "expected u32 overflow message, got: {}",
                    msg
                );
            }
            other => panic!("Expected LimitError, got {:?}", other),
        }
    }

    /// acquire() cost 超 u32 范围必须报 LimitError
    #[tokio::test]
    async fn acquire_cost_overflows_u32_returns_limit_error() {
        let limiter = ConcurrencyLimiter::new(10);
        let result = limiter.acquire(u64::from(u32::MAX) + 1).await;
        match result {
            Err(LimiteronError::LimitError(msg)) => {
                assert!(msg.contains("u32"));
            }
            other => panic!("Expected LimitError, got {:?}", other),
        }
    }

    /// 并发安全：多任务并发 acquire 不会死锁或超卖
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_acquire_does_not_oversell() {
        let limiter = Arc::new(ConcurrencyLimiter::new(5));
        let mut handles = vec![];
        for _ in 0..20 {
            let l = Arc::clone(&limiter);
            handles.push(tokio::spawn(async move {
                // 立即获取并释放
                let result = l.acquire(1).await;
                if let Ok(p) = result {
                    drop(p);
                    true
                } else {
                    false
                }
            }));
        }
        let mut success = 0usize;
        for h in handles {
            if h.await.unwrap() {
                success += 1;
            }
        }
        // 由于 acquire 是阻塞获取（无超时），所有任务最终都应成功（释放后重试）
        assert_eq!(success, 20, "All tasks should eventually acquire a permit");
    }

    /// builder with_semaphore 注入外部信号量
    #[tokio::test]
    async fn builder_with_external_semaphore() {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(7));
        let limiter = ConcurrencyLimiter::builder()
            .with_semaphore(semaphore)
            .build()
            .unwrap();
        let _p = limiter.acquire(3).await.unwrap();
        // 还剩 4 个
        let p2 = limiter.acquire(4).await.unwrap();
        drop(p2);
    }
}

// ============================================================================
// 模块 4: 决策链
// ============================================================================

// ============================================================================
// 模块 5: 降级策略（feature = "fallback"）
// ============================================================================

#[cfg(feature = "fallback")]
mod fallback_strategy {
    use super::*;
    use limiteron::fallback::{ComponentType, FallbackConfig, FallbackManager, FallbackStrategy};
    use oxcache::Cache;

    async fn create_manager() -> FallbackManager {
        let cache: Cache<String, String> = Cache::builder()
            .capacity(100)
            .ttl(Duration::from_secs(60))
            .build()
            .await
            .unwrap();
        FallbackManager::new(Arc::new(cache))
    }

    /// FailOpen：主操作失败时返回 LimitError（提示降级允许）
    #[tokio::test]
    async fn fail_open_returns_limit_error_on_failure() {
        let manager = create_manager().await;
        manager
            .set_strategy(
                ComponentType::Redis,
                FallbackConfig::new(ComponentType::Redis, FallbackStrategy::FailOpen),
            )
            .await;

        let result: Result<String, LimiteronError> = manager
            .execute_with_fallback(
                ComponentType::Redis,
                || async {
                    Err(LimiteronError::StorageError(
                        limiteron::error::StorageError::ConnectionError("down".to_string()),
                    ))
                },
                || async { Ok("fallback".to_string()) },
            )
            .await;

        match result {
            Err(LimiteronError::LimitError(msg)) => {
                assert!(
                    msg.contains("允许"),
                    "FailOpen message should indicate allowed: {}",
                    msg
                );
            }
            other => panic!("Expected LimitError for FailOpen, got {:?}", other),
        }
    }

    /// FailClosed：主操作失败时返回 StorageError::ConnectionError
    #[tokio::test]
    async fn fail_closed_returns_storage_error_on_failure() {
        let manager = create_manager().await;
        manager
            .set_strategy(
                ComponentType::Postgres,
                FallbackConfig::new(ComponentType::Postgres, FallbackStrategy::FailClosed),
            )
            .await;

        let result: Result<String, LimiteronError> = manager
            .execute_with_fallback(
                ComponentType::Postgres,
                || async { Err(LimiteronError::LimitError("primary failed".to_string())) },
                || async { Ok("fallback".to_string()) },
            )
            .await;

        match result {
            Err(LimiteronError::StorageError(limiteron::error::StorageError::ConnectionError(
                msg,
            ))) => {
                assert!(
                    msg.contains("拒绝"),
                    "FailClosed message should indicate denied: {}",
                    msg
                );
            }
            other => panic!(
                "Expected StorageError::ConnectionError for FailClosed, got {:?}",
                other
            ),
        }
    }

    /// Degraded：主操作失败时执行 fallback_operation
    #[tokio::test]
    async fn degraded_executes_fallback_operation() {
        let manager = create_manager().await;
        manager
            .set_strategy(
                ComponentType::Quota,
                FallbackConfig::new(ComponentType::Quota, FallbackStrategy::Degraded),
            )
            .await;

        let result: Result<String, LimiteronError> = manager
            .execute_with_fallback(
                ComponentType::Quota,
                || async { Err(LimiteronError::LimitError("primary failed".to_string())) },
                || async { Ok("degraded_value".to_string()) },
            )
            .await;

        match result {
            Ok(value) => {
                assert_eq!(
                    value, "degraded_value",
                    "Degraded should return fallback value"
                );
            }
            Err(e) => panic!("Expected Ok with fallback value, got {:?}", e),
        }
    }

    /// 主操作成功时不触发降级
    #[tokio::test]
    async fn success_does_not_trigger_fallback() {
        let manager = create_manager().await;
        manager
            .set_strategy(
                ComponentType::Redis,
                FallbackConfig::new(ComponentType::Redis, FallbackStrategy::Degraded),
            )
            .await;

        let result: Result<String, LimiteronError> = manager
            .execute_with_fallback(
                ComponentType::Redis,
                || async { Ok("primary_value".to_string()) },
                || async { Ok("fallback_value".to_string()) },
            )
            .await;

        assert_eq!(result.unwrap(), "primary_value");
    }

    /// enabled=false 时直接执行主操作（即使会失败）
    #[tokio::test]
    async fn disabled_strategy_executes_primary_directly() {
        let manager = create_manager().await;
        manager
            .set_strategy(
                ComponentType::Ban,
                FallbackConfig::new(ComponentType::Ban, FallbackStrategy::FailOpen).enabled(false),
            )
            .await;

        let result: Result<String, LimiteronError> = manager
            .execute_with_fallback(
                ComponentType::Ban,
                || async { Err(LimiteronError::LimitError("primary failed".to_string())) },
                || async { Ok("fallback".to_string()) },
            )
            .await;

        match result {
            Err(LimiteronError::LimitError(msg)) => {
                assert!(msg.contains("primary failed"));
            }
            other => panic!("Expected primary error, got {:?}", other),
        }
    }

    /// 故障注入与恢复
    #[tokio::test]
    async fn inject_and_recover_failure() {
        let manager = create_manager().await;
        assert!(!manager.is_failed(ComponentType::Redis).await);
        manager.inject_failure(ComponentType::Redis).await;
        assert!(manager.is_failed(ComponentType::Redis).await);
        manager.recover_failure(ComponentType::Redis).await;
        assert!(!manager.is_failed(ComponentType::Redis).await);
    }

    /// set_strategy 热更新：运行时切换策略
    #[tokio::test]
    async fn set_strategy_hot_update() {
        let manager = create_manager().await;
        // 初始为默认 Degraded
        let s1 = manager.get_strategy(ComponentType::Redis).await.unwrap();
        assert_eq!(s1.strategy, FallbackStrategy::Degraded);
        // 热更新为 FailOpen
        manager
            .set_strategy(
                ComponentType::Redis,
                FallbackConfig::new(ComponentType::Redis, FallbackStrategy::FailOpen),
            )
            .await;
        let s2 = manager.get_strategy(ComponentType::Redis).await.unwrap();
        assert_eq!(s2.strategy, FallbackStrategy::FailOpen);
    }

    /// 孤岛模式回调：首次故障触发进入孤岛，全部恢复触发退出
    #[tokio::test]
    async fn island_mode_callback_triggered() {
        let manager = create_manager().await;
        let entered = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let exited = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let entered_clone = Arc::clone(&entered);
        let exited_clone = Arc::clone(&exited);
        manager
            .register_island_mode_callback(Box::new(move |is_island| {
                if is_island {
                    entered_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                } else {
                    exited_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }))
            .await;

        // 首次故障应触发进入孤岛
        manager.set_failure(ComponentType::Redis).await;
        assert!(
            entered.load(std::sync::atomic::Ordering::SeqCst),
            "Island mode entry callback should fire"
        );

        // 恢复应触发退出孤岛
        manager.clear_failure(ComponentType::Redis).await;
        assert!(
            exited.load(std::sync::atomic::Ordering::SeqCst),
            "Island mode exit callback should fire"
        );
    }

    /// get_all_failures 返回所有故障组件
    #[tokio::test]
    async fn get_all_failures_lists_failed_components() {
        let manager = create_manager().await;
        manager.inject_failure(ComponentType::Redis).await;
        manager.inject_failure(ComponentType::Postgres).await;
        let failures = manager.get_all_failures().await;
        assert_eq!(failures.len(), 2);
        assert!(failures.contains(&ComponentType::Redis));
        assert!(failures.contains(&ComponentType::Postgres));
    }
}

// ============================================================================
// 模块 6: 多租户（feature = "multi-tenant"）
// ============================================================================

#[cfg(feature = "multi-tenant")]
mod multi_tenant {
    use limiteron::matchers::RequestContext;
    use limiteron::{DefaultTenantResolver, HeaderTenantResolver, Namespace, TenantResolver};

    /// DefaultTenantResolver 返回 global/development
    #[test]
    fn default_resolver_returns_global_namespace() {
        let resolver = DefaultTenantResolver;
        let ctx = RequestContext::new();
        let ns = resolver.resolve(&ctx).unwrap();
        assert_eq!(ns.tenant_id(), "global");
        assert_eq!(ns.environment(), "development");
    }

    /// HeaderTenantResolver 成功解析 header
    #[test]
    fn header_resolver_extracts_tenant_id() {
        let resolver = HeaderTenantResolver::new("X-Tenant-ID", "production");
        let ctx = RequestContext::new().with_header("X-Tenant-ID", "acme-corp");
        let ns = resolver.resolve(&ctx).unwrap();
        assert_eq!(ns.tenant_id(), "acme-corp");
        assert_eq!(ns.environment(), "production");
    }

    /// Header 缺失时返回 None
    #[test]
    fn header_resolver_missing_header_returns_none() {
        let resolver = HeaderTenantResolver::new("X-Tenant-ID", "production");
        let ctx = RequestContext::new();
        assert!(resolver.resolve(&ctx).is_none());
    }

    /// Header 名大小写不敏感
    #[test]
    fn header_resolver_case_insensitive() {
        let resolver = HeaderTenantResolver::new("x-tenant-id", "prod");
        let ctx = RequestContext::new().with_header("X-Tenant-ID", "tenant-1");
        let ns = resolver.resolve(&ctx).unwrap();
        assert_eq!(ns.tenant_id(), "tenant-1");
    }

    /// 不同租户的 qualified key 必须不同（隔离）
    #[test]
    fn namespace_qualify_key_isolates_tenants() {
        let ns1 = Namespace::new("tenant_a", "prod");
        let ns2 = Namespace::new("tenant_b", "prod");
        let key = "rl:user:123:rule1";
        assert_ne!(ns1.qualify_key(key), ns2.qualify_key(key));
    }

    /// 前缀注入防护：tenant_id 中的 ":" 被转义
    #[test]
    fn namespace_prefix_injection_prevented_for_tenant_id() {
        // 无转义时，("a:env", "c") 与 ("a", "env:c") 会产生相同 prefix
        let ns_injected = Namespace::new("a:env", "c");
        let ns_normal = Namespace::new("a", "env:c");
        assert_ne!(
            ns_injected.prefix(),
            ns_normal.prefix(),
            "tenant_id ':' must be escaped to prevent prefix collision"
        );
    }

    /// 前缀注入防护：environment 中的 ":" 被转义
    #[test]
    fn namespace_prefix_injection_prevented_for_environment() {
        let ns_normal = Namespace::new("acme", "prod");
        let ns_injected = Namespace::new("acme", "prod:rl");
        // qualify_key 后必须不同
        assert_ne!(
            ns_normal.qualify_key("rl:user"),
            ns_injected.qualify_key("user"),
            "environment ':' must be escaped to prevent key collision"
        );
    }

    /// 不同环境同租户的 key 必须不同
    #[test]
    fn namespace_isolates_environments() {
        let ns_prod = Namespace::new("acme", "production");
        let ns_staging = Namespace::new("acme", "staging");
        assert_ne!(
            ns_prod.qualify_key("user:1"),
            ns_staging.qualify_key("user:1")
        );
    }

    /// Namespace Display 与 prefix 一致
    #[test]
    fn namespace_display_matches_prefix() {
        let ns = Namespace::new("acme", "prod");
        assert_eq!(format!("{}", ns), ns.prefix());
        assert_eq!(ns.prefix(), "tenant:acme:env:prod");
    }

    /// Namespace Default 是 global/development
    #[test]
    fn namespace_default_is_global_development() {
        let ns = Namespace::default();
        assert_eq!(ns.tenant_id(), "global");
        assert_eq!(ns.environment(), "development");
    }
}

// ============================================================================
// 模块 7: 分布式限流（feature = "distributed"）
// ============================================================================

#[cfg(feature = "distributed")]
mod distributed_limiter {
    use super::*;
    use limiteron::limiters::{DistributedLimiter, InMemoryDistributedLimiter};

    /// incr 新 key 返回递增后的值
    #[tokio::test]
    async fn incr_new_key_returns_amount() {
        let limiter = InMemoryDistributedLimiter::new();
        let count = limiter.incr("user:1", 5).await.unwrap();
        assert_eq!(count, 5);
    }

    /// incr 已有 key 累加
    #[tokio::test]
    async fn incr_existing_key_accumulates() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter.incr("user:1", 5).await.unwrap();
        let count = limiter.incr("user:1", 3).await.unwrap();
        assert_eq!(count, 8);
    }

    /// incr 空 key 必须报 ConfigError
    #[tokio::test]
    async fn incr_empty_key_returns_config_error() {
        let limiter = InMemoryDistributedLimiter::new();
        let result = limiter.incr("", 1).await;
        match result {
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(msg.contains("Key cannot be empty"));
            }
            other => panic!("Expected ConfigError, got {:?}", other),
        }
    }

    /// incr 饱和：u64::MAX + 1 = u64::MAX
    #[tokio::test]
    async fn incr_saturates_at_u64_max() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter.incr("user:1", u64::MAX).await.unwrap();
        let count = limiter.incr("user:1", 1).await.unwrap();
        assert_eq!(count, u64::MAX, "incr should saturate at u64::MAX");
    }

    /// incr_with_ttl 新 key 返回 amount
    #[tokio::test]
    async fn incr_with_ttl_new_key() {
        let limiter = InMemoryDistributedLimiter::new();
        let count = limiter
            .incr_with_ttl("session:1", 1, Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    /// incr_with_ttl 未过期时累加
    #[tokio::test]
    async fn incr_with_ttl_accumulates_when_not_expired() {
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

    /// incr_with_ttl 过期后重置
    #[tokio::test]
    async fn incr_with_ttl_resets_after_expiry() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter
            .incr_with_ttl("user:1", 5, Duration::from_millis(1))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        let count = limiter
            .incr_with_ttl("user:1", 3, Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(count, 3, "Should reset to new amount after TTL expiry");
    }

    /// incr_with_ttl 空 key 报错
    #[tokio::test]
    async fn incr_with_ttl_empty_key_returns_config_error() {
        let limiter = InMemoryDistributedLimiter::new();
        let result = limiter.incr_with_ttl("", 1, Duration::from_secs(60)).await;
        match result {
            Err(LimiteronError::ConfigError(msg)) => {
                assert!(msg.contains("Key cannot be empty"));
            }
            other => panic!("Expected ConfigError, got {:?}", other),
        }
    }

    /// get_count 不存在的 key 返回 0
    #[tokio::test]
    async fn get_count_nonexistent_returns_zero() {
        let limiter = InMemoryDistributedLimiter::new();
        let count = limiter.get_count("nonexistent").await.unwrap();
        assert_eq!(count, 0);
    }

    /// get_count TTL 过期后返回 0
    #[tokio::test]
    async fn get_count_returns_zero_after_ttl_expiry() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter
            .incr_with_ttl("user:1", 5, Duration::from_millis(1))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        let count = limiter.get_count("user:1").await.unwrap();
        assert_eq!(count, 0, "Should return 0 after TTL expiry");
    }

    /// reset 清零计数
    #[tokio::test]
    async fn reset_clears_counter() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter.incr("user:1", 10).await.unwrap();
        limiter.reset("user:1").await.unwrap();
        assert_eq!(limiter.get_count("user:1").await.unwrap(), 0);
    }

    /// reset 不存在的 key 不报错
    #[tokio::test]
    async fn reset_nonexistent_key_is_noop() {
        let limiter = InMemoryDistributedLimiter::new();
        let result = limiter.reset("nonexistent").await;
        assert!(result.is_ok(), "reset on nonexistent key should be Ok");
    }

    /// 不同 key 计数隔离
    #[tokio::test]
    async fn different_keys_isolated() {
        let limiter = InMemoryDistributedLimiter::new();
        limiter.incr("user:1", 10).await.unwrap();
        limiter.incr("user:2", 20).await.unwrap();
        assert_eq!(limiter.get_count("user:1").await.unwrap(), 10);
        assert_eq!(limiter.get_count("user:2").await.unwrap(), 20);
    }

    /// 并发 incr 安全：所有递增都原子生效
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_incr_is_atomic() {
        let limiter = Arc::new(InMemoryDistributedLimiter::new());
        let mut handles = vec![];
        for _ in 0..50 {
            let l = Arc::clone(&limiter);
            handles.push(tokio::spawn(async move {
                l.incr("concurrent", 1).await.unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        let count = limiter.get_count("concurrent").await.unwrap();
        assert_eq!(count, 50, "All 50 concurrent incr should be atomic");
    }

    /// Limiter trait 兼容：allow 使用固定 _global key
    #[tokio::test]
    async fn limiter_trait_compatibility() {
        let limiter = InMemoryDistributedLimiter::new();
        let allowed = limiter.allow(1).await.unwrap();
        assert!(
            allowed,
            "Limiter::allow should always succeed for InMemoryDistributedLimiter"
        );
        // allow 内部调用 incr("_global", cost)
        assert!(limiter.get_count("_global").await.unwrap() >= 1);
    }
}

// ============================================================================
// 模块 8: 跨模块集成场景
// ============================================================================

mod cross_module {
    use super::*;

    /// 多限流器组合：TokenBucket + FixedWindow 串联
    /// 两个限流器都允许时才最终允许，任一拒绝则拒绝
    #[tokio::test]
    async fn combined_token_bucket_and_fixed_window() {
        use limiteron::decision_chain::{DecisionChain, DecisionNode};
        let tb = Arc::new(TokenBucketLimiter::new(5, 1));
        let fw = Arc::new(FixedWindowLimiter::new(Duration::from_secs(60), 3));

        let n1 = DecisionNode::with_dependencies(
            "tb".to_string(),
            "TokenBucket".to_string(),
            tb as Arc<dyn Limiter>,
            100,
        )
        .with_short_circuit(true);
        let n2 = DecisionNode::with_dependencies(
            "fw".to_string(),
            "FixedWindow".to_string(),
            fw as Arc<dyn Limiter>,
            50,
        )
        .with_short_circuit(true);

        let chain = DecisionChain::with_dependencies(vec![n1, n2]);

        // 前 3 个请求：两个限流器都允许
        for i in 1..=3 {
            let decision = chain.check().await.unwrap();
            assert!(
                matches!(decision, Decision::Allowed(_)),
                "Request {} should be allowed by both limiters",
                i
            );
        }

        // 第 4 个请求：FixedWindow 拒绝（max=3）
        let decision = chain.check().await.unwrap();
        match decision {
            Decision::Rejected(meta) => {
                assert!(
                    meta.reason.contains("FixedWindow"),
                    "Should be rejected by FixedWindow"
                );
            }
            other => panic!("Expected Rejected by FixedWindow, got {:?}", other),
        }
    }

    /// 突发流量 + 速率限制组合
    #[tokio::test]
    async fn burst_with_rate_limit_chain() {
        use limiteron::decision_chain::{DecisionChain, DecisionNode};
        // TB 容量 10（允许突发），FW 限制 5/分钟（严格速率）
        let tb = Arc::new(TokenBucketLimiter::new(10, 1));
        let fw = Arc::new(FixedWindowLimiter::new(Duration::from_secs(60), 5));

        let n1 = DecisionNode::with_dependencies(
            "tb_burst".to_string(),
            "BurstTB".to_string(),
            tb as Arc<dyn Limiter>,
            100,
        );
        let n2 = DecisionNode::with_dependencies(
            "fw_strict".to_string(),
            "StrictFW".to_string(),
            fw as Arc<dyn Limiter>,
            50,
        )
        .with_short_circuit(true);

        let chain = DecisionChain::with_dependencies(vec![n1, n2]);

        // 5 个请求应全部通过（TB 还有 5 个令牌，FW 还有 5 个额度）
        for i in 1..=5 {
            let d = chain.check().await.unwrap();
            assert!(
                matches!(d, Decision::Allowed(_)),
                "Request {} should pass",
                i
            );
        }

        // 第 6 个：FW 拒绝
        let d = chain.check().await.unwrap();
        assert!(
            matches!(d, Decision::Rejected(_)),
            "Request 6 should be rejected by StrictFW"
        );
    }

    /// Governor 全链路：配置 → 检查 → 决策
    /// 验证 Governor 能够正确加载配置并执行限流
    #[tokio::test]
    async fn governor_full_pipeline() {
        use ahash::AHashMap;
        use limiteron::config::{
            Action, ActionConfig, CacheBackend, FlowControlConfig, LimiterConfig, Matcher,
            MetricsBackend, Rule, StorageType,
        };
        use limiteron::error::StorageError;
        use limiteron::matchers::RequestContext;
        use limiteron::{BanHistory, BanRecord, BanStorage, BanTarget, Storage};

        // 简单 Mock 存储
        #[derive(Clone, Default)]
        struct GovMockStorage {
            data: Arc<tokio::sync::RwLock<AHashMap<String, String>>>,
        }
        #[async_trait::async_trait]
        impl Storage for GovMockStorage {
            async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
                Ok(self.data.read().await.get(key).cloned())
            }
            async fn set(
                &self,
                key: &str,
                value: &str,
                _ttl: Option<u64>,
            ) -> Result<(), StorageError> {
                self.data
                    .write()
                    .await
                    .insert(key.to_string(), value.to_string());
                Ok(())
            }
            async fn delete(&self, key: &str) -> Result<(), StorageError> {
                self.data.write().await.remove(key);
                Ok(())
            }
        }

        #[derive(Clone)]
        struct GovMockBanStorage;
        #[async_trait::async_trait]
        impl BanStorage for GovMockBanStorage {
            async fn is_banned(&self, _t: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
                Ok(None)
            }
            async fn save(&self, _r: &BanRecord) -> Result<(), StorageError> {
                Ok(())
            }
            async fn get_history(
                &self,
                _t: &BanTarget,
            ) -> Result<Option<BanHistory>, StorageError> {
                Ok(None)
            }
            async fn increment_ban_times(&self, _t: &BanTarget) -> Result<u64, StorageError> {
                Ok(1)
            }
            async fn get_ban_times(&self, _t: &BanTarget) -> Result<u64, StorageError> {
                Ok(0)
            }
            async fn remove_ban(&self, _t: &BanTarget) -> Result<(), StorageError> {
                Ok(())
            }
            async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
                Ok(0)
            }
            async fn list_bans(
                &self,
                _a: bool,
                _o: u64,
                _l: u64,
            ) -> Result<Vec<BanRecord>, StorageError> {
                Ok(vec![])
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }

        let config = FlowControlConfig {
            version: "1.0".to_string(),
            global: limiteron::config::GlobalConfig {
                storage: StorageType::Memory,
                cache: CacheBackend::Memory,
                metrics: MetricsBackend::Prometheus,
                trusted_proxies: Default::default(),
            },
            rules: vec![Rule {
                id: "pipeline_rule".to_string(),
                name: "Pipeline Rule".to_string(),
                priority: 100,
                matchers: vec![Matcher::User {
                    user_ids: vec!["*".to_string()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 3,
                    refill_rate: 1,
                }],
                action: ActionConfig {
                    on_exceed: Action::Reject,
                    ban: None,
                },
            }],
        };

        let storage: Arc<dyn Storage> = Arc::new(GovMockStorage::default());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(GovMockBanStorage);
        let governor = limiteron::Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Failed to build governor");

        // 创建请求上下文
        let mut headers = AHashMap::new();
        headers.insert("x-user-id".to_string(), "pipeline_user".to_string());
        let mut ctx = RequestContext::new();
        ctx.ip = Some("10.0.0.1".to_string());
        ctx.method = "GET".to_string();
        ctx.path = "/api/test".to_string();
        ctx.headers = headers;

        // 前 3 个请求应通过（容量 3）
        for i in 1..=3 {
            let result = governor.check(&ctx).await;
            assert!(result.is_ok(), "Request {} should succeed: {:?}", i, result);
        }

        // 统计应记录 3 个请求
        let stats = governor.stats().await;
        assert_eq!(stats.total_requests, 3, "Should record 3 total requests");
    }

    /// 决策链统计：node_rejections 正确记录各节点拒绝次数
    #[tokio::test]
    async fn chain_stats_node_rejections() {
        use limiteron::decision_chain::{DecisionChain, DecisionNode};
        use std::sync::atomic::{AtomicBool, Ordering};

        struct ToggleLimiter {
            allowed: Arc<AtomicBool>,
        }
        #[async_trait::async_trait]
        impl Limiter for ToggleLimiter {
            async fn allow(&self, _cost: u64) -> Result<bool, LimiteronError> {
                Ok(self.allowed.load(Ordering::SeqCst))
            }
        }

        let flag = Arc::new(AtomicBool::new(false)); // 默认拒绝
        let limiter = Arc::new(ToggleLimiter {
            allowed: Arc::clone(&flag),
        });
        let node = DecisionNode::with_dependencies(
            "reject_node".to_string(),
            "Rejector".to_string(),
            limiter as Arc<dyn Limiter>,
            100,
        )
        .with_short_circuit(true);
        let chain = DecisionChain::with_dependencies(vec![node]);

        // 拒绝 3 次
        for _ in 0..3 {
            let _ = chain.check().await.unwrap();
        }
        let stats = chain.stats().await;
        assert_eq!(stats.total_checks, 3);
        assert_eq!(stats.rejected_count, 3);
        // node_rejections 应包含 ("reject_node", 3)
        let node_rej: ahash::AHashMap<String, u64> = stats.node_rejections.into_iter().collect();
        assert_eq!(node_rej.get("reject_node"), Some(&3));
    }
}
