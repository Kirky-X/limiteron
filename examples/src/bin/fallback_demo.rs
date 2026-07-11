// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Fallback Manager 示例
//!
//! 演示降级策略管理器的使用：策略配置、故障注入、降级执行、孤岛模式。
//!
//! # 涵盖 API
//!
//! - `FallbackManager::new(l2_cache)` (需要 oxcache::Cache)
//! - `FallbackConfig`（`new`、`enabled`、`timeout`、`max_retries`）
//! - `FallbackStrategy` 枚举（`FailOpen`、`FailClosed`、`Degraded`）
//! - `ComponentType` 枚举（`Redis`、`Postgres`、`L2Cache` 等）
//! - `set_strategy` / `get_strategy`
//! - `execute_with_fallback` (带降级的主操作执行)
//! - `inject_failure` / `recover_failure` / `is_failed` / `get_all_failures`
//! - `register_island_mode_callback` (孤岛模式回调)
//!
//! # 运行方式
//!
//! ```bash
//! cargo run --bin fallback_demo --features fallback
//! ```

use limiteron::fallback::{ComponentType, FallbackConfig, FallbackManager, FallbackStrategy};
use limiteron::FlowGuardError;
use oxcache::Cache;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Fallback Manager Demo ===\n");

    demo_strategy_config().await?;
    demo_failure_injection().await?;
    demo_execute_with_fallback().await?;
    demo_island_mode().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

/// 创建测试用的 FallbackManager
async fn create_manager() -> FallbackManager {
    let cache: Cache<String, String> = Cache::builder()
        .capacity(10000)
        .ttl(Duration::from_secs(60))
        .build()
        .await
        .expect("cache build should succeed");
    FallbackManager::new(Arc::new(cache))
}

/// 演示降级策略配置
async fn demo_strategy_config() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 1. Strategy Configuration ---\n");

    let manager = create_manager().await;

    // 查看默认策略
    println!("  Default strategies:");
    for component in [
        ComponentType::Redis,
        ComponentType::Postgres,
        ComponentType::L2Cache,
        ComponentType::Config,
    ] {
        if let Some(config) = manager.get_strategy(component.clone()).await {
            println!(
                "    {:<10} -> strategy={:?}, timeout={:?}, retries={}",
                component.as_str(),
                config.strategy,
                config.timeout,
                config.max_retries
            );
        }
    }

    // 自定义 Redis 策略：FailOpen + 短超时 + 1 次重试
    let redis_config = FallbackConfig::new(ComponentType::Redis, FallbackStrategy::FailOpen)
        .enabled(true)
        .timeout(Duration::from_secs(2))
        .max_retries(1);
    manager
        .set_strategy(ComponentType::Redis, redis_config)
        .await;

    // 自定义 Postgres 策略：FailClosed + 长超时
    let pg_config = FallbackConfig::new(ComponentType::Postgres, FallbackStrategy::FailClosed)
        .timeout(Duration::from_secs(10))
        .max_retries(5);
    manager
        .set_strategy(ComponentType::Postgres, pg_config)
        .await;

    println!("\n  After customization:");
    let redis = manager.get_strategy(ComponentType::Redis).await.unwrap();
    let postgres = manager.get_strategy(ComponentType::Postgres).await.unwrap();
    println!(
        "    Redis:    strategy={:?}, timeout={:?}, retries={}",
        redis.strategy, redis.timeout, redis.max_retries
    );
    println!(
        "    Postgres: strategy={:?}, timeout={:?}, retries={}",
        postgres.strategy, postgres.timeout, postgres.max_retries
    );
    println!();
    Ok(())
}

/// 演示故障注入与恢复
async fn demo_failure_injection() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 2. Failure Injection & Recovery ---\n");

    let manager = create_manager().await;

    println!("  Initial state:");
    println!(
        "    Redis failed: {}",
        manager.is_failed(ComponentType::Redis).await
    );
    println!(
        "    Redis failure count: {}",
        manager.get_failure_count(ComponentType::Redis).await
    );

    // 注入故障
    manager.inject_failure(ComponentType::Redis).await;
    manager.inject_failure(ComponentType::Postgres).await;
    println!("\n  After inject_failure(Redis, Postgres):");
    println!(
        "    Redis failed: {}",
        manager.is_failed(ComponentType::Redis).await
    );
    println!(
        "    Postgres failed: {}",
        manager.is_failed(ComponentType::Postgres).await
    );

    // 获取所有故障
    let failures = manager.get_all_failures().await;
    println!("    All failures: {:?}", failures);

    // 记录故障（通过 record_failure）
    manager
        .record_failure(ComponentType::L2Cache, "connection timeout")
        .await;
    println!("\n  After record_failure(L2Cache, 'connection timeout'):");
    let failures = manager.get_all_failures().await;
    println!("    All failures: {:?}", failures);

    // 恢复故障
    manager.recover_failure(ComponentType::Redis).await;
    println!("\n  After recover_failure(Redis):");
    println!(
        "    Redis failed: {}",
        manager.is_failed(ComponentType::Redis).await
    );
    let failures = manager.get_all_failures().await;
    println!("    Remaining failures: {:?}", failures);

    // 清除所有故障
    manager.clear_failure(ComponentType::Postgres).await;
    manager.clear_failure(ComponentType::L2Cache).await;
    println!("\n  After clearing all:");
    println!("    All failures: {:?}", manager.get_all_failures().await);
    println!();
    Ok(())
}

/// 演示 execute_with_fallback：主操作失败时执行降级
async fn demo_execute_with_fallback() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 3. execute_with_fallback ---\n");

    let manager = create_manager().await;

    // 场景 1：主操作成功
    let result: Result<String, FlowGuardError> = manager
        .execute_with_fallback(
            ComponentType::Redis,
            || async { Ok("primary success".to_string()) },
            || async { Ok("fallback value".to_string()) },
        )
        .await;
    println!("  Scenario 1 (primary succeeds): {:?}", result);

    // 场景 2：主操作失败，执行降级操作
    // 使用 Degraded 策略（默认），降级操作会被调用
    let result: Result<String, FlowGuardError> = manager
        .execute_with_fallback(
            ComponentType::Redis,
            || async { Err(FlowGuardError::ConfigError("redis down".to_string())) },
            || async { Ok("fallback success".to_string()) },
        )
        .await;
    println!("  Scenario 2 (primary fails, Degraded): {:?}", result);

    // 场景 3：FailOpen 策略（主操作失败时返回特定错误）
    manager
        .set_strategy(
            ComponentType::Redis,
            FallbackConfig::new(ComponentType::Redis, FallbackStrategy::FailOpen),
        )
        .await;
    let result: Result<String, FlowGuardError> = manager
        .execute_with_fallback(
            ComponentType::Redis,
            || async { Err(FlowGuardError::ConfigError("redis down".to_string())) },
            || async { Ok("fallback value".to_string()) },
        )
        .await;
    println!("  Scenario 3 (FailOpen): {:?}", result);

    // 场景 4：FailClosed 策略
    manager
        .set_strategy(
            ComponentType::Redis,
            FallbackConfig::new(ComponentType::Redis, FallbackStrategy::FailClosed),
        )
        .await;
    let result: Result<String, FlowGuardError> = manager
        .execute_with_fallback(
            ComponentType::Redis,
            || async { Err(FlowGuardError::ConfigError("redis down".to_string())) },
            || async { Ok("fallback value".to_string()) },
        )
        .await;
    println!("  Scenario 4 (FailClosed): {:?}", result);
    println!();
    Ok(())
}

/// 演示孤岛模式回调
async fn demo_island_mode() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 4. Island Mode Callback ---\n");

    let manager = create_manager().await;

    // 注册孤岛模式回调
    let callback_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let count_clone = callback_count.clone();
    manager
        .register_island_mode_callback(Box::new(move |is_island: bool| {
            count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            println!("    [callback] island mode changed: {}", is_island);
        }))
        .await;

    println!("  Callback registered");

    // 注入故障触发孤岛模式
    manager.inject_failure(ComponentType::Redis).await;
    println!("  After inject_failure(Redis):");
    println!(
        "    callback invocations: {}",
        callback_count.load(std::sync::atomic::Ordering::SeqCst)
    );

    // 恢复故障
    manager.recover_failure(ComponentType::Redis).await;
    println!("\n  After recover_failure(Redis):");
    println!(
        "    callback invocations: {}",
        callback_count.load(std::sync::atomic::Ordering::SeqCst)
    );
    println!();
    Ok(())
}
