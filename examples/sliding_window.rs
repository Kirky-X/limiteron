//! 滑动窗口限流器示例
//!
//! 本示例演示如何使用 SlidingWindowLimiter 进行滑动窗口速率限制。
//!
//! 运行方式: `cargo run --example sliding_window`

use limiteron::limiters::{Limiter, SlidingWindowLimiter};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    println!("=== 滑动窗口限流器示例 ===\n");

    let limiter = SlidingWindowLimiter::new(Duration::from_secs(1), 5);
    println!("创建滑动窗口限流器: 窗口=1秒, 最大请求=5\n");

    println!("--- 演示滑动窗口限流 ---");
    for i in 1..=8 {
        match limiter.allow(1).await {
            Ok(true) => println!("请求 {}: ✅ 允许", i),
            Ok(false) => println!("请求 {}: ❌ 被限流", i),
            Err(e) => println!("请求 {}: ❌ 错误: {:?}", i, e),
        }
    }

    println!("\n--- 演示时间滑动特性 ---\n");
    println!("滑动窗口的核心特性: 边界随时间平滑滑动");

    let sliding_limiter = SlidingWindowLimiter::new(Duration::from_millis(100), 3);

    for _ in 1..=3 {
        let _ = sliding_limiter.allow(1).await;
    }
    println!("发送 3 个请求");

    println!("等待 110ms 让窗口滑动...");
    sleep(Duration::from_millis(110)).await;

    match sliding_limiter.allow(1).await {
        Ok(true) => println!("窗口滑动后: ✅ 允许"),
        Ok(false) => println!("窗口滑动后: ❌ 被限流"),
        Err(e) => println!("窗口滑动后: ❌ 错误: {:?}", e),
    }

    println!("\n=== 示例完成 ===");
    println!("滑动窗口适合需要平滑流量控制的场景");
}
