//! 封禁管理模块集成测试
//!
//! 测试封禁管理模块的基本功能

#[cfg(feature = "ban-manager")]
use limiteron::BanStorage;
#[cfg(feature = "ban-manager")]
use limiteron::ban::{BackoffConfig, BanManager, BanManagerConfig, BanSource, BanTarget};
#[cfg(feature = "ban-manager")]
use std::sync::Arc;
#[cfg(feature = "ban-manager")]
use std::time::Duration;

/// 测试 BanTarget 变体
#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn test_ban_target_ip() {
    let target = BanTarget::Ip("192.168.1.1".to_string());
    assert!(matches!(target, BanTarget::Ip(_)));
}

#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn test_ban_target_user_id() {
    let target = BanTarget::UserId("user123".to_string());
    assert!(matches!(target, BanTarget::UserId(_)));
}

#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn test_ban_target_mac() {
    let target = BanTarget::Mac("00:11:22:33:44:55".to_string());
    assert!(matches!(target, BanTarget::Mac(_)));
}

/// 测试 BanSource 变体
#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn test_ban_source_variants() {
    let _ = BanSource::Auto;
    let _ = BanSource::Manual {
        operator: "admin".to_string(),
    };
}

/// 测试 BanManagerConfig 默认值
#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn test_ban_manager_config_default() {
    let config = BanManagerConfig::default();
    assert!(config.enable_auto_unban);
    assert!(config.auto_unban_interval > 0);
    assert!(config.backoff.max_duration > 0);
}

/// 测试 BackoffConfig 默认值
#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn test_backoff_config_default() {
    let config = BackoffConfig::default();
    assert!(config.max_duration > 0);
    assert!(config.first_duration > 0);
}

/// 测试创建和读取封禁
#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn test_create_and_read_ban() {
    let storage: Arc<dyn BanStorage> = Arc::new(crate::common::MockBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let manager = BanManager::with_dependencies(storage, config)
        .await
        .unwrap();

    let target = BanTarget::UserId("user-1".to_string());
    manager
        .create_ban(
            target.clone(),
            "abuse".to_string(),
            BanSource::Manual {
                operator: "admin".to_string(),
            },
            serde_json::json!({"case": "manual"}),
            Some(Duration::from_secs(30)),
        )
        .await
        .unwrap();

    let found = manager.is_banned(&target).await.unwrap();
    assert!(found.is_some());
}

/// 测试更新封禁原因
#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn test_update_ban_reason() {
    let storage: Arc<dyn BanStorage> = Arc::new(crate::common::MockBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let manager = BanManager::with_dependencies(storage, config)
        .await
        .unwrap();

    let target = BanTarget::Ip("10.0.0.1".to_string());
    manager
        .create_ban(
            target.clone(),
            "abuse".to_string(),
            BanSource::Auto,
            serde_json::json!({"case": "update"}),
            Some(Duration::from_secs(60)),
        )
        .await
        .unwrap();

    let updated = manager
        .update_ban(&target, Some("rate limit".to_string()), None, None)
        .await
        .unwrap();
    assert!(updated.is_some());
    assert_eq!(updated.unwrap().reason, "rate limit");
}
