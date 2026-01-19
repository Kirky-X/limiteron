//! 并发控制器示例
//!
//! 本示例演示如何使用 ConcurrencyLimiter 控制并发操作数量。
//!
//! 运行方式: `cargo run --example concurrency`

use futures::future::join_all;
use limiteron::limiters::ConcurrencyLimiter;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    println!("=== 并发控制器示例 ===\n");

    let limiter = ConcurrencyLimiter::new(3);
    println!("创建并发控制器: 最大并发=3\n");

    // 基本 acquire/release 模式
    println!("--- 演示 acquire/release 模式 ---\n");

    match limiter.acquire(1).await {
        Ok(permit) => {
            println!("✅ 成功获取许可");
            sleep(Duration::from_millis(100)).await;
            drop(permit);
            println!("🔓 释放许可\n");
        }
        Err(e) => println!("❌ 获取许可失败: {:?}", e),
    }

    // 并发限制演示
    println!("--- 演示并发限制 ---\n");
    let limiter = Arc::new(ConcurrencyLimiter::new(2));
    let mut handles = vec![];

    println!("启动 5 个并发任务 (最大并发=2)...");

    for i in 1..=5 {
        let limiter = Arc::clone(&limiter);
        handles.push(tokio::spawn(async move {
            match limiter.acquire(1).await {
                Ok(_permit) => {
                    println!("  [任务 {}] ✅ 获取许可", i);
                    sleep(Duration::from_millis(50)).await;
                    println!("  [任务 {}] 🔓 释放许可", i);
                    true
                }
                Err(_) => {
                    println!("  [任务 {}] ⏳ 等待超时", i);
                    false
                }
            }
        }));
    }

    let _ = futures::future::join_all(handles).await;

    // 超时演示
    println!("\n--- 演示带超时的并发控制 ---\n");
    let timeout_limiter = ConcurrencyLimiter::with_timeout(1, Duration::from_millis(100));

    match timeout_limiter.acquire(1).await {
        Ok(_permit) => println!("请求 1: ✅ 成功"),
        Err(e) => println!("请求 1: ❌ 失败: {:?}", e),
    }

    match timeout_limiter.acquire(1).await {
        Ok(_permit) => println!("请求 2: ✅ 成功"),
        Err(e) => println!("请求 2: ❌ 超时 (预期): {:?}", e),
    }

    println!("\n=== 示例完成 ===");
    println!("并发控制器适合限制同时操作的数量，如数据库连接、API调用等");
}
