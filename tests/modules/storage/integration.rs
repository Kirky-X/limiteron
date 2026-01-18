//! 存储模块集成测试
//!
//! 测试存储模块的基本功能

use limiteron::storage::MemoryStorage;

/// 测试存储模块导入
#[tokio::test]
async fn test_storage_module_import() {
    #[allow(unused_variables)]
    let storage = MemoryStorage::new();
    // 验证存储可以创建
}
