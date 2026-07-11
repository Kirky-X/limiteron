// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 熔断保护场景测试
//!
//! 测试后端失败触发熔断，以及熔断恢复后正常访问的完整流程

#[cfg(feature = "circuit-breaker")]
use limiteron::circuit::{CircuitBreaker, CircuitBreakerConfig};
#[cfg(feature = "circuit-breaker")]
use limiteron::error::{CircuitState, FlowGuardError};
#[cfg(feature = "circuit-breaker")]
use std::time::Duration;

// ==================== E2E Scenario Tests ====================

/// 场景 1: 后端失败触发熔断
///
/// 当后端服务连续失败达到阈值时，熔断器自动打开。
#[tokio::test]
#[cfg(feature = "circuit-breaker")]
async fn e2e_circuit_breaker_failures_trigger_open() {
    let config = CircuitBreakerConfig::new(
        3,                       // failure_threshold: 3 次失败后熔断
        1,                       // success_threshold: 1 次成功后恢复
        Duration::from_secs(30), // timeout: 30 秒后尝试恢复
    );
    let breaker = CircuitBreaker::new(config);

    // 初始状态应该是关闭的
    assert!(
        breaker.is_closed().await,
        "Circuit should be closed initially"
    );

    // 模拟连续失败
    for i in 1..=3 {
        let result = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::BanError(
                    "service unavailable".to_string(),
                ))
            })
            .await;

        assert!(result.is_err(), "Failure {} should return error", i);

        if i < 3 {
            assert!(
                breaker.is_closed().await,
                "Circuit should be closed before threshold"
            );
        }
    }

    // 熔断器应该打开
    assert!(
        breaker.is_open().await,
        "Circuit should be open after failures"
    );

    // 验证统计信息
    let stats = breaker.get_stats().await;
    assert_eq!(stats.failure_count, 3, "Should have 3 failures");
    assert_eq!(stats.state, CircuitState::Open);
}

/// 场景 2: 熔断状态下快速失败
///
/// 熔断器打开后，后续请求直接失败，不调用后端服务。
#[tokio::test]
#[cfg(feature = "circuit-breaker")]
async fn e2e_circuit_breaker_fast_fail_when_open() {
    let config = CircuitBreakerConfig::new(
        2,                       // failure_threshold
        1,                       // success_threshold
        Duration::from_secs(60), // long timeout
    );
    let breaker = CircuitBreaker::new(config);

    // 触发熔断
    for _ in 0..2 {
        let _ = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::BanError("error".to_string()))
            })
            .await;
    }

    assert!(breaker.is_open().await, "Circuit should be open");

    // 在熔断状态下，请求应该快速失败
    let start = std::time::Instant::now();
    let result = breaker
        .execute(|| async {
            // 这个闭包不应该被执行
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok::<(), FlowGuardError>(())
        })
        .await;

    let elapsed = start.elapsed();

    assert!(result.is_err(), "Request should fail when circuit is open");
    assert!(
        elapsed < Duration::from_millis(100),
        "Should fail fast, took {:?}",
        elapsed
    );

    // 验证错误消息
    match result {
        Err(FlowGuardError::LimitError(msg)) => {
            assert!(
                msg.contains("熔断器打开") || msg.contains("请求被拒绝"),
                "Error message should indicate circuit is open: {}",
                msg
            );
        }
        _ => panic!("Expected LimitError"),
    }
}

/// 场景 3: 熔断恢复后正常访问
///
/// 熔断器超时后进入半开状态，成功请求后恢复正常。
#[tokio::test]
#[cfg(feature = "circuit-breaker")]
async fn e2e_circuit_breaker_recovery_after_timeout() {
    let config = CircuitBreakerConfig::new(
        2,                          // failure_threshold
        1,                          // success_threshold
        Duration::from_millis(100), // short timeout for testing
    );
    let breaker = CircuitBreaker::new(config);

    // 触发熔断
    for _ in 0..2 {
        let _ = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::BanError("error".to_string()))
            })
            .await;
    }

    assert!(breaker.is_open().await, "Circuit should be open");

    // 等待超时
    tokio::time::sleep(Duration::from_millis(150)).await;

    // 发送成功请求，应该触发恢复
    let result = breaker
        .execute(|| async { Ok::<(), FlowGuardError>(()) })
        .await;

    assert!(result.is_ok(), "Request should succeed in half-open state");

    // 熔断器应该恢复到关闭状态
    assert!(
        breaker.is_closed().await,
        "Circuit should be closed after successful recovery"
    );

    // 验证统计信息（恢复到 Closed 状态时计数器会被重置）
    let stats = breaker.get_stats().await;
    assert_eq!(
        stats.success_count, 0,
        "Success count should be reset after recovery to Closed"
    );
    assert_eq!(
        stats.failure_count, 0,
        "Failure count should be reset after recovery to Closed"
    );
}

/// 场景 4: 半开状态下失败重新熔断
///
/// 半开状态下如果再次失败，熔断器重新打开。
#[tokio::test]
#[cfg(feature = "circuit-breaker")]
async fn e2e_circuit_breaker_reopen_on_half_open_failure() {
    let config = CircuitBreakerConfig::new(
        2,                          // failure_threshold
        2,                          // success_threshold: 需要 2 次成功
        Duration::from_millis(100), // short timeout
    );
    let breaker = CircuitBreaker::new(config);

    // 触发熔断
    for _ in 0..2 {
        let _ = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::BanError("error".to_string()))
            })
            .await;
    }

    assert!(breaker.is_open().await, "Circuit should be open");

    // 等待超时进入半开状态
    tokio::time::sleep(Duration::from_millis(150)).await;

    // 在半开状态下失败
    let result = breaker
        .execute(|| async {
            Err::<(), FlowGuardError>(FlowGuardError::BanError("still failing".to_string()))
        })
        .await;

    assert!(result.is_err(), "Request should fail");

    // 熔断器应该重新打开
    assert!(
        breaker.is_open().await,
        "Circuit should be open again after half-open failure"
    );
}

/// 场景 5: 熔断器统计信息
///
/// 熔断器正确记录成功和失败次数。
#[tokio::test]
#[cfg(feature = "circuit-breaker")]
async fn e2e_circuit_breaker_statistics() {
    let config = CircuitBreakerConfig::new(
        10, // high threshold
        1,  // success_threshold
        Duration::from_secs(30),
    );
    let breaker = CircuitBreaker::new(config);

    // 执行一些成功请求
    for _ in 0..3 {
        let _ = breaker
            .execute(|| async { Ok::<(), FlowGuardError>(()) })
            .await;
    }

    // 执行一些失败请求
    for _ in 0..2 {
        let _ = breaker
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::BanError("error".to_string()))
            })
            .await;
    }

    let stats = breaker.get_stats().await;
    assert_eq!(stats.success_count, 3, "Should have 3 successes");
    assert_eq!(stats.failure_count, 2, "Should have 2 failures");
    assert_eq!(stats.state, CircuitState::Closed);
}

/// 场景 6: 并发请求下的熔断保护
///
/// 高并发场景下熔断器正确工作。
///
/// 注意: CircuitBreaker 未实现 Clone，无法在并发任务间共享，
/// 该测试待 CircuitBreaker 支持 Clone 或提供 Arc 包装方案后启用。
#[cfg(any())]
#[tokio::test]
#[cfg(feature = "circuit-breaker")]
async fn e2e_circuit_breaker_concurrent_protection() {
    let config = CircuitBreakerConfig::new(
        5, // failure_threshold
        1, // success_threshold
        Duration::from_secs(30),
    );
    let breaker = CircuitBreaker::new(config);

    let mut handles = vec![];

    // 并发发送请求
    for i in 0..20 {
        let breaker_clone = breaker.clone();
        handles.push(tokio::spawn(async move {
            breaker_clone
                .execute(|| async {
                    if i < 10 {
                        // 前 10 个请求失败
                        Err::<(), FlowGuardError>(FlowGuardError::BanError("error".to_string()))
                    } else {
                        // 后 10 个请求成功
                        Ok::<(), FlowGuardError>(())
                    }
                })
                .await
        }));
    }

    let mut success_count = 0;
    let mut failure_count = 0;

    for handle in handles {
        match handle.await.unwrap() {
            Ok(_) => success_count += 1,
            Err(_) => failure_count += 1,
        }
    }

    // 熔断器应该已经打开
    assert!(
        breaker.is_open().await,
        "Circuit should be open after concurrent failures"
    );

    // 至少有一些请求被熔断器拒绝
    assert!(
        failure_count > 5,
        "Should have some failures due to circuit breaker"
    );
}

/// 场景 7: 熔断器手动控制
///
/// 可以手动打开或关闭熔断器。
///
/// 注意: CircuitBreaker 未提供 trip() 方法（手动打开熔断），
/// 该测试待 CircuitBreaker 提供手动控制 API 后启用。
#[cfg(any())]
#[tokio::test]
#[cfg(feature = "circuit-breaker")]
async fn e2e_circuit_breaker_manual_control() {
    let breaker = CircuitBreaker::new(CircuitBreakerConfig::default());

    // 初始状态
    assert!(breaker.is_closed().await);

    // 手动打开
    breaker.trip().await;
    assert!(
        breaker.is_open().await,
        "Circuit should be open after manual trip"
    );

    // 手动关闭
    breaker.reset().await;
    assert!(
        breaker.is_closed().await,
        "Circuit should be closed after manual reset"
    );
}

/// 场景 8: 熔断器超时配置
///
/// 验证熔断器的超时配置正确生效。
#[tokio::test]
#[cfg(feature = "circuit-breaker")]
async fn e2e_circuit_breaker_timeout_config() {
    let config = CircuitBreakerConfig::new(
        1,                          // failure_threshold
        1,                          // success_threshold
        Duration::from_millis(200), // timeout
    );
    let breaker = CircuitBreaker::new(config);

    // 触发熔断
    let _ = breaker
        .execute(|| async {
            Err::<(), FlowGuardError>(FlowGuardError::BanError("error".to_string()))
        })
        .await;

    assert!(breaker.is_open().await);

    // 立即尝试，应该仍然打开
    let result = breaker
        .execute(|| async { Ok::<(), FlowGuardError>(()) })
        .await;
    assert!(result.is_err(), "Should still fail immediately after trip");

    // 等待超时
    tokio::time::sleep(Duration::from_millis(250)).await;

    // 现在应该可以尝试恢复
    let result = breaker
        .execute(|| async { Ok::<(), FlowGuardError>(()) })
        .await;
    assert!(result.is_ok(), "Should succeed after timeout");
}
