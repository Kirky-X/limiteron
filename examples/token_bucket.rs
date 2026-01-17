//! 令牌桶限流器示例
//!
//! 本示例演示如何使用 TokenBucketLimiter 进行速率限制。
//!
//! 运行方式: `cargo run --example token_bucket`

use limiteron::limiters::{Limiter, TokenBucketLimiter};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    println!("=== 令牌桶限流器示例 ===\n");

    // 创建令牌桶限流器: 桶容量=10, 补充速率=5/秒
    let limiter = TokenBucketLimiter::new(10, 5);
    println!("创建令牌桶: 容量=10, 补充速率=5/秒\n");

    // 演示基本限流
    println!("--- 演示基本限流 ---");
    for i in 1..=15 {
        match limiter.allow(1).await {
            Ok(true) => println!("请求 {}: ✅ 允许", i),
            Ok(false) => println!("请求 {}: ❌ 被限流", i),
            Err(e) => println!("请求 {}: ❌ 错误: {:?}", i, e),
        }
    }

    println!("\n--- 演示令牌补充 ---\n");
    println!("消费所有令牌后等待 1 秒...");
    for _ in 0..10 {
        let _ = limiter.allow(1).await;
    }
    sleep(Duration::from_secs(1)).await;

    match limiter.allow(1).await {
        Ok(true) => println!("1 秒后: ✅ 令牌已补充"),
        Ok(false) => println!("1 秒后: ❌ 仍被限流"),
        Err(e) => println!("1 秒后: ❌ 错误: {:?}", e),
    }

    println!("\n=== 示例完成 ===");
}
