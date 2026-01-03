//! 封禁管理示例
//!
//! 演示如何使用BanManager进行封禁管理，包括：
//! - 手动封禁
//! - 自动封禁（指数退避算法）
//! - 封禁检查
//! - 优先级管理

use limiteron::ban_manager::{BackoffConfig, BanManager, BanManagerConfig, BanSource};
use limiteron::config::FlowControlConfig;
use limiteron::governor::Governor;
use limiteron::matchers::RequestContext;
use limiteron::storage::{BanTarget, MemoryBanStorage, MemoryStorage};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志（如果启用了telemetry feature）
    #[cfg(feature = "telemetry")]
    tracing_subscriber::fmt::init();

    println!("=== Limiteron 封禁管理示例 ===\n");

    // 1. 创建BanManager
    println!("1. 创建BanManager");
    let ban_storage = Arc::new(MemoryBanStorage::new());
    let ban_manager = Arc::new(BanManager::new(ban_storage.clone(), None).await?);
    println!("✓ BanManager创建成功\n");

    // 2. 手动封禁用户
    println!("2. 手动封禁用户 user123");
    let target = BanTarget::UserId("user123".to_string());
    let ban_detail = ban_manager
        .create_ban(
            target.clone(),
            "违反服务条款".to_string(),
            BanSource::Manual {
                operator: "admin".to_string(),
            },
            serde_json::json!({"violation_type": "terms_of_service"}),
            Some(Duration::from_secs(3600)), // 封禁1小时
        )
        .await?;

    println!("  - 封禁ID: {}", ban_detail.id);
    println!("  - 封禁原因: {}", ban_detail.reason);
    println!("  - 封禁时长: {:?}\n", ban_detail.duration);

    // 3. 检查封禁状态
    println!("3. 检查用户封禁状态");
    if let Some(detail) = ban_manager.read_ban(&target).await? {
        println!("  - 用户被封禁");
        println!("  - 封禁次数: {}", detail.ban_times);
        println!("  - 过期时间: {}\n", detail.expires_at);
    } else {
        println!("  - 用户未被封禁\n");
    }

    // 4. 自动封禁（指数退避算法）
    println!("4. 自动封禁演示（指数退避算法）");
    let backoff_config = BackoffConfig {
        first_duration: 60,    // 第一次：1分钟
        second_duration: 300,  // 第二次：5分钟
        third_duration: 1800,  // 第三次：30分钟
        fourth_duration: 7200, // 第四次及以上：2小时
        max_duration: 86400,   // 最大：24小时
    };

    let ban_manager_config = BanManagerConfig {
        backoff: backoff_config,
        enable_auto_unban: true,
        auto_unban_interval: 60,
    };

    ban_manager.update_config(ban_manager_config).await?;

    // 模拟多次违规
    for i in 1..=5 {
        let target = BanTarget::Ip(format!("192.168.1.{}", i));
        let duration = ban_manager.calculate_ban_duration(i).await;
        println!("  - 第{}次违规: 封禁时长 {:?}", i, duration);
    }
    println!();

    // 5. 封禁优先级检查
    println!("5. 封禁优先级检查");
    let ip_target = BanTarget::Ip("192.168.1.100".to_string());
    let user_target = BanTarget::UserId("priority_user".to_string());

    // 创建封禁
    ban_manager
        .create_ban(
            user_target.clone(),
            "用户违规".to_string(),
            BanSource::Auto,
            serde_json::json!({}),
            None,
        )
        .await?;

    ban_manager
        .create_ban(
            ip_target.clone(),
            "IP违规".to_string(),
            BanSource::Auto,
            serde_json::json!({}),
            None,
        )
        .await?;

    // 检查优先级（IP应该优先）
    let targets = vec![user_target.clone(), ip_target.clone()];
    if let Some(detail) = ban_manager.check_ban_priority(&targets).await? {
        println!("  - 最高优先级封禁: {:?}", detail.target);
        println!("  - 原因: {}\n", detail.reason);
    }

    // 6. 解封
    println!("6. 解封用户 user123");
    let unbanned = ban_manager.delete_ban(&target, "admin".to_string()).await?;

    if unbanned {
        println!("  - 解封成功\n");
    } else {
        println!("  - 用户未被封禁\n");
    }

    // 7. 集成到Governor
    println!("7. 集成到Governor");
    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
        },
        rules: vec![limiteron::config::Rule {
            id: "default_rule".to_string(),
            name: "Default Rule".to_string(),
            priority: 100,
            matchers: vec![limiteron::config::Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![limiteron::config::LimiterConfig::TokenBucket {
                capacity: 1000,
                refill_rate: 10,
            }],
            action: limiteron::config::ActionConfig {
                on_exceed: "reject".to_string(),
                ban: None,
            },
        }],
    };

    let storage = Arc::new(MemoryStorage::new());
    let governor = Governor::new(config, storage, ban_storage.clone(), None, None).await?;

    // 封禁一个用户
    let ban_target = BanTarget::UserId("governor_user".to_string());
    ban_manager
        .create_ban(
            ban_target.clone(),
            "Governor测试封禁".to_string(),
            BanSource::Auto,
            serde_json::json!({}),
            Some(Duration::from_secs(300)),
        )
        .await?;

    // 检查请求
    let context = RequestContext::new()
        .with_header("X-User-Id", "governor_user")
        .with_client_ip("192.168.1.200");

    match governor.check(&context).await? {
        limiteron::Decision::Allowed(_) => println!("  - 请求被允许"),
        limiteron::Decision::Rejected(reason) => println!("  - 请求被拒绝: {}", reason),
        limiteron::Decision::Banned(info) => {
            println!("  - 请求被封禁");
            println!("  - 原因: {}", info.reason);
            println!("  - 解封时间: {}", info.banned_until);
        }
    }

    println!("\n=== 示例完成 ===");

    // 停止自动解封任务
    ban_manager.stop_auto_unban_task().await;

    Ok(())
}
