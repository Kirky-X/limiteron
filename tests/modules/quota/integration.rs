//! 配额控制模块集成测试
//!
//! 测试配额控制模块的基本功能

use limiteron::quota_controller::{QuotaConfig, QuotaController, QuotaType};
use limiteron::storage::MemoryStorage;

/// 测试配额控制器模块导入
#[tokio::test]
async fn test_quota_controller_module_import() {
    let storage = MemoryStorage::new();
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 1000,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: Default::default(),
    };

    #[allow(unused_variables)]
    let controller = QuotaController::new(storage, config);
    // 验证配额控制器可以创建
}