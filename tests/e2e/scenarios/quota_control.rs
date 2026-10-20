// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 配额消费场景测试
//!
//! 测试配额正确追踪和告警正确触发的完整流程

#[cfg(feature = "quota-control")]
use ahash::AHashMap;
#[cfg(feature = "quota-control")]
use limiteron::chrono::{Duration, Utc};
#[cfg(feature = "quota-control")]
use limiteron::error::StorageError;
#[cfg(feature = "quota-control")]
use limiteron::quota::{AlertChannel, AlertConfig, QuotaConfig, QuotaController, QuotaType};
#[cfg(feature = "quota-control")]
use limiteron::{QuotaInfo, QuotaStorage};
#[cfg(feature = "quota-control")]
use std::sync::Arc;
#[cfg(feature = "quota-control")]
use std::time::Duration as StdDuration;

// 存储（产品内存实现）

#[cfg(feature = "quota-control")]
#[derive(Clone)]
struct TestQuotaStorage {
    quotas: Arc<tokio::sync::RwLock<AHashMap<String, QuotaInfo>>>,
}

#[cfg(feature = "quota-control")]
impl TestQuotaStorage {
    fn new() -> Self {
        Self {
            quotas: Arc::new(tokio::sync::RwLock::new(AHashMap::new())),
        }
    }
}

#[cfg(feature = "quota-control")]
#[async_trait::async_trait]
impl QuotaStorage for TestQuotaStorage {
    async fn get_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError> {
        let key = format!("{}:{}", user_id, resource);
        let quotas = self.quotas.read().await;
        Ok(quotas.get(&key).cloned())
    }

    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
        limit: u64,
        window: StdDuration,
    ) -> Result<limiteron::error::ConsumeResult, StorageError> {
        let key = format!("{}:{}", user_id, resource);
        let mut quotas = self.quotas.write().await;

        let quota_info = quotas.entry(key.clone()).or_insert_with(|| {
            let now = Utc::now();
            QuotaInfo {
                consumed: 0,
                limit,
                window_start: now,
                window_end: now + Duration::from_std(window).unwrap_or(Duration::seconds(3600)),
            }
        });

        let now = Utc::now();
        if now >= quota_info.window_end {
            // 窗口已过期，重置消费量
            quota_info.consumed = 0;
            quota_info.window_start = now;
            quota_info.window_end =
                now + Duration::from_std(window).unwrap_or(Duration::seconds(3600));
            quota_info.limit = limit;
        }

        if quota_info.consumed + cost > quota_info.limit {
            let usage_percent = if limit > 0 {
                ((quota_info.consumed + cost) as f64 / limit as f64) * 100.0
            } else {
                100.0
            };
            return Ok(limiteron::error::ConsumeResult {
                allowed: false,
                remaining: quota_info.limit.saturating_sub(quota_info.consumed),
                alert_triggered: false,
                usage_percent,
            });
        }

        quota_info.consumed += cost;

        let usage_percent = if limit > 0 {
            (quota_info.consumed as f64 / limit as f64) * 100.0
        } else {
            0.0
        };

        Ok(limiteron::error::ConsumeResult {
            allowed: true,
            remaining: quota_info.limit.saturating_sub(quota_info.consumed),
            alert_triggered: false,
            usage_percent,
        })
    }

    async fn reset(
        &self,
        user_id: &str,
        resource: &str,
        limit: u64,
        window: StdDuration,
    ) -> Result<(), StorageError> {
        let key = format!("{}:{}", user_id, resource);
        let mut quotas = self.quotas.write().await;

        let now = Utc::now();
        quotas.insert(
            key,
            QuotaInfo {
                consumed: 0,
                limit,
                window_start: now,
                window_end: now + Duration::from_std(window).unwrap_or(Duration::seconds(3600)),
            },
        );

        Ok(())
    }
}

// E2E Scenario Tests

/// 场景 1: 配额正确追踪
///
/// 用户消费配额后，系统正确追踪剩余配额。
#[tokio::test]
#[cfg(feature = "quota-control")]
async fn e2e_quota_tracking_correct() {
    let storage: Arc<dyn QuotaStorage> = Arc::new(TestQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 100,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            ..Default::default()
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);

    // 消费 30 个配额
    let result = controller.consume("user1", "api_calls", 30).await.unwrap();
    assert!(result.allowed, "first consumption should be allowed");
    assert_eq!(result.remaining, 70, "remaining quota should be 70");

    // 消费 20 个配额
    let result = controller.consume("user1", "api_calls", 20).await.unwrap();
    assert!(result.allowed, "second consumption should be allowed");
    assert_eq!(result.remaining, 50, "remaining quota should be 50");

    // 消费 50 个配额
    let result = controller.consume("user1", "api_calls", 50).await.unwrap();
    assert!(result.allowed, "third consumption should be allowed");
    assert_eq!(result.remaining, 0, "remaining quota should be 0");
}

/// 场景 2: 配额耗尽拒绝
///
/// 配额耗尽后，后续消费请求被拒绝。
#[tokio::test]
#[cfg(feature = "quota-control")]
async fn e2e_quota_exhaustion_denied() {
    let storage: Arc<dyn QuotaStorage> = Arc::new(TestQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 50,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            ..Default::default()
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);

    // 消费全部配额
    let result = controller.consume("user1", "api_calls", 50).await.unwrap();
    assert!(
        result.allowed,
        "consuming the entire quota should be allowed"
    );
    assert_eq!(result.remaining, 0, "remaining quota should be 0");

    let result = controller.consume("user1", "api_calls", 1).await.unwrap();
    assert!(
        !result.allowed,
        "should be rejected after quota is exhausted"
    );
    assert_eq!(result.remaining, 0, "remaining quota should be 0");
}

/// 场景 3: 告警正确触发
///
/// 配额使用达到告警阈值时，系统正确触发告警。
#[tokio::test]
#[cfg(feature = "quota-control")]
async fn e2e_quota_alert_triggered() {
    let storage: Arc<dyn QuotaStorage> = Arc::new(TestQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 100,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: true,
            thresholds: vec![80, 90, 100],
            channels: vec![AlertChannel::Log],
            dedup_window: 300,
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);

    // 消费 80 个配额，应该触发 80% 告警
    let result = controller.consume("user1", "api_calls", 80).await.unwrap();
    assert!(result.allowed, "consumption should be allowed");
    assert!(
        result.alert_triggered,
        "reaching the 80% threshold should trigger an alert"
    );

    // 继续消费 10 个，达到 90%
    let result = controller.consume("user1", "api_calls", 10).await.unwrap();
    assert!(result.allowed, "consumption should be allowed");
    // 由于去重窗口，可能不会触发新告警

    // 继续消费 10 个，达到 100%
    let result = controller.consume("user1", "api_calls", 10).await.unwrap();
    assert!(result.allowed, "consumption should be allowed");
}

/// 场景 4: 不同用户配额独立
///
/// 不同用户的配额是独立的，互不影响。
#[tokio::test]
#[cfg(feature = "quota-control")]
async fn e2e_quota_independent_per_user() {
    let storage: Arc<dyn QuotaStorage> = Arc::new(TestQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 50,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            ..Default::default()
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);

    // 用户 A 消费全部配额
    let result = controller.consume("user_a", "api_calls", 50).await.unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 0);

    // 用户 A 再消费应该被拒绝
    let result = controller.consume("user_a", "api_calls", 1).await.unwrap();
    assert!(
        !result.allowed,
        "user A should be rejected after quota exhaustion"
    );

    // 用户 B 应该可以正常消费
    let result = controller.consume("user_b", "api_calls", 30).await.unwrap();
    assert!(result.allowed, "user B should be able to consume");
    assert_eq!(result.remaining, 20, "user B remaining quota should be 20");
}

/// 场景 5: 配额重置
///
/// 管理员可以手动重置用户配额。
#[tokio::test]
#[cfg(feature = "quota-control")]
async fn e2e_quota_reset() {
    let storage: Arc<dyn QuotaStorage> = Arc::new(TestQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 100,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            ..Default::default()
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);

    // 消费全部配额
    let result = controller.consume("user1", "api_calls", 100).await.unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 0);

    controller.reset_quota("user1", "api_calls").await.unwrap();

    let result = controller.consume("user1", "api_calls", 50).await.unwrap();
    assert!(result.allowed, "should be able to consume after reset");
    assert_eq!(
        result.remaining, 50,
        "remaining quota after reset should be 50"
    );
}

/// 场景 6: 透支功能
///
/// 启用透支后，用户可以超过配额限制一定比例。
#[tokio::test]
#[cfg(feature = "quota-control")]
async fn e2e_quota_overdraft() {
    let storage: Arc<dyn QuotaStorage> = Arc::new(TestQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 100,
        window_size: 3600,
        allow_overdraft: true,
        overdraft_limit_percent: 20, // 20% 透支
        alert_config: AlertConfig {
            enabled: false,
            ..Default::default()
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);

    // 消费到原始上限
    let result = controller.consume("user1", "api_calls", 100).await.unwrap();
    assert!(
        result.allowed,
        "consuming up to the limit should be allowed"
    );
    // 剩余包含透支额度: 120 - 100 = 20
    assert_eq!(
        result.remaining, 20,
        "remaining should include overdraft allowance"
    );

    // 消费透支额度
    let result = controller.consume("user1", "api_calls", 15).await.unwrap();
    assert!(
        result.allowed,
        "should be allowed within the overdraft allowance"
    );
    assert_eq!(result.remaining, 5, "remaining overdraft should be 5");

    // 超过透支上限
    let result = controller.consume("user1", "api_calls", 10).await.unwrap();
    assert!(
        !result.allowed,
        "should be rejected beyond the overdraft limit"
    );
}

/// 场景 7: 不同资源独立配额
///
/// 同一用户的不同资源配额是独立的。
#[tokio::test]
#[cfg(feature = "quota-control")]
async fn e2e_quota_independent_per_resource() {
    let storage: Arc<dyn QuotaStorage> = Arc::new(TestQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 50,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            ..Default::default()
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);

    // 消费资源 A 的全部配额
    let result = controller.consume("user1", "resource_a", 50).await.unwrap();
    assert!(result.allowed);
    assert_eq!(result.remaining, 0);

    // 资源 A 再消费应该被拒绝
    let result = controller.consume("user1", "resource_a", 1).await.unwrap();
    assert!(
        !result.allowed,
        "resource A should be rejected after quota exhaustion"
    );

    // 资源 B 应该可以正常消费
    let result = controller.consume("user1", "resource_b", 30).await.unwrap();
    assert!(result.allowed, "resource B should be able to consume");
    assert_eq!(
        result.remaining, 20,
        "resource B remaining quota should be 20"
    );
}

/// 场景 8: 并发消费安全性
///
/// 高并发场景下配额消费正确追踪。
#[tokio::test]
#[cfg(feature = "quota-control")]
async fn e2e_quota_concurrent_safety() {
    let storage: Arc<dyn QuotaStorage> = Arc::new(TestQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 100,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            ..Default::default()
        },
    };
    let controller = Arc::new(QuotaController::with_dependencies(storage, config));

    let mut handles = vec![];

    // 创建 20 个并发任务，每个消费 10 个配额
    for _ in 0..20 {
        let controller_clone = Arc::clone(&controller);
        handles.push(tokio::spawn(async move {
            controller_clone.consume("user1", "api_calls", 10).await
        }));
    }

    let mut allowed_count = 0;
    let mut denied_count = 0;
    let mut total_consumed = 0u64;

    for handle in handles {
        let result = handle.await.unwrap().unwrap();
        if result.allowed {
            allowed_count += 1;
            total_consumed += 10;
        } else {
            denied_count += 1;
        }
    }

    // 总消费量不应该超过限制
    assert!(
        total_consumed <= 100,
        "total consumption {} should not exceed the limit of 100",
        total_consumed
    );

    // 应该有部分请求被拒绝
    assert!(denied_count > 0, "some requests should be denied");

    // 允许的请求数 * 10 应该等于总消费量
    assert_eq!(
        allowed_count * 10,
        total_consumed as usize,
        "allowed request count should match total consumption"
    );
}

/// 场景 9: 使用率计算正确
///
/// 配额使用率计算正确。
#[tokio::test]
#[cfg(feature = "quota-control")]
async fn e2e_quota_usage_percent() {
    let storage: Arc<dyn QuotaStorage> = Arc::new(TestQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 200,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            ..Default::default()
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);

    // 消费 50 个，使用率应该是 25%
    let result = controller.consume("user1", "api_calls", 50).await.unwrap();
    assert!(result.allowed);
    assert!(
        (result.usage_percent - 25.0).abs() < 0.1,
        "usage should be 25%, actual {}%",
        result.usage_percent
    );

    // 消费 50 个，总共 100 个，使用率应该是 50%
    let result = controller.consume("user1", "api_calls", 50).await.unwrap();
    assert!(result.allowed);
    assert!(
        (result.usage_percent - 50.0).abs() < 0.1,
        "usage should be 50%, actual {}%",
        result.usage_percent
    );

    // 消费 100 个，总共 200 个，使用率应该是 100%
    let result = controller.consume("user1", "api_calls", 100).await.unwrap();
    assert!(result.allowed);
    assert!(
        (result.usage_percent - 100.0).abs() < 0.1,
        "usage should be 100%, actual {}%",
        result.usage_percent
    );
}

/// 场景 10: 不同配额类型
///
/// 系统支持不同类型的配额（Token、Money、Count）。
#[tokio::test]
#[cfg(feature = "quota-control")]
async fn e2e_quota_different_types() {
    // Token 类型
    let storage: Arc<dyn QuotaStorage> = Arc::new(TestQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Token,
        limit: 1000,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            ..Default::default()
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);
    let result = controller.consume("user1", "tokens", 100).await.unwrap();
    assert!(result.allowed, "Token-type consumption should be allowed");

    // Money 类型
    let storage: Arc<dyn QuotaStorage> = Arc::new(TestQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Money,
        limit: 10000, // 100.00 元，以分为单位
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            ..Default::default()
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);
    let result = controller.consume("user2", "payments", 500).await.unwrap();
    assert!(result.allowed, "Money-type consumption should be allowed");

    // Count 类型
    let storage: Arc<dyn QuotaStorage> = Arc::new(TestQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 100,
        window_size: 3600,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            ..Default::default()
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);
    let result = controller.consume("user3", "requests", 10).await.unwrap();
    assert!(result.allowed, "Count-type consumption should be allowed");
}
