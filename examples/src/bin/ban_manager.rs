// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Ban Manager Example
//!
//! Demonstrates ban management functionality:
//! - Creating bans for different targets (IP, User ID, MAC)
//! - Checking ban status
//! - Updating and removing bans
//!
//! Run: cargo run --bin ban_manager --features ban-manager

use limiteron::ban::{BanManager, BanManagerConfig, BanSource};
use limiteron::error::FlowGuardError;
use limiteron::{BanStorage, BanTarget};
use limiteron_examples::MemoryBanStorage;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), FlowGuardError> {
    println!("=== Limiteron Ban Manager Demo ===\n");

    demo_ip_ban().await?;
    demo_user_ban().await?;
    demo_ban_update().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

async fn demo_ip_ban() -> Result<(), FlowGuardError> {
    println!("--- IP Ban Demo ---\n");

    let storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let ban_manager = BanManager::with_dependencies(storage, config).await?;

    let target = BanTarget::Ip("192.168.1.100".to_string());
    println!("Creating ban for IP: 192.168.1.100");

    let detail = ban_manager
        .create_ban(
            target.clone(),
            "suspicious activity".to_string(),
            BanSource::Auto,
            serde_json::json!({"source": "rate_limiter", "attempts": 100}),
            Some(Duration::from_secs(3600)),
        )
        .await?;

    println!("  Ban created: {:?}", detail.target);
    println!("  Reason: {}", detail.reason);
    println!("  Source: {:?}", detail.source);
    println!("  Duration: {:?}\n", detail.duration);

    let is_banned = ban_manager.is_banned(&target).await?;
    println!("  Is banned: {}\n", is_banned.is_some());

    Ok(())
}

async fn demo_user_ban() -> Result<(), FlowGuardError> {
    println!("--- User Ban Demo ---\n");

    let storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let ban_manager = BanManager::with_dependencies(storage, config).await?;

    let target = BanTarget::UserId("user-12345".to_string());
    println!("Creating manual ban for user: user-12345");

    let detail = ban_manager
        .create_ban(
            target.clone(),
            "terms of service violation".to_string(),
            BanSource::Manual {
                operator: "admin@example.com".to_string(),
            },
            serde_json::json!({"case_id": "CASE-2024-001", "severity": "high"}),
            Some(Duration::from_secs(86400)),
        )
        .await?;

    println!("  Ban created for user: {:?}", detail.target);
    println!("  Operator: {:?}", detail.source);
    println!("  Metadata: {:?}\n", detail.metadata);

    let history = ban_manager.get_history(&target).await?;
    println!("  Ban history: {:?}\n", history);

    Ok(())
}

async fn demo_ban_update() -> Result<(), FlowGuardError> {
    println!("--- Ban Update Demo ---\n");

    let storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let ban_manager = BanManager::with_dependencies(storage, config).await?;

    let target = BanTarget::Ip("10.0.0.50".to_string());
    println!("Creating initial ban...");

    ban_manager
        .create_ban(
            target.clone(),
            "rate limit exceeded".to_string(),
            BanSource::Auto,
            serde_json::json!({"requests": 1000}),
            Some(Duration::from_secs(300)),
        )
        .await?;

    let initial = ban_manager.is_banned(&target).await?.unwrap();
    println!("  Initial reason: {}", initial.reason);

    println!("\nUpdating ban reason...");
    let updated = ban_manager
        .update_ban(
            &target,
            Some("repeated rate limit violations".to_string()),
            None,
            None,
        )
        .await?;

    if let Some(record) = updated {
        println!("  Updated reason: {}", record.reason);
    }

    println!("\nRemoving ban...");
    ban_manager.delete_ban(&target, "admin".to_string()).await?;
    let is_banned = ban_manager.is_banned(&target).await?;
    println!("  Is banned after removal: {}\n", is_banned.is_some());

    Ok(())
}
