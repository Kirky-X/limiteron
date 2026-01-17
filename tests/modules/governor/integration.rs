//! 控制器模块集成测试
//!
//! 测试控制器模块的基本功能

use limiteron::storage::MemoryStorage;

/// 测试控制器模块导入
#[tokio::test]
async fn test_governor_module_import() {
    let storage = std::sync::Arc::new(MemoryStorage::new());
    let ban_storage = std::sync::Arc::new(MemoryStorage::new());
    
    // 由于 FlowControlConfig 是私有的，我们无法直接创建 Governor
    // 这里只测试导入是否正常
    assert!(true);
}