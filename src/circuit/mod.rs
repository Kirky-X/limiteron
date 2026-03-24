//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 熔断器模块
//!
//! 提供熔断器功能，支持三状态转换和自动恢复。
//!
//! # 特性
//!
//! - **三状态**: Closed（关闭）、Open（打开）、HalfOpen（半开）
//! - **自动熔断**: 失败次数达到阈值自动熔断
//! - **自动恢复**: 超时后自动探测恢复
//! - **线程安全**: 使用Arc和原子操作保证线程安全
//! - **统计信息**: 提供详细的统计信息
//!
//! # 示例
//!
//! ```rust
//! use limiteron::circuit::{CircuitBreaker, CircuitBreakerConfig};
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = CircuitBreakerConfig::new(5, 2, Duration::from_secs(60));
//!     let breaker = CircuitBreaker::new(config);
//!
//!     let result = breaker.execute(|| async {
//!         // 执行操作
//!         Ok::<(), limiteron::error::FlowGuardError>(())
//!     }).await;
//! }
//! ```

pub mod types;

pub use types::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerBuilder};
