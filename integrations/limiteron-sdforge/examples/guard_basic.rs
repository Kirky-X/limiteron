// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 核心 guard 示例（无需 sdforge feature）：限流 + 封禁升级的请求判定面。
//!
//! 运行：`cargo run -p limiteron-sdforge --example guard_basic`

use limiteron::Governor;
use limiteron::config::{
    Action, ActionConfig, BanConfig, BanScope, CacheBackend, FlowControlConfig, LimiterConfig,
    Matcher, MetricsBackend, Rule, StorageType,
};
use limiteron::storage::{MemoryBanStorage, MemoryStorage};
use limiteron_sdforge::{Guard, GuardConfig, GuardDecision, GuardIdentity};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: StorageType::Memory,
            cache: CacheBackend::Memory,
            metrics: MetricsBackend::Prometheus,
            trusted_proxies: Default::default(),
        },
        rules: vec![Rule {
            id: "api_guard".to_string(),
            name: "API Guard".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 2,
                refill_rate: 1,
            }],
            action: ActionConfig {
                on_exceed: Action::Reject,
                ban: Some(BanConfig {
                    threshold: 3,
                    initial_duration: "60s".to_string(),
                    backoff_multiplier: 2.0,
                    max_duration: "600s".to_string(),
                    scope: BanScope::User,
                }),
            },
        }],
    };
    let governor = Arc::new(
        Governor::builder()
            .with_config(config)
            .with_storage(Arc::new(MemoryStorage::new()))
            .with_ban_storage(Arc::new(MemoryBanStorage::new()))
            .build()
            .await?,
    );

    let guard = Guard::with_config(governor, GuardConfig::new(GuardIdentity::UserId));

    for i in 0..6 {
        match guard.check("tenant:42").await {
            GuardDecision::Allowed => println!("request {i}: ALLOWED"),
            GuardDecision::Throttled {
                reason,
                retry_after_secs,
            } => {
                println!("request {i}: THROTTLED ({reason}, retry after {retry_after_secs}s)")
            }
            GuardDecision::Banned {
                reason,
                banned_until,
                ban_times,
            } => {
                println!("request {i}: BANNED ({reason}) until {banned_until}, strike {ban_times}")
            }
        }
    }
    Ok(())
}
