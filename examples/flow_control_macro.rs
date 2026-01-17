//! Flow Control 宏示例
//!
//! 本示例演示 #[flow_control] 宏的使用方式。
//!
//! 运行方式: `cargo run --example flow_control_macro`

#[tokio::main]
async fn main() {
    println!("=== Flow Control 宏示例 ===\n");

    println!("需要启用 macros 特性:");
    println!("  limiteron = {{ version = \"1.0\", features = [\"macros\"] }}");
    println!();

    println!("--- 宏配置选项 ---\n");
    println!("#[flow_control] 支持的配置:");
    println!("  - rate: 速率限制 (如 \"100/s\", \"1000/m\")");
    println!("  - quota: 配额限制 (如 \"10000/d\", \"1000/m\")");
    println!("  - concurrency: 并发限制");
    println!("  - ban_after: 超过多少次后封禁");
    println!("  - ban_duration: 封禁时长");

    println!("\n--- 使用示例 ---\n");
    println!("  use limiteron::flow_control;");
    println!();
    println!("  #[flow_control(rate = \"100/s\")]");
    println!("  async fn public_api(user_id: &str) -> Result<String, FlowGuardError> {{");
    println!("      Ok(format!(\"Hello, {{user_id}}\"))");
    println!("  }}");
    println!();
    println!("  #[flow_control(rate = \"10/s\", quota = \"1000/m\")]");
    println!("  async fn premium_api(user_id: &str) -> Result<String, FlowGuardError> {{");
    println!("      Ok(format!(\"Premium for {{user_id}}\"))");
    println!("  }}");

    println!("\n=== 示例完成 ===");
}
