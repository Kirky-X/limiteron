//! BanManager + Storage 集成测试
//!
//! 测试 BanManager 与存储层的集成，验证封禁管理的完整生命周期。

#[cfg(feature = "ban-manager")]
mod ban_manager_tests {
    use crate::common::{MockBanStorage, MockQuotaStorage};
    use limiteron::ban::BanManager;
    use limiteron::config::BanConfig;
    use limiteron::{BanStorage, Storage};
    use std::sync::Arc;
    use std::time::Duration;

    // ==================== 辅助函数 ====================

    /// 创建测试用的 BanManager
    fn create_ban_manager() -> BanManager {
        let storage: Arc<dyn BanStorage> = Arc::new(MockBanStorage::new());
        BanManager::new(storage)
    }

    /// 创建带有自定义配置的 BanManager
    fn create_ban_manager_with_config(config: BanConfig) -> BanManager {
        let storage: Arc<dyn BanStorage> = Arc::new(MockBanStorage::new());
        BanManager::with_storage(storage, config)
    }

    // ==================== 封禁操作测试 ====================

    /// 测试添加封禁记录 - IP 封禁
    #[tokio::test]
    async fn test_ban_ip_address() {
        let manager = create_ban_manager();

        // 添加 IP 封禁
        let result = manager
            .ban(
                &limiteron::BanTarget::Ip("192.168.1.100".to_string()),
                Duration::from_secs(3600),
                Some("恶意请求".to_string()),
                None,
            )
            .await;

        assert!(result.is_ok(), "Ban IP should succeed");

        // 验证封禁记录
        let is_banned = manager
            .is_banned(&limiteron::BanTarget::Ip(
                "192.168.1.100".to_string(),
            ))
            .await;
        assert!(is_banned, "IP should be banned");
    }

    /// 测试添加封禁记录 - 用户封禁
    #[tokio::test]
    async fn test_ban_user() {
        let manager = create_ban_manager();

        // 添加用户封禁
        let result = manager
            .ban(
                &limiteron::BanTarget::UserId("user_12345".to_string()),
                Duration::from_secs(7200),
                Some("违规操作".to_string()),
                None,
            )
            .await;

        assert!(result.is_ok(), "Ban user should succeed");

        // 验证封禁记录
        let is_banned = manager
            .is_banned(&limiteron::BanTarget::UserId(
                "user_12345".to_string(),
            ))
            .await;
        assert!(is_banned, "User should be banned");
    }

    /// 测试解封操作
    #[tokio::test]
    async fn test_unban() {
        let manager = create_ban_manager();

        // 先添加封禁
        manager
            .ban(
                &limiteron::BanTarget::Ip("192.168.1.200".to_string()),
                Duration::from_secs(3600),
                Some("测试封禁".to_string()),
                None,
            )
            .await
            .unwrap();

        // 验证已封禁
        assert!(
            manager
                .is_banned(&limiteron::BanTarget::Ip(
                    "192.168.1.200".to_string()
                ))
                .await
        );

        // 解封
        let result = manager
            .unban(&limiteron::BanTarget::Ip(
                "192.168.1.200".to_string(),
            ))
            .await;
        assert!(result.is_ok(), "Unban should succeed");

        // 验证已解封
        assert!(
            !manager
                .is_banned(&limiteron::BanTarget::Ip(
                    "192.168.1.200".to_string()
                ))
                .await
        );
    }

    /// 测试解封不存在的记录
    #[tokio::test]
    async fn test_unban_nonexistent() {
        let manager = create_ban_manager();

        // 解封不存在的记录应该成功（幂等操作）
        let result = manager
            .unban(&limiteron::BanTarget::Ip(
                "nonexistent_ip".to_string(),
            ))
            .await;
        assert!(result.is_ok(), "Unban nonexistent should succeed");
    }

    // ==================== 封禁过期测试 ====================

    /// 测试封禁过期自动解除
    #[tokio::test]
    async fn test_ban_expiration() {
        let manager = create_ban_manager();

        // 添加短期封禁（1秒）
        manager
            .ban(
                &limiteron::BanTarget::Ip("192.168.1.50".to_string()),
                Duration::from_secs(1),
                Some("短期封禁测试".to_string()),
                None,
            )
            .await
            .unwrap();

        // 立即检查应该被封禁
        assert!(
            manager
                .is_banned(&limiteron::BanTarget::Ip(
                    "192.168.1.50".to_string()
                ))
                .await
        );

        // 等待过期
        tokio::time::sleep(Duration::from_millis(1100)).await;

        // 过期后应该自动解除
        assert!(
            !manager
                .is_banned(&limiteron::BanTarget::Ip(
                    "192.168.1.50".to_string()
                ))
                .await,
            "Ban should be expired"
        );
    }

    // ==================== 批量操作测试 ====================

    /// 测试批量封禁
    #[tokio::test]
    async fn test_batch_ban() {
        let manager = create_ban_manager();

        let targets = vec![
            limiteron::BanTarget::Ip("192.168.1.1".to_string()),
            limiteron::BanTarget::Ip("192.168.1.2".to_string()),
            limiteron::BanTarget::Ip("192.168.1.3".to_string()),
            limiteron::BanTarget::UserId("user_a".to_string()),
            limiteron::BanTarget::UserId("user_b".to_string()),
        ];

        // 批量封禁
        for target in &targets {
            let result = manager
                .ban(
                    target,
                    Duration::from_secs(3600),
                    Some("批量封禁测试".to_string()),
                    None,
                )
                .await;
            assert!(result.is_ok(), "Batch ban should succeed");
        }

        // 验证所有都已封禁
        for target in &targets {
            assert!(manager.is_banned(target).await, "Target should be banned");
        }
    }

    /// 测试批量解封
    #[tokio::test]
    async fn test_batch_unban() {
        let manager = create_ban_manager();

        // 先批量封禁
        let targets: Vec<limiteron::BanTarget> = vec![
            limiteron::BanTarget::Ip("10.0.0.1".to_string()),
            limiteron::BanTarget::Ip("10.0.0.2".to_string()),
            limiteron::BanTarget::Ip("10.0.0.3".to_string()),
        ];

        for target in &targets {
            manager
                .ban(target, Duration::from_secs(3600), None, None)
                .await
                .unwrap();
        }

        // 验证都已封禁
        for target in &targets {
            assert!(manager.is_banned(target).await);
        }

        // 批量解封
        for target in &targets {
            manager.unban(target).await.unwrap();
        }

        // 验证都已解封
        for target in &targets {
            assert!(!manager.is_banned(target).await);
        }
    }

    // ==================== 并发操作测试 ====================

    /// 测试并发封禁操作
    #[tokio::test]
    async fn test_concurrent_ban_operations() {
        let manager = Arc::new(create_ban_manager());
        let mut handles = vec![];

        // 并发添加封禁
        for i in 0..20 {
            let mgr = Arc::clone(&manager);
            handles.push(tokio::spawn(async move {
                mgr.ban(
                    &limiteron::BanTarget::Ip(format!("concurrent_{}", i)),
                    Duration::from_secs(3600),
                    Some(format!("并发封禁 {}", i)),
                    None,
                )
                .await
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
                    .is_banned(&limiteron::BanTarget::Ip(format!(
                        "concurrent_{}",
                        i
                    )))
                    .await,
                "Concurrent ban {} should exist",
                i
            );
        }
    }

    /// 测试并发封禁和解封
    #[tokio::test]
    async fn test_concurrent_ban_unban() {
        let manager = Arc::new(create_ban_manager());
        let mut handles = vec![];

        // 并发封禁和解封同一标识符
        for _ in 0..10 {
            let mgr = Arc::clone(&manager);
            handles.push(tokio::spawn(async move {
                mgr.ban(
                    &limiteron::BanTarget::Ip("concurrent_same".to_string()),
                    Duration::from_secs(3600),
                    None,
                    None,
                )
                .await
            }));

            let mgr = Arc::clone(&manager);
            handles.push(tokio::spawn(async move {
                mgr.unban(&limiteron::BanTarget::Ip(
                    "concurrent_same".to_string(),
                ))
                .await
            }));
        }

        // 等待所有操作完成
        let _results: Vec<_> = futures::future::join_all(handles).await;

        // 最终状态应该是确定的（要么封禁要么未封禁）
        let _ = manager
            .is_banned(&limiteron::BanTarget::Ip(
                "concurrent_same".to_string(),
            ))
            .await;
    }

    // ==================== 存储集成测试 ====================

    /// 测试与 Mock 存储的集成
    #[tokio::test]
    async fn test_mock_storage_integration() {
        let storage = Arc::new(MockBanStorage::new());
        let manager = BanManager::new(Arc::clone(&storage) as Arc<dyn BanStorage>);

        // 添加封禁
        manager
            .ban(
                &limiteron::BanTarget::Ip("storage_test".to_string()),
                Duration::from_secs(3600),
                None,
                None,
            )
            .await
            .unwrap();

        // 验证封禁存在
        assert!(
            manager
                .is_banned(&limiteron::BanTarget::Ip(
                    "storage_test".to_string()
                ))
                .await
        );

        // 解封
        manager
            .unban(&limiteron::BanTarget::Ip(
                "storage_test".to_string(),
            ))
            .await
            .unwrap();

        // 验证已解封
        assert!(
            !manager
                .is_banned(&limiteron::BanTarget::Ip(
                    "storage_test".to_string()
                ))
                .await
        );
    }

    // ==================== 清理过期封禁测试 ====================

    /// 测试清理过期封禁
    #[tokio::test]
    async fn test_cleanup_expired_bans() {
        let manager = create_ban_manager();

        // 添加一些封禁
        manager
            .ban(
                &limiteron::BanTarget::Ip("cleanup_permanent".to_string()),
                Duration::from_secs(u64::MAX),
                None,
                None,
            )
            .await
            .unwrap();

        manager
            .ban(
                &limiteron::BanTarget::Ip("cleanup_short".to_string()),
                Duration::from_millis(100),
                None,
                None,
            )
            .await
            .unwrap();

        // 等待短期封禁过期
        tokio::time::sleep(Duration::from_millis(200)).await;

        // 执行清理
        let cleaned = manager.cleanup_expired().await;

        // 验证清理结果
        assert!(cleaned >= 1, "Should clean at least 1 expired ban");

        // 验证永久封禁仍然存在
        assert!(
            manager
                .is_banned(&limiteron::BanTarget::Ip(
                    "cleanup_permanent".to_string()
                ))
                .await,
            "Permanent ban should still exist"
        );

        // 验证过期封禁已删除
        assert!(
            !manager
                .is_banned(&limiteron::BanTarget::Ip(
                    "cleanup_short".to_string()
                ))
                .await,
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
