// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 管理控制面API
//!
//! 提供轻量HTTP管理接口,用于在运行时查询和操作系统状态:
//! - 查看限流计数
//! - 管理封禁
//! - 调整配额
//! - 查看熔断器状态
//!
//! ## 使用方法
//! ```ignore
//! use limiteron::admin::AdminServer;
//! use limiteron::admin::AdminApiConfig;
//!
//! let config = AdminApiConfig::default();
//! let server = AdminServer::new(governor, config);
//! server.start().await?;
//! ```

#[cfg(feature = "admin-client")]
pub mod client;
#[cfg(feature = "admin-api")]
pub mod config;
#[cfg(feature = "admin-api")]
pub mod handlers;
#[cfg(feature = "openapi")]
pub mod openapi;
#[cfg(feature = "admin-api")]
pub mod routes;
#[cfg(feature = "admin-api")]
pub mod server;
#[cfg(feature = "admin-api")]
pub mod service;
#[cfg(feature = "admin-ui")]
pub mod web;

#[cfg(feature = "admin-api")]
pub use config::{AdminApiConfig, AdminRole};
#[cfg(feature = "admin-api")]
pub use server::AdminServer;
#[cfg(feature = "admin-api")]
pub use server::LimiteronState;
#[cfg(feature = "admin-api")]
pub use service::{AdminService, AdminServiceError, GovernorAdminService};
#[cfg(feature = "admin-ui")]
pub use web::{ReadOnlySnapshotSource, WebUiConfig, WebUiServer};
