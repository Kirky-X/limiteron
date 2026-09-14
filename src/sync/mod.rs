// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 同步限流原语（tokio-free）
//!
//! 为纯同步消费者（CLI 工具、嵌入式运行时、`block_on` 桥接层）提供与 async
//! 版本（[`crate::circuit`]、[`crate::limiters`]）状态机语义一致的同步实现。
//! 本模块仅依赖 `std` + `parking_lot`，不引入 tokio 与任何异步运行时。
//!
//! # 与 async 版本的设计差异
//!
//! - 时间经 [`Clock`] 注入（默认 [`SystemClock`]），测试用 [`MockClock`]
//!   （`test-clock` feature）手动推进，不依赖真实睡眠；
//! - [`SyncCircuitBreaker`] 的半开探针准入由状态机内建：`Open` 冷却到期后
//!   首个 `admit` 调用原子地完成 `Open → HalfOpen` 转换并成为探针，后续调用
//!   在探针结算前一律拒绝（async 版经 `half_open_max_calls` CAS 放行多个探针）；
//! - 不含慢调用率熔断与 [`ErrorClassifier`](crate::circuit::ErrorClassifier)：
//!   同步场景下调用方持有原始错误类型，失败判定由包装的闭包自身决定
//!   （`Result` 的 `Err` 即失败）。
//!
//! # 示例
//!
//! ```rust
//! use limiteron::sync::{SyncCircuitBreaker, SyncFixedWindowLimiter};
//! use std::time::Duration;
//!
//! // 连续失败 3 次熔断，冷却 30 秒
//! let breaker = SyncCircuitBreaker::new(3, Duration::from_secs(30));
//! let result = breaker.call(|| Ok::<i32, String>(42)).unwrap();
//! assert_eq!(result, 42);
//!
//! // 每标识每 60 秒窗口至多 120 次
//! let limiter = SyncFixedWindowLimiter::new(120, Duration::from_secs(60));
//! assert!(limiter.check("client-a").is_ok());
//! ```

pub mod circuit;
pub mod fixed_window;

pub use circuit::{CircuitCallError, SyncCircuitBreaker};
pub use fixed_window::{RateLimitRejection, SyncFixedWindowLimiter};
