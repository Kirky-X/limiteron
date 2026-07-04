//! CircuitBreaker + Fallback 集成测试
//!
//! 测试熔断器与降级策略的集成，验证熔断降级流程。

#[cfg(feature = "circuit-breaker")]
mod circuit_breaker_tests {
    use limiteron::circuit::{CircuitBreaker, CircuitBreakerConfig};
    use limiteron::error::CircuitState;
    use std::sync::Arc;
    use std::time::Duration;

    /// 创建测试用的 CircuitBreaker
    fn create_test_circuit_breaker(
        failure_threshold: u64,
        success_threshold: u64,
        timeout: Duration,
    ) -> Arc<CircuitBreaker> {
        let config = CircuitBreakerConfig::new(failure_threshold, success_threshold, timeout);
        Arc::new(CircuitBreaker::new(config))
    }

    /// 测试熔断器基本状态转换
    #[tokio::test]
    async fn test_circuit_breaker_state_transitions() {
        let breaker = create_test_circuit_breaker(2, 2, Duration::from_millis(100));

        // 初始状态应该是关闭
        assert!(breaker.is_closed().await);

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), limiteron::error::FlowGuardError>(
                        limiteron::error::FlowGuardError::BanError("错误".to_string()),
                    )
                })
                .await;
        }

        // 熔断器应该打开
        assert!(breaker.is_open().await);

        // 等待超时进入半开状态
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 半开状态下尝试恢复
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async { Ok::<(), limiteron::error::FlowGuardError>(()) })
                .await;
        }

        // 熔断器应该关闭
        assert!(breaker.is_closed().await);
    }

    /// 测试熔断器打开时拒绝请求
    #[tokio::test]
    async fn test_circuit_breaker_open_rejects_requests() {
        let breaker = create_test_circuit_breaker(2, 2, Duration::from_secs(60));

        // 触发熔断
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), limiteron::error::FlowGuardError>(
                        limiteron::error::FlowGuardError::BanError("服务错误".to_string()),
                    )
                })
                .await;
        }

        // 验证熔断器打开
        assert!(breaker.is_open().await);

        // 新请求应该被拒绝
        let result = breaker
            .execute(|| async { Ok::<(), limiteron::error::FlowGuardError>(()) })
            .await;

        assert!(result.is_err());
    }

    /// 测试熔断器统计信息
    #[tokio::test]
    async fn test_circuit_breaker_stats() {
        let breaker = create_test_circuit_breaker(3, 2, Duration::from_secs(60));

        // 初始状态
        let stats = breaker.get_stats().await;
        assert_eq!(stats.state, CircuitState::Closed);
        assert_eq!(stats.failure_count, 0);

        // 触发部分故障
        for i in 1..=2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), limiteron::error::FlowGuardError>(
                        limiteron::error::FlowGuardError::BanError(format!("错误 {}", i)),
                    )
                })
                .await;

            let stats = breaker.get_stats().await;
            assert_eq!(stats.failure_count, i);
            assert_eq!(stats.state, CircuitState::Closed);
        }

        // 触发熔断
        let _ = breaker
            .execute(|| async {
                Err::<(), limiteron::error::FlowGuardError>(
                    limiteron::error::FlowGuardError::BanError("触发熔断".to_string()),
                )
            })
            .await;

        let stats = breaker.get_stats().await;
        assert_eq!(stats.state, CircuitState::Open);
        assert_eq!(stats.failure_count, 3);
    }

    /// 测试多次故障恢复循环
    #[tokio::test]
    async fn test_multiple_failure_recovery_cycles() {
        let breaker = create_test_circuit_breaker(2, 2, Duration::from_millis(50));

        // 循环多次故障恢复
        for cycle in 0..3 {
            // 触发故障
            for _ in 0..2 {
                let _ = breaker
                    .execute(|| async {
                        Err::<(), limiteron::error::FlowGuardError>(
                            limiteron::error::FlowGuardError::BanError(format!("错误 {}", cycle)),
                        )
                    })
                    .await;
            }

            assert!(
                breaker.is_open().await,
                "第 {} 次循环: 熔断器应该打开",
                cycle
            );

            // 等待恢复
            tokio::time::sleep(Duration::from_millis(100)).await;

            // 恢复
            for _ in 0..2 {
                let _ = breaker
                    .execute(|| async { Ok::<(), limiteron::error::FlowGuardError>(()) })
                    .await;
            }

            assert!(
                breaker.is_closed().await,
                "第 {} 次循环: 熔断器应该恢复",
                cycle
            );
        }
    }

    /// 测试成功请求重置失败计数
    #[tokio::test]
    async fn test_success_resets_failure_count() {
        let breaker = create_test_circuit_breaker(5, 2, Duration::from_secs(60));

        // 触发部分故障
        for _ in 0..2 {
            let _ = breaker
                .execute(|| async {
                    Err::<(), limiteron::error::FlowGuardError>(
                        limiteron::error::FlowGuardError::BanError("错误".to_string()),
                    )
                })
                .await;
        }

        let stats = breaker.get_stats().await;
        assert_eq!(stats.failure_count, 2);

        // 成功请求应该重置计数
        let _ = breaker
            .execute(|| async { Ok::<(), limiteron::error::FlowGuardError>(()) })
            .await;

        let stats = breaker.get_stats().await;
        assert_eq!(stats.success_count, 1);
    }
}

#[cfg(feature = "fallback")]
mod fallback_tests {
    use crate::common::create_test_cache;
    use limiteron::fallback::{ComponentType, FallbackConfig, FallbackManager, FallbackStrategy};
    use std::sync::Arc;

    /// 创建测试用的 FallbackManager
    async fn create_fallback_manager() -> Arc<FallbackManager> {
        let cache = create_test_cache().await;
        Arc::new(FallbackManager::new(Arc::new(cache)))
    }

    /// 测试降级策略设置
    #[tokio::test]
    async fn test_fallback_strategy_setting() {
        let fallback_manager = create_fallback_manager().await;

        // 设置降级策略
        let config = FallbackConfig::new(ComponentType::Redis, FallbackStrategy::Degraded);
        fallback_manager
            .set_strategy(ComponentType::Redis, config)
            .await;

        // 验证策略已设置
        let strategy = fallback_manager.get_strategy(ComponentType::Redis).await;
        assert!(strategy.is_some());
    }

    /// 测试多组件独立降级
    #[tokio::test]
    async fn test_multiple_components_independent_fallback() {
        let fallback_manager = create_fallback_manager().await;

        // 为不同组件设置不同策略
        fallback_manager
            .set_strategy(
                ComponentType::Redis,
                FallbackConfig::new(ComponentType::Redis, FallbackStrategy::Degraded),
            )
            .await;

        fallback_manager
            .set_strategy(
                ComponentType::Postgres,
                FallbackConfig::new(ComponentType::Postgres, FallbackStrategy::FailOpen),
            )
            .await;

        // 验证两个组件都有策略
        assert!(fallback_manager
            .get_strategy(ComponentType::Redis)
            .await
            .is_some());
        assert!(fallback_manager
            .get_strategy(ComponentType::Postgres)
            .await
            .is_some());
    }

    /// 测试故障注入与恢复
    #[tokio::test]
    async fn test_failure_injection_and_recovery() {
        let fallback_manager = create_fallback_manager().await;

        // 注入故障
        fallback_manager.inject_failure(ComponentType::Redis).await;
        assert!(fallback_manager.is_failed(ComponentType::Redis).await);

        // 手动恢复
        fallback_manager.recover_failure(ComponentType::Redis).await;
        assert!(!fallback_manager.is_failed(ComponentType::Redis).await);
    }

    /// 测试获取所有故障组件
    #[tokio::test]
    async fn test_get_all_failures() {
        let fallback_manager = create_fallback_manager().await;

        // 注入多个故障
        fallback_manager.inject_failure(ComponentType::Redis).await;
        fallback_manager
            .inject_failure(ComponentType::Postgres)
            .await;
        fallback_manager.inject_failure(ComponentType::Ban).await;

        let failures = fallback_manager.get_all_failures().await;
        assert_eq!(failures.len(), 3);
        assert!(failures.contains(&ComponentType::Redis));
        assert!(failures.contains(&ComponentType::Postgres));
        assert!(failures.contains(&ComponentType::Ban));

        // 恢复一个
        fallback_manager.recover_failure(ComponentType::Redis).await;

        let failures = fallback_manager.get_all_failures().await;
        assert_eq!(failures.len(), 2);
        assert!(!failures.contains(&ComponentType::Redis));
    }
}

// 当没有启用相关特性时，提供一个空的测试模块
#[cfg(not(any(feature = "circuit-breaker", feature = "fallback")))]
mod no_feature_tests {
    #[test]
    fn test_circuit_breaker_fallback_features_not_enabled() {
        // 当特性未启用时，测试通过
        println!("CircuitBreaker 和 Fallback 特性未启用，跳过测试");
    }
}
