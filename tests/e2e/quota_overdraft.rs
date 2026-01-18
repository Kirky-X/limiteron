//! 端到端测试：配额透支告警
//!
//! 测试场景：
//! 1. 设置配额1000，透支200
//! 2. 消费900（正常）
//! 3. 消费150（触发80%告警）
//! 4. 消费100（透支，触发告警）
//! 5. 尝试再消费200（失败）

use limiteron::{
    quota_controller::{AlertChannel, AlertConfig, QuotaConfig, QuotaController, QuotaType},
    storage::MemoryStorage,
};
use std::sync::Arc;

/// 端到端测试：配额透支告警
#[tokio::test]
async fn test_e2e_quota_overdraft_alert() {
    let storage = MemoryStorage::new();
    let user_id = "quota_overdraft_user";
    let resource = "api_calls";

    // 配置：配额1000，允许透支20%（200）
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 1000,
        window_size: 3600,
        allow_overdraft: true,
        overdraft_limit_percent: 20,
        alert_config: AlertConfig {
            enabled: true,
            thresholds: vec![80, 90, 100, 110], // 80%, 90%, 100%, 110%
            channels: vec![AlertChannel::Log],
            dedup_window: 300,
        },
    };

    let controller = QuotaController::new(storage, config);

    // Step 1: 消费900（正常，90%使用率）
    let result = controller.consume(user_id, resource, 900).await.unwrap();
    assert!(result.allowed, "Should allow 900 consumption");
    // 总额度 = 1000 + 200 = 1200
    // 剩余 = 1200 - 900 = 300
    assert_eq!(result.remaining, 300, "Should have 300 remaining");
    assert!(result.alert_triggered, "Should trigger 90% alert");

    println!("✓ Step 1: Consumed 900 (90%), alert triggered");

    // Step 2: 消费150（达到1050，进入透支）
    let result = controller.consume(user_id, resource, 150).await.unwrap();
    assert!(result.allowed, "Should allow 150 (overdraft)");
    // 剩余 = 1200 - 1050 = 150
    assert_eq!(
        result.remaining, 150,
        "Remaining should be 150 (in overdraft)"
    );
    assert!(result.alert_triggered, "Should trigger overdraft alert");

    println!("✓ Step 2: Consumed 150 (overdraft to 1050), alert triggered");

    // Step 3: 尝试再消费151（应该失败，总共1201超过透支上限1200）
    // total_limit = 1200; // 1000 + 200 overdraft
    let result = controller.consume(user_id, resource, 151).await.unwrap();
    assert!(
        !result.allowed,
        "Should reject 151 (exceeds overdraft limit)"
    );
    assert_eq!(result.remaining, 150, "Should have 150 remaining capacity");

    println!("✓ Step 3: Rejected 151 (exceeds overdraft limit of 1200)");

    // Step 4: 验证当前状态
    let quota_state = controller.get_quota(user_id, resource).await.unwrap();
    assert!(quota_state.is_some(), "Quota state should exist");
    let state = quota_state.unwrap();
    assert_eq!(state.consumed, 1050, "Consumed should be 1050");

    println!("✓ Step 4: Current consumed: 1050/1000 (50 in overdraft)");

    println!("✓ E2E test passed: Quota overdraft and alert flow completed");
}

/// 端到端测试：多资源配额隔离
#[tokio::test]
async fn test_e2e_quota_multi_resource() {
    let storage = MemoryStorage::new();
    let user_id = "multi_resource_user";

    // API调用配额
    let api_config = QuotaConfig {
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

    // 存储配额
    let storage_config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 500,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            ..Default::default()
        },
    };

    let api_controller = QuotaController::new(storage.clone(), api_config);
    let storage_controller = QuotaController::new(storage, storage_config);

    // 消费API配额
    let result = api_controller
        .consume(user_id, "api_calls", 800)
        .await
        .unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 200);

    // 消费存储配额
    let result = storage_controller
        .consume(user_id, "storage_mb", 400)
        .await
        .unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 100);

    // 尝试消费更多API配额（应该失败）
    let result = api_controller
        .consume(user_id, "api_calls", 300)
        .await
        .unwrap();
    assert!(!result.allowed);

    // 尝试消费更多存储配额（应该成功）
    let result = storage_controller
        .consume(user_id, "storage_mb", 100)
        .await
        .unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 0);

    println!("✓ E2E test passed: Multi-resource quota isolation works");
}

/// 端到端测试：配额滑动窗口重置
#[tokio::test]
async fn test_e2e_quota_sliding_window_reset() {
    let storage = MemoryStorage::new();
    let user_id = "sliding_window_user";
    let resource = "hourly_quota";

    // 配置：1小时窗口，配额100
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 100,
        window_size: 2, // 2秒（测试用）
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            ..Default::default()
        },
    };

    let controller = QuotaController::new(storage, config);

    // 第1小时：消费80
    let result = controller.consume(user_id, resource, 80).await.unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 20);

    println!("✓ Step 1: Consumed 80, remaining: 20");

    // 等待窗口过期
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // 新窗口：应该可以消费100
    let result = controller.consume(user_id, resource, 100).await.unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 0);

    println!("✓ Step 2: After window reset, consumed 100, remaining: 0");

    println!("✓ E2E test passed: Sliding window reset works");
}

/// 端到端测试：配额告警去重
#[tokio::test]
async fn test_e2e_quota_alert_dedup() {
    let storage = MemoryStorage::new();
    let user_id = "alert_dedup_user";
    let resource = "api_calls";

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
            dedup_window: 5, // 5秒去重窗口
        },
    };

    let controller = QuotaController::new(storage, config);

    // 消费80，触发80%告警
    let result = controller.consume(user_id, resource, 80).await.unwrap();
    assert!(result.allowed);
    assert!(result.alert_triggered, "Should trigger alert");

    println!("✓ Step 1: Alert triggered at 80%");

    // 立即消费到90%，不应该触发告警（去重）
    let result = controller.consume(user_id, resource, 10).await.unwrap();
    assert!(result.allowed);
    assert!(!result.alert_triggered, "Should not trigger alert (dedup)");

    println!("✓ Step 2: Alert deduped at 90%");

    // 等待去重窗口过期
    tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;

    // 清理过期的去重记录
    controller.cleanup_alert_dedup();

    // 再次消费，应该触发告警
    let result = controller.consume(user_id, resource, 5).await.unwrap();
    assert!(result.allowed);
    assert!(
        result.alert_triggered,
        "Should trigger alert after dedup window"
    );

    println!("✓ Step 3: Alert triggered again after dedup window");

    println!("✓ E2E test passed: Alert deduplication works");
}

/// 端到端测试：配额重置
#[tokio::test]
async fn test_e2e_quota_reset() {
    let storage = MemoryStorage::new();
    let user_id = "reset_user";
    let resource = "api_calls";

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

    let controller = QuotaController::new(storage.clone(), config);

    // 消费50
    let result = controller.consume(user_id, resource, 50).await.unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 50);

    // 重置配额
    controller.reset_quota(user_id, resource).await.unwrap();

    // 验证已重置
    let quota_state = controller.get_quota(user_id, resource).await.unwrap();
    assert!(quota_state.is_some());
    assert_eq!(quota_state.unwrap().consumed, 0);

    // 可以再次消费100
    let result = controller.consume(user_id, resource, 100).await.unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 0);

    println!("✓ E2E test passed: Quota reset works");
}

/// 端到端测试：并发配额消费
#[tokio::test]
async fn test_e2e_quota_concurrent_consumption() {
    let storage = MemoryStorage::new();
    let user_id = "concurrent_user";
    let resource = "api_calls";

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

    let controller = Arc::new(QuotaController::new(storage, config));

    // 10个并发任务，每个消费10
    let mut handles = vec![];

    for _ in 0..10 {
        let controller_clone = controller.clone();
        let user_id = user_id.to_string();
        let resource = resource.to_string();

        handles.push(tokio::spawn(async move {
            controller_clone.consume(&user_id, &resource, 10).await
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

    // 应该正好10次成功
    assert_eq!(success_count, 10, "Should have 10 successful consumptions");
    assert_eq!(fail_count, 0, "Should have 0 failed consumptions");

    println!("✓ E2E test passed: Concurrent quota consumption works");
}
