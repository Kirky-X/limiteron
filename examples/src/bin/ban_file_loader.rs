// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! BanFileLoader 示例
//!
//! 演示 [`BanFileLoader`] 从 YAML 文件加载封禁规则到 [`BanManager`]，
//! 以及文件变更时的热重载（hot reload）能力。
//!
//! # 涵盖 API
//!
//! - [`BanFileLoader::new`] - 创建加载器，指定 YAML 文件路径
//! - [`BanFileLoader::load_once`] - 一次性加载封禁规则
//! - [`BanFileLoader::start_watching`] - 启动文件变更热重载
//! - [`BanFileLoader::stop_watching`] - 停止热重载
//! - `BanTarget::Ip` / `BanTarget::UserId` / `BanTarget::Mac` / `BanTarget::Geo`
//!   - 4 种封禁目标在 YAML 中的写法
//! - `LoadResult` - 加载结果（成功/失败计数与失败详情）
//!
//! # YAML 格式
//!
//! ```yaml
//! bans:
//!   - target:
//!       type: ip
//!       value: "192.168.1.1"
//!     reason: "恶意请求"
//!     duration_secs: 3600  # 可选，null = 使用退避算法
//! ```
//!
//! # 运行方式
//!
//! ```bash
//! cargo run --example ban_file_loader --features full
//! ```
//!
//! [`BanFileLoader`]: limiteron::BanFileLoader
//! [`BanManager`]: limiteron::BanManager
//! [`BanTarget::Ip`]: limiteron::BanTarget::Ip

use limiteron::ban::{BanFileLoader, BanManager, BanManagerConfig};
use limiteron::storage::{BanStorage, MemoryBanStorage};
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

/// 初始 YAML 文件内容：包含 ip/user/mac/geo 4 种封禁目标
const INITIAL_YAML: &str = r#"bans:
  - target:
      type: ip
      value: "192.168.1.100"
    reason: "Malicious request"
    duration_secs: 3600
  - target:
      type: user
      value: "user-001"
    reason: "Rule-violating user"
    duration_secs: 7200
  - target:
      type: mac
      value: "00:1A:2B:3C:4D:5E"
    reason: "Device ban"
  - target:
      type: geo
      value:
        country_code: "CN"
    reason: "Region ban"
"#;

/// 修改后的 YAML 文件内容：追加一条 IP 封禁，触发热重载
const UPDATED_YAML: &str = r#"bans:
  - target:
      type: ip
      value: "192.168.1.100"
    reason: "Malicious request"
    duration_secs: 3600
  - target:
      type: user
      value: "user-001"
    reason: "Rule-violating user"
    duration_secs: 7200
  - target:
      type: mac
      value: "00:1A:2B:3C:4D:5E"
    reason: "Device ban"
  - target:
      type: geo
      value:
        country_code: "CN"
    reason: "Region ban"
  - target:
      type: ip
      value: "10.0.0.50"
    reason: "Ban added by hot reload"
"#;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== BanFileLoader Demo ===\n");

    // 1. 创建临时 YAML 文件，写入初始封禁规则
    let mut temp_file = tempfile::NamedTempFile::new()?;
    temp_file.write_all(INITIAL_YAML.as_bytes())?;
    temp_file.flush()?;
    let file_path = temp_file.path().to_path_buf();
    println!("1. Created temp YAML file: {}", file_path.display());

    // 2. 创建 BanManager（使用内存存储）
    let storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let ban_manager = BanManager::with_dependencies(storage, config).await?;
    println!("2. Created BanManager (memory storage)\n");

    // 3. 使用 BanFileLoader::load_once 一次性加载封禁规则
    println!("3. Running load_once to load ban rules...");
    let loader = BanFileLoader::new(&file_path);
    let result = loader.load_once(&ban_manager).await?;
    println!(
        "   Load complete: {} succeeded, {} failed",
        result.success_count, result.failure_count
    );
    if !result.errors.is_empty() {
        for err in &result.errors {
            println!("   Failed: target={}, error={}", err.target_desc, err.error);
        }
    }
    println!();

    // 4. 验证加载结果：检查 4 种目标是否已写入
    println!("4. Verifying load results:");
    let targets_to_check = vec![
        ("IP", "192.168.1.100"),
        ("User", "user-001"),
        ("MAC", "00:1A:2B:3C:4D:5E"),
    ];
    for (label, value) in targets_to_check {
        let target = match label {
            "IP" => limiteron::BanTarget::Ip(value.to_string()),
            "User" => limiteron::BanTarget::UserId(value.to_string()),
            "MAC" => limiteron::BanTarget::Mac(value.to_string()),
            _ => unreachable!(),
        };
        let is_banned = ban_manager.is_banned(&target).await?.is_some();
        println!("   {} [{}] ban status: {}", label, value, is_banned);
    }
    let geo_banned = ban_manager
        .is_banned(&limiteron::BanTarget::Geo {
            country_code: "CN".to_string(),
        })
        .await?
        .is_some();
    println!("   Geo [CN] ban status: {}", geo_banned);
    println!();

    // 5. 启动文件热重载（start_watching）
    println!("5. Starting file hot reload (start_watching)...");
    loader.start_watching(ban_manager.clone()).await?;
    println!("   Hot reload started; watching for file changes\n");

    // 等待监听就绪
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 6. 修改文件触发热重载
    println!("6. Modifying YAML file to append an IP ban (10.0.0.50)...");
    std::fs::write(&file_path, UPDATED_YAML)?;
    println!("   File modified; waiting for hot reload to trigger...\n");

    // 等待 notify 触发 + 防抖（500ms）+ 重载完成
    // notify 轮询间隔 2s + 防抖 500ms，需等待足够时间
    tokio::time::sleep(Duration::from_secs(4)).await;

    // 7. 验证热重载结果
    println!("7. Verifying hot reload results:");
    let new_target = limiteron::BanTarget::Ip("10.0.0.50".to_string());
    let new_ban = ban_manager.is_banned(&new_target).await?;
    match new_ban {
        Some(record) => {
            println!(
                "   New ban loaded: target={:?}, reason={}",
                record.target, record.reason
            );
        }
        None => {
            println!(
                "   Warning: hot reload did not finish in the expected time (notify polling interval may be long)"
            );
            println!("   Reloading manually to demonstrate load_once idempotency...");
            let reload_result = loader.load_once(&ban_manager).await?;
            println!(
                "   Manual reload complete: {} succeeded, {} failed",
                reload_result.success_count, reload_result.failure_count
            );
        }
    }
    println!();

    // 8. 停止监听
    println!("8. Stopping file hot reload (stop_watching)");
    loader.stop_watching().await;
    println!("   Hot reload stopped\n");

    println!("=== BanFileLoader Demo complete ===");

    // 显式关闭临时文件以清理
    drop(temp_file);
    Ok(())
}
