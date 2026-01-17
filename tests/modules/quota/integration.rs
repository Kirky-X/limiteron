//! 配额控制模块集成测试
//!
//! 测试配额控制器与其他组件的集成

use limiteron::quota_controller::{
    AlertChannel, AlertConfig, QuotaConfig, QuotaController, QuotaType,
};
use limiteron::storage::MemoryStorage;

/// 测试配额控制器与存储的集成
#[tokio::test]
async fn test_quota_controller_with_storage() {
    let storage = MemoryStorage::new();
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 1000,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            ..Default::default()
        },
    };

    let controller = QuotaController::new(storage, config);

    // 消费配额
    let result = controller.consume("user1", "resource1", 500).await.unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 500);
}

/// 测试配额控制器的告警功能
#[tokio::test]
async fn test_quota_controller_alert() {
    let storage = MemoryStorage::new();
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 100,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: true,
            thresholds: vec![80],
            channels: vec![AlertChannel::Log],
            dedup_window: 300,
        },
    };

    let controller = QuotaController::new(storage, config);

    // 消费到80%，触发告警
    let result = controller.consume("user1", "resource1", 80).await.unwrap();
    assert!(result.allowed);
    assert!(result.alert_triggered);
}

/// 测试配额控制器的并发消费
#[tokio::test]
async fn test_quota_controller_concurrent_consumption() {
    let storage = MemoryStorage::new();
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 100,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            ..Default::default()
        },
    };

    let controller = std::sync::Arc::new(QuotaController::new(storage, config));
    let mut handles = vec![];

    // 10个并发任务，每个消费10
    for _ in 0..10 {
        let controller_clone = controller.clone();
        handles.push(tokio::spawn(async move {
            controller_clone.consume("user1", "resource1", 10).await
        }));
    }

    let mut success_count = 0;
    for handle in handles {
        let result = handle.await.unwrap().unwrap();
        if result.allowed {
            success_count += 1;
        }
    }

    // 应该正好10次成功
    assert_eq!(success_count, 10);
}
