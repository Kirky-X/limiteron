//! 流量控制宏使用示例
//!
//! 演示如何使用 `#[flow_control]` 宏为函数自动注入限流检查。

use limiteron::flow_control;

/// 简单的速率限制示例
///
/// 每秒最多允许100个请求
#[flow_control(rate = "100/s")]
async fn simple_rate_limit_api(user_id: &str) -> Result<String, limiteron::FlowGuardError> {
    Ok(format!("Hello, {}!", user_id))
}

/// 复合限流示例
///
/// - 速率限制：每秒最多100个请求
/// - 配额限制：每小时最多1000个请求
#[flow_control(rate = "100/s", quota = "1000/h")]
async fn composite_limit_api(user_id: &str) -> Result<String, limiteron::FlowGuardError> {
    Ok(format!("Hello, {}! This is a composite limit.", user_id))
}

/// 并发限制示例
///
/// 最多允许10个并发请求
#[flow_control(concurrency = 10)]
async fn concurrent_api(user_id: &str) -> Result<String, limiteron::FlowGuardError> {
    // 模拟耗时操作
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    Ok(format!("Hello, {}! This is a concurrent limit.", user_id))
}

/// 自定义配置示例
///
/// - 速率限制：每秒最多50个请求
/// - 标识符：使用user_id和ip
/// - 超限行为：拒绝
/// - 拒绝消息：自定义消息
#[flow_control(
    rate = "50/s",
    identifiers = ["user_id", "ip"],
    on_exceed = "reject",
    reject_message = "Too many requests, please try again later"
)]
async fn custom_config_api(user_id: &str, ip: &str) -> Result<String, limiteron::FlowGuardError> {
    Ok(format!("Hello, {} from {}!", user_id, ip))
}

/// 同步函数示例
///
/// 同步函数也支持限流
#[flow_control(rate = "10/s")]
fn sync_api(user_id: &str) -> Result<String, limiteron::FlowGuardError> {
    Ok(format!("Hello, {}! This is a sync function.", user_id))
}

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("=== 流量控制宏示例 ===\n");

    // 1. 简单的速率限制
    println!("1. 简单的速率限制示例:");
    for i in 0..5 {
        match simple_rate_limit_api("user123").await {
            Ok(result) => println!("   请求 {}: {}", i + 1, result),
            Err(e) => println!("   请求 {}: 错误 - {}", i + 1, e),
        }
    }
    println!();

    // 2. 复合限流
    println!("2. 复合限流示例:");
    for i in 0..5 {
        match composite_limit_api("user456").await {
            Ok(result) => println!("   请求 {}: {}", i + 1, result),
            Err(e) => println!("   请求 {}: 错误 - {}", i + 1, e),
        }
    }
    println!();

    // 3. 并发限制
    println!("3. 并发限制示例:");
    let mut handles = vec![];
    for i in 0..5 {
        let handle = tokio::spawn(async move { concurrent_api("user789").await });
        handles.push(handle);
    }

    for handle in handles {
        match handle.await {
            Ok(Ok(result)) => println!("   并发请求结果: {}", result),
            Ok(Err(e)) => println!("   并发请求错误: {}", e),
            Err(e) => println!("   任务错误: {}", e),
            Err(e) => println!("   并发请求错误: {}", e),
        }
    }
    println!();

    // 4. 自定义配置
    println!("4. 自定义配置示例:");
    for i in 0..3 {
        match custom_config_api("user000", "192.168.1.1").await {
            result => println!("   请求 {}: {}", i + 1, result),
        }
    }
    println!();

    // 5. 同步函数
    println!("5. 同步函数示例:");
    for i in 0..3 {
        match sync_api("user_sync") {
            Ok(result) => println!("   请求 {}: {}", i + 1, result),
            Err(e) => println!("   请求 {}: 错误 - {}", i + 1, e),
        }
    }
    println!();

    println!("示例完成！");
}
