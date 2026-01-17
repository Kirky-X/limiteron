//! 内存存储示例
//!
//! 本示例演示内存存储后端的使用方式。
//!
//! 运行方式: `cargo run --example storage_memory`

#[tokio::main]
async fn main() {
    println!("=== 内存存储示例 ===\n");

    println!("--- 存储接口 ---\n");
    println!("Storage trait 定义的方法:");
    println!("  - get_count(): 获取计数");
    println!("  - increment_count(): 增加计数");
    println!("  - reset_count(): 重置计数");

    println!("\n--- BanStorage 接口 ---\n");
    println!("BanStorage trait 定义的方法:");
    println!("  - is_banned(): 检查是否封禁");
    println!("  - add_ban(): 添加封禁");
    println!("  - remove_ban(): 移除封禁");
    println!("  - list_bans(): 列出封禁");

    println!("\n--- L2 缓存 ---\n");
    println!("L2Cache 提供内存缓存功能:");
    println!("  - set(): 设置缓存值");
    println!("  - get(): 获取缓存值");
    println!("  - delete(): 删除缓存");
    println!("  - clear(): 清空缓存");

    println!("\n=== 示例完成 ===");
}
