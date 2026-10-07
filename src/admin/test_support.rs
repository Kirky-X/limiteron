// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Admin 模块测试辅助函数
//!
//! 集中定义 `make_valid_config` / `make_governor` / `make_state` 等公共构造，
//! 供 handlers / routes / server 三个子模块的 `#[cfg(test)]` 模块复用，
//! 避免同一份辅助代码在多处逐字复制。

use std::sync::Arc;

use crate::admin::LimiteronState;
use crate::config::{
    Action, ActionConfig, FlowControlConfig, GlobalConfig, LimiterConfig, Matcher, Rule,
};
use crate::governor::Governor;
use crate::storage::{BanStorage, MemoryBanStorage, MemoryStorage, Storage};

/// 构造包含至少一条规则的合法 FlowControlConfig
///
/// `Governor::new()` 现内置兜底规则可直接使用；此函数用于需要
/// 自定义规则集（如断言特定规则行为）的测试场景。
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

/// LimiteronState 可选依赖集合
///
/// feature 关闭时对应字段整体缺席；调用点用 `..Default::default()` 补空，
/// 新增状态字段只需改此处与 `make_state_with` 两个组装点。
#[derive(Default)]
pub struct TestDeps {
    #[cfg(feature = "ban-manager")]
    pub ban_manager: Option<Arc<crate::BanManager>>,
    #[cfg(feature = "quota-control")]
    pub quota_controller: Option<Arc<crate::QuotaController>>,
    #[cfg(feature = "circuit-breaker")]
    pub circuit_breaker: Option<Arc<crate::CircuitBreaker>>,
    #[cfg(feature = "prometheus")]
    pub metrics: Option<Arc<crate::telemetry::Metrics>>,
}

/// 用既有实例组装 LimiteronState（状态组装的唯一入口）
#[cfg_attr(
    not(any(
        feature = "ban-manager",
        feature = "quota-control",
        feature = "circuit-breaker",
        feature = "prometheus"
    )),
    allow(unused_variables)
)]
pub fn make_state_with(governor: Arc<Governor>, deps: TestDeps) -> LimiteronState {
    LimiteronState {
        governor,
        #[cfg(feature = "ban-manager")]
        ban_manager: deps.ban_manager,
        #[cfg(feature = "quota-control")]
        quota_controller: deps.quota_controller,
        #[cfg(feature = "circuit-breaker")]
        circuit_breaker: deps.circuit_breaker,
        #[cfg(feature = "prometheus")]
        metrics: deps.metrics,
    }
}

/// 构造最小可用 LimiteronState（仅 Governor，可选组件均为 None）
pub async fn make_state() -> LimiteronState {
    make_state_with(Arc::new(make_governor().await), TestDeps::default())
}

/// 构造带 BanManager 的 LimiteronState（用于封禁相关测试）
#[cfg(feature = "ban-manager")]
pub async fn make_state_with_ban_manager() -> LimiteronState {
    use crate::BanManager;
    let ban_manager = Arc::new(
        BanManager::new()
            .await
            .expect("BanManager::new should succeed"),
    );
    make_state_with(
        Arc::new(make_governor().await),
        TestDeps {
            ban_manager: Some(ban_manager),
            ..Default::default()
        },
    )
}
