//! 配额控制器示例
//!
//! 本示例演示 QuotaController 的使用方式。
//!
//! 运行方式: `cargo run --example quota_controller`

#[tokio::main]
async fn main() {
    println!("=== 配额控制器示例 ===\n");

    println!("需要启用 quota-control 特性:");
    println!("  limiteron = {{ version = \"1.0\", features = [\"quota-control\"] }}");
    println!();

    println!("--- QuotaController 功能 ---\n");
    println!("配额控制器提供:");
    println!("  - allocate_quota(): 分配配额");
    println!("  - consume_quota(): 消耗配额");
    println!("  - get_quota_state(): 获取配额状态");
    println!("  - consume_quota_with_overdraft(): 透支配额");

    println!("\n--- 配额类型 ---\n");
    println!("支持的配额类型:");
    println!("  - UserDaily: 用户每日配额");
    println!("  - UserWeekly: 用户每周配额");
    println!("  - UserMonthly: 用户每月配额");
    println!("  - ApiKeyDaily: API Key 每日配额");
    println!("  - IpHourly: IP 每小时配额");

    println!("\n--- 配额告警 ---\n");
    println!("支持配额告警:");
    println!("  - 当配额使用率达到阈值时触发告警");
    println!("  - 支持多种告警渠道 (Log, Webhook 等)");

    println!("\n=== 示例完成 ===");
}
