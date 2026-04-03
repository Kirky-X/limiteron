//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 多租户命名空间模块
//!
//! 提供租户隔离的限流命名空间支持，确保不同租户的限流键相互隔离。

pub mod config;
pub mod resolver;

pub use config::Namespace;
pub use resolver::TenantResolver;
