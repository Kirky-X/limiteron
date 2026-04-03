//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 配置模块
//!
//! 整合所有配置相关的子模块，提供统一的配置管理接口。

// 子模块声明
#[cfg(feature = "confers")]
pub mod loader;
pub mod types;
// TODO: watcher and security modules need to be implemented
// #[cfg(feature = "config-watcher")]
// pub mod watcher;
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

// Re-export config loader (requires confers feature)
#[cfg(feature = "confers")]
pub use crate::config::loader::ConfigLoader;

// Re-export config watcher (requires config-watcher feature)
// TODO: implement config-watcher module
// #[cfg(feature = "config-watcher")]
// pub use crate::config::watcher::{ConfigChangeCallback, ConfigWatcher, WatchMode};

// Re-export config security (requires config-security feature)
// TODO: implement config-security module
// #[cfg(feature = "config-security")]
// pub use crate::config::security::{ConfigSecurityReport, ConfigSecurityValidator};
