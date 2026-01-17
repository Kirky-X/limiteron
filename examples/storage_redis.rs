//! Redis 存储示例
//!
//! 本示例演示 Redis 存储后端的使用方式。
//!
//! 运行方式: `cargo run --example storage_redis`

#[tokio::main]
async fn main() {
    println!("=== Redis 存储示例 ===\n");

    println!("需要启用 redis 特性:");
    println!("  limiteron = {{ version = \"1.0\", features = [\"redis\"] }}");
    println!();

    println!("--- Redis 配置 ---\n");
    println!("连接配置:");
    println!("  - host: Redis 主机地址");
    println!("  - port: Redis 端口 (默认 6379)");
    println!("  - password: 密码 (可选)");
    println!("  - key_prefix: 键前缀");
    println!("  - pool_size: 连接池大小");

    println!("\n--- 功能说明 ---\n");
    println!("Redis 存储支持:");
    println!("  - 高性能: 内存存储，极快读写");
    println!("  - 分布式: 支持多节点部署");
    println!("  - 自动过期: 支持 TTL 设置");

    println!("\n--- 集群支持 ---\n");
    println!("Redis 集群配置:");
    println!("  - 支持多主节点");
    println!("  - 自动分片");
    println!("  - 故障转移");

    println!("\n=== 示例完成 ===");
}
