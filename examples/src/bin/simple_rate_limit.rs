// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 简单限流示例
//!
//! 演示最基本的限流器使用方式

use limiteron::limiters::{Limiter, TokenBucketLimiter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Simple rate limit example ===");

    let limiter = TokenBucketLimiter::new(10, 1);

    for i in 0..15 {
        match limiter.allow(1).await {
            Ok(true) => println!("request {} ✅ allowed", i),
            Ok(false) => println!("request {} ❌ limited", i),
            Err(e) => println!("request {} error: {:?}", i, e),
        }
    }

    println!("\nExample complete!");
    Ok(())
}
