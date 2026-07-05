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
pub mod concurrency;
pub mod factory;
pub mod fixed_window;
#[cfg(feature = "gcra")]
pub mod gcra;
#[cfg(feature = "macros")]
pub(crate) mod manager;
pub mod sharded_sliding_window;
#[allow(deprecated)]
pub mod sliding_window;
pub mod token_bucket;
pub mod traits;

// Quota limiter (feature-gated)
#[cfg(feature = "quota-control")]
pub mod quota_limiter;

// Re-export all public types
pub use concurrency::ConcurrencyLimiter;
pub use fixed_window::FixedWindowLimiter;
pub use sharded_sliding_window::ShardedSlidingWindowLimiter;
pub use token_bucket::TokenBucketLimiter;
pub use traits::Limiter;

#[cfg(feature = "quota-control")]
pub use quota_limiter::QuotaLimiter;

#[cfg(feature = "gcra")]
pub use gcra::GcraLimiter;
