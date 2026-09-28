// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
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
//! - `manager`: 全局限流器管理器（供 `#[flow_control]` 宏使用）

// 子模块
pub mod batch_prefetch;
pub mod concurrency;
pub mod factory;
pub mod fixed_window;
#[cfg(feature = "gcra")]
pub mod gcra;
pub mod htb;
#[cfg(feature = "manager")]
pub mod manager;
pub mod sharded_sliding_window;
#[allow(deprecated)]
pub mod sliding_window;
pub mod token_bucket;
pub mod traits;

// 自适应并发限流器（AIMD 窗口；此前 no-op feature 的真实实现）
#[cfg(feature = "adaptive-limiting")]
pub mod adaptive;

// 优先级队列限流器（按优先级调度的配额分配；此前 no-op feature 的真实实现）
#[cfg(feature = "priority-queue")]
pub mod priority_queue;

// 准入控制器（并发 + 速率双门；此前 no-op feature 的真实实现）
#[cfg(feature = "admission-control")]
pub mod admission_control;

// Quota limiter (feature-gated)
#[cfg(feature = "quota-control")]
pub mod quota_limiter;

// Distributed limiter (feature-gated)
#[cfg(feature = "distributed")]
pub mod distributed;

// Re-export all public types
pub use batch_prefetch::{BatchTokenPrefetcher, PrefetchResult};
pub use concurrency::ConcurrencyLimiter;
pub use fixed_window::FixedWindowLimiter;
pub use htb::HierarchicalTokenBucket;
pub use sharded_sliding_window::ShardedSlidingWindowLimiter;
#[allow(deprecated)]
pub use sliding_window::SlidingWindowLimiter;
pub use token_bucket::TokenBucketLimiter;
pub use traits::{Limiter, RateLimitSnapshot};

#[cfg(feature = "adaptive-limiting")]
pub use adaptive::{AdaptiveConcurrencyConfig, AdaptiveConcurrencyLimiter, AdaptivePermit};

#[cfg(feature = "priority-queue")]
pub use priority_queue::{PriorityQueueConfig, PriorityQueueLimiter};

#[cfg(feature = "admission-control")]
pub use admission_control::{
    AdmissionControlConfig, AdmissionController, AdmissionPermit, AdmissionStats,
};

#[cfg(feature = "quota-control")]
pub use quota_limiter::QuotaLimiter;

#[cfg(feature = "gcra")]
pub use gcra::GcraLimiter;

#[cfg(feature = "distributed")]
pub use distributed::InMemoryDistributedLimiter;
#[cfg(all(feature = "distributed", feature = "lua-script"))]
pub use distributed::RedisDistributedLimiter;
#[cfg(feature = "distributed")]
pub use traits::DistributedLimiter;
