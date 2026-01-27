//! 控制器模块集成测试
//!
//! 测试控制器模块的基本功能

/// 测试控制器模块导入
#[tokio::test]
async fn test_governor_module_import() {
    // 测试导入是否正常（不依赖存储后端）
    let _ = limiteron::governor::GovernorStats::default();
}
