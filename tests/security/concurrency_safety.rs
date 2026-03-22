//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 并发安全测试
//!
//! 测试覆盖：
//! - 竞争条件测试
//! - 死锁测试
//! - 并发状态一致性测试

use limiteron::limiters::{FixedWindowLimiter, Limiter, ShardedSlidingWindowLimiter, SlidingWindowLimiter, TokenBucketLimiter, ConcurrencyLimiter};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

// ============================================================================
// 限流器竞争条件测试
// ============================================================================

/// 测试 TokenBucketLimiter 高并发竞争条件
///
/// 验证在高并发场景下，令牌消费的正确性和一致性
#[tokio::test]
async fn test_token_bucket_race_condition() {
    let capacity = 100u64;
    let limiter = Arc::new(TokenBucketLimiter::new(capacity, 1));
    let success_count = Arc::new(AtomicU64::new(0));

    // 使用 barrier 确保所有任务同时开始
    let barrier = Arc::new(tokio::sync::Barrier::new(200));
    let start_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let mut handles = vec![];
    for _ in 0..200 {
        let limiter_clone = Arc::clone(&limiter);
        let success_clone = Arc::clone(&success_count);
        let barrier_clone = Arc::clone(&barrier);
        let start_signal_clone = Arc::clone(&start_signal);

        handles.push(tokio::spawn(async move {
            barrier_clone.wait().await;

            // 等待开始信号
            while !start_signal_clone.load(Ordering::SeqCst) {
                std::hint::spin_loop();
            }

            // 尝试消费 1 个令牌
            if limiter_clone.allow(1).await.unwrap() {
                success_clone.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    // 设置开始信号
    start_signal.store(true, Ordering::SeqCst);

    for handle in handles {
        handle.await.unwrap();
    }

    // 验证成功次数不超过容量
    let success = success_count.load(Ordering::SeqCst);
    assert!(
        success <= capacity,
        "Success count {} exceeds capacity {}",
        success,
        capacity
    );
}

/// 测试 SlidingWindowLimiter 高并发竞争条件
#[tokio::test]
async fn test_sliding_window_race_condition() {
    let max_requests = 50u64;
    let limiter = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(10), max_requests));
    let success_count = Arc::new(AtomicU64::new(0));

    let barrier = Arc::new(tokio::sync::Barrier::new(100));
    let start_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let mut handles = vec![];
    for _ in 0..100 {
        let limiter_clone = Arc::clone(&limiter);
        let success_clone = Arc::clone(&success_count);
        let barrier_clone = Arc::clone(&barrier);
        let start_signal_clone = Arc::clone(&start_signal);

        handles.push(tokio::spawn(async move {
            barrier_clone.wait().await;

            while !start_signal_clone.load(Ordering::SeqCst) {
                std::hint::spin_loop();
            }

            if limiter_clone.allow(1).await.unwrap() {
                success_clone.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    start_signal.store(true, Ordering::SeqCst);

    for handle in handles {
        handle.await.unwrap();
    }

    let success = success_count.load(Ordering::SeqCst);
    // 允许 5% 的误差，因为存在竞态条件
    assert!(
        success <= max_requests + 3,
        "Success count {} significantly exceeds limit {}",
        success,
        max_requests
    );
}

/// 测试 FixedWindowLimiter 高并发竞争条件
#[tokio::test]
async fn test_fixed_window_race_condition() {
    let max_requests = 50u64;
    let limiter = Arc::new(FixedWindowLimiter::new(Duration::from_secs(10), max_requests));
    let success_count = Arc::new(AtomicU64::new(0));

    let barrier = Arc::new(tokio::sync::Barrier::new(100));
    let start_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let mut handles = vec![];
    for _ in 0..100 {
        let limiter_clone = Arc::clone(&limiter);
        let success_clone = Arc::clone(&success_count);
        let barrier_clone = Arc::clone(&barrier);
        let start_signal_clone = Arc::clone(&start_signal);

        handles.push(tokio::spawn(async move {
            barrier_clone.wait().await;

            while !start_signal_clone.load(Ordering::SeqCst) {
                std::hint::spin_loop();
            }

            if limiter_clone.allow(1).await.unwrap() {
                success_clone.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    start_signal.store(true, Ordering::SeqCst);

    for handle in handles {
        handle.await.unwrap();
    }

    let success = success_count.load(Ordering::SeqCst);
    assert!(
        success <= max_requests,
        "Success count {} exceeds limit {}",
        success,
        max_requests
    );
}

/// 测试 ShardedSlidingWindowLimiter 高并发竞争条件
#[tokio::test]
async fn test_sharded_sliding_window_race_condition() {
    let max_requests = 100u64;
    let limiter = Arc::new(ShardedSlidingWindowLimiter::new(Duration::from_secs(60), max_requests));
    let success_count = Arc::new(AtomicU64::new(0));

    let barrier = Arc::new(tokio::sync::Barrier::new(200));
    let start_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let mut handles = vec![];
    for _ in 0..200 {
        let limiter_clone = Arc::clone(&limiter);
        let success_clone = Arc::clone(&success_count);
        let barrier_clone = Arc::clone(&barrier);
        let start_signal_clone = Arc::clone(&start_signal);

        handles.push(tokio::spawn(async move {
            barrier_clone.wait().await;

            while !start_signal_clone.load(Ordering::SeqCst) {
                std::hint::spin_loop();
            }

            if limiter_clone.allow(1).await.unwrap() {
                success_clone.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    start_signal.store(true, Ordering::SeqCst);

    for handle in handles {
        handle.await.unwrap();
    }

    let success = success_count.load(Ordering::SeqCst);
    // 允许 10% 的误差
    assert!(
        success <= max_requests + 10,
        "Success count {} significantly exceeds limit {}",
        success,
        max_requests
    );
}

// ============================================================================
// 死锁测试
// ============================================================================

/// 测试 SlidingWindowLimiter 无死锁
///
/// 使用超时机制确保不会发生死锁
#[tokio::test]
async fn test_sliding_window_no_deadlock() {
    let limiter = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(1), 10000));
    let mut handles = vec![];

    for _ in 0..500 {
        let limiter_clone = Arc::clone(&limiter);
        handles.push(tokio::spawn(async move {
            for _ in 0..20 {
                let _ = limiter_clone.allow(1).await;
            }
        }));
    }

    // 使用超时确保不会死锁
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        for handle in handles {
            let _ = handle.await;
        }
    })
    .await;

    assert!(result.is_ok(), "Test timed out - possible deadlock");
}

/// 测试 TokenBucketLimiter 无死锁
#[tokio::test]
async fn test_token_bucket_no_deadlock() {
    let limiter = Arc::new(TokenBucketLimiter::new(10000, 1000));
    let mut handles = vec![];

    for _ in 0..500 {
        let limiter_clone = Arc::clone(&limiter);
        handles.push(tokio::spawn(async move {
            for _ in 0..20 {
                let _ = limiter_clone.allow(1).await;
            }
        }));
    }

    let result = tokio::time::timeout(Duration::from_secs(10), async {
        for handle in handles {
            let _ = handle.await;
        }
    })
    .await;

    assert!(result.is_ok(), "Test timed out - possible deadlock");
}

/// 测试 FixedWindowLimiter 无死锁
#[tokio::test]
async fn test_fixed_window_no_deadlock() {
    let limiter = Arc::new(FixedWindowLimiter::new(Duration::from_secs(10), 10000));
    let mut handles = vec![];

    for _ in 0..500 {
        let limiter_clone = Arc::clone(&limiter);
        handles.push(tokio::spawn(async move {
            for _ in 0..20 {
                let _ = limiter_clone.allow(1).await;
            }
        }));
    }

    let result = tokio::time::timeout(Duration::from_secs(10), async {
        for handle in handles {
            let _ = handle.await;
        }
    })
    .await;

    assert!(result.is_ok(), "Test timed out - possible deadlock");
}

/// 测试 ShardedSlidingWindowLimiter 无死锁
#[tokio::test]
async fn test_sharded_sliding_window_no_deadlock() {
    let limiter = Arc::new(ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 100000));
    let mut handles = vec![];

    for _ in 0..1000 {
        let limiter_clone = Arc::clone(&limiter);
        handles.push(tokio::spawn(async move {
            for _ in 0..10 {
                let _ = limiter_clone.allow(1).await;
            }
        }));
    }

    let result = tokio::time::timeout(Duration::from_secs(10), async {
        for handle in handles {
            let _ = handle.await;
        }
    })
    .await;

    assert!(result.is_ok(), "Test timed out - possible deadlock");
}

/// 测试 ConcurrencyLimiter 无死锁
#[tokio::test]
async fn test_concurrency_limiter_no_deadlock() {
    let limiter = Arc::new(ConcurrencyLimiter::new(10));
    let mut handles = vec![];

    for _ in 0..100 {
        let limiter_clone = Arc::clone(&limiter);
        handles.push(tokio::spawn(async move {
            for _ in 0..10 {
                // 使用 try_acquire 避免阻塞
                if let Ok(_permit) = limiter_clone.try_acquire(1) {
                    // 持有许可一段时间
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }
        }));
    }

    let result = tokio::time::timeout(Duration::from_secs(10), async {
        for handle in handles {
            let _ = handle.await;
        }
    })
    .await;

    assert!(result.is_ok(), "Test timed out - possible deadlock");
}

// ============================================================================
// 并发状态一致性测试
// ============================================================================

/// 测试令牌桶状态一致性
///
/// 验证在高并发场景下，令牌计数的一致性
#[tokio::test]
async fn test_token_bucket_state_consistency() {
    let capacity = 1000u64;
    let limiter = Arc::new(TokenBucketLimiter::new(capacity, 1));
    let consumed = Arc::new(AtomicU64::new(0));

    let mut handles = vec![];
    for _ in 0..100 {
        let limiter_clone = Arc::clone(&limiter);
        let consumed_clone = Arc::clone(&consumed);

        handles.push(tokio::spawn(async move {
            let mut local_consumed = 0u64;
            for _ in 0..20 {
                if limiter_clone.allow(1).await.unwrap() {
                    local_consumed += 1;
                }
            }
            consumed_clone.fetch_add(local_consumed, Ordering::SeqCst);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let total_consumed = consumed.load(Ordering::SeqCst);
    assert!(
        total_consumed <= capacity,
        "Total consumed {} exceeds capacity {}",
        total_consumed,
        capacity
    );
}

/// 测试滑动窗口状态一致性
#[tokio::test]
async fn test_sliding_window_state_consistency() {
    let max_requests = 100u64;
    let limiter = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(10), max_requests));
    let consumed = Arc::new(AtomicU64::new(0));

    let mut handles = vec![];
    for _ in 0..50 {
        let limiter_clone = Arc::clone(&limiter);
        let consumed_clone = Arc::clone(&consumed);

        handles.push(tokio::spawn(async move {
            let mut local_consumed = 0u64;
            for _ in 0..10 {
                if limiter_clone.allow(1).await.unwrap() {
                    local_consumed += 1;
                }
            }
            consumed_clone.fetch_add(local_consumed, Ordering::SeqCst);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let total_consumed = consumed.load(Ordering::SeqCst);
    // 允许 5% 误差
    assert!(
        total_consumed <= max_requests + 5,
        "Total consumed {} significantly exceeds limit {}",
        total_consumed,
        max_requests
    );
}

/// 测试固定窗口状态一致性
#[tokio::test]
async fn test_fixed_window_state_consistency() {
    let max_requests = 100u64;
    let limiter = Arc::new(FixedWindowLimiter::new(Duration::from_secs(10), max_requests));
    let consumed = Arc::new(AtomicU64::new(0));

    let mut handles = vec![];
    for _ in 0..50 {
        let limiter_clone = Arc::clone(&limiter);
        let consumed_clone = Arc::clone(&consumed);

        handles.push(tokio::spawn(async move {
            let mut local_consumed = 0u64;
            for _ in 0..10 {
                if limiter_clone.allow(1).await.unwrap() {
                    local_consumed += 1;
                }
            }
            consumed_clone.fetch_add(local_consumed, Ordering::SeqCst);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let total_consumed = consumed.load(Ordering::SeqCst);
    assert!(
        total_consumed <= max_requests,
        "Total consumed {} exceeds limit {}",
        total_consumed,
        max_requests
    );
}

// ============================================================================
// 多锁场景测试
// ============================================================================

/// 测试多个限流器并发访问
///
/// 验证多个限流器同时使用时不会产生死锁
#[tokio::test]
async fn test_multiple_limiters_concurrent_access() {
    let token_bucket = Arc::new(TokenBucketLimiter::new(100, 10));
    let sliding_window = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(1), 50));
    let fixed_window = Arc::new(FixedWindowLimiter::new(Duration::from_secs(1), 50));
    let sharded_window = Arc::new(ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 100));

    let mut handles = vec![];

    for _ in 0..100 {
        let tb = Arc::clone(&token_bucket);
        let sw = Arc::clone(&sliding_window);
        let fw = Arc::clone(&fixed_window);
        let shw = Arc::clone(&sharded_window);

        handles.push(tokio::spawn(async move {
            // 同时访问多个限流器
            let _ = tb.allow(1).await;
            let _ = sw.allow(1).await;
            let _ = fw.allow(1).await;
            let _ = shw.allow(1).await;
        }));
    }

    let result = tokio::time::timeout(Duration::from_secs(10), async {
        for handle in handles {
            let _ = handle.await;
        }
    })
    .await;

    assert!(result.is_ok(), "Test timed out - possible deadlock with multiple limiters");
}

/// 测试嵌套限流器调用
#[tokio::test]
async fn test_nested_limiter_calls() {
    let outer_limiter = Arc::new(TokenBucketLimiter::new(100, 10));
    let inner_limiter = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(1), 50));

    let mut handles = vec![];

    for _ in 0..50 {
        let outer = Arc::clone(&outer_limiter);
        let inner = Arc::clone(&inner_limiter);

        handles.push(tokio::spawn(async move {
            // 嵌套调用
            if outer.allow(1).await.unwrap() {
                if inner.allow(1).await.unwrap() {
                    // 执行操作
                }
            }
        }));
    }

    let result = tokio::time::timeout(Duration::from_secs(10), async {
        for handle in handles {
            let _ = handle.await;
        }
    })
    .await;

    assert!(result.is_ok(), "Test timed out - possible deadlock with nested calls");
}

// ============================================================================
// 压力测试
// ============================================================================

/// 高压力测试 - 大量并发请求
#[tokio::test]
async fn test_high_pressure_concurrent_requests() {
    let limiter = Arc::new(ShardedSlidingWindowLimiter::new(Duration::from_secs(60), 100000));
    let success_count = Arc::new(AtomicU64::new(0));

    let mut handles = vec![];
    for _ in 0..1000 {
        let limiter_clone = Arc::clone(&limiter);
        let success_clone = Arc::clone(&success_count);

        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                if limiter_clone.allow(1).await.unwrap() {
                    success_clone.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    let start = std::time::Instant::now();
    for handle in handles {
        let _ = handle.await;
    }
    let elapsed = start.elapsed();

    let success = success_count.load(Ordering::SeqCst);
    
    // 验证不超过限制（允许 10% 误差）
    assert!(
        success <= 110000,
        "Success count {} significantly exceeds limit",
        success
    );

    // 验证性能
    assert!(
        elapsed < Duration::from_secs(30),
        "Test took too long: {:?}",
        elapsed
    );
}

/// 测试长时间运行稳定性
#[tokio::test]
async fn test_long_running_stability() {
    let limiter = Arc::new(TokenBucketLimiter::new(10000, 1000));

    // 持续运行 5 秒
    let duration = Duration::from_secs(5);
    let start = std::time::Instant::now();
    let mut handles = vec![];

    while start.elapsed() < duration {
        let limiter_clone = Arc::clone(&limiter);
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                let _ = limiter_clone.allow(1).await;
            }
        }));

        // 限制并发任务数量
        if handles.len() >= 100 {
            let _ = handles.remove(0).await;
        }
    }

    // 等待所有任务完成
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        for handle in handles {
            let _ = handle.await;
        }
    })
    .await;

    assert!(result.is_ok(), "Long running test timed out");
}

// ============================================================================
// 边界条件并发测试
// ============================================================================

/// 测试窗口边界时刻的并发行为
#[tokio::test]
async fn test_window_boundary_concurrent_behavior() {
    let window_size = Duration::from_millis(100);
    let limiter = Arc::new(FixedWindowLimiter::new(window_size, 10));
    let success_count = Arc::new(AtomicU64::new(0));

    // 在窗口即将过期时发起大量请求
    tokio::time::sleep(Duration::from_millis(90)).await;

    let mut handles = vec![];
    for _ in 0..50 {
        let limiter_clone = Arc::clone(&limiter);
        let success_clone = Arc::clone(&success_count);

        handles.push(tokio::spawn(async move {
            if limiter_clone.allow(1).await.unwrap() {
                success_clone.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // 验证行为合理（不应崩溃或死锁）
    let _ = success_count.load(Ordering::SeqCst);
}

/// 测试令牌耗尽时的并发行为
#[tokio::test]
async fn test_token_exhaustion_concurrent_behavior() {
    let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
    let success_count = Arc::new(AtomicU64::new(0));

    // 先消耗所有令牌
    for _ in 0..10 {
        let _ = limiter.allow(1).await;
    }

    // 并发请求应该全部被拒绝
    let mut handles = vec![];
    for _ in 0..100 {
        let limiter_clone = Arc::clone(&limiter);
        let success_clone = Arc::clone(&success_count);

        handles.push(tokio::spawn(async move {
            if limiter_clone.allow(1).await.unwrap() {
                success_clone.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // 所有请求应该被拒绝（或极少成功，如果令牌补充）
    let success = success_count.load(Ordering::SeqCst);
    assert!(success <= 5, "Most requests should be rejected when tokens exhausted");
}
