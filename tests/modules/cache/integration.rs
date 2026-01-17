//! 缓存模块集成测试
//!
//! 测试缓存与其他组件的集成

use limiteron::cache::{L2Cache, L2CacheConfig};

/// 测试L2缓存与存储的集成
#[tokio::test]
async fn test_l2_cache_with_storage() {
    use limiteron::storage::MemoryStorage;

    let storage = MemoryStorage::new();
    let config = L2CacheConfig {
        max_size: 1000,
        ttl: std::time::Duration::from_secs(60),
    };

    let cache = L2Cache::new(1000, std::time::Duration::from_secs(60));

    // 写入缓存
    cache.set("key1", "value1");

    // 从缓存读取
    let value = cache.get("key1");
    assert!(value.is_some());

    // 消费配额
    let result = storage
        .consume(
            "user1",
            "resource1",
            10,
            1000,
            std::time::Duration::from_secs(60),
        )
        .await
        .unwrap();
    assert!(result.allowed);
}

/// 测试缓存的过期清理
#[tokio::test]
async fn test_cache_expiration() {
    let cache = L2Cache::new(1000, std::time::Duration::from_millis(100));

    // 写入缓存
    cache.set("key1", "value1");

    // 立即读取，应该存在
    let value = cache.get("key1");
    assert!(value.is_some());

    // 等待过期
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // 再次读取，应该不存在
    let value = cache.get("key1");
    assert!(value.is_none());
}

/// 测试缓存的并发访问
#[tokio::test]
async fn test_cache_concurrent_access() {
    let cache = std::sync::Arc::new(L2Cache::new(1000, std::time::Duration::from_secs(60)));
    let mut handles = vec![];

    // 10个并发任务
    for i in 0..10 {
        let cache_clone = cache.clone();
        handles.push(tokio::spawn(async move {
            let key = format!("key{}", i);
            let value = format!("value{}", i);

            // 写入缓存
            cache_clone.set(&key, &value);

            // 读取缓存
            let result = cache_clone.get(&key);
            assert!(result.is_some());
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}
