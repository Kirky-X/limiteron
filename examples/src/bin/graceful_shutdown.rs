// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 优雅关闭示例
//!
//! 演示 Governor 的 shutdown() 方法：
//! - 启动 Governor 处理请求
//! - 监听 Ctrl+C 信号
//! - 收到信号后调用 shutdown() 优雅关闭
//! - 验证 shutdown() 的幂等性

use limiteron::config::{
    Action, ActionConfig, FlowControlConfig, GlobalConfig, LimiterConfig, Matcher, Rule,
};
use limiteron::storage::{MemoryBanStorage, MemoryStorage};
use limiteron::{Governor, RequestContext};
use std::sync::Arc;
use std::time::Duration;

fn create_simple_config() -> FlowControlConfig {
    FlowControlConfig {
        version: "0.2.0".to_string(),
        global: GlobalConfig::default(),
        rules: vec![Rule {
            id: "shutdown_demo_rule".to_string(),
            name: "Shutdown Demo Rule".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["demo_user".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 100,
                refill_rate: 10,
            }],
            action: ActionConfig {
                on_exceed: Action::Degrade,
                ban: None,
            },
        }],
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 优雅关闭示例 ===");

    // 使用 builder 模式创建 Governor（带有效配置）
    let governor = Governor::builder()
        .with_config(create_simple_config())
        .with_storage(Arc::new(MemoryStorage::new()))
        .with_ban_storage(Arc::new(MemoryBanStorage::new()))
        .build()
        .await
        .expect("Governor build should succeed");
    println!("Governor 已启动");

    // 检查健康状态
    let health = governor.health_status().await;
    println!(
        "初始健康状态: storage={}, ban_storage={}, cache={}, bg_tasks={}",
        health.storage_healthy,
        health.ban_storage_healthy,
        health.cache_healthy,
        health.background_tasks_alive
    );

    // 模拟后台任务：订阅 shutdown_token
    let token = governor.shutdown_token().clone();
    let bg_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    println!("后台任务: 心跳");
                }
                _ = token.cancelled() => {
                    println!("后台任务: 收到关闭信号，退出");
                    break;
                }
            }
        }
    });

    // 处理几个请求（通过 Governor::check 实际执行限流检查）
    let mut context = RequestContext::new();
    context.user_id = Some("demo_user".to_string());
    context = context.with_path("/api/demo");
    for i in 0..3 {
        let decision = governor.check(&context).await?;
        println!("处理请求 {}: {:?}", i, decision);
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    // 监听 Ctrl+C 信号（带 2 秒超时，便于非交互式运行）
    println!("\n等待关闭信号 (Ctrl+C 或 2 秒超时)...");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("收到 Ctrl+C 信号");
        }
        _ = tokio::time::sleep(Duration::from_secs(2)) => {
            println!("超时触发关闭（非交互式模式）");
        }
    }

    // 调用 shutdown() 优雅关闭
    governor.shutdown().await?;
    println!("Governor shutdown() 调用完成");

    // 验证幂等性：再次调用 shutdown() 应返回 Ok
    let result = governor.shutdown().await;
    assert!(result.is_ok(), "shutdown() 应幂等返回 Ok");
    println!("第二次 shutdown() 幂等返回 Ok ✓");

    // 等待后台任务退出
    bg_handle.await?;
    println!("后台任务已退出");

    // 验证关闭后的健康状态
    let health_after = governor.health_status().await;
    println!(
        "关闭后健康状态: storage={}, ban_storage={}, cache={}, bg_tasks={}",
        health_after.storage_healthy,
        health_after.ban_storage_healthy,
        health_after.cache_healthy,
        health_after.background_tasks_alive
    );
    assert!(
        !health_after.background_tasks_alive,
        "关闭后 background_tasks_alive 应为 false"
    );
    println!("background_tasks_alive=false ✓");

    println!("\n示例完成! 优雅关闭成功");
    Ok(())
}
