//! BanManager + Storage 集成测试
//!
//! 测试 BanManager 与存储层的集成，验证封禁管理的完整生命周期。

#[cfg(feature = "ban-manager")]
mod ban_manager_tests {
    use crate::common::{create_ban_record, MockBanStorage};
    use limiteron::ban::BanManager;
    use limiteron::BanManagerConfig;
    use limiteron::BanStorage;
    use std::sync::Arc;
    use std::time::Duration;

    // ==================== 辅助函数 ====================

    /// 创建测试用的 BanManager
    async fn create_ban_manager() -> BanManager {
        let storage: Arc<dyn BanStorage> = Arc::new(MockBanStorage::new());
        BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap()
    }

    /// 创建带有自定义配置的 BanManager
    #[allow(dead_code)]
    async fn create_ban_manager_with_config(config: BanManagerConfig) -> BanManager {
        let storage: Arc<dyn BanStorage> = Arc::new(MockBanStorage::new());
        BanManager::with_dependencies(storage, config)
            .await
            .unwrap()
    }

    // ==================== 封禁操作测试 ====================

    /// 测试添加封禁记录 - IP 封禁
    #[tokio::test]
    async fn test_ban_ip_address() {
        let manager = create_ban_manager().await;

        // 添加 IP 封禁
        let target = limiteron::BanTarget::Ip("192.168.1.100".to_string());
        let record = create_ban_record(target.clone(), 3600, "恶意请求");
        let result = manager.add_ban(record).await;

        assert!(result.is_ok(), "Ban IP should succeed");

        // 验证封禁记录
        let is_banned = manager.is_banned(&target).await.unwrap().is_some();
        assert!(is_banned, "IP should be banned");
    }

    /// 测试添加封禁记录 - 用户封禁
    #[tokio::test]
    async fn test_ban_user() {
        let manager = create_ban_manager().await;

        // 添加用户封禁
        let target = limiteron::BanTarget::UserId("user_12345".to_string());
        let record = create_ban_record(target.clone(), 7200, "违规操作");
        let result = manager.add_ban(record).await;

        assert!(result.is_ok(), "Ban user should succeed");

        // 验证封禁记录
        let is_banned = manager.is_banned(&target).await.unwrap().is_some();
        assert!(is_banned, "User should be banned");
    }

    /// 测试解封操作
    #[tokio::test]
    async fn test_unban() {
        let manager = create_ban_manager().await;

        // 先添加封禁
        let target = limiteron::BanTarget::Ip("192.168.1.200".to_string());
        let record = create_ban_record(target.clone(), 3600, "测试封禁");
        manager.add_ban(record).await.unwrap();

        // 验证已封禁
        assert!(manager.is_banned(&target).await.unwrap().is_some());

        // 解封
        let result = manager.delete_ban(&target, "admin".to_string()).await;
        assert!(result.is_ok(), "Unban should succeed");

        // 验证已解封
        assert!(manager.is_banned(&target).await.unwrap().is_none());
    }

    /// 测试解封不存在的记录
    #[tokio::test]
    async fn test_unban_nonexistent() {
        let manager = create_ban_manager().await;

        // 解封不存在的记录应该成功（幂等操作，返回 Ok(false)）
        let result = manager
            .delete_ban(
                &limiteron::BanTarget::Ip("nonexistent_ip".to_string()),
                "admin".to_string(),
            )
            .await;
        assert!(result.is_ok(), "Unban nonexistent should succeed");
    }

    // ==================== 封禁过期测试 ====================

    /// 测试封禁过期自动解除
    #[tokio::test]
    async fn test_ban_expiration() {
        let manager = create_ban_manager().await;

        // 添加短期封禁（1秒）
        let target = limiteron::BanTarget::Ip("192.168.1.50".to_string());
        let record = create_ban_record(target.clone(), 1, "短期封禁测试");
        manager.add_ban(record).await.unwrap();

        // 立即检查应该被封禁
        assert!(manager.is_banned(&target).await.unwrap().is_some());

        // 等待过期
        tokio::time::sleep(Duration::from_millis(1100)).await;

        // 过期后应该自动解除
        assert!(
            manager.is_banned(&target).await.unwrap().is_none(),
            "Ban should be expired"
        );
    }

    // ==================== 批量操作测试 ====================

    /// 测试批量封禁
    #[tokio::test]
    async fn test_batch_ban() {
        let manager = create_ban_manager().await;

        let targets = vec![
            limiteron::BanTarget::Ip("192.168.1.1".to_string()),
            limiteron::BanTarget::Ip("192.168.1.2".to_string()),
            limiteron::BanTarget::Ip("192.168.1.3".to_string()),
            limiteron::BanTarget::UserId("user_a".to_string()),
            limiteron::BanTarget::UserId("user_b".to_string()),
        ];

        // 批量封禁
        for target in &targets {
            let record = create_ban_record(target.clone(), 3600, "批量封禁测试");
            let result = manager.add_ban(record).await;
            assert!(result.is_ok(), "Batch ban should succeed");
        }

        // 验证所有都已封禁
        for target in &targets {
            assert!(
                manager.is_banned(target).await.unwrap().is_some(),
                "Target should be banned"
            );
        }
    }

    /// 测试批量解封
    #[tokio::test]
    async fn test_batch_unban() {
        let manager = create_ban_manager().await;

        // 先批量封禁
        let targets: Vec<limiteron::BanTarget> = vec![
            limiteron::BanTarget::Ip("10.0.0.1".to_string()),
            limiteron::BanTarget::Ip("10.0.0.2".to_string()),
            limiteron::BanTarget::Ip("10.0.0.3".to_string()),
        ];

        for target in &targets {
            let record = create_ban_record(target.clone(), 3600, "test");
            manager.add_ban(record).await.unwrap();
        }

        // 验证都已封禁
        for target in &targets {
            assert!(manager.is_banned(target).await.unwrap().is_some());
        }

        // 批量解封
        for target in &targets {
            manager
                .delete_ban(target, "admin".to_string())
                .await
                .unwrap();
        }

        // 验证都已解封
        for target in &targets {
            assert!(manager.is_banned(target).await.unwrap().is_none());
        }
    }

    // ==================== 并发操作测试 ====================

    /// 测试并发封禁操作
    #[tokio::test]
    async fn test_concurrent_ban_operations() {
        let manager = Arc::new(create_ban_manager().await);
        let mut handles = vec![];

        // 并发添加封禁
        for i in 0..20 {
            let mgr = Arc::clone(&manager);
            handles.push(tokio::spawn(async move {
                let target = limiteron::BanTarget::Ip(format!("concurrent_{}", i));
                let record = create_ban_record(target, 3600, &format!("并发封禁 {}", i));
                mgr.add_ban(record).await
            }));
        }

        // 等待所有操作完成
        let results: Vec<_> = futures::future::join_all(handles).await;

        // 验证所有操作成功
        for result in results {
            assert!(result.is_ok() && result.unwrap().is_ok());
        }

        // 验证所有封禁都存在
        for i in 0..20 {
            assert!(
                manager
                    .is_banned(&limiteron::BanTarget::Ip(format!("concurrent_{}", i)))
                    .await
                    .unwrap()
                    .is_some(),
                "Concurrent ban {} should exist",
                i
            );
        }
    }

    /// 测试并发封禁和解封
    #[tokio::test]
    async fn test_concurrent_ban_unban() {
        let manager = Arc::new(create_ban_manager().await);
        let mut handles = vec![];

        // 并发封禁和解封同一标识符
        for _ in 0..10 {
            let mgr = Arc::clone(&manager);
            handles.push(tokio::spawn(async move {
                let target = limiteron::BanTarget::Ip("concurrent_same".to_string());
                let record = create_ban_record(target, 3600, "test");
                mgr.add_ban(record).await
            }));

            let mgr = Arc::clone(&manager);
            handles.push(tokio::spawn(async move {
                mgr.delete_ban(
                    &limiteron::BanTarget::Ip("concurrent_same".to_string()),
                    "admin".to_string(),
                )
                .await
                .map(|_| ())
            }));
        }

        // 等待所有操作完成
        let _results: Vec<_> = futures::future::join_all(handles).await;

        // 最终状态应该是确定的（要么封禁要么未封禁）
        let _ = manager
            .is_banned(&limiteron::BanTarget::Ip("concurrent_same".to_string()))
            .await;
    }

    // ==================== 存储集成测试 ====================

    /// 测试与 Mock 存储的集成
    #[tokio::test]
    async fn test_mock_storage_integration() {
        let storage: Arc<dyn BanStorage> = Arc::new(MockBanStorage::new());
        let manager = BanManager::with_dependencies(storage.clone(), BanManagerConfig::default())
            .await
            .unwrap();

        // 添加封禁
        let target = limiteron::BanTarget::Ip("storage_test".to_string());
        let record = create_ban_record(target.clone(), 3600, "test");
        manager.add_ban(record).await.unwrap();

        // 验证封禁存在
        assert!(manager.is_banned(&target).await.unwrap().is_some());

        // 解封
        manager
            .delete_ban(&target, "admin".to_string())
            .await
            .unwrap();

        // 验证已解封
        assert!(manager.is_banned(&target).await.unwrap().is_none());
    }

    // ==================== 清理过期封禁测试 ====================

    /// 测试清理过期封禁
    ///
    /// 注意：BanManager 没有 cleanup_expired() 方法，
    /// 改用 storage 的 cleanup_expired_bans() 方法进行清理。
    #[tokio::test]
    async fn test_cleanup_expired_bans() {
        let storage: Arc<dyn BanStorage> = Arc::new(MockBanStorage::new());
        let manager = BanManager::with_dependencies(storage.clone(), BanManagerConfig::default())
            .await
            .unwrap();

        // 添加永久封禁（使用 86400 秒避免 u64::MAX 导致的 Duration 转换溢出）
        let permanent_target = limiteron::BanTarget::Ip("cleanup_permanent".to_string());
        let permanent_record = create_ban_record(permanent_target.clone(), 86400, "permanent");
        manager.add_ban(permanent_record).await.unwrap();

        // 添加短期封禁（0 秒，立即过期）
        let short_target = limiteron::BanTarget::Ip("cleanup_short".to_string());
        let short_record = create_ban_record(short_target.clone(), 0, "short");
        manager.add_ban(short_record).await.unwrap();

        // 等待短期封禁过期
        tokio::time::sleep(Duration::from_millis(200)).await;

        // 执行清理（通过 storage 而非 manager）
        let cleaned = storage.cleanup_expired_bans().await.unwrap();

        // 验证清理结果
        assert!(cleaned >= 1, "Should clean at least 1 expired ban");

        // 验证永久封禁仍然存在
        assert!(
            manager
                .is_banned(&permanent_target)
                .await
                .unwrap()
                .is_some(),
            "Permanent ban should still exist"
        );

        // 验证过期封禁已删除
        assert!(
            manager.is_banned(&short_target).await.unwrap().is_none(),
            "Expired ban should be removed"
        );
    }
}

// 当没有启用 ban-manager 特性时，提供一个空的测试模块
#[cfg(not(feature = "ban-manager"))]
mod ban_manager_tests {
    #[test]
    fn test_ban_manager_feature_not_enabled() {
        // 当特性未启用时，测试通过
        println!("ban-manager 特性未启用，跳过测试");
    }
}
