// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 从 limiteron 侧把 sdforge 应用接入限流防护层：
//! 装配 Governor → 包 Guard → 适配为 ForgeRateLimiter → 注入应用。
//!
//! 运行：`cargo run -p limiteron-sdforge --features sdforge --example sdforge_guard`

use limiteron::Governor;
use limiteron::config::{
    Action, ActionConfig, CacheBackend, FlowControlConfig, LimiterConfig, Matcher, MetricsBackend,
    Rule, StorageType,
};
use limiteron::storage::{MemoryBanStorage, MemoryStorage};
use limiteron_sdforge::{Guard, GuardConfig, GuardForgeAdapter, GuardIdentity};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 装配 Governor：规则、存储、熔断器（circuit-breaker feature）在此层挂接
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
                capacity: 3,
                refill_rate: 1,
            }],
            action: ActionConfig {
                on_exceed: Action::Reject,
                ban: None,
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

    // 2. 包 guard：身份维度与故障策略在此显式声明
    let guard = Guard::with_config(governor, GuardConfig::new(GuardIdentity::UserId));

    // 3. 适配为 sdforge 的 ForgeRateLimiter，交给 sdforge 应用（trait-kit
    //    AsyncKit 装配点）使用
    let limiter: Arc<dyn sdforge::domain::ForgeRateLimiter> =
        Arc::new(GuardForgeAdapter::new(guard));

    // 4. 模拟 sdforge 请求路径
    for i in 0..5 {
        let allowed = limiter.check("tenant:42").await?;
        println!(
            "request {i}: {}",
            if allowed { "ALLOWED" } else { "THROTTLED" }
        );
    }
    Ok(())
}
