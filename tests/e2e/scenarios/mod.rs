// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! E2E 用户场景测试
//!
//! 包含完整的业务流程测试场景

pub mod ban_management;
pub mod circuit_breaker;
#[cfg(feature = "quota-control")]
pub mod quota_control;
pub mod rate_limiting;
