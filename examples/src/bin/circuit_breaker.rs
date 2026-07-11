// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Circuit Breaker Example
//!
//! Demonstrates circuit breaker pattern for fault tolerance:
//! - Failure detection and circuit opening
//! - Half-open state for recovery probing
//! - Automatic recovery after timeout
//!
//! Run: cargo run --bin circuit_breaker --features circuit-breaker

use limiteron::circuit::CircuitBreaker;
use limiteron::error::FlowGuardError;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), FlowGuardError> {
    println!("=== Limiteron Circuit Breaker Demo ===\n");

    demo_basic_operations().await?;
    demo_state_transitions().await?;
    demo_recovery().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

async fn demo_basic_operations() -> Result<(), FlowGuardError> {
    println!("--- Basic Circuit Breaker Operations ---");
    println!("Config: failure_threshold=2, success_threshold=1, timeout=100ms\n");

    let breaker = CircuitBreaker::builder()
        .failure_threshold(2)
        .success_threshold(1)
        .timeout(Duration::from_millis(100))
        .half_open_max_calls(2)
        .build();

    println!("Initial state: Closed");
    println!("Is closed: {}", breaker.is_closed().await);
    println!("Is open: {}\n", breaker.is_open().await);

    let success: Result<(), FlowGuardError> = breaker
        .execute(|| async { Ok::<(), FlowGuardError>(()) })
        .await;
    println!("Execute success operation: {:?}", success.is_ok());

    let stats = breaker.get_stats().await;
    println!(
        "Stats: state={:?}, failures={}, successes={}\n",
        stats.state, stats.failure_count, stats.success_count
    );

    Ok(())
}

async fn demo_state_transitions() -> Result<(), FlowGuardError> {
    println!("--- State Transitions Demo ---\n");

    let breaker = CircuitBreaker::builder()
        .failure_threshold(2)
        .success_threshold(1)
        .timeout(Duration::from_millis(100))
        .build();

    async fn fail_operation() -> Result<(), FlowGuardError> {
        Err(FlowGuardError::LimitError("operation failed".to_string()))
    }

    println!("Triggering failures to open circuit...");
    for i in 1..=3 {
        let _ = breaker.execute(fail_operation).await;
        let stats = breaker.get_stats().await;
        println!(
            "  Failure {}: state={:?}, failure_count={}",
            i, stats.state, stats.failure_count
        );
    }

    println!("\nCircuit is now OPEN");
    println!("Is open: {}\n", breaker.is_open().await);

    let result = breaker
        .execute(|| async { Ok::<(), FlowGuardError>(()) })
        .await;
    println!("Request in open state: {:?} (fast fail)\n", result);

    Ok(())
}

async fn demo_recovery() -> Result<(), FlowGuardError> {
    println!("--- Recovery Demo ---\n");

    let breaker = CircuitBreaker::builder()
        .failure_threshold(2)
        .success_threshold(1)
        .timeout(Duration::from_millis(100))
        .build();

    async fn fail_operation() -> Result<(), FlowGuardError> {
        Err(FlowGuardError::LimitError("operation failed".to_string()))
    }

    async fn success_operation() -> Result<(), FlowGuardError> {
        Ok(())
    }

    for _ in 0..2 {
        let _ = breaker.execute(fail_operation).await;
    }
    println!("Circuit opened after 2 failures");

    println!("Waiting for timeout (120ms)...");
    tokio::time::sleep(Duration::from_millis(120)).await;

    println!("\nProbing with success operation...");
    let result: Result<(), FlowGuardError> = breaker.execute(success_operation).await;
    println!("Probe result: {:?}", result.is_ok());

    let stats = breaker.get_stats().await;
    println!(
        "After recovery: state={:?}, successes={}\n",
        stats.state, stats.success_count
    );

    Ok(())
}
