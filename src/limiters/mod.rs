//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 限流器模块
//!
//! 实现各种限流算法。

//! # 子模块
//!
//! - `traits`: Limiter trait 定义和通用验证函数
//! - `token_bucket`: 令牌桶限流器
//! - `sliding_window`: 滑动窗口限流器（已弃用）
//! - `sharded_sliding_window`: 分片滑动窗口限流器（推荐）
//! - `fixed_window`: 固定窗口限流器
//! - `concurrency`: 并发控制器
//! - `factory`: 限流器工厂
//! - `manager`: 限流器管理器

// 子模块
pub mod traits;
pub mod token_bucket;
#[allow(deprecated)]
pub mod sliding_window;
pub mod sharded_sliding_window;
pub mod fixed_window;
pub mod concurrency;
pub mod factory;
pub mod manager;

// Quota limiter (feature-gated)
#[cfg(feature = "quota-control")]
pub mod quota_limiter;

// Re-export all public types
pub use traits::Limiter;
pub use token_bucket::TokenBucketLimiter;
#[allow(deprecated)]
pub use sliding_window::SlidingWindowLimiter;
pub use sharded_sliding_window::ShardedSlidingWindowLimiter;
pub use fixed_window::FixedWindowLimiter;
pub use concurrency::{ConcurrencyLimiter, ConcurrencyLimiterBuilder};
pub use factory::LimiterFactory;
pub use manager::LimiterManager;

#[cfg(feature = "quota-control")]
pub use quota_limiter::QuotaLimiter;
