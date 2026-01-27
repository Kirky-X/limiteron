//! 封禁管理模块集成测试
//!
//! 测试封禁管理模块的基本功能

#[cfg(feature = "ban-manager")]
use limiteron::ban_manager::{BanManager, BanManagerConfig};

/// 测试封禁管理器模块导入
#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn test_ban_manager_module_import() {
    // 测试模块导入（完整测试需要 PostgreSQL）
    let config = BanManagerConfig::default();
    // 验证配置可以创建
    assert!(config.enable_auto_unban);
}
