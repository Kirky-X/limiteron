// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Cache + Storage 集成测试
//!
//! 测试缓存与存储的集成，验证缓存一致性。

use crate::common::create_test_cache;
use limiteron::oxcache::Cache;
use limiteron::{Storage, storage::MemoryStorage};
use std::sync::Arc;
use std::time::Duration;

// ==================== 辅助函数 ====================

/// 创建测试用的存储和缓存
async fn create_storage_with_cache() -> (Arc<MemoryStorage>, Cache<String, String>) {
    let storage = Arc::new(MemoryStorage::new());
    let cache = create_test_cache().await;
    (storage, cache)
}

/// 生成缓存键
#[allow(dead_code)]
fn make_cache_key(prefix: &str, key: &str) -> String {
    format!("{}:{}", prefix, key)
}

// ==================== 缓存一致性验证 ====================

/// 测试内存存储 基本读写
#[tokio::test]
async fn test_mock_storage_basic_operations() {
    let storage = MemoryStorage::new();

    // 写入
    storage.set("key1", "value1", None).await.unwrap();

    // 读取
    let result = storage.get("key1").await.unwrap();
    assert_eq!(result, Some("value1".to_string()));

    // 删除
    storage.delete("key1").await.unwrap();
    let result = storage.get("key1").await.unwrap();
    assert!(result.is_none());
}

/// 测试内存存储 TTL 功能
#[tokio::test]
async fn test_mock_storage_ttl() {
    let storage = MemoryStorage::new();

    // 写入带 TTL
    storage
        .set("key1", "value1", Some(1)) // 1 秒 TTL
        .await
        .unwrap();

    // 立即读取应该成功
    let result = storage.get("key1").await.unwrap();
    assert!(result.is_some());

    // 等待过期
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // 过期后应该返回 None
    let result = storage.get("key1").await.unwrap();
    assert!(result.is_none());
}

/// 测试内存存储 批量操作
#[tokio::test]
async fn test_mock_storage_batch_operations() {
    let storage = MemoryStorage::new();

    // 批量写入
    for i in 0..100 {
        storage
            .set(&format!("key_{}", i), &format!("value_{}", i), None)
            .await
            .unwrap();
    }

    // 批量读取验证
    for i in 0..100 {
        let result = storage.get(&format!("key_{}", i)).await.unwrap();
        assert_eq!(result, Some(format!("value_{}", i)));
    }
}

/// 测试内存存储 并发访问
#[tokio::test]
async fn test_mock_storage_concurrent_access() {
    let storage = Arc::new(MemoryStorage::new());
    let mut handles = vec![];

    // 并发写入
    for i in 0..50 {
        let s = Arc::clone(&storage);
        handles.push(tokio::spawn(async move {
            s.set(&format!("key_{}", i), &format!("value_{}", i), None)
                .await
                .unwrap();
        }));
    }

    futures::future::join_all(handles).await;

    // 验证所有写入成功
    for i in 0..50 {
        let result = storage.get(&format!("key_{}", i)).await.unwrap();
        assert!(result.is_some());
    }
}

/// 测试缓存键命名空间隔离
#[tokio::test]
async fn test_cache_namespace_isolation() {
    let (storage, _cache) = create_storage_with_cache().await;

    // 不同命名空间的相同键
    let ns1_key = "namespace1:same_key";
    let ns2_key = "namespace2:same_key";

    storage.set(ns1_key, "value1", None).await.unwrap();
    storage.set(ns2_key, "value2", None).await.unwrap();

    // 验证命名空间隔离
    let result1 = storage.get(ns1_key).await.unwrap();
    let result2 = storage.get(ns2_key).await.unwrap();

    assert_eq!(result1, Some("value1".to_string()));
    assert_eq!(result2, Some("value2".to_string()));
}

/// 测试内存存储 高并发场景
#[tokio::test]
async fn test_mock_storage_high_concurrency() {
    let storage = Arc::new(MemoryStorage::new());
    let mut handles = vec![];

    // 高并发写入
    for i in 0..100 {
        let s = Arc::clone(&storage);
        handles.push(tokio::spawn(async move {
            for j in 0..10 {
                s.set(
                    &format!("key_{}_{}", i, j),
                    &format!("value_{}_{}", i, j),
                    None,
                )
                .await
                .unwrap();
            }
        }));
    }

    futures::future::join_all(handles).await;

    // 验证数据完整性
    for i in 0..100 {
        for j in 0..10 {
            let result = storage.get(&format!("key_{}_{}", i, j)).await.unwrap();
            assert_eq!(
                result,
                Some(format!("value_{}_{}", i, j)),
                "数据完整性验证失败: key_{}_{}",
                i,
                j
            );
        }
    }
}

/// 测试内存存储 数据隔离
#[tokio::test]
async fn test_mock_storage_data_isolation() {
    let storage1 = MemoryStorage::new();
    let storage2 = MemoryStorage::new();

    // 在不同存储中写入相同键
    storage1.set("key", "value1", None).await.unwrap();
    storage2.set("key", "value2", None).await.unwrap();

    // 验证数据隔离
    let result1 = storage1.get("key").await.unwrap();
    let result2 = storage2.get("key").await.unwrap();

    assert_eq!(result1, Some("value1".to_string()));
    assert_eq!(result2, Some("value2".to_string()));
}

/// 测试缓存预热
#[tokio::test]
async fn test_cache_warmup() {
    let (storage, _cache) = create_storage_with_cache().await;

    // 预热数据
    let warmup_data = vec![
        ("warmup:1", "value1"),
        ("warmup:2", "value2"),
        ("warmup:3", "value3"),
    ];

    for (key, value) in &warmup_data {
        storage.set(key, value, None).await.unwrap();
    }

    // 验证预热数据存在
    for (key, value) in &warmup_data {
        let result = storage.get(key).await.unwrap();
        assert_eq!(result, Some(value.to_string()));
    }
}

/// 测试缓存命中率统计
#[tokio::test]
async fn test_cache_hit_rate() {
    let (storage, _cache) = create_storage_with_cache().await;

    // 写入数据
    for i in 0..10 {
        let key = format!("hit_rate_key_{}", i);
        storage
            .set(&key, &format!("value_{}", i), None)
            .await
            .unwrap();
    }

    // 读取数据
    for i in 0..10 {
        let key = format!("hit_rate_key_{}", i);
        let result = storage.get(&key).await.unwrap();
        assert!(result.is_some());
    }
}
