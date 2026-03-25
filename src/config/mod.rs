//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 配置模块
//!
//! 整合所有配置相关的子模块，提供统一的配置管理接口。
//!
//! # 使用 confers 库进行配置加载
//!
//! 本模块的类型定义支持通过 confers 库进行配置加载和验证：
//!
//! ```rust,ignore
//! use confers::ConfigBuilder;
//! use limiteron::config::FlowControlConfig;
//!
//! // 从文件加载配置
//! let config: FlowControlConfig = ConfigBuilder::new()
//!     .file("config.toml")
//!     .env_prefix("LIMITERON")
//!     .build()?;
//!
//! // 使用 garde 进行验证
//! use garde::Validate;
//! config.validate()?;
//! ```

// 子模块声明
pub mod types;

// Re-export all config types from types module
pub use crate::config::types::{
    Action, ActionConfig, BanConfig, BanScope, CacheBackend, ChangeSource, ConfigChangeRecord,
    ConfigHistory, FlowControlConfig, GlobalConfig, LimiterConfig, Matcher, MetricsBackend,
    OverdraftConfig, Rule, StorageType, TrustedProxyConfig,
};

// Backward compatibility alias
#[allow(unused_imports)]
pub use Matcher as ConfigMatcher;

// Re-export confers types for convenience (when confers feature is enabled)
#[cfg(feature = "confers")]
pub use confers::ConfigBuilder;
