//! Redis存储和L3缓存使用示例
//!
//! 演示如何使用RedisStorage、Lua脚本和L3Cache

use limiteron::{L3Cache, L3CacheConfig, RedisConfig, RedisStorage, RetryStats, Storage};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("=== Redis存储和L3缓存使用示例 ===\n");

    // 示例1: 创建Redis存储
    println!("示例1: 创建Redis存储");
    let redis_config = RedisConfig::new("redis://127.0.0.1:6379")
        .db(0)
        .connection_timeout(Duration::from_secs(5))
        .io_timeout(Duration::from_secs(5))
        .max_retries(3)
        .enable_lua(true);

    match RedisStorage::new(redis_config).await {
        Ok(redis_storage) => {
            println!("✓ Redis存储创建成功");

            // 示例2: 基本CRUD操作
            println!("\n示例2: 基本CRUD操作");
            redis_storage.set("user:1", "Alice", Some(3600)).await?;
            println!("✓ 设置键值: user:1 = Alice");

            let value = redis_storage.get("user:1").await?;
            println!("✓ 获取键值: user:1 = {:?}", value);

            redis_storage.delete("user:1").await?;
            println!("✓ 删除键: user:1");

            // 示例3: 查看重试统计
            println!("\n示例3: 重试统计");
            let stats = redis_storage.retry_stats();
            println!("  总重试次数: {}", stats.total_retries());
            println!("  成功重试: {}", stats.successful_retries());
            println!("  失败重试: {}", stats.failed_retries());

            // 示例4: 滑动窗口限流
            println!("\n示例4: 滑动窗口限流");
            let (allowed, count, reset_time) = redis_storage
                .sliding_window("rate_limit:api", Duration::from_secs(60), 10)
                .await?;
            println!("  允许: {}", allowed);
            println!("  当前计数: {}", count);
            println!("  重置时间: {}", reset_time);

            // 示例5: 固定窗口限流
            println!("\n示例5: 固定窗口限流");
            let (allowed, count, reset_time) = redis_storage
                .fixed_window("rate_limit:api:fixed", Duration::from_secs(60), 10)
                .await?;
            println!("  允许: {}", allowed);
            println!("  当前计数: {}", count);
            println!("  重置时间: {}", reset_time);

            // 示例6: 令牌桶限流
            println!("\n示例6: 令牌桶限流");
            let (allowed, remaining, refill_time) = redis_storage
                .token_bucket("token_bucket:api", 100, 10, 1)
                .await?;
            println!("  允许: {}", allowed);
            println!("  剩余令牌: {}", remaining);
            println!("  下次补充时间: {}", refill_time);
        }
        Err(e) => {
            println!("✗ Redis存储创建失败: {}", e);
            println!("  将使用降级模式演示L3缓存");
        }
    }

    // 示例7: 创建L3缓存
    println!("\n示例7: 创建L3缓存");
    let l3_config = L3CacheConfig::new("redis://127.0.0.1:6379")
        .l2_capacity(1000)
        .l2_default_ttl(Duration::from_secs(300))
        .l3_default_ttl(Duration::from_secs(600))
        .enable_cache_penetration_protection(true);

    let l3_cache = L3Cache::new(l3_config).await?;
    println!("✓ L3缓存创建成功");
    println!("  降级状态: {}", l3_cache.is_degraded().await);

    // 示例8: L3缓存基本操作
    println!("\n示例8: L3缓存基本操作");
    l3_cache.set("cache:key1", "value1", None).await;
    println!("✓ 设置缓存: cache:key1 = value1");

    let value = l3_cache.get("cache:key1").await;
    println!("✓ 获取缓存: cache:key1 = {:?}", value);

    // 示例9: 批量操作
    println!("\n示例9: 批量操作");
    l3_cache
        .batch_set(&[
            ("batch:key1".to_string(), "value1".to_string(), None),
            ("batch:key2".to_string(), "value2".to_string(), None),
            ("batch:key3".to_string(), "value3".to_string(), None),
        ])
        .await;
    println!("✓ 批量设置3个键");

    let keys = vec![
        "batch:key1".to_string(),
        "batch:key2".to_string(),
        "batch:key3".to_string(),
    ];
    let result = l3_cache.batch_get(&keys).await;
    println!("✓ 批量获取: {} 个键", result.len());

    l3_cache
        .batch_delete(&["batch:key1".to_string(), "batch:key2".to_string()])
        .await;
    println!("✓ 批量删除2个键");

    // 示例10: get_or_load（缓存穿透保护）
    println!("\n示例10: get_or_load（缓存穿透保护）");
    let value = l3_cache
        .get_or_load("load:key1", || async {
            // 模拟从数据库加载
            Ok("loaded_from_db".to_string())
        })
        .await?;
    println!("✓ 加载值: load:key1 = {}", value);

    // 第二次从缓存获取
    let value = l3_cache
        .get_or_load("load:key1", || async {
            // 这次不会被调用
            Ok("should_not_be_called".to_string())
        })
        .await?;
    println!("✓ 从缓存获取: load:key1 = {}", value);

    // 示例11: 查看统计信息
    println!("\n示例11: L3缓存统计信息");
    let stats = l3_cache.stats();
    println!("  L2命中次数: {}", stats.l2_hits());
    println!("  L3命中次数: {}", stats.l3_hits());
    println!("  未命中次数: {}", stats.misses());
    println!("  总命中率: {:.2}%", stats.overall_hit_rate() * 100.0);
    println!("  降级次数: {}", stats.degradations());
    println!("  恢复次数: {}", stats.recoveries());
    println!("  缓存穿透保护次数: {}", stats.penetration_protections());

    // 示例12: 清空缓存
    println!("\n示例12: 清空缓存");
    l3_cache.clear().await;
    println!("✓ 缓存已清空");
    println!("  L2缓存大小: {}", l3_cache.l2_cache().len().await);

    // 关闭缓存
    l3_cache.shutdown().await;
    println!("\n✓ L3缓存已关闭");

    println!("\n=== 示例完成 ===");
    Ok(())
}
