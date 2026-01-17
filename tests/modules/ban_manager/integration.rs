//! 封禁管理模块集成测试
//!
//! 测试封禁管理器与其他组件的集成

use limiteron::ban_manager::{BackoffConfig, BanManager, BanManagerConfig};
use limiteron::storage::{BanRecord, BanTarget, MemoryStorage};
use std::sync::Arc;
use std::time::Duration;

/// 测试封禁管理器与存储的集成
#[tokio::test]
async fn test_ban_manager_with_storage() {
    let storage = Arc::new(MemoryStorage::new());
    let config = BanManagerConfig {
        backoff: BackoffConfig {
            first_duration: 5,
            second_duration: 10,
            third_duration: 20,
            fourth_duration: 40,
            max_duration: 60,
        },
        enable_auto_unban: true,
        auto_unban_interval: 5,
    };

    let ban_manager = BanManager::new(storage, Some(config)).await.unwrap();

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

    ban_manager.add_ban(ban).await.unwrap();

    // 查询封禁
    let is_banned = ban_manager
        .is_banned(&BanTarget::Ip("192.168.1.1".to_string()))
        .await
        .unwrap()
        .is_some();
    assert!(is_banned);
}

/// 测试封禁管理器与限流器的集成
#[tokio::test]
async fn test_ban_manager_with_limiter() {
    use limiteron::limiters::TokenBucketLimiter;

    let storage = Arc::new(MemoryStorage::new());
    let config = BanManagerConfig::default();

    let ban_manager = BanManager::new(storage.clone(), Some(config))
        .await
        .unwrap();
    let limiter = TokenBucketLimiter::new(100, 10);

    // 添加封禁
    let ban = BanRecord {
        target: BanTarget::UserId("user1".to_string()),
        ban_times: 1,
        duration: Duration::from_secs(60),
        banned_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(60),
        is_manual: false,
        reason: "Rate limit exceeded".to_string(),
    };

    ban_manager.add_ban(ban).await.unwrap();

    // 检查限流器
    let allowed = limiter.allow(1).await;
    assert!(allowed);

    // 检查封禁
    let is_banned = ban_manager
        .is_banned(&BanTarget::UserId("user1".to_string()))
        .await
        .unwrap()
        .is_some();
    assert!(is_banned);
}

/// 测试封禁管理器的指数退避
#[tokio::test]
async fn test_ban_manager_exponential_backoff() {
    let storage = Arc::new(MemoryStorage::new());
    let config = BanManagerConfig {
        backoff: BackoffConfig {
            first_duration: 5,
            second_duration: 10,
            third_duration: 20,
            fourth_duration: 40,
            max_duration: 60,
        },
        enable_auto_unban: false,
        auto_unban_interval: 5,
    };

    let ban_manager = BanManager::new(storage, Some(config)).await.unwrap();

    // 测试指数退避
    let ban_times = 1;
    let duration = ban_manager.calculate_ban_duration(ban_times).await;
    assert_eq!(duration.as_secs(), 5);

    let ban_times = 2;
    let duration = ban_manager.calculate_ban_duration(ban_times).await;
    assert_eq!(duration.as_secs(), 10);
}

/// 测试封禁管理器的并发操作
#[tokio::test]
async fn test_ban_manager_concurrent_operations() {
    let storage = Arc::new(MemoryStorage::new());
    let config = BanManagerConfig::default();

    let ban_manager = Arc::new(BanManager::new(storage, Some(config)).await.unwrap());
    let mut handles = vec![];

    // 10个并发任务
    for i in 0..10 {
        let ban_manager_clone = ban_manager.clone();
        let target = BanTarget::Ip(format!("192.168.1.{}", i));

        handles.push(tokio::spawn(async move {
            let ban = BanRecord {
                target: target.clone(),
                ban_times: 1,
                duration: Duration::from_secs(60),
                banned_at: chrono::Utc::now(),
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(60),
                is_manual: false,
                reason: format!("Ban {}", i),
            };

            ban_manager_clone.add_ban(ban).await.unwrap();

            let is_banned = ban_manager_clone
                .is_banned(&target)
                .await
                .unwrap()
                .is_some();
            assert!(is_banned);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}
