//! 内存存储单元测试
//!
//! 测试内存存储的基础功能

use limiteron::storage::{BanStorage, QuotaStorage};
use std::time::Duration;

#[tokio::test]
async fn test_memory_quota_storage() {
    use limiteron::storage::MemoryStorage;

    let storage = MemoryStorage::new();

    // 消费配额
    let result = storage
        .consume("user1", "resource1", 100, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 900);

    // 再次消费
    let result = storage
        .consume("user1", "resource1", 500, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 400);

    // 超过限制
    let result = storage
        .consume("user1", "resource1", 500, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(!result.allowed);
}

#[tokio::test]
async fn test_memory_ban_storage() {
    use limiteron::storage::{BanRecord, BanTarget, MemoryStorage};

    let storage = MemoryStorage::new();

    // 添加封禁
    let ban = BanRecord {
        target: BanTarget::Ip("192.168.1.1".to_string()),
        ban_times: 1,
        duration: Duration::from_secs(60),
        banned_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(60),
        is_manual: false,
        reason: "Test".to_string(),
    };

    storage.save(&ban).await.unwrap();

    // 查询封禁
    let result = storage
        .is_banned(&BanTarget::Ip("192.168.1.1".to_string()))
        .await
        .unwrap();
    assert!(result.is_some());

    // 移除封禁
    storage
        .remove_ban(&BanTarget::Ip("192.168.1.1".to_string()))
        .await
        .unwrap();

    // 再次查询
    let result = storage
        .is_banned(&BanTarget::Ip("192.168.1.1".to_string()))
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_memory_storage_reset() {
    use limiteron::storage::{MemoryStorage, QuotaStorage};

    let storage = MemoryStorage::new();

    // 消费配额
    let result = storage
        .consume("user1", "resource1", 500, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 500);

    // 重置配额
    storage
        .reset("user1", "resource1", 1000, Duration::from_secs(60))
        .await
        .unwrap();

    // 验证已重置
    let result = storage
        .consume("user1", "resource1", 1000, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 0);
}

#[tokio::test]
async fn test_memory_storage_get_quota() {
    use limiteron::storage::{MemoryStorage, QuotaStorage};

    let storage = MemoryStorage::new();

    // 消费配额
    let _ = storage
        .consume("user1", "resource1", 300, 1000, Duration::from_secs(60))
        .await
        .unwrap();

    // 获取配额
    let quota = storage.get_quota("user1", "resource1").await.unwrap();
    assert!(quota.is_some());
    assert_eq!(quota.unwrap().consumed, 300);

    // 获取不存在的配额
    let quota = storage.get_quota("user2", "resource2").await.unwrap();
    assert!(quota.is_none());
}
