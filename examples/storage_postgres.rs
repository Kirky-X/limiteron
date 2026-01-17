//! PostgreSQL 存储示例
//!
//! 本示例演示 PostgreSQL 存储后端的使用方式。
//!
//! 运行方式: `cargo run --example storage_postgres`

#[tokio::main]
async fn main() {
    println!("=== PostgreSQL 存储示例 ===\n");

    println!("需要启用 postgres 特性:");
    println!("  limiteron = {{ version = \"1.0\", features = [\"postgres\"] }}");
    println!();

    println!("--- PostgreSQL 配置 ---\n");
    println!("连接配置:");
    println!("  - host: 数据库主机地址");
    println!("  - port: 数据库端口 (默认 5432)");
    println!("  - username: 用户名");
    println!("  - password: 密码");
    println!("  - database: 数据库名");
    println!("  - pool_size: 连接池大小");

    println!("\n--- 功能说明 ---\n");
    println!("PostgreSQL 存储支持:");
    println!("  - 持久化存储: 数据保存在数据库中");
    println!("  - 事务支持: 支持事务操作");
    println!("  - 高可用: 支持主从复制");

    println!("\n--- 表结构 ---\n");
    println!("自动创建的表:");
    println!("  - limiteron_rate_limits: 速率限制计数");
    println!("  - limiteron_quotas: 配额记录");
    println!("  - limiteron_bans: 封禁记录");

    println!("\n=== 示例完成 ===");
}
