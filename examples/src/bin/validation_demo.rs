// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Validation 示例
//!
//! 演示 limiteron 的统一验证模块，涵盖 IP 地址、用户 ID、MAC 地址、
//! API Key、封禁目标等多种验证函数。
//!
//! # 涵盖 API
//!
//! - `validate_ip_address` (IPv4 / IPv6 / 带端口)
//! - `validate_user_id` (允许字母数字、`_`、`-`、`@`、`.`)
//! - `validate_mac_address` (支持 `:`/`-`/`.` 分隔符)
//! - `validate_api_key` / `validate_ban_reason` / `validate_header_value` / `validate_path`
//! - `validate_length` (通用长度验证)
//! - `validate_ban_target` (针对 `BanTarget` 枚举的验证)
//!
//! # 运行方式
//!
//! ```bash
//! cargo run --bin validation_demo --features validation
//! ```

use limiteron::BanTarget;
use limiteron::validation::{
    validate_api_key, validate_ban_reason, validate_ban_target, validate_header_value,
    validate_ip_address, validate_length, validate_mac_address, validate_path, validate_user_id,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Validation Demo ===\n");

    demo_ip_validation();
    demo_user_id_validation();
    demo_mac_validation();
    demo_length_based_validations();
    demo_ban_target_validation();

    println!("\n=== All demos completed ===");
    Ok(())
}

/// 演示 IP 地址验证（IPv4 / IPv6 / 带端口）
fn demo_ip_validation() {
    println!("--- 1. IP Address Validation ---\n");

    let valid_ipv4 = [
        "192.168.1.1",
        "10.0.0.1",
        "127.0.0.1:8080",
        "255.255.255.255",
    ];
    let valid_ipv6 = [
        "::1",
        "2001:db8::1",
        "::ffff:192.168.1.1",
        "[2001:db8::1]:443",
    ];
    let invalid_ips = ["", "256.1.1.1", "192.168.1", "gggg::1", "[::1"];

    println!("  Valid IPv4 addresses:");
    for ip in &valid_ipv4 {
        let result = validate_ip_address(ip);
        println!("    {:<25} -> {}", ip, format_result(&result));
    }

    println!("\n  Valid IPv6 addresses:");
    for ip in &valid_ipv6 {
        let result = validate_ip_address(ip);
        println!("    {:<30} -> {}", ip, format_result(&result));
    }

    println!("\n  Invalid IP addresses:");
    for ip in &invalid_ips {
        let result = validate_ip_address(ip);
        println!("    {:<25} -> {}", ip, format_result(&result));
    }
    println!();
}

/// 演示用户 ID 验证
fn demo_user_id_validation() {
    println!("--- 2. User ID Validation ---\n");

    let valid_ids = [
        "user123",
        "user-name_123",
        "user@example.com",
        "first.last@domain.com",
    ];
    let invalid_ids = ["", "user name", "user@#$%", "user/flag"];

    println!("  Valid user IDs:");
    for id in &valid_ids {
        let result = validate_user_id(id);
        println!("    {:<30} -> {}", id, format_result(&result));
    }

    println!("\n  Invalid user IDs:");
    for id in &invalid_ids {
        let result = validate_user_id(id);
        println!("    {:<30} -> {}", id, format_result(&result));
    }
    println!();
}

/// 演示 MAC 地址验证（支持多种分隔符）
fn demo_mac_validation() {
    println!("--- 3. MAC Address Validation ---\n");

    let valid_macs = [
        "00:1A:2B:3C:4D:5E",
        "00-1A-2B-3C-4D-5E",
        "001A.2B3C.4D5E",
        "001A2B3C4D5E",
        "aa:bb:cc:dd:ee:ff",
    ];
    let invalid_macs = [
        "",
        "00:1A:2B:3C:4D",
        "00:1A:2B:3C:4D:5E:6F",
        "00:1A:2B:3C:4D:GG",
    ];

    println!("  Valid MAC addresses:");
    for mac in &valid_macs {
        let result = validate_mac_address(mac);
        println!("    {:<25} -> {}", mac, format_result(&result));
    }

    println!("\n  Invalid MAC addresses:");
    for mac in &invalid_macs {
        let result = validate_mac_address(mac);
        println!("    {:<25} -> {}", mac, format_result(&result));
    }
    println!();
}

/// 演示基于长度限制的验证函数
fn demo_length_based_validations() {
    println!("--- 4. Length-based Validations ---\n");

    // validate_length: 通用长度验证
    let r1 = validate_length("hello", 10, "test field");
    let r2 = validate_length("this is too long", 5, "short field");
    println!("  validate_length('hello', max=10): {}", format_result(&r1));
    println!(
        "  validate_length('this is too long', max=5): {}",
        format_result(&r2)
    );

    // validate_ban_reason: 最大长度 500
    let r3 = validate_ban_reason("Spam behavior detected");
    let r4 = validate_ban_reason(&"x".repeat(501));
    println!(
        "\n  validate_ban_reason('Spam behavior'): {}",
        format_result(&r3)
    );
    println!("  validate_ban_reason(501 chars): {}", format_result(&r4));

    // validate_api_key: 最大长度 512
    let r5 = validate_api_key("sk-abc123xyz");
    let r6 = validate_api_key(&"a".repeat(513));
    println!(
        "\n  validate_api_key('sk-abc123xyz'): {}",
        format_result(&r5)
    );
    println!("  validate_api_key(513 chars): {}", format_result(&r6));

    // validate_header_value: 最大长度 8192
    let r7 = validate_header_value("application/json; charset=utf-8");
    let r8 = validate_header_value(&"x".repeat(8193));
    println!(
        "\n  validate_header_value('application/json'): {}",
        format_result(&r7)
    );
    println!(
        "  validate_header_value(8193 chars): {}",
        format_result(&r8)
    );

    // validate_path: 最大长度 2048
    let r9 = validate_path("/api/v1/users/12345");
    let r10 = validate_path(&format!("/{}", "x".repeat(2048)));
    println!(
        "\n  validate_path('/api/v1/users/12345'): {}",
        format_result(&r9)
    );
    println!("  validate_path(2049 chars): {}", format_result(&r10));
    println!();
}

/// 演示 BanTarget 验证
fn demo_ban_target_validation() {
    println!("--- 5. BanTarget Validation ---\n");

    let targets = vec![
        BanTarget::Ip("192.168.1.1".to_string()),
        BanTarget::Ip("::1".to_string()),
        BanTarget::UserId("user-001".to_string()),
        BanTarget::Mac("00:1A:2B:3C:4D:5E".to_string()),
    ];

    let invalid_targets = vec![
        BanTarget::Ip("invalid-ip".to_string()),
        BanTarget::UserId("".to_string()),
        BanTarget::Mac("not-a-mac".to_string()),
    ];

    println!("  Valid BanTargets:");
    for target in &targets {
        let result = validate_ban_target(target);
        println!(
            "    {:<35} -> {}",
            format!("{:?}", target),
            format_result(&result)
        );
    }

    println!("\n  Invalid BanTargets:");
    for target in &invalid_targets {
        let result = validate_ban_target(target);
        println!(
            "    {:<35} -> {}",
            format!("{:?}", target),
            format_result(&result)
        );
    }
    println!();
}

/// 格式化验证结果
fn format_result(result: &Result<(), limiteron::LimiteronError>) -> String {
    match result {
        Ok(()) => "✅ valid".to_string(),
        Err(e) => format!("❌ invalid ({})", e),
    }
}
