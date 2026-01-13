//!
//! 配额管理示例
//!
//! 运行示例：
//! ```bash
//! cargo run --example quota_management --features macros,quota-control
//! ```

use limiteron::flow_control;

/// 复合限流示例（速率限制 + 配额限制）
///
/// - 速率限制：每秒最多5个请求
/// - 配额限制：每分钟最多100个请求
#[flow_control(rate = "5/s", quota = "100/m")]
async fn quota_api(user_id: &str) -> Result<String, limiteron::FlowGuardError> {
    Ok(format!("Hello, {}!", user_id))
}

#[tokio::main]
async fn main() {
    println!("=== 配额管理示例 ===\n");

    // 尝试多个请求
    for i in 0..10 {
        match quota_api("user456").await {
            Ok(result) => println!("   请求 {}: {}", i + 1, result),
            Err(e) => println!("   请求 {}: 错误 - {}", i + 1, e),
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }
}
