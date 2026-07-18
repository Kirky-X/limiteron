// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 测试模块根目录
//!
//! 导出所有功能模块的测试
//!
//! 注意：storage测试已移除，存储由dbnexus完全接管

#[allow(unused_imports)]
pub mod adapters;
#[cfg(feature = "audit-log")]
#[allow(unused_imports)]
pub mod audit_log;
#[allow(unused_imports)]
pub mod authorization;
#[allow(unused_imports)]
pub mod ban_manager;
#[cfg(feature = "cache-service")]
#[allow(unused_imports)]
pub mod cache_service;
#[allow(unused_imports)]
pub mod circuit_breaker;
#[allow(unused_imports)]
pub mod config;
#[allow(unused_imports)]
pub mod decision_chain;
#[allow(unused_imports)]
pub mod fallback;
#[allow(unused_imports)]
pub mod governor;
#[allow(unused_imports)]
pub mod limiters;
#[allow(unused_imports)]
pub mod matchers;
#[cfg(feature = "quota-control")]
#[allow(unused_imports)]
pub mod quota;
#[allow(unused_imports)]
pub mod telemetry;
#[allow(unused_imports)]
pub mod validation;

#[allow(unused_imports)]
pub use adapters::*;
#[cfg(feature = "audit-log")]
#[allow(unused_imports)]
pub use audit_log::*;
#[allow(unused_imports)]
pub use authorization::*;
#[allow(unused_imports)]
pub use ban_manager::*;
#[cfg(feature = "cache-service")]
#[allow(unused_imports)]
pub use cache_service::*;
#[allow(unused_imports)]
pub use circuit_breaker::*;
#[allow(unused_imports)]
pub use config::*;
#[allow(unused_imports)]
pub use decision_chain::*;
#[allow(unused_imports)]
pub use fallback::*;
#[allow(unused_imports)]
pub use governor::*;
#[allow(unused_imports)]
pub use limiters::*;
#[allow(unused_imports)]
pub use matchers::*;
#[cfg(feature = "quota-control")]
#[allow(unused_imports)]
pub use quota::*;
#[allow(unused_imports)]
pub use telemetry::*;
#[allow(unused_imports)]
pub use validation::*;
