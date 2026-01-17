//! 限流器模块集成测试
//!
//! 测试限流器与其他组件的集成

use limiteron::limiters::{
    ConcurrencyLimiter, FixedWindowLimiter, Limiter, SlidingWindowLimiter, TokenBucketLimiter,
};
use limiteron::storage::{MemoryStorage, QuotaStorage};
use std::sync::Arc;
use std::time::Duration;

/// 测试令牌桶限流器与存储的集成
#[tokio::test]
async fn test_token_bucket_with_storage() {
    let limiter = TokenBucketLimiter::new(1000, 100); // 容量1000，每秒补充100
    let storage = MemoryStorage::new();

    // 消费配额
    let result = storage
        .consume("user1", "resource1", 200, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(result.allowed);

    // 使用限流器检查
    let key = "user1:resource1";
    let allowed = limiter.allow(200).await;
    assert!(allowed);
}

/// 测试滑动窗口限流器与存储的集成
#[tokio::test]
async fn test_sliding_window_with_storage() {
    let limiter = SlidingWindowLimiter::new(Duration::from_secs(1), 100);
    let storage = MemoryStorage::new();

    // 消费配额
    let result = storage
        .consume("user1", "resource1", 50, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(result.allowed);

    // 使用限流器检查
    for _ in 0..50 {
        let allowed = limiter.allow(1).await;
        assert!(allowed);
    }
}

/// 测试固定窗口限流器与存储的集成
#[tokio::test]
async fn test_fixed_window_with_storage() {
    let limiter = FixedWindowLimiter::new(Duration::from_secs(1), 100);
    let storage = MemoryStorage::new();

    // 消费配额
    let result = storage
        .consume("user1", "resource1", 50, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(result.allowed);

    // 使用限流器检查
    for _ in 0..50 {
        let allowed = limiter.allow(1).await;
        assert!(allowed);
    }
}

/// 测试并发控制器与存储的集成
#[tokio::test]
async fn test_concurrency_limiter_with_storage() {
    let limiter = ConcurrencyLimiter::new(10);
    let storage = MemoryStorage::new();

    // 消费配额
    let result = storage
        .consume("user1", "resource1", 5, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(result.allowed);

    // 使用并发控制器检查
    let allowed = limiter.allow(5).await;
    assert!(allowed);
}

/// 测试限流器与配额控制器的集成
#[tokio::test]
async fn test_limiter_with_quota_controller() {
    use limiteron::quota_controller::{QuotaConfig, QuotaController, QuotaType};

    let storage = MemoryStorage::new();
    let limiter = TokenBucketLimiter::new(1000, 100);
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 1000,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: Default::default(),
    };

    let controller = QuotaController::new(storage, config);

    // 先检查配额
    let quota_result = controller.consume("user1", "resource1", 500).await.unwrap();
    assert!(quota_result.allowed);

    // 再使用限流器检查
    let allowed = limiter.allow(500).await;
    assert!(allowed);
}

/// 测试限流器并发场景
#[tokio::test]
async fn test_limiter_concurrent_scenario() {
    let limiter = Arc::new(TokenBucketLimiter::new(1000, 100));
    let storage = Arc::new(MemoryStorage::new());
    let mut handles = vec![];

    // 10个并发任务
    for i in 0..10 {
        let limiter_clone = limiter.clone();
        let storage_clone = storage.clone();
        let user_id = format!("user_{}", i);

        handles.push(tokio::spawn(async move {
            // 使用限流器检查
            let allowed = limiter_clone.allow(50).await;
            assert!(allowed);

            // 消费配额
            let result = storage_clone
                .consume(&user_id, "resource1", 50, 1000, Duration::from_secs(60))
                .await
                .unwrap();
            assert!(result.allowed);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}

/// 测试限流器窗口切换
#[tokio::test]
async fn test_limiter_window_transition() {
    let limiter = SlidingWindowLimiter::new(Duration::from_millis(100), 10);
    let storage = MemoryStorage::new();

    // 第一个窗口
    for _ in 0..10 {
        let allowed = limiter.allow(1).await;
        assert!(allowed);
    }

    // 应该被拒绝
    let allowed = limiter.allow(1).await;
    assert!(!allowed);

    // 等待窗口切换
    tokio::time::sleep(Duration::from_millis(101)).await;

    // 新窗口应该允许
    let allowed = limiter.allow(1).await;
    assert!(allowed);

    // 消费配额
    let result = storage
        .consume("user1", "resource1", 1, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(result.allowed);
}

/// 测试限流器与存储的一致性
#[tokio::test]
async fn test_limiter_storage_consistency() {
    let limiter = TokenBucketLimiter::new(1000, 100);
    let storage = MemoryStorage::new();
    let user_id = "user1";
    let resource = "resource1";

    // 消费配额
    let result = storage
        .consume(user_id, resource, 300, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 700);

    // 使用限流器检查
    let allowed = limiter.allow(300).await;
    assert!(allowed);

    // 获取配额状态
    let quota = storage.get_quota(user_id, resource).await.unwrap();
    assert!(quota.is_some());
    assert_eq!(quota.unwrap().consumed, 300);
}

/// 测试限流器与存储的重置一致性
#[tokio::test]
async fn test_limiter_storage_reset_consistency() {
    let limiter = FixedWindowLimiter::new(Duration::from_secs(1), 100);
    let storage = MemoryStorage::new();
    let user_id = "user1";
    let resource = "resource1";

    // 消费配额
    let result = storage
        .consume(user_id, resource, 500, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(result.allowed);

    // 重置配额
    storage
        .reset(user_id, resource, 1000, Duration::from_secs(60))
        .await
        .unwrap();

    // 验证配额已重置
    let quota = storage.get_quota(user_id, resource).await.unwrap();
    // reset 可能会删除键，如果键不存在则测试通过
    assert!(quota.is_none() || quota.unwrap().consumed == 0);

    // 新窗口应该允许
    tokio::time::sleep(Duration::from_millis(1001)).await;
    let allowed = limiter.allow(1).await;
    assert!(allowed);
}

/// 测试限流器与存储的并发一致性
#[tokio::test]
async fn test_limiter_storage_concurrent_consistency() {
    let limiter = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(1), 100));
    let storage = Arc::new(MemoryStorage::new());
    let user_id = "user1";
    let resource = "resource1";
    let mut handles = vec![];

    // 20个并发任务
    for i in 0..20 {
        let limiter_clone = limiter.clone();
        let storage_clone = storage.clone();

        handles.push(tokio::spawn(async move {
            // 使用限流器检查
            let allowed = limiter_clone.allow(10).await;
            if allowed {
                // 消费配额
                let result = storage_clone
                    .consume(user_id, resource, 10, 1000, Duration::from_secs(60))
                    .await
                    .unwrap();
                assert!(result.allowed);
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // 验证最终的配额状态
    let quota = storage.get_quota(user_id, resource).await.unwrap();
    assert!(quota.is_some());
    // 应该消费了不超过 100 的配额
    assert!(quota.unwrap().consumed <= 100);
}
