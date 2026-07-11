// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Matcher + Limiter 集成测试
//!
//! 测试规则匹配器与限流器的集成，验证规则匹配与限流联动。

use crate::common::{MockQuotaStorage, RequestContextBuilder, create_governor};
use limiteron::config::{
    Action, ActionConfig, CacheBackend, ConfigMatcher as Matcher, FlowControlConfig, LimiterConfig,
    MetricsBackend, Rule, StorageType,
};
use limiteron::error::Decision;
use limiteron::{BanStorage, Storage};
use std::sync::Arc;

// ==================== 辅助函数 ====================

/// 创建带有自定义限流器配置的 Governor
async fn create_governor_with_limiters(limiters: Vec<LimiterConfig>) -> Arc<limiteron::Governor> {
    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: StorageType::Memory,
            cache: CacheBackend::Memory,
            metrics: MetricsBackend::Prometheus,
            trusted_proxies: Default::default(),
        },
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters,
            action: ActionConfig {
                on_exceed: Action::Reject,
                ban: None,
            },
        }],
    };

    let storage: Arc<dyn Storage> = Arc::new(MockQuotaStorage::new());
    let ban_storage: Arc<dyn BanStorage> = Arc::new(crate::common::MockBanStorage::new());

    let governor = Arc::new(
        limiteron::Governor::builder()
            .with_config(config)
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Failed to create governor"),
    );

    // 禁用 L1 缓存以确保限流器状态正确更新
    // L1 缓存会绕过限流检查导致令牌不被消耗
    governor.disable_l1_cache();

    governor
}

// ==================== 规则匹配与限流联动验证 ====================

/// 测试基本限流决策
#[tokio::test]
async fn test_basic_rate_limiting() {
    let governor = create_governor().await;

    let ctx = RequestContextBuilder::new()
        .user_id("test_user")
        .ip("192.168.1.1")
        .path("/api/test")
        .method("GET")
        .build();

    // 执行检查
    let result = governor.check(&ctx).await;
    assert!(result.is_ok(), "Governor check should succeed");

    let decision = result.unwrap();
    match decision {
        Decision::Allowed(_) => {
            // 请求被允许
        }
        Decision::Rejected(_) | Decision::Banned(_) => {
            panic!("Expected request to be allowed");
        }
    }
}

/// 测试限流触发后的拒绝
#[tokio::test]
async fn test_rate_limit_rejection() {
    // 创建容量很小的限流器配置
    let governor = create_governor_with_limiters(vec![LimiterConfig::TokenBucket {
        capacity: 2,
        refill_rate: 1,
    }])
    .await;

    let ctx = RequestContextBuilder::new()
        .user_id("limited_user")
        .ip("10.0.0.1")
        .build();

    // 前两次请求应该被允许
    let result1 = governor.check(&ctx).await.unwrap();
    let result2 = governor.check(&ctx).await.unwrap();

    assert!(matches!(result1, Decision::Allowed(_)));
    assert!(matches!(result2, Decision::Allowed(_)));

    // 第三次请求应该被拒绝
    let result3 = governor.check(&ctx).await.unwrap();
    assert!(
        matches!(result3, Decision::Rejected(_)),
        "Third request should be rejected"
    );
}

/// 测试多限流器协作
#[tokio::test]
async fn test_multiple_limiters() {
    let governor = create_governor_with_limiters(vec![
        LimiterConfig::TokenBucket {
            capacity: 100,
            refill_rate: 10,
        },
        LimiterConfig::SlidingWindow {
            window_size: "1s".to_string(),
            max_requests: 2,
        },
    ])
    .await;

    let ctx = RequestContextBuilder::new()
        .user_id("multi_limiter_user")
        .ip("10.0.0.2")
        .build();

    // 前两次请求应该被允许
    let result1 = governor.check(&ctx).await.unwrap();
    let result2 = governor.check(&ctx).await.unwrap();
    assert!(matches!(result1, Decision::Allowed(_)));
    assert!(matches!(result2, Decision::Allowed(_)));

    // 第三次请求应该被拒绝（SlidingWindow 限制）
    let result3 = governor.check(&ctx).await.unwrap();
    assert!(
        matches!(result3, Decision::Rejected(_)),
        "Third request should be rejected by sliding window"
    );
}

/// 测试不同用户独立限流
#[tokio::test]
async fn test_independent_user_rate_limiting() {
    let governor = create_governor_with_limiters(vec![LimiterConfig::TokenBucket {
        capacity: 5,
        refill_rate: 1,
    }])
    .await;

    // 使用不同的用户ID，每个用户独立限流
    for i in 0..3 {
        let ctx = RequestContextBuilder::new()
            .user_id(&format!("independent_user_{}", i))
            .ip("10.0.0.3")
            .build();

        let result = governor.check(&ctx).await.unwrap();
        assert!(
            matches!(result, Decision::Allowed(_)),
            "Request {} should be allowed",
            i
        );
    }
}

/// 测试并发请求限流
#[tokio::test]
async fn test_concurrent_rate_limiting() {
    let governor = create_governor_with_limiters(vec![LimiterConfig::TokenBucket {
        capacity: 50,
        refill_rate: 10,
    }])
    .await;

    let mut handles = vec![];

    // 并发发送 30 个请求
    for i in 0..30 {
        let gov_clone = Arc::clone(&governor);
        handles.push(tokio::spawn(async move {
            let ctx = RequestContextBuilder::new()
                .user_id(&format!("concurrent_user_{}", i % 10))
                .ip(&format!("10.0.0.{}", i % 10))
                .build();

            gov_clone.check(&ctx).await
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles).await;

    let mut allowed = 0;

    for result in results {
        if let Ok(Ok(decision)) = result {
            match decision {
                Decision::Allowed(_) => allowed += 1,
                Decision::Rejected(_) | Decision::Banned(_) => {}
            }
        }
    }

    // 验证有请求被允许
    assert!(allowed > 0, "Some requests should be allowed");
}

/// 测试 TokenBucket 限流器集成
#[tokio::test]
async fn test_token_bucket_integration() {
    let governor = create_governor_with_limiters(vec![LimiterConfig::TokenBucket {
        capacity: 10,
        refill_rate: 2,
    }])
    .await;

    let ctx = RequestContextBuilder::new()
        .user_id("token_bucket_user")
        .ip("10.0.0.10")
        .build();

    // 消耗所有令牌
    for i in 0..10 {
        let result = governor.check(&ctx).await.unwrap();
        assert!(
            matches!(result, Decision::Allowed(_)),
            "Request {} should be allowed",
            i
        );
    }

    // 第11次请求应该被拒绝
    let result = governor.check(&ctx).await.unwrap();
    assert!(
        matches!(result, Decision::Rejected(_)),
        "Request 11 should be rejected"
    );
}

/// 测试 SlidingWindow 限流器集成
#[tokio::test]
async fn test_sliding_window_integration() {
    let governor = create_governor_with_limiters(vec![LimiterConfig::SlidingWindow {
        window_size: "1s".to_string(),
        max_requests: 5,
    }])
    .await;

    let ctx = RequestContextBuilder::new()
        .user_id("sliding_window_user")
        .ip("10.0.0.11")
        .build();

    // 在窗口内发送 5 个请求
    for i in 0..5 {
        let result = governor.check(&ctx).await.unwrap();
        assert!(
            matches!(result, Decision::Allowed(_)),
            "Request {} should be allowed",
            i
        );
    }

    // 第6个请求应该被拒绝
    let result = governor.check(&ctx).await.unwrap();
    assert!(
        matches!(result, Decision::Rejected(_)),
        "Request 6 should be rejected"
    );
}

/// 测试 FixedWindow 限流器集成
#[tokio::test]
async fn test_fixed_window_integration() {
    let governor = create_governor_with_limiters(vec![LimiterConfig::FixedWindow {
        window_size: "1s".to_string(),
        max_requests: 3,
    }])
    .await;

    let ctx = RequestContextBuilder::new()
        .user_id("fixed_window_user")
        .ip("10.0.0.12")
        .build();

    // 在窗口内发送 3 个请求
    for i in 0..3 {
        let result = governor.check(&ctx).await.unwrap();
        assert!(
            matches!(result, Decision::Allowed(_)),
            "Request {} should be allowed",
            i
        );
    }

    // 第4个请求应该被拒绝
    let result = governor.check(&ctx).await.unwrap();
    assert!(
        matches!(result, Decision::Rejected(_)),
        "Request 4 should be rejected"
    );
}

/// 测试统计信息更新
#[tokio::test]
async fn test_stats_update() {
    let governor = create_governor_with_limiters(vec![LimiterConfig::TokenBucket {
        capacity: 5,
        refill_rate: 1,
    }])
    .await;

    // 发送多个请求
    for i in 0..10 {
        let ctx = RequestContextBuilder::new()
            .user_id(&format!("stats_user_{}", i % 3))
            .ip(&format!("10.0.0.{}", i % 3))
            .build();

        let _ = governor.check(&ctx).await;
    }

    // 验证统计信息
    let stats = governor.stats().await;
    assert_eq!(stats.total_requests, 10);
    assert!(stats.allowed_requests > 0);
}

/// 测试健康检查
#[tokio::test]
async fn test_health_check() {
    let governor = create_governor().await;

    // 执行健康检查
    let result = governor.health_check().await;
    assert!(result.is_ok(), "Health check should succeed");
}
