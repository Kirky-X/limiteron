//! 封禁管理器示例
//!
//! 本示例演示 BanManager 的使用方式。
//!
//! 运行方式: `cargo run --example ban_manager`

#[tokio::main]
async fn main() {
    println!("=== 封禁管理器示例 ===\n");

    println!("需要启用 ban-manager 特性:");
    println!("  limiteron = {{ version = \"1.0\", features = [\"ban-manager\"] }}");
    println!();

    println!("--- BanManager 功能 ---\n");
    println!("封禁管理器提供:");
    println!("  - ban_user(): 封禁用户");
    println!("  - ban_ip(): 封禁 IP 地址");
    println!("  - is_banned(): 检查是否被封禁");
    println!("  - unban_user(): 解除用户封禁");
    println!("  - unban_ip(): 解除 IP 封禁");
    println!("  - list_bans(): 列出封禁记录");

    println!("\n--- 封禁优先级 ---\n");
    println!("封禁优先级（从高到低）:");
    println!("  1. IP 地址封禁 (BanPriority::Ip)");
    println!("  2. 用户 ID 封禁 (BanPriority::UserId)");
    println!("  3. MAC 地址封禁 (BanPriority::Mac)");
    println!("  4. 设备 ID 封禁 (BanPriority::DeviceId)");
    println!("  5. API Key 封禁 (BanPriority::ApiKey)");

    println!("\n--- 自动封禁 ---\n");
    println!("自动封禁支持指数退避:");
    println!("  - 第1次封禁: 1 分钟");
    println!("  - 第2次封禁: 5 分钟");
    println!("  - 第3次封禁: 30 分钟");
    println!("  - 第4次封禁: 2 小时");
    println!("  - 之后封禁: 24 小时");

    println!("\n=== 示例完成 ===");
}
