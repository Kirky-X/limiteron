//! 配额控制模块集成测试
//!
//! 测试配额控制模块的基本功能

#[cfg(feature = "quota-control")]
use limiteron::quota_controller::{QuotaConfig, QuotaController, QuotaType};

/// 测试配额控制器模块导入
#[tokio::test]
#[cfg(feature = "quota-control")]
async fn test_quota_controller_module_import() {
    // 测试模块导入（完整测试需要 PostgreSQL）
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 1000,
        window_size: 3600,
        allow_overdraw: false,
        overdraft_limit_percent: 0,
        alert_config: Default::default(),
    };
    // 验证配置可以创建
    assert_eq!(config.limit, 1000);
}
