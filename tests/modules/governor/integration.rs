//! 控制器模块集成测试
//!
//! 测试控制器与其他组件的集成

use limiteron::config::{FlowControlConfig, LimiterConfig, Matcher as ConfigMatcher, Rule};
use limiteron::governor::Governor;
use limiteron::matchers::RequestContext;
use limiteron::storage::MemoryStorage;
use std::sync::Arc;

/// 测试控制器与存储的集成
#[tokio::test]
async fn test_governor_with_storage() {
    let config = FlowControlConfig::default();
    let storage = Arc::new(MemoryStorage::new());
    let ban_storage = Arc::new(MemoryStorage::new());

    let governor = Governor::new(
        config,
        storage,
        ban_storage,
        #[cfg(feature = "monitoring")]
        None,
        #[cfg(feature = "telemetry")]
        None,
    )
    .await
    .unwrap();

    // 创建请求上下文
    let ctx = RequestContext::new();

    // 检查请求
    let decision = governor.check(&ctx).await.unwrap();
    assert!(matches!(decision, limiteron::error::Decision::Allowed(_)));
}

/// 测试控制器的决策链
#[tokio::test]
async fn test_governor_decision_chain() {
    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
        },
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![ConfigMatcher::User {
                user_ids: vec!["test_user".to_string()],
            }],
            limiters: vec![LimiterConfig::SlidingWindow {
                window_size: "1s".to_string(),
                max_requests: 100,
            }],
            action: limiteron::config::ActionConfig {
                on_exceed: "reject".to_string(),
                ban: None,
            },
        }],
    };

    let storage = Arc::new(MemoryStorage::new());
    let ban_storage = Arc::new(MemoryStorage::new());

    let governor = Governor::new(
        config,
        storage,
        ban_storage,
        #[cfg(feature = "monitoring")]
        None,
        #[cfg(feature = "telemetry")]
        None,
    )
    .await
    .unwrap();

    // 创建匹配的请求上下文
    let mut ctx = RequestContext::new();
    ctx.user_id = Some("test_user".to_string());

    // 检查请求
    let decision = governor.check(&ctx).await.unwrap();
    assert!(matches!(decision, limiteron::error::Decision::Allowed(_)));
}

/// 测试控制器的并发请求
#[tokio::test]
async fn test_governor_concurrent_requests() {
    let config = FlowControlConfig::default();
    let storage = Arc::new(MemoryStorage::new());
    let ban_storage = Arc::new(MemoryStorage::new());

    let governor = Arc::new(
        Governor::new(
            config,
            storage,
            ban_storage,
            #[cfg(feature = "monitoring")]
            None,
            #[cfg(feature = "telemetry")]
            None,
        )
        .await
        .unwrap(),
    );

    let mut handles = vec![];

    // 10个并发请求
    for _ in 0..10 {
        let governor_clone = governor.clone();
        handles.push(tokio::spawn(async move {
            let ctx = RequestContext::new();
            governor_clone.check(&ctx).await
        }));
    }

    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }
}
