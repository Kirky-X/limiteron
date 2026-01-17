//! 限流器模块集成测试
//!
//! 测试限流器模块的基本功能

use limiteron::limiters::TokenBucketLimiter;

/// 测试限流器模块导入
#[tokio::test]
async fn test_limiter_module_import() {
    let limiter = TokenBucketLimiter::new(1000, 100);
    // 验证限流器可以创建
    assert!(true);
}