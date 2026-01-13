//!
//! 简单速率限制示例
//!
//! 运行示例：
//! ```bash
//! cargo run --example simple_rate_limit --features macros
//! ```

use limiteron::flow_control;

/// 简单的速率限制示例
///
/// 每秒最多允许5个请求
#[flow_control(rate = "5/s")]
async fn simple_rate_limit_api(user_id: &str) -> Result<String, limiteron::FlowGuardError> {
    Ok(format!("Hello, {}!", user_id))
}

#[tokio::main]
async fn main() {
    println!("=== 简单速率限制示例 ===\n");

    // 尝试10个请求，前5个应该成功
    for i in 0..10 {
        match simple_rate_limit_api("user123").await {
            Ok(result) => println!("   请求 {}: {}", i + 1, result),
            Err(e) => println!("   请求 {}: 错误 - {}", i + 1, e),
        }
        // 添加短暂延迟，让速率限制有时间恢复
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }
}
