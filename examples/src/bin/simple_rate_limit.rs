// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 简单限流示例
//!
//! 演示最基本的限流器使用方式

use limiteron::limiters::{Limiter, TokenBucketLimiter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 简单限流示例 ===");

    let limiter = TokenBucketLimiter::new(10, 1);

    for i in 0..15 {
        match limiter.allow(1).await {
            Ok(true) => println!("请求 {} ✅ 允许", i),
            Ok(false) => println!("请求 {} ❌ 限流", i),
            Err(e) => println!("请求 {} 错误: {:?}", i, e),
        }
    }

    println!("\n示例完成!");
    Ok(())
}
