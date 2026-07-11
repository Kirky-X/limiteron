// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Tower 中间件层
//!
//! 提供基于 Tower Service/Layer 的 HTTP 中间件实现，将 Governor 流量控制
//! 集成到 Rust Web 框架（如 Axum、Hyper、Tower-http）中。
//!
//! # 架构
//!
//! ```text
//! Request → RateLimitLayer → RateLimitService → Inner Service
//!                                      ↓
//!                            Governor.check()
//!                                      ↓
//!                            注入响应头 → Response
//! ```
//!
//! # 使用示例
//!
//! ```rust,no_run
//! use limiteron::middleware::{RateLimitLayer, RateLimitConfig};
//! use limiteron::Governor;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     // 创建 Governor 实例
//!     let governor = Governor::new().await;
//!
//!     // 创建限流中间件层
//!     let layer = RateLimitLayer::new(
//!         Arc::new(governor),
//!         RateLimitConfig::default(),
//!     );
//!
//!     // 与 Tower 服务链集成
//!     // let service = layer.make_service(inner_service);
//! }
//! ```
//!
//! # 响应头
//!
//! 中间件会自动注入标准限流响应头：
//! - `RateLimit-Limit`: 限流上限
//! - `RateLimit-Remaining`: 剩余可用次数
//! - `RateLimit-Reset`: 重置时间戳（Unix 秒）
//! - `Retry-After`: 重试等待时间（秒，仅在请求被拒绝时）

// 子模块
mod headers;
mod tower_middleware;

// 重新导出公共类型
pub use headers::{RateLimitHeaderValues, inject_rate_limit_headers};
pub use tower_middleware::{IntoRequestContext, RateLimitConfig, RateLimitLayer, RateLimitService};
