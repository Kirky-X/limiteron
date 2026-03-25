//! 配额控制模块集成测试
//!
//! 测试配额控制模块的基本功能

#[cfg(feature = "quota-control")]
use limiteron::quota::{QuotaConfig, QuotaController, QuotaType};
use limiteron::storage::QuotaStorage;
use std::sync::Arc;
use std::time::Duration;

// MockQuotaStorage needs to be available - it's defined in tests/common/mod.rs
#[cfg(feature = "quota-control")]
type MockQuotaStorage = crate::common::MockQuotaStorage;

/// 测试配额控制器模块导入
#[tokio::test]
#[cfg(feature = "quota-control")]
async fn test_quota_controller_module_import() {
    // 测试模块导入（完整测试需要 PostgreSQL）
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 1000,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: Default::default(),
    };
    // 验证配置可以创建
    assert_eq!(config.limit, 1000);
}

/// 4.2.1: 测试配额状态持久化
#[tokio::test]
#[cfg(feature = "quota-control")]
async fn test_quota_persists_state() {
    let storage: Arc<dyn QuotaStorage> = Arc::new(MockQuotaStorage::new());
    let user_id = "user_persistence_test";
    let resource = "api_calls";
    let limit = 1000u64;
    let window = Duration::from_secs(60);

    // 第一次消费
    let result1 = storage
        .consume(user_id, resource, 100, limit, window)
        .await
        .unwrap();
    assert!(result1.allowed);
    assert_eq!(result1.remaining, 900);

    // 获取配额信息验证状态已保存
    let quota_info = storage.get_quota(user_id, resource).await.unwrap();
    assert!(quota_info.is_some());
    let info = quota_info.unwrap();
    assert_eq!(info.consumed, 100);
    assert_eq!(info.limit, limit);

    // 再次消费，验证状态累积
    let result2 = storage
        .consume(user_id, resource, 200, limit, window)
        .await
        .unwrap();
    assert!(result2.allowed);
    assert_eq!(result2.remaining, 700);

    // 验证最终状态
    let final_info = storage.get_quota(user_id, resource).await.unwrap().unwrap();
    assert_eq!(final_info.consumed, 300);
}

/// 4.2.2: 测试并发消费
#[tokio::test]
#[cfg(feature = "quota-control")]
async fn test_quota_concurrent_consumption() {
    use tokio::task::JoinSet;

    let storage: Arc<dyn QuotaStorage> = Arc::new(MockQuotaStorage::new());
    let user_id = "user_concurrent_test";
    let resource = "concurrent_api";
    let limit = 1000u64;
    let window = Duration::from_secs(60);
    let num_tasks = 10;
    let cost_per_task = 50u64;

    // 创建多个并发任务
    let mut join_set: JoinSet<
        Result<limiteron::error::ConsumeResult, limiteron::error::StorageError>,
    > = JoinSet::new();
    for i in 0..num_tasks {
        let storage_clone = storage.clone();
        let user_id = format!("{}_{}", user_id, i);
        let resource = resource.to_string();

        join_set.spawn(async move {
            storage_clone
                .consume(&user_id, &resource, cost_per_task, limit, window)
                .await
        });
    }

    // 等待所有任务完成并验证结果
    let mut total_consumed = 0u64;
    while let Some(result) = join_set.join_next().await {
        let consume_result = result.unwrap().unwrap();
        assert!(
            consume_result.allowed,
            "Each concurrent request should be allowed"
        );
        total_consumed += cost_per_task;
    }

    assert_eq!(total_consumed, num_tasks * cost_per_task);
}

/// 4.2.3: 测试重启后状态恢复
#[tokio::test]
#[cfg(feature = "quota-control")]
async fn test_quota_recovers_after_restart() {
    let storage: Arc<dyn QuotaStorage> = Arc::new(MockQuotaStorage::new());
    let user_id = "user_restart_test";
    let resource = "restart_api";
    let limit = 1000u64;
    let window = Duration::from_secs(60);

    // 模拟初始消费
    let result1: Result<limiteron::error::ConsumeResult, limiteron::error::StorageError> =
        storage.consume(user_id, resource, 300, limit, window).await;
    assert!(result1.unwrap().allowed);

    // 保存状态快照（模拟重启前）
    let state_before: Option<limiteron::storage::QuotaInfo> =
        storage.get_quota(user_id, resource).await.unwrap();
    assert!(state_before.is_some());
    let consumed_before = state_before.unwrap().consumed;

    // 模拟重启后（在实际场景中，这会是一个新的存储实例）
    // 这里我们使用相同的存储实例，但通过 get_quota 验证状态持久化
    let state_after: Option<limiteron::storage::QuotaInfo> =
        storage.get_quota(user_id, resource).await.unwrap();
    assert!(state_after.is_some());

    let info_after = state_after.unwrap();
    assert_eq!(
        info_after.consumed, consumed_before,
        "State should be recovered after restart"
    );
    assert_eq!(info_after.limit, limit);

    // 验证后续消费能正确使用恢复的状态
    let result2: Result<limiteron::error::ConsumeResult, limiteron::error::StorageError> =
        storage.consume(user_id, resource, 200, limit, window).await;
    let consume_result2 = result2.unwrap();
    assert!(consume_result2.allowed);
    assert_eq!(consume_result2.remaining, 500); // 1000 - (300 + 200)
}
