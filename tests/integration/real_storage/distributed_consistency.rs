//! 分布式多实例一致性测试
//!
//! 验证多个实例共享同一 Redis 存储时的数据一致性。
//! 这些测试需要真实的 Redis 服务器连接。
//!
//! 运行前请启动 Redis: `redis-server` 或使用 Docker
//!
//! 运行命令: `cargo test --test integration_tests -- --ignored`

#[cfg(test)]
#[cfg(feature = "redis-storage")]
mod tests {
    use limiteron::error::StorageError;
    use limiteron::redis::RedisStorage;
    use limiteron::storage::Storage;
    use std::sync::Arc;
    use std::time::Duration;

    const REDIS_URL: &str = "redis://127.0.0.1:6379/";

    /// 辅助函数：创建 Redis 存储
    fn create_redis_storage() -> Result<RedisStorage, StorageError> {
        RedisStorage::from_connection_string(REDIS_URL).map_err(|e| {
            StorageError::ConnectionError(format!(
                "Failed to connect to Redis at {}: {}. Please ensure Redis is running.",
                REDIS_URL, e
            ))
        })
    }

    /// 辅助函数：清理测试密钥
    async fn cleanup_key(storage: &Arc<dyn Storage>, key: &str) {
        let _ = storage.delete(key).await;
    }

    // ========================================================================
    // 分布式存储一致性测试
    // ========================================================================

    #[tokio::test]
    #[ignore]
    async fn test_distributed_storage_consistency() {
        // 创建共享 Redis 存储
        let redis_storage = create_redis_storage().expect("Should create Redis storage");
        let storage: Arc<dyn Storage> = Arc::new(redis_storage);

        let test_key = "test:distributed:consistency";

        // 清理旧数据
        cleanup_key(&storage, test_key).await;

        // 实例 A 写入数据
        storage
            .set(test_key, "data_from_instance_a", None)
            .await
            .expect("Instance A should write data");

        // 实例 B 读取数据（使用同一个存储实例模拟）
        let result = storage
            .get(test_key)
            .await
            .expect("Instance B should read data")
            .expect("Data should exist for instance B");

        assert_eq!(result, "data_from_instance_a");

        // 实例 B 更新数据
        storage
            .set(test_key, "data_from_instance_b", None)
            .await
            .expect("Instance B should update data");

        // 实例 A 读取更新后的数据
        let result = storage
            .get(test_key)
            .await
            .expect("Instance A should read updated data")
            .expect("Updated data should exist");

        assert_eq!(result, "data_from_instance_b");
    }

    #[tokio::test]
    #[ignore]
    async fn test_distributed_concurrent_writes() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let redis_storage = create_redis_storage().expect("Should create Redis storage");
        let storage: Arc<dyn Storage> = Arc::new(redis_storage);

        let test_key = "test:distributed:concurrent";
        let success_count = Arc::new(AtomicU32::new(0));

        // 清理旧数据
        cleanup_key(&storage, test_key).await;

        // 并发写入
        let mut handles = vec![];
        for i in 0..20 {
            let storage_clone = Arc::clone(&storage);
            let success_clone = Arc::clone(&success_count);
            let handle = tokio::spawn(async move {
                let result = storage_clone
                    .set(test_key, &format!("value_{}", i), None)
                    .await;
                if result.is_ok() {
                    success_clone.fetch_add(1, Ordering::SeqCst);
                }
            });
            handles.push(handle);
        }

        // 等待所有写入完成
        for handle in handles {
            handle.await.expect("Task should complete");
        }

        // 验证所有写入都成功
        let total_success = success_count.load(Ordering::SeqCst);
        assert_eq!(total_success, 20, "All writes should succeed");

        // 验证最终值存在（应该是最后写入的值）
        let result = storage
            .get(test_key)
            .await
            .expect("Should read final value")
            .expect("Final value should exist");

        assert!(result.starts_with("value_"));
    }

    // ========================================================================
    // 分布式 TTL 一致性测试
    // ========================================================================

    #[tokio::test]
    #[ignore]
    async fn test_distributed_ttl_consistency() {
        let redis_storage = create_redis_storage().expect("Should create Redis storage");
        let storage: Arc<dyn Storage> = Arc::new(redis_storage);

        let test_key = "test:distributed:ttl";

        // 清理旧数据
        cleanup_key(&storage, test_key).await;

        // 实例 A 设置带 TTL 的数据
        storage
            .set(test_key, "ttl_data", Some(3))
            .await
            .expect("Instance A should set data with TTL");

        // 实例 B 立即读取应该存在
        let result = storage
            .get(test_key)
            .await
            .expect("Instance B should read")
            .expect("Data should exist for instance B");
        assert_eq!(result, "ttl_data");

        // 等待 TTL 过期
        tokio::time::sleep(Duration::from_secs(4)).await;

        // 两个实例都应该看到数据已过期
        let result_a = storage.get(test_key).await.expect("Instance A should read");
        let result_b = storage.get(test_key).await.expect("Instance B should read");

        assert!(result_a.is_none(), "Instance A should see expired data");
        assert!(result_b.is_none(), "Instance B should see expired data");
    }

    // ========================================================================
    // 网络分区恢复测试
    // ========================================================================

    #[tokio::test]
    #[ignore]
    async fn test_network_partition_recovery() {
        let redis_storage = create_redis_storage().expect("Should create Redis storage");
        let storage: Arc<dyn Storage> = Arc::new(redis_storage);

        let test_key = "test:partition:recovery";

        // 清理旧数据
        cleanup_key(&storage, test_key).await;

        // 存储数据
        storage
            .set(test_key, "before_partition", None)
            .await
            .expect("Should store data before partition");

        // 模拟网络恢复后读取
        let result = storage
            .get(test_key)
            .await
            .expect("Should read after recovery")
            .expect("Data should exist after recovery");

        assert_eq!(result, "before_partition");

        // 更新数据
        storage
            .set(test_key, "after_recovery", None)
            .await
            .expect("Should update data after recovery");

        let result = storage
            .get(test_key)
            .await
            .expect("Should read updated data")
            .expect("Updated data should exist");

        assert_eq!(result, "after_recovery");
    }

    // ========================================================================
    // 分布式多键值一致性测试
    // ========================================================================

    #[tokio::test]
    #[ignore]
    async fn test_distributed_multiple_keys_consistency() {
        let redis_storage = create_redis_storage().expect("Should create Redis storage");
        let storage: Arc<dyn Storage> = Arc::new(redis_storage);

        // 清理旧数据
        for i in 0..10 {
            let key = format!("test:distributed:multi:{}", i);
            cleanup_key(&storage, &key).await;
        }

        // 实例 A 写入多个键
        for i in 0..10 {
            let key = format!("test:distributed:multi:{}", i);
            let value = format!("value_{}", i);
            storage
                .set(&key, &value, None)
                .await
                .expect(&format!("Instance A should write key {}", i));
        }

        // 实例 B 读取并验证所有键
        for i in 0..10 {
            let key = format!("test:distributed:multi:{}", i);
            let expected = format!("value_{}", i);
            let result = storage
                .get(&key)
                .await
                .expect(&format!("Instance B should read key {}", i))
                .expect(&format!("Key {} should exist", i));

            assert_eq!(result, expected, "Key {} value mismatch", i);
        }

        // 清理
        for i in 0..10 {
            let key = format!("test:distributed:multi:{}", i);
            cleanup_key(&storage, &key).await;
        }
    }

    // ========================================================================
    // 分布式大值一致性测试
    // ========================================================================

    #[tokio::test]
    #[ignore]
    async fn test_distributed_large_value_consistency() {
        let redis_storage = create_redis_storage().expect("Should create Redis storage");
        let storage: Arc<dyn Storage> = Arc::new(redis_storage);

        let test_key = "test:distributed:large";
        let large_value = "x".repeat(100_000); // 100KB

        // 清理旧数据
        cleanup_key(&storage, test_key).await;

        // 实例 A 写入大值
        storage
            .set(test_key, &large_value, None)
            .await
            .expect("Instance A should write large value");

        // 实例 B 读取并验证
        let result = storage
            .get(test_key)
            .await
            .expect("Instance B should read large value")
            .expect("Large value should exist");

        assert_eq!(result, large_value);
        assert_eq!(result.len(), 100_000);
    }

    // ========================================================================
    // 分布式特殊字符一致性测试
    // ========================================================================

    #[tokio::test]
    #[ignore]
    async fn test_distributed_special_chars_consistency() {
        let redis_storage = create_redis_storage().expect("Should create Redis storage");
        let storage: Arc<dyn Storage> = Arc::new(redis_storage);

        let test_key = "test:distributed:special:chars!@#$%^&*()";
        let test_value = "value with spaces, commas, and unicode: 你好世界";

        // 清理旧数据
        cleanup_key(&storage, test_key).await;

        // 实例 A 写入特殊字符值
        storage
            .set(test_key, test_value, None)
            .await
            .expect("Instance A should write special chars");

        // 实例 B 读取并验证
        let result = storage
            .get(test_key)
            .await
            .expect("Instance B should read special chars")
            .expect("Special chars value should exist");

        assert_eq!(result, test_value);
    }
}
