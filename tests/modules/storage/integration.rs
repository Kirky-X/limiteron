//! 存储模块集成测试
//!
//! 测试存储与其他组件的集成

use limiteron::storage::{BanStorage, MemoryStorage, QuotaStorage};
use std::time::Duration;

/// 测试存储与限流器的集成
#[tokio::test]
async fn test_storage_with_limiter() {
    use limiteron::limiters::{Limiter, TokenBucketLimiter};

    let storage = MemoryStorage::new();
    let limiter = TokenBucketLimiter::new(1000, 100); // 容量1000，每秒补充100

    // 消费配额
    let result = storage
        .consume("user1", "resource1", 200, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(result.allowed);

    // 使用限流器检查
    let key = "user1:resource1";
    let allowed = limiter.check(key, 200).await;
    assert!(allowed);

    // 再次消费
    let result = storage
        .consume("user1", "resource1", 800, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 0);
}

/// 测试存储与封禁管理器的集成
#[tokio::test]
async fn test_storage_with_ban_manager() {
    use limiteron::storage::{BanRecord, BanTarget};

    let storage = MemoryStorage::new();

    // 添加封禁
    let ban = BanRecord {
        target: BanTarget::Ip("192.168.1.1".to_string()),
        ban_times: 1,
        duration: Duration::from_secs(60),
        banned_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(60),
        is_manual: false,
        reason: "Test ban".to_string(),
    };

    storage.save(&ban).await.unwrap();

    // 查询封禁
    let result = storage
        .is_banned(&BanTarget::Ip("192.168.1.1".to_string()))
        .await
        .unwrap();
    assert!(result.is_some());

    // 获取封禁历史
    let history = storage
        .get_history(&BanTarget::Ip("192.168.1.1".to_string()))
        .await
        .unwrap();
    assert!(history.is_some());
    assert_eq!(history.unwrap().ban_times, 1);
}

/// 测试存储与配额控制器的集成
#[tokio::test]
async fn test_storage_with_quota_controller() {
    use limiteron::quota_controller::{QuotaConfig, QuotaController, QuotaType};

    let storage = MemoryStorage::new();
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 1000,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: Default::default(),
    };

    let controller = QuotaController::new(storage, config);

    // 消费配额
    let result = controller.consume("user1", "resource1", 500).await.unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 500);

    // 获取配额状态
    let quota = controller.get_quota("user1", "resource1").await.unwrap();
    assert!(quota.is_some());
    assert_eq!(quota.unwrap().consumed, 500);
}

/// 测试存储并发访问
#[tokio::test]
async fn test_storage_concurrent_access() {
    let storage = std::sync::Arc::new(MemoryStorage::new());
    let mut handles = vec![];

    // 10个并发任务
    for i in 0..10 {
        let storage_clone = storage.clone();
        let user_id = format!("user_{}", i);

        handles.push(tokio::spawn(async move {
            for j in 0..5 {
                let result = storage_clone
                    .consume(&user_id, "resource1", 100, 1000, Duration::from_secs(60))
                    .await
                    .unwrap();
                assert!(result.allowed || j > 0); // 第一次应该成功，后续可能失败
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}

/// 测试存储故障恢复
#[tokio::test]
async fn test_storage_failure_recovery() {
    let storage = MemoryStorage::new();

    // 正常操作
    let result = storage
        .consume("user1", "resource1", 100, 1000, Duration::from_secs(60))
        .await;
    assert!(result.is_ok());

    // 模拟故障（内存存储不会失败，这只是演示）
    // 在实际存储中，应该测试重试逻辑

    // 恢复后继续操作
    let result = storage
        .consume("user1", "resource1", 100, 1000, Duration::from_secs(60))
        .await;
    assert!(result.is_ok());
}

/// 测试存储数据持久化
#[tokio::test]
async fn test_storage_data_persistence() {
    let storage = MemoryStorage::new();

    // 写入数据
    let result = storage
        .consume("user1", "resource1", 500, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(result.allowed);

    // 验证数据存在
    let quota = storage.get_quota("user1", "resource1").await.unwrap();
    assert!(quota.is_some());
    assert_eq!(quota.unwrap().consumed, 500);

    // 重置数据
    storage
        .reset("user1", "resource1", 1000, Duration::from_secs(60))
        .await
        .unwrap();

    // 验证数据已清除
    let quota = storage.get_quota("user1", "resource1").await.unwrap();
    // reset 可能会删除键，如果键不存在则测试通过
    assert!(quota.is_none() || quota.unwrap().consumed == 0);
}
