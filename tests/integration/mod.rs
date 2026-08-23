// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 集成测试模块
//!
//! 测试各组件之间的集成和交互
//!
//! 注意：集成测试严禁使用Mock对象，必须测试真实的功能交互（本目录已全部产品化）。
//! 下沉的故障注入用例在 tests/unit/（unit 层允许测试替身）

pub mod ban_manager_storage;
pub mod cache_storage;
pub mod circuit_breaker_fallback;
pub mod config;
pub mod governor_limiters;
pub mod matcher_limiter;
#[cfg(all(feature = "quota-control", feature = "cache-storage"))]
pub mod quota_alert;
pub mod real_storage;
