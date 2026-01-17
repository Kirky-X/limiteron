//! Governor 流量控制器示例
//!
//! 本示例演示 Governor 的使用方式。
//!
//! 运行方式: `cargo run --example governor`

#[tokio::main]
async fn main() {
    println!("=== Governor 流量控制器示例 ===\n");

    println!("--- Governor 功能 ---\n");
    println!("Governor 是流量控制的主控制器，提供:");
    println!("  - 限流检查 (Rate Limiting)");
    println!("  - 封禁检查 (Ban Checking)");
    println!("  - 配额控制 (Quota Control)");
    println!("  - 熔断保护 (Circuit Breaking)");
    println!("  - 规则匹配 (Rule Matching)");

    println!("\n--- 使用方式 ---\n");
    println!("需要提供存储后端:");
    println!("  let storage: Arc<dyn Storage> = ...");
    println!("  let ban_storage: Arc<dyn BanStorage> = ...");
    println!("  let governor = Governor::new(config, storage, ban_storage).await?;");

    println!("\n--- 决策流程 ---\n");
    println!("  1. 提取标识符 (UserId, Ip, ApiKey, DeviceId)");
    println!("  2. 检查封禁状态");
    println!("  3. 应用速率限制");
    println!("  4. 应用配额控制");
    println!("  5. 检查熔断器状态");
    println!("  6. 返回最终决策");

    println!("\n=== 示例完成 ===");
}
