//! 宏使用示例
//!
//! 演示使用 flow_control 宏进行声明式限流配置

#[cfg(feature = "macros")]
use limiteron::flow_control;

#[cfg(feature = "macros")]
#[flow_control(rate = "10/s")]
async fn api_handler() -> Result<String, limiteron::error::FlowGuardError> {
    Ok("Success".to_string())
}

#[cfg(feature = "macros")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 宏使用示例 ===");

    for i in 0..15 {
        match api_handler().await {
            Ok(result) => println!("请求 {} ✅ {}", i, result),
            Err(e) => println!("请求 {} ❌ {:?}", i, e),
        }
    }

    println!("\n示例完成!");
    Ok(())
}

#[cfg(not(feature = "macros"))]
fn main() {
    eprintln!("此示例需要 'macros' 特性");
    eprintln!("运行: cargo run --bin macro_usage --features macros");
}
