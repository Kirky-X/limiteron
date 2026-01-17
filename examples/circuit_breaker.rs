//! 熔断器示例
//!
//! 本示例演示 CircuitBreaker 的使用方式。
//!
//! 运行方式: `cargo run --example circuit_breaker`

#[tokio::main]
async fn main() {
    println!("=== 熔断器示例 ===\n");

    println!("需要启用 circuit-breaker 特性:");
    println!("  limiteron = {{ version = \"1.0\", features = [\"circuit-breaker\"] }}");
    println!();

    println!("--- 熔断器状态 ---\n");
    println!("  Closed (关闭): 正常运行，允许请求通过");
    println!("  Open (打开): 快速失败，拒绝请求");
    println!("  Half-Open (半开): 尝试恢复，允许有限请求");

    println!("\n--- 状态转换 ---\n");
    println!("  Closed -> Open: 失败次数达到阈值");
    println!("  Open -> Half-Open: 超时时间到期");
    println!("  Half-Open -> Closed: 成功次数达到阈值");
    println!("  Half-Open -> Open: 恢复期间再次失败");

    println!("\n--- 配置选项 ---\n");
    println!("  - failure_threshold: 失败次数或失败率阈值");
    println!("  - success_threshold: 成功次数或成功率阈值");
    println!("  - timeout: Open 状态持续时间");

    println!("\n=== 示例完成 ===");
}
