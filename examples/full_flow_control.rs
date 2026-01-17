//! 完整流量控制示例
//!
//! 本示例演示完整的流量控制集成。
//!
//! 运行方式: `cargo run --example full_flow_control`

#[tokio::main]
async fn main() {
    println!("=== 完整流量控制示例 ===\n");

    println!("--- 组件协同 ---\n");
    println!("流量控制的决策链:");
    println!("  1. 提取标识符 (IP, UserId, API Key 等)");
    println!("  2. 检查封禁状态");
    println!("  3. 应用速率限制");
    println!("  4. 应用配额控制");
    println!("  5. 检查熔断器状态");
    println!("  6. 返回最终决策");

    println!("\n--- 核心组件 ---\n");
    println!("  - Governor: 主控制器");
    println!("  - RuleMatcher: 规则匹配");
    println!("  - TokenBucketLimiter: 令牌桶限流");
    println!("  - FixedWindowLimiter: 固定窗口限流");
    println!("  - SlidingWindowLimiter: 滑动窗口限流");
    println!("  - ConcurrencyLimiter: 并发控制");
    println!("  - BanManager: 封禁管理");
    println!("  - QuotaController: 配额控制");
    println!("  - CircuitBreaker: 熔断保护");

    println!("\n--- 使用方式 ---\n");
    println!("需要提供存储后端:");
    println!("  let storage: Arc<dyn Storage> = ...");
    println!("  let ban_storage: Arc<dyn BanStorage> = ...");
    println!("  let governor = Governor::new(config, storage, ban_storage).await?;");

    println!("\n=== 示例完成 ===");
}
