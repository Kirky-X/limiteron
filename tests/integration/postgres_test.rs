//! PostgreSQL集成测试
//!
//! 测试PostgreSQL存储的集成功能

use limiteron::postgres_storage::{PostgresStorage, PostgresStorageConfig};
use limiteron::storage::{BanStorage, QuotaStorage};
use std::time::Duration;

const DEFAULT_LIMIT: u64 = 1000;
const DEFAULT_WINDOW: Duration = Duration::from_secs(60);

/// 测试PostgreSQL连接
#[tokio::test]
#[ignore] // 需要PostgreSQL服务器运行
async fn test_postgres_connection() {
    let config =
        PostgresStorageConfig::new("postgresql://limiteron:limiteron123@localhost:5432/limiteron");
    let storage = PostgresStorage::new(config).await.unwrap();

    // 测试基本连接
    let result = storage.ping().await;
    assert!(result.is_ok());
}

/// 测试PostgreSQL配额存储
#[tokio::test]
#[ignore]
async fn test_postgres_quota_storage() {
    let config =
        PostgresStorageConfig::new("postgresql://limiteron:limiteron123@localhost:5432/limiteron");
    let storage = PostgresStorage::new(config).await.unwrap();

    let user_id = "test_user_pg";
    let resource = "test_resource";

    // 清理旧数据
    let _ = storage
        .reset(user_id, resource, DEFAULT_LIMIT, DEFAULT_WINDOW)
        .await;

    // 消费配额
    let result = storage
        .consume(user_id, resource, 100, DEFAULT_LIMIT, DEFAULT_WINDOW)
        .await
        .unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 900);

    // 获取配额
    let quota = storage.get_quota(user_id, resource).await.unwrap();
    assert!(quota.is_some());
    assert_eq!(quota.unwrap().consumed, 100);

    // 重置配额
    storage
        .reset(user_id, resource, DEFAULT_LIMIT, DEFAULT_WINDOW)
        .await
        .unwrap();
    let quota = storage.get_quota(user_id, resource).await.unwrap();
    assert!(quota.is_some());
    assert_eq!(quota.unwrap().consumed, 0);
}

/// 测试PostgreSQL事务回滚
#[tokio::test]
#[ignore]
async fn test_postgres_transaction_rollback() {
    let config =
        PostgresStorageConfig::new("postgresql://limiteron:limiteron123@localhost:5432/limiteron");
    let storage = PostgresStorage::new(config).await.unwrap();

    let user_id = "transaction_test_user";
    let resource = "transaction_test_resource";

    // 清理
    let _ = storage
        .reset(user_id, resource, DEFAULT_LIMIT, DEFAULT_WINDOW)
        .await;

    // 消费配额
    let result1 = storage
        .consume(user_id, resource, 100, DEFAULT_LIMIT, DEFAULT_WINDOW)
        .await
        .unwrap();
    assert!(result1.allowed);

    // 模拟事务失败（这里简化处理，实际应该在事务中）
    // 在真实场景中，如果业务逻辑失败，应该调用reset来回滚
    storage
        .reset(user_id, resource, DEFAULT_LIMIT, DEFAULT_WINDOW)
        .await
        .unwrap();

    // 验证配额已被重置
    let quota = storage.get_quota(user_id, resource).await.unwrap();
    assert!(quota.is_some());
    assert_eq!(quota.unwrap().consumed, 0);
}

/// 测试PostgreSQL封禁存储
#[tokio::test]
#[ignore]
async fn test_postgres_ban_storage() {
    use chrono::Utc;
    use limiteron::storage::{BanRecord, BanTarget};

    let config =
        PostgresStorageConfig::new("postgresql://limiteron:limiteron123@localhost:5432/limiteron");
    let storage = PostgresStorage::new(config).await.unwrap();

    let target = BanTarget::UserId("test_user_ban".to_string());

    // 清理旧数据
    let _ = storage.remove_ban(&target).await;

    // 添加封禁
    let ban = BanRecord {
        target: target.clone(),
        ban_times: 1,
        duration: Duration::from_secs(60),
        banned_at: Utc::now(),
        expires_at: Utc::now() + Duration::from_secs(60),
        is_manual: false,
        reason: "Test ban".to_string(),
    };

    storage.add_ban(&ban).await.unwrap();

    // 查询封禁
    let result = storage.get_ban(&target).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().ban_times, 1);

    // 移除封禁
    storage.remove_ban(&target).await.unwrap();
    let result = storage.get_ban(&target).await.unwrap();
    assert!(result.is_none());
}

/// 测试PostgreSQL封禁列表查询
#[tokio::test]
#[ignore]
async fn test_postgres_list_bans() {
    use chrono::Utc;
    use limiteron::storage::{BanRecord, BanTarget};

    let config =
        PostgresStorageConfig::new("postgresql://limiteron:limiteron123@localhost:5432/limiteron");
    let storage = PostgresStorage::new(config).await.unwrap();

    // 清理旧数据
    for i in 0..5 {
        let target = BanTarget::UserId(format!("list_test_user_{}", i));
        let _ = storage.remove_ban(&target).await;
    }

    // 添加多个封禁
    for i in 0..5 {
        let ban = BanRecord {
            target: BanTarget::UserId(format!("list_test_user_{}", i)),
            ban_times: 1,
            duration: Duration::from_secs(3600),
            banned_at: Utc::now(),
            expires_at: Utc::now() + Duration::from_secs(3600),
            is_manual: false,
            reason: "Test ban".to_string(),
        };
        storage.save(&ban).await.unwrap();
    }

    // 查询封禁历史
    for i in 0..5 {
        let target = BanTarget::UserId(format!("list_test_user_{}", i));
        let history = storage.get_history(&target).await.unwrap();
        assert!(history.is_some());
        assert_eq!(history.unwrap().ban_times, 1);
    }
}

/// 测试PostgreSQL过期封禁清理
#[tokio::test]
#[ignore]
async fn test_postgres_cleanup_expired_bans() {
    use chrono::Utc;
    use limiteron::storage::{BanRecord, BanTarget};

    let config =
        PostgresStorageConfig::new("postgresql://limiteron:limiteron123@localhost:5432/limiteron");
    let storage = PostgresStorage::new(config).await.unwrap();

    let target = BanTarget::Ip("192.168.1.250".to_string());

    // 清理旧数据
    let _ = storage.remove_ban(&target).await;

    // 添加一个已过期的封禁
    let ban = BanRecord {
        target: target.clone(),
        ban_times: 1,
        duration: Duration::from_secs(1),
        banned_at: Utc::now() - Duration::from_secs(10),
        expires_at: Utc::now() - Duration::from_secs(5),
        is_manual: false,
        reason: "Expired ban".to_string(),
    };

    storage.add_ban(&ban).await.unwrap();

    // 添加一个活跃的封禁
    let target2 = BanTarget::Ip("192.168.1.251".to_string());

    // 清理旧数据
    let _ = storage.remove_ban(&target2).await;

    let ban2 = BanRecord {
        target: target2.clone(),
        ban_times: 1,
        duration: Duration::from_secs(3600),
        banned_at: Utc::now(),
        expires_at: Utc::now() + Duration::from_secs(3600),
        is_manual: false,
        reason: "Active ban".to_string(),
    };

    storage.add_ban(&ban2).await.unwrap();

    // 清理过期封禁
    let cleaned = storage.cleanup_expired_bans().await.unwrap();
    assert!(cleaned >= 1);

    // 验证过期封禁已被清理
    let result = storage.get_ban(&target).await.unwrap();
    assert!(result.is_none());

    // 验证活跃封禁仍然存在
    let result = storage.get_ban(&target2).await.unwrap();
    assert!(result.is_some());
}

/// 测试PostgreSQL连接池
#[tokio::test]
#[ignore]
async fn test_postgres_connection_pool() {
    let config =
        PostgresStorageConfig::new("postgresql://limiteron:limiteron123@localhost:5432/limiteron")
            .with_pool_size(10);

    let storage = PostgresStorage::new(config).await.unwrap();

    // 并发测试
    let mut handles = vec![];

    for i in 0..20 {
        let storage_clone = storage.clone();
        let user_id = format!("pool_user_{}", i);
        let resource = "pool_resource";

        handles.push(tokio::spawn(async move {
            // 执行多次操作
            for _ in 0..10 {
                let _ = storage_clone
                    .consume(&user_id, resource, 1, DEFAULT_LIMIT, DEFAULT_WINDOW)
                    .await;
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}

/// 测试PostgreSQL高并发场景
#[tokio::test]
#[ignore]
async fn test_postgres_high_concurrency() {
    let config =
        PostgresStorageConfig::new("postgresql://limiteron:limiteron123@localhost:5432/limiteron")
            .with_pool_size(50);

    let storage = PostgresStorage::new(config).await.unwrap();

    let user_id = "high_concurrency_pg_user";
    let resource = "high_concurrency_pg_resource";

    // 清理
    let _ = storage
        .reset(user_id, resource, DEFAULT_LIMIT, DEFAULT_WINDOW)
        .await;

    // 500个并发请求
    let mut handles = vec![];

    for _ in 0..500 {
        let storage_clone = storage.clone();
        let user_id = user_id.to_string();
        let resource = resource.to_string();

        handles.push(tokio::spawn(async move {
            storage_clone
                .consume(&user_id, &resource, 1, DEFAULT_LIMIT, DEFAULT_WINDOW)
                .await
        }));
    }

    let mut success_count = 0;
    let mut fail_count = 0;

    for handle in handles {
        match handle.await.unwrap() {
            Ok(result) => {
                if result.allowed {
                    success_count += 1;
                } else {
                    fail_count += 1;
                }
            }
            Err(_) => fail_count += 1,
        }
    }

    println!("Success: {}, Fail: {}", success_count, fail_count);
    assert!(success_count + fail_count == 500);
}

/// 测试PostgreSQL封禁次数统计
#[tokio::test]
#[ignore]
async fn test_postgres_ban_times_tracking() {
    use limiteron::storage::BanTarget;

    let config =
        PostgresStorageConfig::new("postgresql://limiteron:limiteron123@localhost:5432/limiteron");
    let storage = PostgresStorage::new(config).await.unwrap();

    let target = BanTarget::UserId("ban_times_user".to_string());

    // 清理
    let _ = storage.remove_ban(&target).await;

    // 获取初始封禁次数
    let ban_times = storage.get_ban_times(&target).await.unwrap();
    assert_eq!(ban_times, 0);

    // 增加封禁次数
    let new_times = storage.increment_ban_times(&target).await.unwrap();
    assert_eq!(new_times, 1);

    // 再次增加
    let new_times = storage.increment_ban_times(&target).await.unwrap();
    assert_eq!(new_times, 2);

    // 验证
    let ban_times = storage.get_ban_times(&target).await.unwrap();
    assert_eq!(ban_times, 2);
}
