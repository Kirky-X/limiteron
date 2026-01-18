//! 缓存模块集成测试
//!
//! 测试缓存模块的基本功能

use limiteron::cache::L2Cache;
use std::time::Duration;

/// 测试缓存模块导入
#[tokio::test]
async fn test_cache_module_import() {
    let capacity = 1000;
    let default_ttl = Duration::from_secs(60);

    #[allow(unused_variables)]
    let cache = L2Cache::new(capacity, default_ttl);
    // 验证缓存可以创建（测试通过）
}
