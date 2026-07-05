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

#[cfg(feature = "admin-api")]
pub mod config;
#[cfg(feature = "admin-api")]
pub mod handlers;
#[cfg(feature = "admin-api")]
pub mod routes;
#[cfg(feature = "admin-api")]
pub mod server;
#[cfg(all(feature = "admin-api", test))]
mod test_support;

#[cfg(feature = "admin-api")]
pub use config::AdminApiConfig;
#[cfg(feature = "admin-api")]
pub use server::AdminServer;
