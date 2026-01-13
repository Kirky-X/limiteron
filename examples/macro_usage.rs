//!
//! 流量控制宏示例
//!
//! 运行示例：
//! ```bash
//! cargo run --example macro_usage --features macros
//! ```

use limiteron::flow_control;

/// 简单的速率限制示例
///
/// 每秒最多允许5个请求
#[flow_control(rate = "5/s")]
async fn simple_rate_limit_api(user_id: &str) -> Result<String, limiteron::FlowGuardError> {
    Ok(format!("Hello, {}!", user_id))
}

/// 复合限流示例（速率限制 + 配额限制）
///
/// - 速率限制：每秒最多5个请求
/// - 配额限制：每分钟最多100个请求
#[flow_control(rate = "5/s", quota = "100/m")]
async fn composite_limit_api(user_id: &str) -> Result<String, limiteron::FlowGuardError> {
    Ok(format!("Hello, {}! Composite limit.", user_id))
}

/// 并发限制示例
///
/// 最多允许3个并发请求
#[flow_control(concurrency = 3)]
async fn concurrent_api(user_id: &str) -> Result<String, limiteron::FlowGuardError> {
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    Ok(format!("Hello, {}! Concurrent limit.", user_id))
}

#[tokio::main]
async fn main() {
    println!("=== 流量控制宏示例 ===\n");

    // 1. 简单速率限制
    println!("1. 简单速率限制:");
    for i in 0..7 {
        match simple_rate_limit_api("user123").await {
            Ok(result) => println!("   {}: {}", i + 1, result),
            Err(e) => println!("   {}: 错误 - {}", i + 1, e),
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }
    println!();

    // 2. 复合限流
    println!("2. 复合限流:");
    for i in 0..7 {
        match composite_limit_api("user456").await {
            Ok(result) => println!("   {}: {}", i + 1, result),
            Err(e) => println!("   {}: 错误 - {}", i + 1, e),
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }
    println!();

    // 3. 并发限制
    println!("3. 并发限制 (并发请求5个，只允许3个):");
    let mut handles = vec![];
    for i in 0..5 {
        let user_id = format!("user_{}", i);
        handles.push(tokio::spawn(async move { concurrent_api(&user_id).await }));
    }
    for (i, handle) in handles.into_iter().enumerate() {
        match handle.await.unwrap() {
            Ok(result) => println!("   任务 {}: {}", i + 1, result),
            Err(e) => println!("   任务 {}: 错误 - {}", i + 1, e),
        }
    }
}
