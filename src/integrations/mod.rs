// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! limiteron integration modules with external frameworks.
//!
//! Integrations are feature-gated so the core limiteron library stays
//! dependency-free when integrations are not needed.

#[cfg(feature = "inklog")]
pub mod inklog;

#[cfg(feature = "kit")]
pub mod kit;

#[cfg(feature = "config-confers")]
pub mod confers;

// dbnexus 查询限流端口：语义文档化 + limiteron 实现。
// 仅依赖核心 limiters（无外部依赖），决策热路径不经过本模块。
pub mod query_throttle;
