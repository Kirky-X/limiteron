// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 宏使用示例
//!
//! 注意: flow_control 宏需要内部 API (GLOBAL_LIMITER_MANAGER)
//! 当前版本暂不支持。推荐使用 Governor API 进行限流。

use limiteron::limiters::{Limiter, TokenBucketLimiter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Macro usage example ===\n");
    println!("flow_control macro unavailable (needs internal APIs)\n");

    // 演示使用 TokenBucketLimiter 进行限流
    println!("Rate limiting with TokenBucketLimiter:");
    let limiter = TokenBucketLimiter::new(10, 1); // 10 容量，每秒补充 1

    for i in 0..15 {
        match limiter.allow(1).await {
            Ok(true) => println!("request {} ✅ allowed", i),
            Ok(false) => println!("request {} ❌ limited", i),
            Err(e) => println!("request {} ❌ error: {:?}", i, e),
        }
    }

    println!("\nExample complete!");
    Ok(())
}
