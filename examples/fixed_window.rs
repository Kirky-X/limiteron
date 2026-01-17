//! 固定窗口限流器示例
//!
//! 本示例演示如何使用 FixedWindowLimiter 进行固定窗口速率限制。
//!
//! 运行方式: `cargo run --example fixed_window`

use limiteron::limiters::{FixedWindowLimiter, Limiter};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    println!("=== 固定窗口限流器示例 ===\n");

    let limiter = FixedWindowLimiter::new(Duration::from_secs(1), 5);
    println!("创建固定窗口限流器: 窗口=1秒, 最大请求=5\n");

    println!("--- 演示窗口内请求 ---");
    for i in 1..=8 {
        match limiter.allow(1).await {
            Ok(true) => println!("请求 {}: ✅ 允许", i),
            Ok(false) => println!("请求 {}: ❌ 被限流", i),
            Err(e) => println!("请求 {}: ❌ 错误: {:?}", i, e),
        }
    }

    println!("\n--- 演示窗口重置 ---\n");
    println!("等待 1.1 秒让窗口重置...");
    sleep(Duration::from_millis(1100)).await;

    for i in 1..=3 {
        match limiter.allow(1).await {
            Ok(true) => println!("窗口重置后请求 {}: ✅ 允许", i),
            Ok(false) => println!("窗口重置后请求 {}: ❌ 被限流", i),
            Err(e) => println!("窗口重置后请求 {}: ❌ 错误: {:?}", i, e),
        }
    }

    println!("\n=== 示例完成 ===");
}
