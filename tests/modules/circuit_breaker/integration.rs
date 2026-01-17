//! 熔断器模块集成测试
//!
//! 测试熔断器与其他组件的集成

use limiteron::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};

/// 测试熔断器与存储的集成
#[tokio::test]
async fn test_circuit_breaker_with_storage() {
    use limiteron::storage::MemoryStorage;

    let storage = MemoryStorage::new();
    let config = CircuitBreakerConfig {
        failure_threshold: 5,
        success_threshold: 3,
        timeout: std::time::Duration::from_secs(60),
    };

    let circuit_breaker = CircuitBreaker::new("test_service", config);

    // 测试正常状态
    let result = circuit_breaker
        .call(|| async {
            // 模拟成功的操作
            storage
                .consume(
                    "user1",
                    "resource1",
                    10,
                    1000,
                    std::time::Duration::from_secs(60),
                )
                .await
        })
        .await;

    assert!(result.is_ok());
}

/// 测试熔断器的状态转换
#[tokio::test]
async fn test_circuit_breaker_state_transitions() {
    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        timeout: std::time::Duration::from_secs(1),
    };

    let circuit_breaker = CircuitBreaker::new("test_service", config);

    // 初始状态应该是关闭的
    assert_eq!(circuit_breaker.state(), "Closed");

    // 触发失败
    for _ in 0..3 {
        let _ = circuit_breaker
            .call(|| async {
                Err::<(), _>(std::io::Error::new(std::io::ErrorKind::Other, "Test error"))
            })
            .await;
    }

    // 应该转为打开状态
    assert_eq!(circuit_breaker.state(), "Open");
}

/// 测试熔断器的自动恢复
#[tokio::test]
async fn test_circuit_breaker_auto_recovery() {
    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        timeout: std::time::Duration::from_millis(100),
    };

    let circuit_breaker = CircuitBreaker::new("test_service", config);

    // 触发失败
    for _ in 0..3 {
        let _ = circuit_breaker
            .call(|| async {
                Err::<(), _>(std::io::Error::new(std::io::ErrorKind::Other, "Test error"))
            })
            .await;
    }

    // 等待超时
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // 应该转为半开状态
    assert_eq!(circuit_breaker.state(), "HalfOpen");
}
