//! 并发安全测试
//!
//! 测试覆盖：
//! - 竞争条件测试（限流器竞争条件、封禁状态竞争条件、配额消费竞争条件）
//! - 死锁测试（多锁场景死锁检测、超时恢复验证）

#[allow(unused_imports)]
use crate::common::{MockBanStorage, MockQuotaStorage, create_ban_record};
use limiteron::limiters::{
    FixedWindowLimiter, Limiter, ShardedSlidingWindowLimiter, TokenBucketLimiter,
};
use limiteron::{BanStorage, QuotaStorage};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::Barrier;
use tokio::time::timeout;

use limiteron::error::ConsumeResult;

use limiteron::BanTarget;
#[cfg(feature = "ban-manager")]
use limiteron::ban::BanManager;

// ============================================================================
// 限流器竞争条件测试
// ============================================================================

/// 测试令牌桶限流器的并发安全性
#[tokio::test]
async fn test_token_bucket_concurrent_safety() {
    let limiter = Arc::new(TokenBucketLimiter::new(1000, 100));
    let success_count = Arc::new(AtomicU64::new(0));
    let fail_count = Arc::new(AtomicU64::new(0));
    let barrier = Arc::new(Barrier::new(100));

    let mut handles = vec![];

    for _ in 0..100 {
        let limiter = Arc::clone(&limiter);
        let success_count = Arc::clone(&success_count);
        let fail_count = Arc::clone(&fail_count);
        let barrier = Arc::clone(&barrier);

        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            match limiter.allow(1).await {
                Ok(true) => success_count.fetch_add(1, Ordering::SeqCst),
                Ok(false) => fail_count.fetch_add(1, Ordering::SeqCst),
                Err(_) => fail_count.fetch_add(1, Ordering::SeqCst),
            }
        }));
    }

    for handle in handles {
        handle.await.expect("Task should complete");
    }

    let success = success_count.load(Ordering::SeqCst);
    let fail = fail_count.load(Ordering::SeqCst);

    assert_eq!(success + fail, 100);
    assert!(
        success <= 100,
        "Success count should not exceed capacity limit"
    );
    assert!(success > 0, "Some requests should succeed");
}

/// 测试滑动窗口限流器的并发安全性
#[tokio::test]
async fn test_sliding_window_concurrent_safety() {
    let limiter = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        100,
    ));
    let success_count = Arc::new(AtomicU64::new(0));
    let fail_count = Arc::new(AtomicU64::new(0));
    let barrier = Arc::new(Barrier::new(150));

    let mut handles = vec![];

    for _ in 0..150 {
        let limiter = Arc::clone(&limiter);
        let success_count = Arc::clone(&success_count);
        let fail_count = Arc::clone(&fail_count);
        let barrier = Arc::clone(&barrier);

        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            match limiter.allow(1).await {
                Ok(true) => success_count.fetch_add(1, Ordering::SeqCst),
                Ok(false) => fail_count.fetch_add(1, Ordering::SeqCst),
                Err(_) => fail_count.fetch_add(1, Ordering::SeqCst),
            }
        }));
    }

    for handle in handles {
        handle.await.expect("Task should complete");
    }

    let success = success_count.load(Ordering::SeqCst);
    let fail = fail_count.load(Ordering::SeqCst);

    assert_eq!(success + fail, 150);
    assert!(
        success <= 100,
        "Success count should not exceed window limit"
    );
}

/// 测试固定窗口限流器的并发安全性
#[tokio::test]
async fn test_fixed_window_concurrent_safety() {
    let limiter = Arc::new(FixedWindowLimiter::new(Duration::from_secs(1), 100));
    let success_count = Arc::new(AtomicU64::new(0));
    let fail_count = Arc::new(AtomicU64::new(0));
    let barrier = Arc::new(Barrier::new(150));

    let mut handles = vec![];

    for _ in 0..150 {
        let limiter = Arc::clone(&limiter);
        let success_count = Arc::clone(&success_count);
        let fail_count = Arc::clone(&fail_count);
        let barrier = Arc::clone(&barrier);

        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            match limiter.allow(1).await {
                Ok(true) => success_count.fetch_add(1, Ordering::SeqCst),
                Ok(false) => fail_count.fetch_add(1, Ordering::SeqCst),
                Err(_) => fail_count.fetch_add(1, Ordering::SeqCst),
            }
        }));
    }

    for handle in handles {
        handle.await.expect("Task should complete");
    }

    let success = success_count.load(Ordering::SeqCst);
    let fail = fail_count.load(Ordering::SeqCst);

    assert_eq!(success + fail, 150);
}

/// 测试配额消费的并发安全性
#[tokio::test]
async fn test_quota_concurrent_safety() {
    let storage = Arc::new(MockQuotaStorage::new());
    let consumed = Arc::new(AtomicU64::new(0));
    let barrier = Arc::new(Barrier::new(100));

    let mut handles = vec![];

    for _ in 0..100 {
        let storage = Arc::clone(&storage);
        let consumed = Arc::clone(&consumed);
        let barrier = Arc::clone(&barrier);

        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            let result: ConsumeResult = storage
                .consume("user_1", "resource_1", 1, 50, Duration::from_secs(60))
                .await
                .expect("Consume should succeed");

            if result.allowed {
                consumed.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    for handle in handles {
        handle.await.expect("Task should complete");
    }

    let total_consumed = consumed.load(Ordering::SeqCst);
    assert!(
        total_consumed <= 50,
        "Total consumed should not exceed limit"
    );
}

// ============================================================================
// 死锁测试
// ============================================================================

/// 测试多锁场景下的死锁检测
#[tokio::test]
async fn test_no_deadlock_in_multi_lock_scenario() {
    let quota_storage = Arc::new(MockQuotaStorage::new());
    let ban_storage = Arc::new(MockBanStorage::new());
    let counter = Arc::new(AtomicU64::new(0));
    let barrier = Arc::new(Barrier::new(20));

    let mut handles = vec![];

    for _ in 0..20 {
        let quota_storage = Arc::clone(&quota_storage);
        let ban_storage = Arc::clone(&ban_storage);
        let counter = Arc::clone(&counter);
        let barrier = Arc::clone(&barrier);

        handles.push(tokio::spawn(async move {
            barrier.wait().await;

            let _ = quota_storage
                .consume("user_1", "resource_1", 1, 100, Duration::from_secs(60))
                .await;

            let _ = ban_storage
                .is_banned(&BanTarget::UserId("user_1".to_string()))
                .await;

            counter.fetch_add(1, Ordering::SeqCst);
        }));
    }

    let result = timeout(Duration::from_secs(10), async {
        for handle in handles {
            handle.await.expect("Task should complete without deadlock");
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "Operations should complete without deadlock"
    );

    let final_count = counter.load(Ordering::SeqCst);
    assert_eq!(final_count, 20, "All operations should complete");
}

/// 测试锁获取超时恢复
#[tokio::test]
async fn test_lock_timeout_recovery() {
    let storage = Arc::new(MockQuotaStorage::new());
    let barrier = Arc::new(Barrier::new(100));

    let mut handles = vec![];

    for _ in 0..100 {
        let storage = Arc::clone(&storage);
        let barrier = Arc::clone(&barrier);

        handles.push(tokio::spawn(async move {
            barrier.wait().await;

            let result = timeout(
                Duration::from_millis(100),
                storage.consume("user_1", "resource_1", 1, 1000, Duration::from_secs(60)),
            )
            .await;

            result.is_ok()
        }));
    }

    let mut success = 0;
    let mut _timeout_count = 0;

    for handle in handles {
        match handle.await {
            Ok(true) => success += 1,
            Ok(false) => _timeout_count += 1,
            Err(_) => _timeout_count += 1,
        }
    }

    assert!(success > 50, "Most operations should succeed");
}

// ============================================================================
// 封禁状态竞争条件测试
// ============================================================================

/// 测试封禁管理器的并发封禁操作
#[cfg(feature = "ban-manager")]
#[tokio::test]
async fn test_ban_manager_concurrent_safety() {
    let storage = Arc::new(MockBanStorage::new());
    let ban_manager = BanManager::builder()
        .with_storage(storage)
        .build()
        .await
        .expect("Failed to create ban manager");

    let target = BanTarget::Ip("192.168.1.1".to_string());
    let barrier = Arc::new(Barrier::new(50));

    let mut handles = vec![];

    // 并发封禁操作
    for i in 0..25 {
        let ban_manager = ban_manager.clone();
        let target = target.clone();
        let barrier = Arc::clone(&barrier);

        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            let _ = ban_manager
                .add_ban(create_ban_record(
                    target.clone(),
                    60,
                    &format!("Ban reason {}", i),
                ))
                .await;
        }));
    }

    // 并发解封操作
    for _ in 0..25 {
        let ban_manager = ban_manager.clone();
        let target = target.clone();
        let barrier = Arc::clone(&barrier);

        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            let _ = ban_manager.delete_ban(&target, "admin".to_string()).await;
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }

    let result = ban_manager.is_banned(&target).await;
    assert!(result.is_ok(), "Ban check should succeed");
}
