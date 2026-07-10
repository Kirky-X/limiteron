//! PostgreSQL Storage 集成测试
//!
//! 这些测试需要真实的 PostgreSQL 数据库连接。
//! 运行前请启动 Docker Compose: `docker-compose up -d`
//!
//! 运行命令: `cargo test --test integration_tests -- --ignored`

#[cfg(test)]
#[cfg(feature = "postgres")]
mod tests {
    use limiteron::adapters::StorageFactory;
    use limiteron::error::StorageError;
    use limiteron::{BanRecord, BanStorage, BanTarget, QuotaStorage, Storage};
    use std::sync::Arc;
    use std::time::Duration;

    const POSTGRES_DSN: &str = "postgresql://limiteron:limiteron_dev@localhost:5434/limiteron_test";

    /// 辅助函数：创建 StorageFactory 并初始化
    async fn create_storage_factory() -> Result<StorageFactory, StorageError> {
        let mut factory = StorageFactory::from_dsn(POSTGRES_DSN);
        factory
            .initialize(None)
            .await
            .map_err(|e| StorageError::ConnectionError(format!(
                "Failed to connect to PostgreSQL at {}: {}. Please ensure Docker is running: `docker-compose up -d`",
                POSTGRES_DSN, e
            )))?;
        Ok(factory)
    }

    /// 辅助函数：创建所有存储适配器
    async fn create_all_storages()
    -> Result<(Arc<dyn Storage>, Arc<dyn BanStorage>, Arc<dyn QuotaStorage>), StorageError> {
        let factory = create_storage_factory().await?;
        let storage = factory.create_storage().await?;
        let ban_storage = factory.create_ban_storage().await?;
        let quota_storage = factory.create_quota_storage().await?;
        Ok((storage, ban_storage, quota_storage))
    }

    // ========================================================================
    // Storage 测试 (Storage trait)
    // ========================================================================

    #[tokio::test]
    #[ignore]
    async fn test_postgres_storage_connection() {
        let factory = create_storage_factory().await;
        assert!(
            factory.is_ok(),
            "Failed to connect to PostgreSQL. Please ensure Docker is running: `docker-compose up -d`"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_storage_crud() {
        let (storage, _, _) = create_all_storages().await.expect("Should create storages");

        let test_key = "test:storage:crud";

        // Create
        storage
            .set(test_key, "test_value", None)
            .await
            .expect("Should set value");

        // Read
        let result = storage
            .get(test_key)
            .await
            .expect("Should get value")
            .expect("Value should exist");
        assert_eq!(result, "test_value");

        // Update
        storage
            .set(test_key, "updated_value", None)
            .await
            .expect("Should update value");

        let result = storage
            .get(test_key)
            .await
            .expect("Should get value")
            .expect("Value should exist");
        assert_eq!(result, "updated_value");

        // Delete
        storage.delete(test_key).await.expect("Should delete value");

        let result = storage.get(test_key).await.expect("Should get value");
        assert!(result.is_none(), "Value should be None after deletion");
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_storage_ttl() {
        let (storage, _, _) = create_all_storages().await.expect("Should create storages");

        let test_key = "test:storage:ttl";

        // 设置带 TTL 的值（2秒）
        storage
            .set(test_key, "ttl_value", Some(2))
            .await
            .expect("Should set value with TTL");

        // 立即获取应该存在
        let result = storage
            .get(test_key)
            .await
            .expect("Should get value")
            .expect("Value should exist before TTL expires");
        assert_eq!(result, "ttl_value");

        // 等待 TTL 过期
        tokio::time::sleep(Duration::from_secs(3)).await;

        // 过期后应该返回 None
        let result = storage.get(test_key).await.expect("Should get value");
        assert!(result.is_none(), "Value should be None after TTL expires");
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_storage_concurrent_access() {
        let factory = create_storage_factory()
            .await
            .expect("Should create factory");
        let storage = factory
            .create_storage()
            .await
            .expect("Should create storage");

        let test_key = "test:storage:concurrent";
        let storage: Arc<dyn Storage> = storage;

        // 并发写入
        let mut handles = vec![];
        for i in 0..10 {
            let storage_clone = Arc::clone(&storage);
            let handle = tokio::spawn(async move {
                storage_clone
                    .set(test_key, &format!("value_{}", i), None)
                    .await
            });
            handles.push(handle);
        }

        // 等待所有写入完成
        for handle in handles {
            handle
                .await
                .expect("Task should complete")
                .expect("Set should succeed");
        }

        // 验证最终值存在
        let result = storage
            .get(test_key)
            .await
            .expect("Should get value")
            .expect("Value should exist");
        assert!(result.starts_with("value_"));
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_storage_multiple_keys() {
        let (storage, _, _) = create_all_storages().await.expect("Should create storages");

        // 设置多个密钥
        for i in 0..5 {
            let key = format!("test:storage:multi:key_{}", i);
            let value = format!("value_{}", i);
            storage
                .set(&key, &value, None)
                .await
                .expect("Should set value");
        }

        // 验证所有密钥
        for i in 0..5 {
            let key = format!("test:storage:multi:key_{}", i);
            let expected = format!("value_{}", i);
            let result = storage
                .get(&key)
                .await
                .expect("Should get value")
                .expect("Value should exist");
            assert_eq!(result, expected);

            // 清理
            storage.delete(&key).await.expect("Should delete value");
        }
    }

    // ========================================================================
    // BanStorage 测试 (BanStorage trait)
    // ========================================================================

    #[tokio::test]
    #[ignore]
    async fn test_postgres_ban_storage_connection() {
        let factory = create_storage_factory().await;
        assert!(
            factory.is_ok(),
            "Failed to connect to PostgreSQL. Please ensure Docker is running: `docker-compose up -d`"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_ban_storage_crud() {
        let (_, ban_storage, _) = create_all_storages().await.expect("Should create storages");

        let target = BanTarget::Ip("192.168.1.100".to_string());

        // 清理可能存在的旧数据
        let _ = ban_storage.remove_ban(&target).await;

        // Create - 保存封禁记录
        let ban_record = BanRecord {
            target: target.clone(),
            ban_times: 1,
            duration: Duration::from_secs(3600),
            banned_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            is_manual: false,
            reason: "Test ban".to_string(),
        };

        ban_storage
            .save(&ban_record)
            .await
            .expect("Should save ban record");

        // Read - 检查封禁
        let found = ban_storage
            .is_banned(&target)
            .await
            .expect("Should check ban")
            .expect("Ban record should exist");

        assert_eq!(found.reason, "Test ban");
        assert_eq!(found.ban_times, 1);
        assert!(!found.is_manual);

        // Update - 增加封禁次数
        let times = ban_storage
            .increment_ban_times(&target)
            .await
            .expect("Should increment ban times");
        assert_eq!(times, 2);

        // Verify update
        let updated = ban_storage
            .is_banned(&target)
            .await
            .expect("Should check ban")
            .expect("Ban record should exist");
        assert_eq!(updated.ban_times, 2);

        // Delete - 移除封禁
        ban_storage
            .remove_ban(&target)
            .await
            .expect("Should remove ban");

        let found = ban_storage
            .is_banned(&target)
            .await
            .expect("Should check ban");
        assert!(found.is_none(), "Ban record should be removed");
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_ban_storage_expiry() {
        let (_, ban_storage, _) = create_all_storages().await.expect("Should create storages");

        let target = BanTarget::UserId("test_user_expired".to_string());

        // 清理可能存在的旧数据
        let _ = ban_storage.remove_ban(&target).await;

        // 创建已过期的封禁记录
        let ban_record = BanRecord {
            target: target.clone(),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: chrono::Utc::now() - chrono::Duration::hours(2),
            expires_at: chrono::Utc::now() - chrono::Duration::hours(1), // 已过期
            is_manual: false,
            reason: "Expired ban".to_string(),
        };

        ban_storage
            .save(&ban_record)
            .await
            .expect("Should save ban record");

        // 过期的封禁应该查询不到
        let found = ban_storage
            .is_banned(&target)
            .await
            .expect("Should check ban");
        assert!(
            found.is_none(),
            "Expired ban should not be found in is_banned"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_ban_storage_list_bans() {
        let (_, ban_storage, _) = create_all_storages().await.expect("Should create storages");

        // 添加多个封禁
        for i in 0..5 {
            let target = BanTarget::Ip(format!("192.168.2.{}", i));
            let ban_record = BanRecord {
                target,
                ban_times: 1,
                duration: Duration::from_secs(3600),
                banned_at: chrono::Utc::now(),
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
                is_manual: false,
                reason: format!("Test ban {}", i),
            };
            ban_storage
                .save(&ban_record)
                .await
                .expect("Should save ban record");
        }

        // 测试列出所有封禁
        let bans = ban_storage
            .list_bans(false, 0, 10)
            .await
            .expect("Should list bans");
        assert!(bans.len() >= 5, "Should have at least 5 bans");

        // 测试分页
        let bans_page1 = ban_storage
            .list_bans(false, 0, 3)
            .await
            .expect("Should list bans page 1");
        let bans_page2 = ban_storage
            .list_bans(false, 3, 3)
            .await
            .expect("Should list bans page 2");

        assert_eq!(bans_page1.len(), 3);
        assert_eq!(bans_page2.len(), 2); // 剩下的 2 个
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_ban_storage_cleanup_expired() {
        let (_, ban_storage, _) = create_all_storages().await.expect("Should create storages");

        // 创建多个已过期的封禁
        for i in 0..3 {
            let target = BanTarget::Ip(format!("192.168.3.{}", i));
            let ban_record = BanRecord {
                target,
                ban_times: 1,
                duration: Duration::from_secs(60),
                banned_at: chrono::Utc::now() - chrono::Duration::hours(2),
                expires_at: chrono::Utc::now() - chrono::Duration::hours(1),
                is_manual: false,
                reason: format!("Expired ban {}", i),
            };
            ban_storage
                .save(&ban_record)
                .await
                .expect("Should save expired ban record");
        }

        // 清理过期封禁
        let cleaned = ban_storage
            .cleanup_expired_bans()
            .await
            .expect("Should cleanup expired bans");
        assert!(cleaned >= 3, "Should clean at least 3 expired bans");
    }

    // ========================================================================
    // QuotaStorage 测试 (QuotaStorage trait)
    // ========================================================================

    #[tokio::test]
    #[ignore]
    async fn test_postgres_quota_storage_connection() {
        let factory = create_storage_factory().await;
        assert!(
            factory.is_ok(),
            "Failed to connect to PostgreSQL. Please ensure Docker is running: `docker-compose up -d`"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_quota_storage_crud() {
        let (_, _, quota_storage) = create_all_storages().await.expect("Should create storages");

        let user_id = "test_user_quota";
        let resource = "api_calls";

        // Create - 消费配额
        let result = quota_storage
            .consume(user_id, resource, 10, 100, Duration::from_secs(3600))
            .await
            .expect("Should consume quota");

        assert!(result.allowed, "Quota consumption should be allowed");
        assert_eq!(result.remaining, 90);

        // Read - 获取配额信息
        let quota = quota_storage
            .get_quota(user_id, resource)
            .await
            .expect("Should get quota")
            .expect("Quota should exist");

        assert_eq!(quota.consumed, 10);
        assert_eq!(quota.limit, 100);

        // Consume more
        let result = quota_storage
            .consume(user_id, resource, 50, 100, Duration::from_secs(3600))
            .await
            .expect("Should consume more quota");

        assert!(result.allowed, "Should still be allowed");
        assert_eq!(result.remaining, 40);

        // Try to exceed limit
        let result = quota_storage
            .consume(user_id, resource, 50, 100, Duration::from_secs(3600))
            .await
            .expect("Should try to exceed quota");

        assert!(!result.allowed, "Should be denied when exceeding limit");
        assert_eq!(result.remaining, 40); // 应该保持不变
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_quota_storage_reset() {
        let (_, _, quota_storage) = create_all_storages().await.expect("Should create storages");

        let user_id = "test_user_reset";
        let resource = "api_calls_reset";

        // 消费一些配额
        quota_storage
            .consume(user_id, resource, 50, 100, Duration::from_secs(3600))
            .await
            .expect("Should consume quota");

        // 验证消费
        let quota = quota_storage
            .get_quota(user_id, resource)
            .await
            .expect("Should get quota")
            .expect("Quota should exist");
        assert_eq!(quota.consumed, 50);

        // 重置配额
        quota_storage
            .reset(user_id, resource, 100, Duration::from_secs(3600))
            .await
            .expect("Should reset quota");

        // 验证重置后为新窗口
        let quota = quota_storage
            .get_quota(user_id, resource)
            .await
            .expect("Should get quota")
            .expect("Quota should exist after reset");
        assert_eq!(quota.consumed, 0);
        assert_eq!(quota.limit, 100);
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres_quota_storage_multiple_users() {
        let (_, _, quota_storage) = create_all_storages().await.expect("Should create storages");

        // 多个用户独立消费配额
        for i in 0..5 {
            let user_id = &format!("test_user_multi_{}", i);
            let resource = "api_calls_multi";

            let result = quota_storage
                .consume(user_id, resource, 10, 100, Duration::from_secs(3600))
                .await
                .expect("Should consume quota");

            assert!(result.allowed, "Should be allowed for user {}", i);
        }

        // 验证每个用户的配额独立
        for i in 0..5 {
            let user_id = &format!("test_user_multi_{}", i);
            let resource = "api_calls_multi";

            let quota = quota_storage
                .get_quota(user_id, resource)
                .await
                .expect("Should get quota")
                .expect("Quota should exist");

            assert_eq!(quota.consumed, 10, "User {} should have consumed 10", i);
            assert_eq!(quota.limit, 100);
        }
    }
}
