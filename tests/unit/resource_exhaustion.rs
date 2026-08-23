// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 资源耗尽上限用例（自 tests/security/resource_exhaustion_tests.rs 下沉）
//!
//! 下沉理由：max_entries 容量上限注入属故障/边界模拟（产品无容量上限配置），
//! 单元层正当场景；e2e/集成面禁 mock。断言与原用例一致。

use limiteron::Storage;
use limiteron::error::StorageError;
use ahash::AHashMap as HashMap;
use std::sync::Arc;

/// 带容量上限的 Storage 测试替身（unit 层专用）
#[derive(Clone, Default)]
struct BoundedStorage {
    data: Arc<std::sync::RwLock<HashMap<String, String>>> ,
    max_entries: usize,
}

impl BoundedStorage {
    fn with_max_entries(max: usize) -> Self {
        Self { data: Default::default(), max_entries: max }
    }
}

#[async_trait::async_trait]
impl Storage for BoundedStorage {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        Ok(self.data.read().unwrap().get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str, _ttl: Option<u64>) -> Result<(), StorageError> {
        let mut data = self.data.write().unwrap();
        if data.len() >= self.max_entries && !data.contains_key(key) {
            return Err(StorageError::QueryError("超过最大条目限制".to_string()));
        }
        data.insert(key.to_string(), value.to_string());
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.data.write().unwrap().remove(key);
        Ok(())
    }
}

/// 达到容量上限后写入被拒绝，成功数不超过上限（原用例断言不变）
#[tokio::test]
async fn test_memory_limit_validation() {
    let storage = BoundedStorage::with_max_entries(100);
    let mut success_count = 0;
    let mut limit_reached_count = 0;

    for i in 0..200 {
        let key = format!("user_{}", i);
        let value = format!("value_{}", i);
        match storage.set(&key, &value, Some(60)).await {
            Ok(_) => success_count += 1,
            Err(_) => limit_reached_count += 1,
        }
    }
    assert!(success_count <= 100, "Success count should not exceed limit");
    assert!(limit_reached_count > 0, "Some requests should be rejected due to limit");
}

/// 限流容量下并发写入部分接受部分降级（原用例断言不变）
#[tokio::test]
async fn test_graceful_degradation() {
    let storage = Arc::new(BoundedStorage::with_max_entries(10));
    let accepted = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let degraded = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let mut handles = vec![];

    for i in 0..100 {
        let storage = Arc::clone(&storage);
        let accepted = Arc::clone(&accepted);
        let degraded = Arc::clone(&degraded);
        handles.push(tokio::spawn(async move {
            let key = format!("degrade_key_{}", i);
            match storage.set(&key, &key, Some(60)).await {
                Ok(_) => { accepted.fetch_add(1, std::sync::atomic::Ordering::SeqCst); }
                Err(_) => { degraded.fetch_add(1, std::sync::atomic::Ordering::SeqCst); }
            }
        }));
    }
    for handle in handles { handle.await.expect("Task should complete"); }
    let total_accepted = accepted.load(std::sync::atomic::Ordering::SeqCst);
    let total_degraded = degraded.load(std::sync::atomic::Ordering::SeqCst);
    assert!(total_accepted > 0, "Some requests should be accepted");
    assert!(total_degraded > 0, "Some requests should be degraded");
    assert!(total_accepted <= 10, "Accepted count should not exceed limit");
}
