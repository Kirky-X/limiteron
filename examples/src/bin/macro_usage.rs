//! 宏使用示例
//!
//! 注意: flow_control 宏需要内部 API (GLOBAL_LIMITER_MANAGER)
//! 当前版本暂不支持。推荐使用 Governor API 进行限流。

use limiteron::limiters::{Limiter, TokenBucketLimiter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 宏使用示例 ===\n");
    println!("flow_control 宏暂不可用（需要内部 API）\n");

    // 演示使用 TokenBucketLimiter 进行限流
    println!("使用 TokenBucketLimiter 进行限流:");
    let limiter = TokenBucketLimiter::new(10, 1); // 10 容量，每秒补充 1

    for i in 0..15 {
        match limiter.allow(1).await {
            Ok(true) => println!("请求 {} ✅ 允许", i),
            Ok(false) => println!("请求 {} ❌ 限流", i),
            Err(e) => println!("请求 {} ❌ 错误: {:?}", i, e),
        }
    }

    println!("\n示例完成!");
    Ok(())
}
