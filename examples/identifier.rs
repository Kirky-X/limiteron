//! 标识符提取示例
//!
//! 本示例演示标识符提取器的使用方式。
//!
//! 运行方式: `cargo run --example identifier`

#[tokio::main]
async fn main() {
    println!("=== 标识符提取示例 ===\n");

    println!("--- 标识符类型 ---\n");
    println!("支持的标识符类型:");
    println!("  - UserId: 用户唯一标识");
    println!("  - Ip: IP 地址");
    println!("  - Mac: MAC 地址");
    println!("  - ApiKey: API 密钥");
    println!("  - DeviceId: 设备标识");

    println!("\n--- 提取器类型 ---\n");
    println!("  - UserIdExtractor: 从请求头提取用户 ID");
    println!("  - IpExtractor: 从连接地址提取 IP");
    println!("  - ApiKeyExtractor: 从请求头提取 API Key");
    println!("  - DeviceIdExtractor: 从请求头提取设备 ID");
    println!("  - MacExtractor: 从请求头提取 MAC 地址");
    println!("  - CompositeExtractor: 组合多个提取器");

    println!("\n--- 使用方式 ---\n");
    println!("  let extractor = UserIdExtractor::new(Some(header), Some(query))");
    println!("  let context = RequestContext::new(user_id, Some(socket_addr));");
    println!("  if let Some(Identifier::UserId(id)) = extractor.extract(&context) {{ ... }}");

    println!("\n=== 示例完成 ===");
}
