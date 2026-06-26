//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 配额控制器模块
//!
//! 实现配额控制功能，支持多种配额类型、滑动窗口重置、透支功能和告警机制。
//! 包含告警去重缓存的自动清理机制，防止内存泄漏。

pub mod controller;

#[cfg(feature = "quota-control")]
pub use controller::{
    AlertChannel, AlertConfig, AlertInfo, QuotaConfig, QuotaController, QuotaControllerBuilder,
    QuotaState, DEFAULT_DEDUP_CLEANUP_INTERVAL_SECS, DEFAULT_DEDUP_WINDOW_SECS,
    DEFAULT_OVERDRAFT_LIMIT_PERCENT, DEFAULT_QUOTA_LIMIT, DEFAULT_WINDOW_SIZE_SECS,
};

// Re-export QuotaType from config module (single source of truth)
#[cfg(feature = "quota-control")]
pub use crate::config::types::QuotaType;
