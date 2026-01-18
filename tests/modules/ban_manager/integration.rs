//! 封禁管理模块集成测试
//!
//! 测试封禁管理模块的基本功能

#[cfg(feature = "ban-manager")]
use limiteron::ban_manager::{BanManager, BanManagerConfig};
#[cfg(feature = "ban-manager")]
use limiteron::storage::MemoryStorage;

/// 测试封禁管理器模块导入
#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn test_ban_manager_module_import() {
    let storage = std::sync::Arc::new(MemoryStorage::new());
    let config = BanManagerConfig::default();
    let ban_manager = BanManager::new(storage, Some(config)).await;
    // 验证封禁管理器可以创建
    assert!(ban_manager.is_ok());
}
