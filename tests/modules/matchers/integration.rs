//! 匹配器模块集成测试
//!
//! 测试匹配器模块的基本功能

use limiteron::matchers::RuleMatcher;

/// 测试匹配器模块导入
#[tokio::test]
async fn test_matcher_module_import() {
    #[allow(unused_variables)]
    let matcher = RuleMatcher::new(vec![]);
    // 验证匹配器可以创建
}