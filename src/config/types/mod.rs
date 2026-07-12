// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 配置模块
//!
//! 定义流量控制的配置结构。

use ahash::AHashSet as HashSet;
use chrono::Utc;
use serde::{Deserialize, Serialize};

// 子模块
mod actions;
mod config;
mod history;
mod limiter;
mod limiter_type;
mod quota_type;
mod rule;

pub use actions::{Action, ActionConfig, BanConfig, BanScope, CacheBackend, MetricsBackend};
pub use config::{GlobalConfig, StorageType, TrustedProxyConfig};
pub use history::{ChangeSource, ConfigChangeRecord, ConfigHistory};
pub(crate) use limiter::parse_window_size;
pub use limiter::{LimiterConfig, OverdraftConfig};
pub use limiter_type::LimiterTypeName;
pub use quota_type::QuotaType;
pub use rule::Matcher;
pub use rule::Rule;

/// 流量控制配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowControlConfig {
    pub version: String,
    pub global: GlobalConfig,
    pub rules: Vec<Rule>,
}

// ============================================================================
// Confers Integration (可选特性)
// ============================================================================
// 注意: confers API 不提供 ConfigMap, Validate, Sanitize traits
// 如需使用 confers 的完整功能，请为 FlowControlConfig derive confers::Config
// 当前实现保持 confers feature 可编译，但不提供额外的 trait 实现

// ============================================================================
// ConfigBuilder - 程序化配置构建（始终可用，不依赖confers）
// ============================================================================

/// 配置构建器
///
/// 提供流式API构建FlowControlConfig配置，不依赖confers库.
///
/// # 示例
///
/// ```rust
/// use limiteron::config::ConfigBuilder;
///
/// let config = ConfigBuilder::new()
///     .with_storage("memory".into())
///     .with_cache("memory".into())
///     .with_metrics("prometheus".into())
///     .with_rule(|rule| {
///         rule.id("default")
///             .name("Default Rule")
///             .priority(100)
///             .token_bucket(1000, 100)
///     })
///     .build();
/// ```
#[derive(Clone, Debug)]
pub struct ConfigBuilder {
    /// 全局配置
    storage: StorageType,
    cache: CacheBackend,
    metrics: MetricsBackend,
    /// 可信代理配置
    trusted_proxies: TrustedProxyConfig,
    /// 规则列表
    rules: Vec<RuleBuilder>,
}

/// 规则构建器
#[derive(Clone, Debug)]
pub struct RuleBuilder {
    id: String,
    name: String,
    priority: u16,
    matchers: Vec<Matcher>,
    limiters: Vec<LimiterConfig>,
    action: ActionConfig,
}

mod types_impl;
