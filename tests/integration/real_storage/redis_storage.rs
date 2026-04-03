//! Redis Storage 集成测试
//!
//! 这些测试需要真实的 Redis 服务器连接。
//! 运行前请启动 Redis: `redis-server` 或使用 Docker
//!
//! 运行命令: `cargo test --test integration_tests -- --ignored`

#[cfg(test)]
#[cfg(feature = "redis-storage")]
mod tests {
    use limiteron::error::StorageError;
    use limiteron::redis::RedisStorage;
    use limiteron::storage::Storage;
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
    async fn cleanup_key(storage: &RedisStorage, key: &str) {
        let _ = storage.delete(key).await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_storage_connection() {
        let storage = create_redis_storage().expect("Should create Redis storage");

        // 测试基本连接
        let result = storage.get("test_connection").await;
        assert!(
            result.is_ok(),
            "Failed to connect to Redis: {:?}. Please ensure Redis is running at {}",
            result,
            REDIS_URL
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_storage_set_get() {
        let storage = create_redis_storage().expect("Should create Redis storage");
        let test_key = "test:set_get";

        cleanup_key(&storage, test_key).await;

        // 设置值
        storage
            .set(test_key, "test_value", None)
            .await
            .expect("Should set value");

        // 获取值
        let result = storage
            .get(test_key)
            .await
            .expect("Should get value")
            .expect("Value should exist");

        assert_eq!(result, "test_value");

        // 清理
        cleanup_key(&storage, test_key).await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_storage_delete() {
        let storage = create_redis_storage().expect("Should create Redis storage");
        let test_key = "test:delete";

        // 设置值
        storage
            .set(test_key, "to_delete", None)
            .await
            .expect("Should set value");

        // 删除值
        storage.delete(test_key).await.expect("Should delete value");

        // 验证已删除
        let result = storage.get(test_key).await.expect("Should get value");
        assert!(result.is_none(), "Value should be None after deletion");
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_storage_ttl() {
        let storage = create_redis_storage().expect("Should create Redis storage");
        let test_key = "test:ttl";

        cleanup_key(&storage, test_key).await;

        // 设置带 TTL 的值（2秒）
        storage
            .set(test_key, "ttl_value", Some(2))
            .await
            .expect("Should set value with TTL");

        // 立即获取应该存在
        let result = storage
            .get(test_key)
            .await
            .expect("Should get value")
            .expect("Value should exist before TTL expires");
        assert_eq!(result, "ttl_value");

        // 等待 TTL 过期
        tokio::time::sleep(Duration::from_secs(3)).await;

        // 过期后应该返回 None
        let result = storage.get(test_key).await.expect("Should get value");
        assert!(result.is_none(), "Value should be None after TTL expires");
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_storage_update_value() {
        let storage = create_redis_storage().expect("Should create Redis storage");
        let test_key = "test:update";

        // 设置初始值
        storage
            .set(test_key, "initial", None)
            .await
            .expect("Should set initial value");

        // 更新值
        storage
            .set(test_key, "updated", None)
            .await
            .expect("Should set updated value");

        // 验证更新
        let result = storage
            .get(test_key)
            .await
            .expect("Should get value")
            .expect("Value should exist");
        assert_eq!(result, "updated");

        // 清理
        cleanup_key(&storage, test_key).await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_storage_get_nonexistent() {
        let storage = create_redis_storage().expect("Should create Redis storage");

        let result = storage
            .get("test:nonexistent_key")
            .await
            .expect("Should not error on missing key");

        assert!(result.is_none(), "Should return None for nonexistent key");
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_storage_concurrent_access() {
        use std::sync::Arc;

        let storage = Arc::new(create_redis_storage().expect("Should create Redis storage"));
        let test_key = "test:concurrent";

        cleanup_key(&storage, test_key).await;

        // 并发写入
        let mut handles = vec![];
        for i in 0..10 {
            let storage_clone = Arc::clone(&storage);
            let handle = tokio::spawn(async move {
                storage_clone
                    .set(test_key, &format!("value_{}", i), None)
                    .await
            });
            handles.push(handle);
        }

        // 等待所有写入完成
        for handle in handles {
            handle
                .await
                .expect("Task should complete")
                .expect("Set should succeed");
        }

        // 验证最终值（应该是最后一个写入的值）
        let result = storage
            .get(test_key)
            .await
            .expect("Should get value")
            .expect("Value should exist");

        // 验证值存在（具体值取决于调度顺序）
        assert!(result.starts_with("value_"));

        // 清理
        cleanup_key(&storage, test_key).await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_storage_special_characters() {
        let storage = create_redis_storage().expect("Should create Redis storage");
        let test_key = "test:special:chars!@#$%^&*()";
        let test_value = "value with spaces, commas, and unicode: 你好世界";

        cleanup_key(&storage, test_key).await;

        // 设置特殊字符值
        storage
            .set(test_key, test_value, None)
            .await
            .expect("Should set value with special characters");

        // 获取并验证
        let result = storage
            .get(test_key)
            .await
            .expect("Should get value")
            .expect("Value should exist");
        assert_eq!(result, test_value);

        // 清理
        cleanup_key(&storage, test_key).await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_storage_empty_value() {
        let storage = create_redis_storage().expect("Should create Redis storage");
        let test_key = "test:empty_value";

        cleanup_key(&storage, test_key).await;

        // 设置空值
        storage
            .set(test_key, "", None)
            .await
            .expect("Should set empty value");

        // 获取并验证
        let result = storage
            .get(test_key)
            .await
            .expect("Should get value")
            .expect("Value should exist");
        assert_eq!(result, "");

        // 清理
        cleanup_key(&storage, test_key).await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_storage_large_value() {
        let storage = create_redis_storage().expect("Should create Redis storage");
        let test_key = "test:large_value";
        let large_value = "x".repeat(100_000); // 100KB

        cleanup_key(&storage, test_key).await;

        // 设置大值
        storage
            .set(test_key, &large_value, None)
            .await
            .expect("Should set large value");

        // 获取并验证
        let result = storage
            .get(test_key)
            .await
            .expect("Should get value")
            .expect("Value should exist");
        assert_eq!(result, large_value);

        // 清理
        cleanup_key(&storage, test_key).await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_storage_multiple_keys() {
        let storage = create_redis_storage().expect("Should create Redis storage");

        // 设置多个密钥
        for i in 0..5 {
            let key = format!("test:multi:key_{}", i);
            let value = format!("value_{}", i);
            storage
                .set(&key, &value, None)
                .await
                .expect("Should set value");
        }

        // 验证所有密钥
        for i in 0..5 {
            let key = format!("test:multi:key_{}", i);
            let expected = format!("value_{}", i);
            let result = storage
                .get(&key)
                .await
                .expect("Should get value")
                .expect("Value should exist");
            assert_eq!(result, expected);

            // 清理
            cleanup_key(&storage, &key).await;
        }
    }
}
