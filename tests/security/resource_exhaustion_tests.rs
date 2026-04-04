//! 资源耗尽测试
//!
//! 测试覆盖：
//! - 内存耗尽测试（大量键创建测试、内存限制验证）
//! - CPU 耗尽测试（复杂模式处理测试、CPU 限制验证）
//! - 连接耗尽测试（大量连接处理测试、优雅降级验证）

use crate::common::MockQuotaStorage;
use limiteron::{Limiter, TokenBucketLimiter};
use limiteron::Storage;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Barrier;

// ============================================================================
// 内存耗尽测试
// ============================================================================

/// 测试大量键创建的内存使用
///
/// 验证系统在创建大量键时的内存行为
#[tokio::test]
async fn test_large_key_creation_memory() {
    let storage = Arc::new(MockQuotaStorage::new());
    let key_count = 10000;
    let barrier = Arc::new(Barrier::new(100));

    let mut handles = vec![];

    for i in 0..key_count {
        let storage = Arc::clone(&storage);
        let barrier = Arc::clone(&barrier);

        handles.push(tokio::spawn(async move {
            if i % 100 == 0 {
                barrier.wait().await;
            }

            let key = format!("user_{}", i);
            let value = format!("value_{}", i);

            storage.set(&key, &value, Some(60)).await
        }));
    }

    let mut success_count = 0;
    let mut error_count = 0;

    for handle in handles {
        match handle.await {
            Ok(Ok(_)) => success_count += 1,
            _ => error_count += 1,
        }
    }

    println!(
        "Large key creation test: {} success, {} errors out of {} keys",
        success_count, error_count, key_count
    );

    // 大多数操作应成功
    assert!(
        success_count > key_count / 2,
        "Most key creations should succeed"
    );
}

/// 测试内存限制验证
///
/// 验证系统在内存限制下的行为
#[tokio::test]
async fn test_memory_limit_validation() {
    // 设置最大条目限制
    use crate::common::MockQuotaBehavior;

    let behavior = MockQuotaBehavior::new().with_max_entries(100);
    let storage = Arc::new(MockQuotaStorage::with_behavior(behavior));

    let mut success_count = 0;
    let mut limit_reached_count = 0;

    // 尝试创建超过限制的条目
    for i in 0..200 {
        let key = format!("user_{}", i);
        let value = format!("value_{}", i);

        match storage.set(&key, &value, Some(60)).await {
            Ok(_) => success_count += 1,
            Err(_) => limit_reached_count += 1,
        }
    }

    println!(
        "Memory limit test: {} success, {} limit reached",
        success_count, limit_reached_count
    );

    // 成功数不应超过限制
    assert!(
        success_count <= 100,
        "Success count should not exceed limit"
    );

    // 应有一些请求因限制被拒绝
    assert!(
        limit_reached_count > 0,
        "Some requests should be rejected due to limit"
    );
}

/// 测试大键值的内存处理
///
/// 验证系统处理大键值时的行为
#[tokio::test]
async fn test_large_value_memory_handling() {
    let storage = Arc::new(MockQuotaStorage::new());

    // 创建大值（1KB）
    let large_value = "x".repeat(1024);

    let mut handles = vec![];

    for i in 0..100 {
        let storage = Arc::clone(&storage);
        let large_value = large_value.clone();

        handles.push(tokio::spawn(async move {
            let key = format!("large_key_{}", i);
            storage.set(&key, &large_value, Some(60)).await
        }));
    }

    let mut success_count = 0;

    for handle in handles {
        if let Ok(Ok(_)) = handle.await {
            success_count += 1;
        }
    }

    println!("Large value test: {} out of 100 succeeded", success_count);

    // 大多数操作应成功
    assert!(
        success_count > 50,
        "Most large value operations should succeed"
    );
}

/// 测试内存压力下的系统稳定性
///
/// 验证系统在内存压力下保持稳定
#[tokio::test]
async fn test_memory_pressure_stability() {
    let storage = Arc::new(MockQuotaStorage::new());
    let operations = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));

    let duration = Duration::from_secs(5);
    let start = Instant::now();

    while start.elapsed() < duration {
        let storage = Arc::clone(&storage);
        let operations = Arc::clone(&operations);
        let errors = Arc::clone(&errors);

        let mut handles = vec![];

        for i in 0..50 {
            let storage = Arc::clone(&storage);
            let operations = Arc::clone(&operations);
            let errors = Arc::clone(&errors);

            handles.push(tokio::spawn(async move {
                operations.fetch_add(1, Ordering::SeqCst);

                let key = format!("pressure_key_{}", i);
                let value = format!("pressure_value_{}", i);

                if storage.set(&key, &value, Some(60)).await.is_err() {
                    errors.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }

        for handle in handles {
            handle.await.expect("Task should complete");
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let total_ops = operations.load(Ordering::SeqCst);
    let total_errors = errors.load(Ordering::SeqCst);

    println!(
        "Memory pressure test: {} operations, {} errors ({:.2}%)",
        total_ops,
        total_errors,
        (total_errors as f64 / total_ops as f64) * 100.0
    );

    // 错误率应保持在合理范围内
    let error_rate = total_errors as f64 / total_ops as f64;
    assert!(error_rate < 0.5, "Error rate should be below 50%");
}

// ============================================================================
// CPU 耗尽测试
// ============================================================================

/// 测试复杂模式处理的 CPU 使用
///
/// 验证系统处理复杂模式时的 CPU 行为
#[tokio::test]
async fn test_complex_pattern_cpu_usage() {
    let limiter = Arc::new(TokenBucketLimiter::new(10000, 1000));
    let processed = Arc::new(AtomicU64::new(0));
    let barrier = Arc::new(Barrier::new(500));

    let mut handles = vec![];

    for _ in 0..500 {
        let limiter = Arc::clone(&limiter);
        let processed = Arc::clone(&processed);
        let barrier = Arc::clone(&barrier);

        handles.push(tokio::spawn(async move {
            barrier.wait().await;

            // 模拟复杂处理
            for _ in 0..10 {
                let _ = limiter.allow(1).await;
            }

            processed.fetch_add(1, Ordering::SeqCst);
        }));
    }

    let start = Instant::now();

    for handle in handles {
        handle.await.expect("Task should complete");
    }

    let elapsed = start.elapsed();
    let total_processed = processed.load(Ordering::SeqCst);

    println!(
        "Complex pattern test: {} processed in {:?}",
        total_processed, elapsed
    );

    // 处理应在合理时间内完成
    assert!(
        elapsed < Duration::from_secs(30),
        "Processing should complete in reasonable time"
    );
    assert_eq!(total_processed, 500, "All patterns should be processed");
}

/// 测试 CPU 限制验证
///
/// 验证系统在高 CPU 负载下的行为
#[tokio::test]
async fn test_cpu_limit_validation() {
    let limiter = Arc::new(TokenBucketLimiter::new(100000, 10000));
    let operations = Arc::new(AtomicU64::new(0));
    let success = Arc::new(AtomicU64::new(0));

    let duration = Duration::from_secs(3);
    let start = Instant::now();

    while start.elapsed() < duration {
        let limiter = Arc::clone(&limiter);
        let operations = Arc::clone(&operations);
        let success = Arc::clone(&success);

        let mut handles = vec![];

        for _ in 0..100 {
            let limiter = Arc::clone(&limiter);
            let operations = Arc::clone(&operations);
            let success = Arc::clone(&success);

            handles.push(tokio::spawn(async move {
                operations.fetch_add(1, Ordering::SeqCst);

                if let Ok(true) = limiter.allow(1).await {
                    success.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }

        for handle in handles {
            handle.await.expect("Task should complete");
        }
    }

    let total_ops = operations.load(Ordering::SeqCst);
    let total_success = success.load(Ordering::SeqCst);

    // 计算吞吐量
    let throughput = total_ops as f64 / duration.as_secs_f64();

    println!(
        "CPU limit test: {} ops, {} success, {:.0} ops/sec",
        total_ops, total_success, throughput
    );

    // 吞吐量应保持在合理范围
    assert!(throughput > 100.0, "Throughput should be reasonable");
}

/// 测试计算密集型任务的 CPU 行为
///
/// 验证系统处理计算密集型任务时的 CPU 使用
#[tokio::test]
async fn test_compute_intensive_cpu_behavior() {
    let limiter = Arc::new(TokenBucketLimiter::new(100000, 10000));
    let barrier = Arc::new(Barrier::new(200));

    let mut handles = vec![];

    for _ in 0..200 {
        let limiter = Arc::clone(&limiter);
        let barrier = Arc::clone(&barrier);

        handles.push(tokio::spawn(async move {
            barrier.wait().await;

            let start = Instant::now();
            let mut count = 0;

            // 在 100ms 内尽可能多地执行操作
            while start.elapsed() < Duration::from_millis(100) {
                let _ = limiter.allow(1).await;
                count += 1;
            }

            count
        }));
    }

    let start = Instant::now();
    let mut total_ops = 0;

    for handle in handles {
        if let Ok(count) = handle.await {
            total_ops += count;
        }
    }

    let elapsed = start.elapsed();

    println!(
        "Compute intensive test: {} ops in {:?} ({:.0} ops/sec)",
        total_ops,
        elapsed,
        total_ops as f64 / elapsed.as_secs_f64()
    );

    // 应完成大量操作
    assert!(
        total_ops > 1000,
        "Should complete significant number of operations"
    );
}

// ============================================================================
// 连接耗尽测试
// ============================================================================

/// 测试大量并发连接处理
///
/// 验证系统处理大量并发连接的能力
#[tokio::test]
async fn test_many_concurrent_connections() {
    let storage = Arc::new(MockQuotaStorage::new());
    let connections = Arc::new(AtomicU64::new(0));
    let successful = Arc::new(AtomicU64::new(0));
    let barrier = Arc::new(Barrier::new(500));

    let mut handles = vec![];

    for _ in 0..500 {
        let storage = Arc::clone(&storage);
        let connections = Arc::clone(&connections);
        let successful = Arc::clone(&successful);
        let barrier = Arc::clone(&barrier);

        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            connections.fetch_add(1, Ordering::SeqCst);

            // 模拟连接操作
            let key = format!("conn_{}", connections.load(Ordering::SeqCst));

            match storage.get(&key).await {
                Ok(_) => {
                    successful.fetch_add(1, Ordering::SeqCst);
                    true
                }
                Err(_) => false,
            }
        }));
    }

    let start = Instant::now();

    for handle in handles {
        handle.await.expect("Task should complete");
    }

    let elapsed = start.elapsed();
    let total_connections = connections.load(Ordering::SeqCst);
    let total_successful = successful.load(Ordering::SeqCst);

    println!(
        "Concurrent connections test: {} connections, {} successful in {:?}",
        total_connections, total_successful, elapsed
    );

    // 所有连接应被处理
    assert_eq!(total_connections, 500, "All connections should be tracked");

    // 大多数操作应成功
    assert!(total_successful > 400, "Most operations should succeed");
}

/// 测试连接超时处理
///
/// 验证系统在连接超时时的行为
#[tokio::test]
async fn test_connection_timeout_handling() {
    let storage = Arc::new(MockQuotaStorage::new());
    let timeout_count = Arc::new(AtomicU64::new(0));
    let success_count = Arc::new(AtomicU64::new(0));

    let mut handles = vec![];

    for i in 0..100 {
        let storage = Arc::clone(&storage);
        let timeout_count = Arc::clone(&timeout_count);
        let success_count = Arc::clone(&success_count);

        handles.push(tokio::spawn(async move {
            let key = format!("timeout_key_{}", i);

            // 设置短超时
            let result = tokio::time::timeout(Duration::from_millis(10), storage.get(&key)).await;

            match result {
                Ok(Ok(_)) => success_count.fetch_add(1, Ordering::SeqCst),
                _ => timeout_count.fetch_add(1, Ordering::SeqCst),
            };
        }));
    }

    for handle in handles {
        handle.await.expect("Task should complete");
    }

    let timeouts = timeout_count.load(Ordering::SeqCst);
    let successes = success_count.load(Ordering::SeqCst);

    println!(
        "Connection timeout test: {} successes, {} timeouts",
        successes, timeouts
    );

    // 总数应等于请求数
    assert_eq!(
        timeouts + successes,
        100,
        "All requests should be accounted for"
    );
}

/// 测试优雅降级验证
///
/// 验证系统在资源不足时能优雅降级
#[tokio::test]
async fn test_graceful_degradation() {
    use crate::common::MockQuotaBehavior;

    // 设置严格的资源限制
    let behavior = MockQuotaBehavior::new()
        .with_max_entries(10)
        .with_fail_mode(false);

    let storage = Arc::new(MockQuotaStorage::with_behavior(behavior));
    let accepted = Arc::new(AtomicU64::new(0));
    let degraded = Arc::new(AtomicU64::new(0));

    let mut handles = vec![];

    for i in 0..100 {
        let storage = Arc::clone(&storage);
        let accepted = Arc::clone(&accepted);
        let degraded = Arc::clone(&degraded);

        handles.push(tokio::spawn(async move {
            let key = format!("degrade_key_{}", i);
            let value = format!("degrade_value_{}", i);

            match storage.set(&key, &value, Some(60)).await {
                Ok(_) => accepted.fetch_add(1, Ordering::SeqCst),
                Err(_) => degraded.fetch_add(1, Ordering::SeqCst),
            };
        }));
    }

    for handle in handles {
        handle.await.expect("Task should complete");
    }

    let total_accepted = accepted.load(Ordering::SeqCst);
    let total_degraded = degraded.load(Ordering::SeqCst);

    println!(
        "Graceful degradation test: {} accepted, {} degraded",
        total_accepted, total_degraded
    );

    // 应有一些请求被接受
    assert!(total_accepted > 0, "Some requests should be accepted");

    // 应有一些请求被降级
    assert!(total_degraded > 0, "Some requests should be degraded");

    // 接受数不应超过限制
    assert!(
        total_accepted <= 10,
        "Accepted count should not exceed limit"
    );
}

/// 测试连接池耗尽恢复
///
/// 验证系统在连接池耗尽后的恢复能力
#[tokio::test]
async fn test_connection_pool_exhaustion_recovery() {
    let storage = Arc::new(MockQuotaStorage::new());
    let phase1_success = Arc::new(AtomicU64::new(0));
    let phase2_success = Arc::new(AtomicU64::new(0));

    // 阶段1：大量并发请求
    {
        let mut handles = vec![];

        for i in 0..200 {
            let storage = Arc::clone(&storage);
            let phase1_success = Arc::clone(&phase1_success);

            handles.push(tokio::spawn(async move {
                let key = format!("phase1_key_{}", i);
                if storage.set(&key, "value", Some(60)).await.is_ok() {
                    phase1_success.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }

        for handle in handles {
            handle.await.expect("Task should complete");
        }
    }

    // 等待资源释放
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 阶段2：恢复后的请求
    {
        let mut handles = vec![];

        for i in 0..100 {
            let storage = Arc::clone(&storage);
            let phase2_success = Arc::clone(&phase2_success);

            handles.push(tokio::spawn(async move {
                let key = format!("phase2_key_{}", i);
                if storage.set(&key, "value", Some(60)).await.is_ok() {
                    phase2_success.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }

        for handle in handles {
            handle.await.expect("Task should complete");
        }
    }

    let p1 = phase1_success.load(Ordering::SeqCst);
    let p2 = phase2_success.load(Ordering::SeqCst);

    println!(
        "Connection pool recovery test: phase1={}, phase2={}",
        p1, p2
    );

    // 阶段2应有合理的成功率
    assert!(p2 > 50, "Phase 2 should have reasonable success rate");
}

// ============================================================================
// 综合资源测试
// ============================================================================

/// 测试资源使用的综合场景
///
/// 验证系统在综合资源压力下的行为
#[tokio::test]
async fn test_comprehensive_resource_usage() {
    let limiter = Arc::new(TokenBucketLimiter::new(50000, 5000));
    let storage = Arc::new(MockQuotaStorage::new());

    let operations = Arc::new(AtomicU64::new(0));
    let success = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));

    let duration = Duration::from_secs(5);
    let start = Instant::now();

    while start.elapsed() < duration {
        let limiter = Arc::clone(&limiter);
        let storage = Arc::clone(&storage);
        let operations = Arc::clone(&operations);
        let success = Arc::clone(&success);
        let errors = Arc::clone(&errors);

        let mut handles = vec![];

        for i in 0..50 {
            let limiter = Arc::clone(&limiter);
            let storage = Arc::clone(&storage);
            let operations = Arc::clone(&operations);
            let success = Arc::clone(&success);
            let errors = Arc::clone(&errors);

            handles.push(tokio::spawn(async move {
                operations.fetch_add(1, Ordering::SeqCst);

                // 限流检查
                let rate_ok = limiter.allow(1).await.unwrap_or(false);

                // 存储操作
                let key = format!("comp_key_{}", i);
                let storage_ok = storage.set(&key, "value", Some(60)).await.is_ok();

                if rate_ok && storage_ok {
                    success.fetch_add(1, Ordering::SeqCst);
                } else {
                    errors.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }

        for handle in handles {
            handle.await.expect("Task should complete");
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let total_ops = operations.load(Ordering::SeqCst);
    let total_success = success.load(Ordering::SeqCst);
    let total_errors = errors.load(Ordering::SeqCst);

    println!(
        "Comprehensive resource test: {} ops, {} success, {} errors",
        total_ops, total_success, total_errors
    );

    // 系统应保持稳定
    assert!(total_ops > 0, "Should have processed operations");

    // 成功率应合理
    let success_rate = total_success as f64 / total_ops as f64;
    assert!(
        success_rate > 0.3,
        "Success rate should be reasonable: {:.2}%",
        success_rate * 100.0
    );
}

/// 测试资源清理
///
/// 验证系统正确清理资源
#[tokio::test]
async fn test_resource_cleanup() {
    let storage = Arc::new(MockQuotaStorage::new());

    // 创建大量条目
    for i in 0..100 {
        let key = format!("cleanup_key_{}", i);
        let _ = storage.set(&key, "value", Some(1)).await; // 1秒 TTL
    }

    // 等待 TTL 过期
    tokio::time::sleep(Duration::from_millis(1100)).await;

    // 验证条目已过期
    let mut expired_count = 0;
    for i in 0..100 {
        let key = format!("cleanup_key_{}", i);
        if storage.get(&key).await.unwrap_or(None).is_none() {
            expired_count += 1;
        }
    }

    println!(
        "Resource cleanup test: {} out of 100 entries expired",
        expired_count
    );

    // 大多数条目应已过期
    assert!(expired_count > 50, "Most entries should be expired");
}
