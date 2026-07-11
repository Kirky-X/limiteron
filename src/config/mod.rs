// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 配置模块
//!
//! 整合所有配置相关的子模块，提供统一的配置管理接口。

// 子模块声明
pub(crate) mod loader;
pub(crate) mod types;
// config-watcher 模块尚未实现：BanFileLoader::start_watching 已提供文件级热重载能力，
// 配置级热重载计划在 v0.4.0 实现（见 roadmap）。
// #[cfg(feature = "config-watcher")]
// pub mod watcher;
// config-security 模块尚未实现：当前依赖 AdminApiConfig::validate() 进行基本校验，
// 完整配置安全审计计划在 v0.4.0 实现（见 roadmap）。
// #[cfg(feature = "config-security")]
// pub mod security;

// Re-export all config types from types module
pub use crate::config::types::{
    Action, ActionConfig, BanConfig, BanScope, CacheBackend, ChangeSource, ConfigBuilder,
    ConfigChangeRecord, ConfigHistory, FlowControlConfig, GlobalConfig, LimiterConfig, Matcher,
    MetricsBackend, OverdraftConfig, Rule, RuleBuilder, StorageType, TrustedProxyConfig,
};
// Backward compatibility alias
#[allow(unused_imports)]
pub use Matcher as ConfigMatcher;

// Re-export config loader
pub use crate::config::loader::ConfigLoader;

// Re-export config watcher (requires config-watcher feature)
// config-watcher 模块尚未实现：BanFileLoader::start_watching 已提供文件级热重载能力，
// 配置级热重载计划在 v0.4.0 实现（见 roadmap）。
// #[cfg(feature = "config-watcher")]
// pub use crate::config::watcher::{ConfigChangeCallback, ConfigWatcher, WatchMode};

// Re-export config security (requires config-security feature)
// config-security 模块尚未实现：当前依赖 AdminApiConfig::validate() 进行基本校验，
// 完整配置安全审计计划在 v0.4.0 实现（见 roadmap）。
// #[cfg(feature = "config-security")]
// pub use crate::config::security::{ConfigSecurityReport, ConfigSecurityValidator};
