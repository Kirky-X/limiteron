// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Storage 错误注入用例（自 tests/integration/cache_storage.rs 下沉）
//!
//! 故障注入属单元层正当场景（unit 层允许测试替身）；断言与原集成用例
//! 完全一致：注入错误 → 操作失败 → 清除错误 → 操作恢复。

use ahash::AHashMap as HashMap;
use limiteron::Storage;
use limiteron::error::StorageError;
use std::sync::Arc;

/// 可注入错误的 Storage 测试替身（unit 层专用，非生产代码）
#[derive(Clone, Default)]
struct ErrorInjectingStorage {
    data: Arc<std::sync::RwLock<HashMap<String, String>>>,
    pending_error: Arc<std::sync::RwLock<Option<StorageError>>>,
}

impl ErrorInjectingStorage {
    async fn inject_error(&self, error: StorageError) {
        let mut guard = self.pending_error.write().unwrap();
        *guard = Some(error);
    }

    async fn clear_error(&self) {
        let mut guard = self.pending_error.write().unwrap();
        *guard = None;
    }

    async fn check_error(&self) -> Result<(), StorageError> {
        let guard = self.pending_error.read().unwrap();
        if let Some(ref err) = *guard {
            return Err(err.clone());
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Storage for ErrorInjectingStorage {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        self.check_error().await?;
        Ok(self.data.read().unwrap().get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str, _ttl: Option<u64>) -> Result<(), StorageError> {
        self.check_error().await?;
        self.data
            .write()
            .unwrap()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.check_error().await?;
        self.data.write().unwrap().remove(key);
        Ok(())
    }
}

/// 注入错误后操作失败，清除错误后操作恢复（原集成用例断言不变）
#[tokio::test]
async fn test_storage_error_injection() {
    let storage = ErrorInjectingStorage::default();

    // 注入错误
    storage
        .inject_error(StorageError::ConnectionError("模拟连接错误".to_string()))
        .await;

    // 操作应该失败
    let result = storage.get("any_key").await;
    assert!(result.is_err());

    // 清除错误
    storage.clear_error().await;

    // 操作应该成功
    let result = storage.get("any_key").await;
    assert!(result.is_ok());
}

/// 错误注入后写入同样失败，清除后恢复写入
#[tokio::test]
async fn test_storage_error_injection_write_path() {
    let storage = ErrorInjectingStorage::default();
    storage.set("key", "value", None).await.unwrap();

    storage
        .inject_error(StorageError::ConnectionError("连接断开".to_string()))
        .await;
    assert!(storage.set("key", "value2", None).await.is_err());
    assert!(storage.delete("key").await.is_err());

    storage.clear_error().await;
    assert!(storage.get("key").await.is_ok());
}
