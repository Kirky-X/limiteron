// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Ban HTTP API 示例
//!
//! 演示通过 [`AdminServer`] 启动管理 API 服务器，并使用 HTTP 客户端
//! 调用 `POST /api/v1/ban` 和 `DELETE /api/v1/ban/{target}?type=` 端点，
//! 创建和解封 ip/user/mac/geo 四种类型的封禁。
//!
//! # 涵盖 API
//!
//! - [`AdminServer::new`] - 创建管理服务器（注入 Governor + AdminApiConfig）
//! - [`AdminServer::with_ban_manager`] - 注入 BanManager
//! - [`AdminServer::start`] - 启动 HTTP 服务器（后台运行）
//! - HTTP 端点：
//!   - `POST /api/v1/ban` - 创建封禁（支持 ip/user/mac/geo）
//!   - `DELETE /api/v1/ban/{target}?type=` - 解除封禁
//!   - `GET /api/v1/status` - 查询系统状态
//! - [`AdminApiConfig`] - 配置监听地址与 API Key
//!
//! # 请求格式
//!
//! `POST /api/v1/ban` 请求体（JSON）：
//! ```json
//! {
//!   "target": {"type": "ip", "value": "192.168.1.100"},
//!   "reason": "恶意请求",
//!   "duration_secs": 3600
//! }
//! ```
//!
//! `DELETE /api/v1/ban/{target}?type=ip` 请求体（JSON）：
//! ```json
//! {"operator": "admin"}
//! ```
//!
//! # 运行方式
//!
//! ```bash
//! cargo run --example ban_http_api --features full
//! ```
//!
//! [`AdminServer`]: limiteron::admin::AdminServer
//! [`AdminApiConfig`]: limiteron::admin::AdminApiConfig

use limiteron::admin::{AdminApiConfig, AdminServer};
use limiteron::ban::{BanManager, BanManagerConfig};
use limiteron::config::{FlowControlConfig, GlobalConfig, RuleBuilder};
use limiteron::storage::{BanStorage, MemoryBanStorage, MemoryStorage, Storage};
use limiteron::Governor;
use std::sync::Arc;

/// 管理 API 的 API Key（必须 ≥16 字符，否则 AdminApiConfig::validate 会失败）
const API_KEY: &str = "limiteron-demo-api-key-32chars!!";

/// 构建 BanManager（内存存储，禁用自动解封以避免后台任务干扰示例）
async fn make_ban_manager() -> BanManager {
    let storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    BanManager::with_dependencies(storage, config)
        .await
        .expect("创建 BanManager 失败")
}

/// 构建 Governor（需提供至少一条规则的非空配置，否则内部 panic）
async fn make_governor() -> Governor {
    let rule = RuleBuilder::new()
        .id("demo-rule")
        .name("Demo rate limit rule")
        .priority(100)
        .user_matcher(vec!["*".to_string()])
        .token_bucket(100, 10)
        .on_reject()
        .build()
        .expect("构建规则失败");

    let config = FlowControlConfig {
        version: "0.2.0".to_string(),
        global: GlobalConfig::default(),
        rules: vec![rule],
    };
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
    Governor::builder()
        .with_config(config)
        .with_storage(storage)
        .with_ban_storage(ban_storage)
        .build()
        .await
        .expect("构建 Governor 失败")
}

/// 构造 POST /api/v1/ban 请求体（4 种 target 类型）
fn make_create_ban_body(target_type: &str, value: &str, reason: &str) -> serde_json::Value {
    match target_type {
        "ip" | "user" | "mac" => serde_json::json!({
            "target": {"type": target_type, "value": value},
            "reason": reason,
            "duration_secs": 3600,
            "operator": "demo-operator",
        }),
        "geo" => serde_json::json!({
            "target": {"type": "geo", "value": {"country_code": value}},
            "reason": reason,
            "duration_secs": 86400,
            "operator": "demo-operator",
        }),
        _ => panic!("不支持的 target 类型: {}", target_type),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Ban HTTP API Demo ===\n");

    // 1. 构建 Governor + BanManager
    let governor = Arc::new(make_governor().await);
    let ban_manager = Arc::new(make_ban_manager().await);

    // 2. 配置 AdminApiConfig（监听 127.0.0.1 随机可用端口）
    //    使用端口 0 让 OS 分配端口，避免端口冲突
    let config = AdminApiConfig::new(API_KEY)
        .with_host("127.0.0.1")
        .with_port(0);

    // 3. 创建并启动 AdminServer
    //    AdminServer::start 会绑定 TcpListener 并阻塞，所以需要先获取路由
    //    这里改为手动绑定 listener 以获取实际端口，再 axum::serve 后台运行
    let admin_server = AdminServer::new(governor, config).with_ban_manager(ban_manager.clone());
    let router = admin_server.into_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let base_url = format!("http://{}", addr);
    println!("1. AdminServer 已在 {} 启动", base_url);

    // 后台运行 HTTP 服务器
    let server_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            eprintln!("AdminServer 错误: {}", e);
        }
    });

    // 4. 创建 HTTP 客户端
    let client = reqwest::Client::new();

    // 5. 演示 POST /api/v1/ban 创建 4 种类型的封禁
    println!("\n2. POST /api/v1/ban 创建封禁:");
    let ban_cases = [
        ("ip", "192.168.1.100", "恶意 IP 请求"),
        ("user", "user-001", "违规用户"),
        ("mac", "00:1A:2B:3C:4D:5E", "设备封禁"),
        ("geo", "CN", "地区封禁"),
    ];

    let mut created_targets: Vec<(String, String)> = Vec::new();
    for (target_type, value, reason) in ban_cases {
        let body = make_create_ban_body(target_type, value, reason);
        let resp = client
            .post(format!("{}/api/v1/ban", base_url))
            .bearer_auth(API_KEY)
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let json: serde_json::Value = resp.json().await?;
        println!(
            "   [{}] target={}/{} → status={}, success={}, ban_id={}",
            target_type,
            target_type,
            value,
            status.as_u16(),
            json["success"].as_bool().unwrap_or(false),
            json["data"]["id"].as_str().unwrap_or("N/A")
        );
        created_targets.push((target_type.to_string(), value.to_string()));
    }

    // 6. 演示 GET /api/v1/status 查询系统状态
    println!("\n3. GET /api/v1/status 查询状态:");
    let resp = client
        .get(format!("{}/api/v1/status", base_url))
        .bearer_auth(API_KEY)
        .send()
        .await?;
    let status_code = resp.status();
    let json: serde_json::Value = resp.json().await?;
    println!(
        "   status={}, active_bans={}",
        status_code.as_u16(),
        json["data"]["active_bans"].as_u64().unwrap_or(0)
    );

    // 7. 演示 DELETE /api/v1/ban/{target}?type= 解除封禁
    println!("\n4. DELETE /api/v1/ban/{{target}}?type= 解除封禁:");
    for (target_type, value) in &created_targets {
        let unban_body = serde_json::json!({
            "operator": "demo-admin",
            "reason": "示例解封"
        });
        let url = format!("{}/api/v1/ban/{}?type={}", base_url, value, target_type);
        let resp = client
            .delete(&url)
            .bearer_auth(API_KEY)
            .json(&unban_body)
            .send()
            .await?;

        let status = resp.status();
        let json: serde_json::Value = resp.json().await?;
        println!(
            "   [{}] target={} → status={}, success={}, message={}",
            target_type,
            value,
            status.as_u16(),
            json["success"].as_bool().unwrap_or(false),
            json["message"].as_str().unwrap_or("N/A")
        );
    }

    // 8. 验证解封后的状态
    println!("\n5. 验证解封后的系统状态:");
    let resp = client
        .get(format!("{}/api/v1/status", base_url))
        .bearer_auth(API_KEY)
        .send()
        .await?;
    let json: serde_json::Value = resp.json().await?;
    println!(
        "   active_bans={} (解封后应为 0)",
        json["data"]["active_bans"].as_u64().unwrap_or(0)
    );

    // 9. 演示错误场景：缺少 Authorization header → 401
    println!("\n6. 错误场景演示: 缺少 Authorization header");
    let resp = client
        .get(format!("{}/api/v1/status", base_url))
        .send()
        .await?;
    println!(
        "   status={} (预期 401 Unauthorized)",
        resp.status().as_u16()
    );

    // 10. 演示错误场景：无效 target 类型 → 400
    println!("\n7. 错误场景演示: 无效的 IP 地址 → 400");
    let bad_body = serde_json::json!({
        "target": {"type": "ip", "value": "999.999.999.999"},
        "reason": "无效 IP 测试"
    });
    let resp = client
        .post(format!("{}/api/v1/ban", base_url))
        .bearer_auth(API_KEY)
        .json(&bad_body)
        .send()
        .await?;
    let status = resp.status();
    let json: serde_json::Value = resp.json().await?;
    println!(
        "   status={}, message={}",
        status.as_u16(),
        json["message"].as_str().unwrap_or("N/A")
    );

    // 停止服务器
    server_handle.abort();
    println!("\n=== Ban HTTP API Demo 完成 ===");

    Ok(())
}
