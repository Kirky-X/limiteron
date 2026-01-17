//! 决策链示例
//!
//! 本示例演示决策链的使用方式。
//!
//! 运行方式: `cargo run --example decision_chain`

#[tokio::main]
async fn main() {
    println!("=== 决策链示例 ===\n");

    println!("--- 决策链功能 ---\n");
    println!("DecisionChain 提供:");
    println!("  - 添加决策节点");
    println!("  - 执行决策流程");
    println!("  - 获取统计信息");

    println!("\n--- 决策节点类型 ---\n");
    println!("支持的决策节点类型:");
    println!("  - Ban: 封禁检查");
    println!("  - RateLimit: 速率限制");
    println!("  - Quota: 配额检查");
    println!("  - Concurrency: 并发控制");
    println!("  - CircuitBreaker: 熔断检查");
    println!("  - Custom: 自定义节点");

    println!("\n--- 使用方式 ---\n");
    println!("  let chain = DecisionChainBuilder::new()");
    println!("      .add_node(DecisionNode::RateLimit {{ ... }})");
    println!("      .add_node(DecisionNode::Quota {{ ... }})");
    println!("      .build();");
    println!();
    println!("  let result = chain.execute(key).await;");

    println!("\n--- 节点顺序 ---\n");
    println!("决策节点的执行顺序影响流量控制的行为:");
    println!("  - 先检查封禁 (Ban)");
    println!("  - 再速率限制 (RateLimit)");
    println!("  - 然后配额检查 (Quota)");
    println!("  - 最后并发控制 (Concurrency)");

    println!("\n=== 示例完成 ===");
}
