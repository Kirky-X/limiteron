//! Redis 存储后端示例
//!
//! 演示如何使用 RedisStorage 作为存储后端，包括：
//! - 基本 KV 操作（set/get/delete）
//! - 封禁管理（add_ban/is_banned/remove_ban）
//! - 配额控制（consume/reset）
//!
//! # 运行前提
//! 1. 启动 Redis 服务器（默认 `redis://127.0.0.1:6379/`）
//! 2. 运行：`cargo run --features redis-storage --bin redis_storage`

use std::time::Duration;

use limiteron::storage::{
    BanRecord, BanStorage, BanTarget, QuotaStorage, RedisStorage, Storage,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Limiteron Redis 存储后端示例 ===\n");

    // 创建 RedisStorage 实例
    let storage = RedisStorage::new("redis://127.0.0.1:6379/").await?;
    println!("[1] RedisStorage 连接成功");

    // ============================================================
    // 1. 基本 KV 操作
    // ============================================================
    println!("\n--- 基本 KV 操作 ---");

    // SET
    storage.set("user:1001:name", "Alice", Some(60)).await?;
    println!("[2] SET user:1001:name = Alice (TTL=60s)");

    // GET
    let value = storage.get("user:1001:name").await?;
    println!("[3] GET user:1001:name = {:?}", value);

    // DELETE
    storage.delete("user:1001:name").await?;
    let deleted = storage.get("user:1001:name").await?;
    println!("[4] DELETE 后 GET user:1001:name = {:?}", deleted);

    // ============================================================
    // 2. 封禁管理
    // ============================================================
    println!("\n--- 封禁管理 ---");

    // 创建封禁记录
    let ban_record = BanRecord {
        target: BanTarget::Ip("192.168.1.100".to_string()),
        ban_times: 1,
        duration: Duration::from_secs(3600),
        banned_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + Duration::from_secs(3600),
        is_manual: true,
        reason: "异常流量检测".to_string(),
    };

    // 添加封禁
    storage.save(&ban_record).await?;
    println!("[5] 添加封禁: IP=192.168.1.100, 原因={}", ban_record.reason);

    // 检查是否被封禁
    let ban_check = storage
        .is_banned(&BanTarget::Ip("192.168.1.100".to_string()))
        .await?;
    println!("[6] is_banned(192.168.1.100) = {}", ban_check.is_some());

    // 获取封禁次数
    let ban_times = storage
        .get_ban_times(&BanTarget::Ip("192.168.1.100".to_string()))
        .await?;
    println!("[7] ban_times(192.168.1.100) = {}", ban_times);

    // 增加封禁次数
    let new_times = storage
        .increment_ban_times(&BanTarget::Ip("192.168.1.100".to_string()))
        .await?;
    println!("[8] increment_ban_times → {}", new_times);

    // 列出封禁记录
    let bans = storage.list_bans(true, 0, 10).await?;
    println!("[9] list_bans(active_only=true) → {} 条记录", bans.len());

    // 移除封禁
    storage
        .remove_ban(&BanTarget::Ip("192.168.1.100".to_string()))
        .await?;
    let after_remove = storage
        .is_banned(&BanTarget::Ip("192.168.1.100".to_string()))
        .await?;
    println!(
        "[10] remove_ban 后 is_banned = {}",
        after_remove.is_none()
    );

    // ============================================================
    // 3. 配额控制
    // ============================================================
    println!("\n--- 配额控制 ---");

    let user_id = "user:2001";
    let resource = "api:upload";
    let limit = 1000u64;
    let window = Duration::from_secs(3600);

    // 重置配额
    storage.reset(user_id, resource, limit, window).await?;
    println!("[11] reset quota: user={}, resource={}, limit={}", user_id, resource, limit);

    // 消费配额
    let cost = 150u64;
    let result = storage.consume(user_id, resource, cost, limit, window).await?;
    println!(
        "[12] consume(cost={}) → allowed={}, remaining={}",
        cost,
        result.allowed,
        result.remaining
    );

    // 获取配额信息
    let quota = storage.get_quota(user_id, resource).await?;
    if let Some(info) = quota {
        println!(
            "[13] get_quota → consumed={}, limit={}, remaining={}",
            info.consumed,
            info.limit,
            info.limit.saturating_sub(info.consumed)
        );
    }

    // 尝试超额消费
    let excess_cost = 900u64;
    let result = storage
        .consume(user_id, resource, excess_cost, limit, window)
        .await?;
    println!(
        "[14] consume(cost={}) → allowed={} (应被拒绝)",
        excess_cost,
        result.allowed
    );

    // 清理
    storage.reset(user_id, resource, limit, window).await?;
    println!("\n[15] 配额已重置，示例完成");

    Ok(())
}
