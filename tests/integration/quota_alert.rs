// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! QuotaController + Alert 集成测试
//!
//! 测试配额控制器与告警系统的集成，验证配额告警联动。

#[cfg(feature = "quota-control")]
mod quota_control_tests {
    use crate::common::MockQuotaStorage;

    use limiteron::quota::{AlertChannel, AlertConfig, QuotaConfig, QuotaController, QuotaType};
    use std::sync::Arc;
    use std::time::Duration;

    // ==================== 辅助函数 ====================

    /// 创建测试用的 QuotaController
    fn create_quota_controller(limit: u64, window_size: u64) -> QuotaController {
        let storage = Arc::new(MockQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit,
            window_size,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: true,
                thresholds: vec![80, 90, 100],
                channels: vec![AlertChannel::Log],
                dedup_window: 5,
            },
        };
        QuotaController::with_dependencies(storage, config)
    }

    /// 创建带自定义告警配置的 QuotaController
    fn create_quota_controller_with_alerts(
        limit: u64,
        thresholds: Vec<u8>,
        dedup_window: u64,
    ) -> QuotaController {
        let storage = Arc::new(MockQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: true,
                thresholds,
                channels: vec![AlertChannel::Log],
                dedup_window,
            },
        };
        QuotaController::with_dependencies(storage, config)
    }

    /// 创建禁用告警的 QuotaController
    fn create_quota_controller_no_alerts(limit: u64) -> QuotaController {
        let storage = Arc::new(MockQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                thresholds: vec![],
                channels: vec![],
                dedup_window: 0,
            },
        };
        QuotaController::with_dependencies(storage, config)
    }

    // ==================== 配额告警联动验证 ====================

    /// 测试达到 80% 阈值触发告警
    #[tokio::test]
    async fn test_alert_triggered_at_80_percent() {
        let controller = create_quota_controller(100, 3600);

        // 消费 80 个配额（80%）
        let result = controller.consume("user1", "resource1", 80).await.unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered, "达到 80% 阈值应该触发告警");
        assert!((result.usage_percent - 80.0).abs() < 0.1);
    }

    /// 测试达到 90% 阈值触发告警
    #[tokio::test]
    async fn test_alert_triggered_at_90_percent() {
        let controller = create_quota_controller(100, 3600);

        // 先消费 80 个
        let _ = controller.consume("user1", "resource1", 80).await.unwrap();

        // 再消费 10 个（达到 90%）
        let result = controller.consume("user1", "resource1", 10).await.unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered, "达到 90% 阈值应该触发告警");
        assert!((result.usage_percent - 90.0).abs() < 0.1);
    }

    /// 测试达到 100% 阈值触发告警
    #[tokio::test]
    async fn test_alert_triggered_at_100_percent() {
        let controller = create_quota_controller(100, 3600);

        // 消费全部配额
        let result = controller.consume("user1", "resource1", 100).await.unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered, "达到 100% 阈值应该触发告警");
        assert!((result.usage_percent - 100.0).abs() < 0.1);
    }

    /// 测试未达到阈值不触发告警
    #[tokio::test]
    async fn test_no_alert_below_threshold() {
        let controller = create_quota_controller(100, 3600);

        // 消费 79 个配额（79%，低于 80% 阈值）
        let result = controller.consume("user1", "resource1", 79).await.unwrap();
        assert!(result.allowed);
        assert!(!result.alert_triggered, "低于阈值不应该触发告警");
    }

    /// 测试告警去重 - 同一阈值不重复触发
    #[tokio::test]
    async fn test_alert_deduplication_same_threshold() {
        let controller = create_quota_controller_with_alerts(100, vec![80], 300);

        // 第一次达到 80%，触发告警
        let result = controller.consume("user1", "resource1", 80).await.unwrap();
        assert!(result.alert_triggered, "首次达到阈值应该触发告警");

        // 继续消费到 85%，不应该再次触发（去重）
        let result = controller.consume("user1", "resource1", 5).await.unwrap();
        assert!(!result.alert_triggered, "同一去重窗口内不应该重复触发告警");

        // 继续消费到 90%，不应该再次触发（去重）
        let result = controller.consume("user1", "resource1", 5).await.unwrap();
        assert!(!result.alert_triggered, "同一去重窗口内不应该重复触发告警");
    }

    /// 测试告警去重窗口过期后重新触发
    #[tokio::test]
    async fn test_alert_dedup_window_expiry() {
        let controller = create_quota_controller_with_alerts(100, vec![80], 1); // 1 秒去重窗口

        // 第一次达到 80%，触发告警
        let result = controller.consume("user1", "resource1", 80).await.unwrap();
        assert!(result.alert_triggered);

        // 等待去重窗口过期
        tokio::time::sleep(Duration::from_millis(1100)).await;

        // 清理过期的去重记录
        controller.cleanup_alert_dedup();

        // 再次消费，应该触发告警
        let result = controller.consume("user1", "resource1", 5).await.unwrap();
        assert!(result.alert_triggered, "去重窗口过期后应该重新触发告警");
    }

    /// 测试多级告警阈值
    #[tokio::test]
    async fn test_multi_level_alert_thresholds() {
        let controller = create_quota_controller_with_alerts(100, vec![50, 75, 90, 100], 1);

        // 50% 告警
        let result = controller.consume("user1", "resource1", 50).await.unwrap();
        assert!(result.alert_triggered, "达到 50% 应该触发告警");

        tokio::time::sleep(Duration::from_millis(1100)).await;
        controller.cleanup_alert_dedup();

        // 75% 告警
        let result = controller.consume("user1", "resource1", 25).await.unwrap();
        assert!(result.alert_triggered, "达到 75% 应该触发告警");

        tokio::time::sleep(Duration::from_millis(1100)).await;
        controller.cleanup_alert_dedup();

        // 90% 告警
        let result = controller.consume("user1", "resource1", 15).await.unwrap();
        assert!(result.alert_triggered, "达到 90% 应该触发告警");

        tokio::time::sleep(Duration::from_millis(1100)).await;
        controller.cleanup_alert_dedup();

        // 100% 告警
        let result = controller.consume("user1", "resource1", 10).await.unwrap();
        assert!(result.alert_triggered, "达到 100% 应该触发告警");
    }

    /// 测试告警禁用
    #[tokio::test]
    async fn test_alert_disabled() {
        let controller = create_quota_controller_no_alerts(100);

        // 消费全部配额
        let result = controller.consume("user1", "resource1", 100).await.unwrap();
        assert!(result.allowed);
        assert!(!result.alert_triggered, "告警禁用时不应触发告警");
    }

    /// 测试不同用户独立告警
    #[tokio::test]
    async fn test_independent_user_alerts() {
        let controller = create_quota_controller(100, 3600);

        // 用户1 达到 80%
        let result = controller.consume("user1", "resource1", 80).await.unwrap();
        assert!(result.alert_triggered);

        // 用户2 达到 80%（独立触发）
        let result = controller.consume("user2", "resource1", 80).await.unwrap();
        assert!(result.alert_triggered, "不同用户应该独立触发告警");
    }

    /// 测试不同资源独立告警
    #[tokio::test]
    async fn test_independent_resource_alerts() {
        let controller = create_quota_controller(100, 3600);

        // 资源1 达到 80%
        let result = controller.consume("user1", "resource1", 80).await.unwrap();
        assert!(result.alert_triggered);

        // 资源2 达到 80%（独立触发）
        let result = controller.consume("user1", "resource2", 80).await.unwrap();
        assert!(result.alert_triggered, "不同资源应该独立触发告警");
    }

    /// 测试配额耗尽后的告警状态
    #[tokio::test]
    async fn test_alert_when_quota_exhausted() {
        let controller = create_quota_controller(100, 3600);

        // 消费全部配额
        let result = controller.consume("user1", "resource1", 100).await.unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered);

        // 尝试再消费，应该被拒绝
        let result = controller.consume("user1", "resource1", 1).await.unwrap();
        assert!(!result.allowed);
        // 拒绝时不应触发告警（因为已经达到 100%）
        assert!(!result.alert_triggered);
    }

    /// 测试配额重置后告警状态
    #[tokio::test]
    async fn test_alert_after_quota_reset() {
        let controller = create_quota_controller(100, 3600);

        // 消费全部配额
        let _ = controller.consume("user1", "resource1", 100).await.unwrap();

        // 重置配额
        controller.reset_quota("user1", "resource1").await.unwrap();

        // 重新消费，应该重新触发告警
        let result = controller.consume("user1", "resource1", 80).await.unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered, "重置后应该重新触发告警");
    }

    /// 测试透支模式下的告警
    #[tokio::test]
    async fn test_alert_with_overdraft() {
        let storage = Arc::new(MockQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: true,
            overdraft_limit_percent: 20, // 20% 透支
            alert_config: AlertConfig {
                enabled: true,
                thresholds: vec![80, 100, 120],
                channels: vec![AlertChannel::Log],
                dedup_window: 5,
            },
        };
        let controller = QuotaController::with_dependencies(storage, config);

        // 消费到 80%（原始限制）
        let result = controller.consume("user1", "resource1", 80).await.unwrap();
        assert!(result.alert_triggered);

        // 消费到 100%
        let result = controller.consume("user1", "resource1", 20).await.unwrap();
        assert!(result.alert_triggered);

        // 消费到 120%（透支上限）
        let result = controller.consume("user1", "resource1", 20).await.unwrap();
        assert!(result.allowed);
        // 透支时也应该触发告警
    }

    /// 测试使用率计算正确性
    #[tokio::test]
    async fn test_usage_percent_accuracy() {
        let controller = create_quota_controller(200, 3600);

        // 测试不同消费量的使用率
        let test_cases = vec![(50, 25.0), (100, 50.0), (150, 75.0), (200, 100.0)];

        for (cost, expected_percent) in test_cases {
            // 重置配额
            controller.reset_quota("user1", "resource1").await.unwrap();

            let result = controller
                .consume("user1", "resource1", cost)
                .await
                .unwrap();
            assert!(
                (result.usage_percent - expected_percent).abs() < 0.1,
                "消费 {} 后使用率应为 {}%，实际为 {}%",
                cost,
                expected_percent,
                result.usage_percent
            );
        }
    }

    /// 测试并发消费时的告警触发
    #[tokio::test]
    async fn test_concurrent_consume_alerts() {
        let controller = Arc::new(create_quota_controller(1000, 3600));
        let mut handles: Vec<tokio::task::JoinHandle<_>> = vec![];

        // 并发消费
        for _ in 0..10 {
            let ctrl = Arc::clone(&controller);
            handles.push(tokio::spawn(async move {
                ctrl.consume("user1", "resource1", 80).await
            }));
        }

        let results: Vec<_> = futures::future::join_all(handles).await;

        // 统计触发告警的次数
        let alert_count = results
            .iter()
            .filter(|r| {
                r.is_ok()
                    && r.as_ref().unwrap().is_ok()
                    && r.as_ref().unwrap().as_ref().unwrap().alert_triggered
            })
            .count();

        // 第一个达到阈值的请求应该触发告警
        assert!(alert_count >= 1, "至少应该有一个请求触发告警");
    }

    /// 测试零消费不触发告警
    #[tokio::test]
    async fn test_zero_cost_no_alert() {
        let controller = create_quota_controller(100, 3600);

        // 零消费
        let result = controller.consume("user1", "resource1", 0).await.unwrap();
        assert!(result.allowed);
        assert!(!result.alert_triggered, "零消费不应触发告警");
        assert_eq!(result.usage_percent, 0.0);
    }

    /// 测试大配额限制的告警
    #[tokio::test]
    async fn test_large_quota_alert() {
        let controller = create_quota_controller_with_alerts(10000, vec![80], 300);

        // 消费 80%
        let result = controller
            .consume("user1", "resource1", 8000)
            .await
            .unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered);
        assert!((result.usage_percent - 80.0).abs() < 0.1);
    }

    /// 测试告警阈值边界
    #[tokio::test]
    async fn test_alert_threshold_boundary() {
        let controller = create_quota_controller_with_alerts(100, vec![80], 300);

        // 消费 79 个（79%，低于阈值）
        let result = controller.consume("user1", "resource1", 79).await.unwrap();
        assert!(!result.alert_triggered, "79% 不应触发 80% 阈值告警");

        // 再消费 1 个（达到 80%）
        let result = controller.consume("user1", "resource1", 1).await.unwrap();
        assert!(result.alert_triggered, "80% 应该触发告警");
    }

    /// 测试多阈值同时触发
    #[tokio::test]
    async fn test_multiple_thresholds_triggered() {
        let controller = create_quota_controller_with_alerts(100, vec![50, 60, 70, 80, 90, 100], 0);

        // 一次性消费 100%，应该触发所有阈值
        let result = controller.consume("user1", "resource1", 100).await.unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered, "应该触发告警");
    }

    /// 测试不同配额类型的告警
    #[tokio::test]
    async fn test_different_quota_types_alerts() {
        // Token 类型
        let storage = Arc::new(MockQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Token,
            limit: 1000,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: true,
                thresholds: vec![80],
                channels: vec![AlertChannel::Log],
                dedup_window: 5,
            },
        };
        let controller = QuotaController::with_dependencies(storage, config);
        let result = controller.consume("user1", "api", 800).await.unwrap();
        assert!(result.alert_triggered);

        // Money 类型
        let storage = Arc::new(MockQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Money,
            limit: 10000,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: true,
                thresholds: vec![80],
                channels: vec![AlertChannel::Log],
                dedup_window: 5,
            },
        };
        let controller = QuotaController::with_dependencies(storage, config);
        let result = controller.consume("user2", "payment", 8000).await.unwrap();
        assert!(result.alert_triggered);
    }
}

// 当没有启用 quota-control 特性时，提供一个空的测试模块
#[cfg(not(feature = "quota-control"))]
mod quota_control_tests {
    #[test]
    fn test_quota_control_feature_not_enabled() {
        // 当特性未启用时，测试通过
        println!("quota-control 特性未启用，跳过测试");
    }
}
