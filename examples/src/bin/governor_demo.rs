//! Governor API 示例
//!
//! 演示 Governor 主控制器的三种构造模式、请求检查、决策解析与统计信息获取。
//!
//! # 涵盖 API
//!
//! - `Governor::new().await` (开箱即用模式)
//! - `Governor::builder()` + `with_config` / `with_storage` / `with_ban_storage` (Builder 模式)
//! - `FlowControlConfig` / `RuleBuilder` (配置构建)
//! - `Governor::check(&context).await` (请求检查)
//! - `Decision` 解析 (Allowed / Rejected / Banned)
//! - `Governor::stats().await` (统计信息)
//!
//! # 运行方式
//!
//! ```bash
//! cargo run --bin governor_demo
//! ```

use limiteron::config::{FlowControlConfig, GlobalConfig, RuleBuilder};
use limiteron::storage::{BanStorage, MemoryBanStorage, MemoryStorage, Storage};
use limiteron::{Decision, Governor, RequestContext};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Governor API Demo ===\n");

    demo_out_of_the_box().await?;
    demo_builder_pattern().await?;
    demo_decision_parsing().await?;
    demo_stats().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

/// 演示开箱即用模式：`Governor::new().await`
///
/// 使用内部默认的内存存储，无需任何外部配置。
/// 适用于快速原型、测试或单实例场景。
async fn demo_out_of_the_box() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 1. Out-of-the-box: Governor::new().await ---\n");

    let governor = Governor::new().await;
    println!("  Governor created with default memory storage");

    let context = RequestContext::new()
        .with_path("/api/v1/users")
        .with_method("GET")
        .with_client_ip("192.168.1.10")
        .with_header("X-User-Id", "user-001");

    let decision = governor.check(&context).await?;
    println!("  Decision for request: {}", format_decision(&decision));
    println!();
    Ok(())
}

/// 演示 Builder 模式：自定义配置与存储依赖注入
///
/// 通过 `Governor::builder()` 链式配置：
/// - `with_config`: 注入自定义 FlowControlConfig
/// - `with_storage`: 注入 Storage 实现
/// - `with_ban_storage`: 注入 BanStorage 实现
async fn demo_builder_pattern() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 2. Builder pattern: custom config + storage injection ---\n");

    // 构建一条规则：匹配 user-001，使用令牌桶限流（容量 5，每秒补充 1）
    let rule = RuleBuilder::new()
        .id("rule-user-001")
        .name("Rate limit for user-001")
        .priority(100)
        .user_matcher(vec!["user-001".to_string()])
        .token_bucket(5, 1)
        .on_reject()
        .build()?;

    let config = FlowControlConfig {
        version: "0.1.0".to_string(),
        global: GlobalConfig::default(),
        rules: vec![rule],
    };

    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

    let governor = Governor::builder()
        .with_config(config)
        .with_storage(storage)
        .with_ban_storage(ban_storage)
        .build()
        .await?;

    println!("  Governor built with 1 rule (token bucket: capacity=5, refill=1/s)");

    // 发送 7 个请求，预期前 5 个允许，后 2 个被限流
    let context = RequestContext::new()
        .with_path("/api/v1/data")
        .with_method("POST")
        .with_header("X-User-Id", "user-001");

    for i in 0..7 {
        let decision = governor.check(&context).await?;
        println!("  Request {}: {}", i, format_decision(&decision));
    }
    println!();
    Ok(())
}

/// 演示 Decision 解析
///
/// `Decision` 是一个枚举，包含三种变体：
/// - `Decision::Allowed(metadata)`: 请求被允许
/// - `Decision::Rejected(metadata)`: 请求被拒绝（限流）
/// - `Decision::Banned(ban_info)`: 请求被封禁
async fn demo_decision_parsing() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 3. Decision parsing ---\n");

    let governor = Governor::new().await;

    let context = RequestContext::new()
        .with_path("/api/v1/health")
        .with_method("GET");

    let decision = governor.check(&context).await?;

    match &decision {
        Decision::Allowed(metadata) => {
            println!(
                "  Allowed: limit={}, remaining={}",
                metadata.limit, metadata.remaining
            );
        }
        Decision::Rejected(metadata) => {
            println!(
                "  Rejected: reason={}, retry_after={}s",
                metadata.reason, metadata.retry_after
            );
        }
        Decision::Banned(info) => {
            println!(
                "  Banned: reason={}, ban_times={}",
                info.reason(),
                info.ban_times()
            );
        }
    }

    // 也可以使用 rate_limit_metadata 获取限流元数据
    if let Some(metadata) = decision.rate_limit_metadata() {
        println!(
            "  Rate limit metadata: limit={}, remaining={}, reset_at={}",
            metadata.limit, metadata.remaining, metadata.reset_at
        );
    }
    println!();
    Ok(())
}

/// 演示统计信息获取
///
/// `Governor::stats().await` 返回 `GovernorStats`，包含：
/// - total_requests: 总请求数
/// - allowed_requests: 允许的请求数
/// - rejected_requests: 拒绝的请求数
/// - banned_requests: 封禁的请求数
/// - error_count: 错误数
/// - last_updated: 最后更新时间
async fn demo_stats() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 4. GovernorStats ---\n");

    let governor = Governor::new().await;

    // 发送几个请求
    for i in 0..3 {
        let context = RequestContext::new()
            .with_path("/api/v1/test")
            .with_method("GET")
            .with_header("X-Request-Id", &format!("req-{}", i));
        let _ = governor.check(&context).await?;
    }

    let stats = governor.stats().await;
    println!("  Total requests:    {}", stats.total_requests);
    println!("  Allowed requests:  {}", stats.allowed_requests);
    println!("  Rejected requests: {}", stats.rejected_requests);
    println!("  Banned requests:   {}", stats.banned_requests);
    println!("  Error count:       {}", stats.error_count);
    if let Some(updated) = stats.last_updated {
        println!("  Last updated:      {}", updated);
    }
    println!();
    Ok(())
}

/// 格式化决策为可读字符串
fn format_decision(decision: &Decision) -> String {
    match decision {
        Decision::Allowed(_) => "✅ Allowed".to_string(),
        Decision::Rejected(m) => format!("❌ Rejected (retry_after={}s)", m.retry_after),
        Decision::Banned(info) => format!("🚫 Banned (reason={})", info.reason()),
    }
}
