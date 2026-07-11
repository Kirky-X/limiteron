// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 熔断器模块集成测试
//!
//! 测试熔断器模块的基本功能

#[cfg(feature = "circuit-breaker")]
use limiteron::circuit::{CircuitBreaker, CircuitBreakerConfig};
#[cfg(feature = "circuit-breaker")]
use limiteron::error::{CircuitState, FlowGuardError};
use std::time::Duration;

/// 测试熔断器模块导入
#[tokio::test]
#[cfg(feature = "circuit-breaker")]
async fn test_circuit_breaker_module_import() {
    #[allow(unused_variables)]
    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        timeout: Duration::from_secs(5),
        half_open_max_calls: 3,
        ..Default::default()
    };

    #[allow(unused_variables)]
    let circuit_breaker = CircuitBreaker::new(config);
    // 验证熔断器可以创建
}

/// 3.3.1: 测试 CircuitBreaker 与 Governor 集成
#[tokio::test]
#[cfg(feature = "circuit-breaker")]
async fn test_circuit_breaker_governor_integration() {
    // 创建熔断器并验证基本功能
    let circuit_breaker = CircuitBreaker::with_dependencies(CircuitBreakerConfig::default());

    // 验证熔断器初始状态为 Closed
    assert!(circuit_breaker.is_closed().await);

    // 执行成功操作
    let result = circuit_breaker
        .execute(|| async { Ok::<(), FlowGuardError>(()) })
        .await;
    assert!(result.is_ok());

    // 验证状态保持 Closed
    assert!(circuit_breaker.is_closed().await);

    // 验证统计信息
    let stats = circuit_breaker.get_stats().await;
    assert_eq!(stats.success_count, 1);
}

/// 3.3.2: 测试 CircuitBreaker 在超时后恢复
#[tokio::test]
#[cfg(feature = "circuit-breaker")]
async fn test_circuit_breaker_recovers_after_timeout() {
    let config = CircuitBreakerConfig::new(
        2,                          // failure_threshold
        1,                          // success_threshold
        Duration::from_millis(100), // short timeout for testing
    );
    let circuit_breaker = CircuitBreaker::new(config);

    // 触发熔断器打开 - 执行失败操作
    // 注意：DefaultErrorClassifier 不将 LimitError/CircuitBreakerError/ValidationError 计为失败，
    // 必须使用其他错误变体（如 BanError）才能触发熔断
    for _ in 0..2 {
        let _ = circuit_breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::BanError("test error".to_string()))
            })
            .await;
    }

    assert!(circuit_breaker.is_open().await);

    // 等待超时
    tokio::time::sleep(Duration::from_millis(150)).await;

    // 状态应该变为 HalfOpen（通过执行操作触发状态检查）
    let _ = circuit_breaker
        .execute(|| async { Ok::<(), FlowGuardError>(()) })
        .await;

    // 成功后应该恢复到 Closed
    assert!(circuit_breaker.is_closed().await);
}

/// 3.3.3: 测试 CircuitBreaker 在打开状态时快速失败
#[tokio::test]
#[cfg(feature = "circuit-breaker")]
async fn test_circuit_breaker_fast_fails_in_open_state() {
    let config = CircuitBreakerConfig::new(
        2,                       // failure_threshold
        1,                       // success_threshold
        Duration::from_secs(10), // long timeout
    );
    let circuit_breaker = CircuitBreaker::new(config);

    // 触发熔断器打开
    // 注意：DefaultErrorClassifier 不将 LimitError 计为失败，使用 BanError 触发熔断
    for _ in 0..2 {
        let _ = circuit_breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::BanError("test error".to_string()))
            })
            .await;
    }

    assert!(circuit_breaker.is_open().await);

    // 在打开状态下，请求应该被快速拒绝
    let result = circuit_breaker
        .execute(|| async { Ok::<(), FlowGuardError>(()) })
        .await;
    assert!(result.is_err());

    // 验证错误类型（熔断器打开时返回 LimitError）
    match result {
        Err(FlowGuardError::LimitError(msg)) => {
            assert!(msg.contains("熔断器打开") || msg.contains("请求被拒绝"));
        }
        _ => panic!("Expected LimitError when circuit breaker is open"),
    }

    // 验证状态仍然是 Open
    assert!(circuit_breaker.is_open().await);
}
