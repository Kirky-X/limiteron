// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Quota Control Example
//!
//! Demonstrates quota management functionality:
//! - Quota consumption tracking
//! - Limit enforcement
//! - Usage percentage calculation
//!
//! Run: cargo run --bin quota_control --features quota-control

use limiteron::error::LimiteronError;
use limiteron::quota::{AlertChannel, AlertConfig, QuotaConfig, QuotaController, QuotaType};
use limiteron::QuotaStorage;
use limiteron_examples::MemoryQuotaStorage;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), LimiteronError> {
    println!("=== Limiteron Quota Control Demo ===\n");

    demo_basic_quota().await?;
    demo_limit_enforcement().await?;
    demo_usage_tracking().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

async fn demo_basic_quota() -> Result<(), LimiteronError> {
    println!("--- Basic Quota Operations ---");
    println!("Config: limit=10, window=60s\n");

    let storage: Arc<dyn QuotaStorage> = Arc::new(MemoryQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 10,
        window_size: 60,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            thresholds: vec![80, 100],
            channels: vec![AlertChannel::Log],
            dedup_window: 60,
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);

    let result = controller.consume("user-1", "api", 3).await?;
    println!(
        "Consume 3: allowed={}, remaining={}",
        result.allowed, result.remaining
    );

    let result = controller.consume("user-1", "api", 2).await?;
    println!(
        "Consume 2: allowed={}, remaining={}",
        result.allowed, result.remaining
    );

    let result = controller.consume("user-1", "api", 5).await?;
    println!(
        "Consume 5: allowed={}, remaining={}\n",
        result.allowed, result.remaining
    );

    Ok(())
}

async fn demo_limit_enforcement() -> Result<(), LimiteronError> {
    println!("--- Limit Enforcement Demo ---\n");

    let storage: Arc<dyn QuotaStorage> = Arc::new(MemoryQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 5,
        window_size: 60,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            thresholds: vec![80, 100],
            channels: vec![AlertChannel::Log],
            dedup_window: 60,
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);

    println!("Consuming quota in increments:");
    for i in 1..=6 {
        let result = controller.consume("user-2", "api", 1).await?;
        println!(
            "  Request {}: allowed={}, remaining={}, usage={:.1}%",
            i, result.allowed, result.remaining, result.usage_percent
        );
    }

    println!("\nQuota exhausted - further requests will be denied\n");

    Ok(())
}

async fn demo_usage_tracking() -> Result<(), LimiteronError> {
    println!("--- Usage Tracking Demo ---\n");

    let storage: Arc<dyn QuotaStorage> = Arc::new(MemoryQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 100,
        window_size: 60,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: true,
            thresholds: vec![50, 80, 100],
            channels: vec![AlertChannel::Log],
            dedup_window: 60,
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);

    println!("Consuming 25 units (25% usage):");
    let result = controller.consume("user-3", "api", 25).await?;
    println!(
        "  allowed={}, usage={:.1}%\n",
        result.allowed, result.usage_percent
    );

    println!("Consuming 30 more units (55% total):");
    let result = controller.consume("user-3", "api", 30).await?;
    println!(
        "  allowed={}, usage={:.1}%\n",
        result.allowed, result.usage_percent
    );

    println!("Consuming 25 more units (80% total - alert threshold):");
    let result = controller.consume("user-3", "api", 25).await?;
    println!(
        "  allowed={}, usage={:.1}%, alert_triggered={}\n",
        result.allowed, result.usage_percent, result.alert_triggered
    );

    Ok(())
}
