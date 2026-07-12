// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Admin 模块测试辅助函数
//!
//! 集中定义 `make_valid_config` / `make_governor` / `make_state` 等公共构造，
//! 供 handlers / routes / server 三个子模块的 `#[cfg(test)]` 模块复用，
//! 避免同一份辅助代码在多处逐字复制（diting HIGH #4）。

use std::sync::Arc;

use crate::admin::AppState;
use crate::config::{
    Action, ActionConfig, FlowControlConfig, GlobalConfig, LimiterConfig, Matcher, Rule,
};
use crate::governor::Governor;
use crate::storage::{BanStorage, MemoryBanStorage, MemoryStorage, Storage};

/// 构造包含至少一条规则的合法 FlowControlConfig
///
/// `Governor::new()` 默认配置无规则会 panic，故测试需要一个非空规则集。
pub fn make_valid_config() -> FlowControlConfig {
    FlowControlConfig {
        version: "0.1.0".to_string(),
        global: GlobalConfig::default(),
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 100,
                refill_rate: 10,
            }],
            action: ActionConfig {
                on_exceed: Action::Reject,
                ban: None,
            },
        }],
    }
}

/// 构造可用的 Governor 实例（避免 `Governor::new()` 的空配置 panic）
pub async fn make_governor() -> Governor {
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
    Governor::builder()
        .with_config(make_valid_config())
        .with_storage(storage)
        .with_ban_storage(ban_storage)
        .build()
        .await
        .expect("Governor build should succeed with valid config")
}

/// 构造最小可用 AppState（仅 Governor，可选组件均为 None）
pub async fn make_state() -> AppState {
    let governor = Arc::new(make_governor().await);
    AppState {
        governor,
        #[cfg(feature = "ban-manager")]
        ban_manager: None,
        #[cfg(feature = "quota-control")]
        quota_controller: None,
        #[cfg(feature = "circuit-breaker")]
        circuit_breaker: None,
    }
}

/// 构造带 BanManager 的 AppState（用于封禁相关测试）
#[cfg(feature = "ban-manager")]
pub async fn make_state_with_ban_manager() -> AppState {
    use crate::BanManager;
    let governor = Arc::new(make_governor().await);
    let ban_manager = Arc::new(
        BanManager::new()
            .await
            .expect("BanManager::new should succeed"),
    );
    AppState {
        governor,
        ban_manager: Some(ban_manager),
        #[cfg(feature = "quota-control")]
        quota_controller: None,
        #[cfg(feature = "circuit-breaker")]
        circuit_breaker: None,
    }
}
