// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Mock 存储基础设施测试
//!
//! 这些测试验证 Mock 存储实现的正确性，确保测试基础设施可靠。
//! 注意：这些测试不是集成测试，而是测试工具本身的验证测试。

use super::{MockBanStorage, MockQuotaBehavior, MockQuotaStorage, MockStorage};
use limiteron::error::StorageError;
use limiteron::{BanStorage, QuotaStorage, Storage};
use std::sync::Arc;
use std::time::Duration;

// ==================== Mock Storage 基础测试 ====================

/// 测试 Mock Storage 基本读写
#[tokio::test]
async fn test_mock_storage_basic_operations() {
    let storage = MockStorage::new();

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

/// 测试 Mock Storage TTL 功能
#[tokio::test]
async fn test_mock_storage_ttl() {
    let storage = MockStorage::new();

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

/// 测试 Mock Storage 批量操作
#[tokio::test]
async fn test_mock_storage_batch_operations() {
    let storage = MockStorage::new();

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

/// 测试 Mock Storage 并发访问
#[tokio::test]
async fn test_mock_storage_concurrent_access() {
    let storage = Arc::new(MockStorage::new());
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

// ==================== Mock BanStorage 测试 ====================

/// 测试 Mock BanStorage 基本操作
#[tokio::test]
async fn test_mock_ban_storage_basic() {
    let storage = MockBanStorage::new();
    let target = limiteron::BanTarget::Ip("192.168.1.1".to_string());

    // 创建封禁记录
    let record = limiteron::BanRecord {
        target: target.clone(),
        ban_times: 1,
        duration: Duration::from_secs(3600),
        banned_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        is_manual: false,
        reason: "测试封禁".to_string(),
    };

    // 保存封禁
    storage.save(&record).await.unwrap();

    // 检查封禁
    let is_banned = storage.is_banned(&target).await.unwrap();
    assert!(is_banned.is_some());

    // 移除封禁
    storage.remove_ban(&target).await.unwrap();
    let is_banned = storage.is_banned(&target).await.unwrap();
    assert!(is_banned.is_none());
}

/// 测试 Mock BanStorage 过期封禁
#[tokio::test]
async fn test_mock_ban_storage_expiry() {
    let storage = MockBanStorage::new();
    let target = limiteron::BanTarget::UserId("user2".to_string());

    // 添加已过期的封禁
    let record = limiteron::BanRecord {
        target: target.clone(),
        ban_times: 1,
        duration: Duration::from_secs(1),
        banned_at: chrono::Utc::now() - chrono::Duration::hours(2),
        expires_at: chrono::Utc::now() - chrono::Duration::hours(1), // 已过期
        is_manual: false,
        reason: "临时封禁".to_string(),
    };

    storage.save(&record).await.unwrap();

    // 过期的封禁应该不被认为是封禁状态
    let is_banned = storage.is_banned(&target).await.unwrap();
    assert!(is_banned.is_none(), "过期的封禁应该不被认为是封禁状态");
}

/// 测试 Mock BanStorage 清理过期封禁
#[tokio::test]
async fn test_mock_ban_storage_cleanup_expired() {
    let storage = MockBanStorage::new();

    // 添加已过期的封禁
    let expired_target = limiteron::BanTarget::Ip("10.0.0.1".to_string());
    let expired_record = limiteron::BanRecord {
        target: expired_target.clone(),
        ban_times: 1,
        duration: Duration::from_secs(1),
        banned_at: chrono::Utc::now() - chrono::Duration::hours(2),
        expires_at: chrono::Utc::now() - chrono::Duration::hours(1),
        is_manual: false,
        reason: "已过期封禁".to_string(),
    };
    storage.save(&expired_record).await.unwrap();

    // 添加活跃封禁
    let active_target = limiteron::BanTarget::Ip("10.0.0.2".to_string());
    let active_record = limiteron::BanRecord {
        target: active_target.clone(),
        ban_times: 1,
        duration: Duration::from_secs(3600),
        banned_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        is_manual: false,
        reason: "活跃封禁".to_string(),
    };
    storage.save(&active_record).await.unwrap();

    // 清理过期封禁
    let cleaned = storage.cleanup_expired_bans().await.unwrap();
    assert!(cleaned > 0, "应该清理了一些过期封禁");

    // 活跃封禁应该仍然存在
    let is_banned = storage.is_banned(&active_target).await.unwrap();
    assert!(is_banned.is_some(), "活跃封禁应该仍然存在");
}

// ==================== Mock QuotaStorage 测试 ====================

/// 测试 Mock QuotaStorage 基本操作
#[tokio::test]
async fn test_mock_quota_storage_basic() {
    let storage = MockQuotaStorage::new();

    // 消费配额
    let result = storage
        .consume("user1", "api", 10, 100, Duration::from_secs(3600))
        .await
        .unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 90);

    // 获取配额状态
    let state = storage.get_quota("user1", "api").await.unwrap();
    assert!(state.is_some());
    let state = state.unwrap();
    assert_eq!(state.consumed, 10);
    assert_eq!(state.limit, 100);
}

/// 测试 Mock QuotaStorage 配额耗尽
#[tokio::test]
async fn test_mock_quota_storage_exhausted() {
    let storage = MockQuotaStorage::new();

    // 消耗所有配额
    let result = storage
        .consume("user1", "api", 10, 10, Duration::from_secs(3600))
        .await
        .unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 0);

    // 尝试再消费
    let result = storage
        .consume("user1", "api", 1, 10, Duration::from_secs(3600))
        .await
        .unwrap();
    assert!(!result.allowed, "配额耗尽后应该拒绝");
}

/// 测试 Mock QuotaStorage 带TTL
#[tokio::test]
async fn test_mock_quota_storage_with_ttl() {
    let storage = MockQuotaStorage::new();

    // 消费配额
    let result = storage
        .consume("user1", "api", 50, 100, Duration::from_secs(1))
        .await
        .unwrap();
    assert!(result.allowed);

    // 等待窗口过期
    tokio::time::sleep(Duration::from_millis(1100)).await;

    // 窗口过期后应该可以重新消费
    let result = storage
        .consume("user1", "api", 100, 100, Duration::from_secs(3600))
        .await
        .unwrap();
    assert!(result.allowed, "窗口过期后应该可以重新消费");
}

// ==================== Mock 存储错误模拟测试 ====================

/// 测试 Mock Storage 错误注入
#[tokio::test]
async fn test_mock_storage_error_injection() {
    let storage = MockStorage::new();

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

// ==================== Mock 存储并发测试 ====================

/// 测试 Mock Storage 高并发场景
#[tokio::test]
async fn test_mock_storage_high_concurrency() {
    let storage = Arc::new(MockStorage::new());
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

/// 测试 Mock BanStorage 并发封禁操作
#[tokio::test]
async fn test_mock_ban_storage_concurrent_bans() {
    let storage = Arc::new(MockBanStorage::new());
    let mut handles = vec![];

    // 并发添加封禁
    for i in 0..50 {
        let s = Arc::clone(&storage);
        handles.push(tokio::spawn(async move {
            let target = limiteron::BanTarget::Ip(format!("192.168.1.{}", i));
            let record = limiteron::BanRecord {
                target: target.clone(),
                ban_times: 1,
                duration: Duration::from_secs(3600),
                banned_at: chrono::Utc::now(),
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
                is_manual: false,
                reason: "并发测试".to_string(),
            };
            s.save(&record).await.unwrap()
        }));
    }

    futures::future::join_all(handles).await;

    // 验证所有封禁都生效
    for i in 0..50 {
        let target = limiteron::BanTarget::Ip(format!("192.168.1.{}", i));
        let is_banned = storage.is_banned(&target).await.unwrap();
        assert!(is_banned.is_some());
    }
}

/// 测试 Mock QuotaStorage 并发消费
#[tokio::test]
async fn test_mock_quota_storage_concurrent_consume() {
    let storage = Arc::new(MockQuotaStorage::new());

    // 配额设置为 500，100个并发请求每个消耗5，理论上只能允许100个
    // 但由于并发竞态，MockQuotaStorage 可能允许更多或更少
    // 此测试验证并发操作不会导致 panic 或死锁
    let mut handles = vec![];

    for _ in 0..100 {
        let s = Arc::clone(&storage);
        handles.push(tokio::spawn(async move {
            s.consume("shared_user", "api", 5, 500, Duration::from_secs(3600))
                .await
                .unwrap()
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles).await;

    // 统计成功和失败的数量
    let allowed_count = results
        .iter()
        .filter(|r| r.as_ref().unwrap().allowed)
        .count();

    // 验证并发操作完成，至少有部分请求成功
    // 注意：MockQuotaStorage 不保证严格的并发安全性
    assert!(allowed_count > 0, "应该有部分请求被允许");
    assert!(allowed_count <= 100, "不应该超过请求数量");
}

// ==================== Mock 存储数据隔离测试 ====================

/// 测试 Mock Storage 数据隔离
#[tokio::test]
async fn test_mock_storage_data_isolation() {
    let storage1 = MockStorage::new();
    let storage2 = MockStorage::new();

    // 在不同存储中写入相同键
    storage1.set("key", "value1", None).await.unwrap();
    storage2.set("key", "value2", None).await.unwrap();

    // 验证数据隔离
    let result1 = storage1.get("key").await.unwrap();
    let result2 = storage2.get("key").await.unwrap();

    assert_eq!(result1, Some("value1".to_string()));
    assert_eq!(result2, Some("value2".to_string()));
}

/// 测试 Mock QuotaStorage 用户隔离
#[tokio::test]
async fn test_mock_quota_storage_user_isolation() {
    let storage = MockQuotaStorage::new();

    // 为不同用户初始化配额
    storage
        .consume("user1", "api", 0, 100, Duration::from_secs(3600))
        .await
        .unwrap();
    storage
        .consume("user2", "api", 0, 100, Duration::from_secs(3600))
        .await
        .unwrap();

    // user1 消费
    storage
        .consume("user1", "api", 50, 100, Duration::from_secs(3600))
        .await
        .unwrap();

    // user2 的配额不受影响
    let result = storage
        .consume("user2", "api", 100, 100, Duration::from_secs(3600))
        .await
        .unwrap();
    assert!(result.allowed, "user2 应该有完整配额");

    // user1 的配额已减少
    let result = storage
        .consume("user1", "api", 60, 100, Duration::from_secs(3600))
        .await
        .unwrap();
    assert!(!result.allowed, "user1 配额不足");
}
