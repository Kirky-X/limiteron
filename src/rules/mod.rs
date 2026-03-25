//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 规则管理模块
//!
//! 提供规则构建和统计管理功能。

// 子模块
pub mod builder;
pub mod stats;

// 重新导出 builder 模块的公共类型
pub use builder::RuleBuilder;

// 重新导出 stats 模块的公共类型
pub use stats::{StatsManager, StatsSnapshot};