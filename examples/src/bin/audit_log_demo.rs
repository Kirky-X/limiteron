// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Audit Log 示例
//!
//! 演示审计日志记录器的使用：事件记录、配置、统计、签名验证。
//!
//! # 涵盖 API
//!
//! - `AuditLogger::new(config).await` / `default().await`
//! - `AuditLogConfig` 构建器（`channel_capacity`、`batch_size`、`signing_key` 等）
//! - `log_decision` / `log_config_change` / `log_ban_operation` / `log_system_event` / `log_error_event`
//! - `AuditLogStats` 统计（`total_events`、`decision_events` 等）
//! - `AuditEvent` 枚举及其方法（`timestamp`、`operation`、`target`、`result`）
//! - `AuditLogEntry` 签名与验证（`sign`、`verify`、`with_signature`）
//! - `verify_integrity` 完整性验证
//!
//! # 运行方式
//!
//! ```bash
//! cargo run --bin audit_log_demo --features audit-log
//! ```

use limiteron::logging::audit::{AuditEvent, AuditLogConfig, AuditLogEntry, AuditLogger};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Audit Log Demo ===\n");

    demo_basic_logging().await;
    demo_config_builder().await;
    demo_event_types().await;
    demo_signature_verification()?;
    demo_stats().await;

    println!("\n=== All demos completed ===");
    Ok(())
}

/// 演示基本日志记录：使用默认配置创建 logger 并记录事件
async fn demo_basic_logging() {
    println!("--- 1. Basic Logging ---\n");

    let logger = AuditLogger::default().await;

    logger
        .log_decision(
            "user-001".to_string(),
            "allowed".to_string(),
            "within rate limit".to_string(),
            Some("req-12345".to_string()),
        )
        .await;

    logger
        .log_ban_operation(
            "192.168.1.100".to_string(),
            "create_ban".to_string(),
            "rate limit exceeded".to_string(),
            "admin@example.com".to_string(),
            Some(chrono::Utc::now() + chrono::Duration::hours(1)),
        )
        .await;

    tokio::time::sleep(Duration::from_millis(200)).await;

    let stats = logger.stats();
    println!("  Total events:       {}", stats.total_events());
    println!("  Decision events:    {}", stats.decision_events());
    println!("  Ban operation events: {}", stats.ban_operation_events());

    logger.shutdown().await;
    println!();
}

/// 演示 AuditLogConfig 构建器
async fn demo_config_builder() {
    println!("--- 2. AuditLogConfig Builder ---\n");

    let config = AuditLogConfig::new()
        .enabled(true)
        .channel_capacity(2048)
        .batch_size(50)
        .batch_timeout(Duration::from_millis(500))
        .signing_key(demo_signing_key())
        .verify_on_read(true);

    println!("  Config:");
    println!("    enabled:          {}", config.enabled);
    println!("    channel_capacity: {}", config.channel_capacity);
    println!("    batch_size:       {}", config.batch_size);
    println!("    batch_timeout:    {:?}", config.batch_timeout);
    println!("    verify_on_read:   {}", config.verify_on_read);
    println!("    signing_key set:  {}", config.signing_key.is_some());

    let logger = AuditLogger::new(config).await;
    println!("\n  Logger created with custom config");
    println!("    config().enabled: {}", logger.config().enabled);

    logger.shutdown().await;
    println!();
}

/// 演示所有事件类型的记录
async fn demo_event_types() {
    println!("--- 3. All Event Types ---\n");

    let logger = AuditLogger::default().await;

    // 1. Decision event
    logger
        .log_decision(
            "user-002".to_string(),
            "rejected".to_string(),
            "rate limit exceeded".to_string(),
            Some("req-67890".to_string()),
        )
        .await;

    // 2. Config change event
    logger
        .log_config_change(
            "v1.0.0".to_string(),
            "v1.1.0".to_string(),
            vec![
                "updated rate limit".to_string(),
                "added new rule".to_string(),
            ],
            Some("admin@example.com".to_string()),
        )
        .await;

    // 3. Ban operation event
    logger
        .log_ban_operation(
            "user-003".to_string(),
            "remove_ban".to_string(),
            "ban expired".to_string(),
            "moderator@example.com".to_string(),
            None,
        )
        .await;

    // 4. System event
    logger
        .log_system_event(
            "WARN".to_string(),
            "high_memory_usage".to_string(),
            "Memory usage reached 85%".to_string(),
        )
        .await;

    // 5. Error event
    logger
        .log_error_event(
            "StorageError".to_string(),
            "Failed to connect to Redis".to_string(),
            Some("at redis_pool.rs:42".to_string()),
        )
        .await;

    tokio::time::sleep(Duration::from_millis(200)).await;

    let stats = logger.stats();
    println!("  Event counts:");
    println!("    Decision events:      {}", stats.decision_events());
    println!("    Config change events: {}", stats.config_change_events());
    println!("    Ban operation events: {}", stats.ban_operation_events());
    println!("    System events:        {}", stats.system_events());
    println!("    Error events:         {}", stats.error_events());
    println!("    Total events:         {}", stats.total_events());

    logger.shutdown().await;
    println!();
}

/// 运行时生成演示签名密钥（时间纳秒 × 进程 ID 混合）：演示运行之间互不相同，
/// 硬编码密钥会随源码进入版本库（CodeQL hard-coded-cryptographic-value）。
fn demo_signing_key() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0u64, |d| d.as_nanos() as u64);
    let pid = std::process::id() as u64;
    let mut seed = nanos ^ (pid << 32) ^ 0x9E37_79B9_7F4A_7C15;
    (0..32)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            char::from(b'a' + (seed % 26) as u8)
        })
        .collect()
}

/// 演示 AuditLogEntry 签名与验证
fn demo_signature_verification() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 4. Signature & Verification ---\n");

    // 演示密钥运行时生成：签名与验证共用同一实例，硬编码密钥会随源码泄露
    let signing_key = demo_signing_key();

    let event = AuditEvent::Decision {
        timestamp: chrono::Utc::now(),
        identifier: "user-001".to_string(),
        decision: "allowed".to_string(),
        reason: "within limit".to_string(),
        request_id: Some("req-001".to_string()),
    };

    println!("  Event:");
    println!("    operation: {}", event.operation());
    println!("    target:    {}", event.target());
    println!("    result:    {}", event.result());
    println!("    timestamp: {}", event.timestamp());

    let unsigned_entry = AuditLogEntry::new(event.clone());
    println!("\n  Unsigned entry:");
    println!("    has signature: {}", unsigned_entry.signature.is_some());

    let verify_result = unsigned_entry.verify(signing_key);
    println!("    verify result: {:?}", verify_result);

    let signed_entry = AuditLogEntry::with_signature(event.clone(), signing_key);
    println!("\n  Signed entry:");
    println!("    has signature: {}", signed_entry.signature.is_some());
    println!(
        "    signature version: {:?}",
        signed_entry.signature_version
    );

    let verify_result = signed_entry.verify(signing_key);
    println!("    verify with correct key: {}", verify_result?);

    // 用错误的密钥验证应该失败
    let wrong_key_result = signed_entry.verify("wrong-key");
    println!("    verify with wrong key:   {:?}", wrong_key_result);

    // 手动签名
    let mut manual_entry = AuditLogEntry::new(event);
    manual_entry.sign(signing_key);
    println!("\n  Manually signed entry:");
    println!("    verify: {}", manual_entry.verify(signing_key)?);
    println!();
    Ok(())
}

/// 演示统计信息
async fn demo_stats() {
    println!("--- 5. AuditLogStats ---\n");

    let config = AuditLogConfig::new()
        .enabled(true)
        .batch_size(10)
        .batch_timeout(Duration::from_millis(100));

    let logger = AuditLogger::new(config).await;

    for i in 0..5 {
        logger
            .log_decision(
                format!("user-{}", i),
                "allowed".to_string(),
                "ok".to_string(),
                None,
            )
            .await;
    }
    for i in 0..3 {
        logger
            .log_system_event(
                "INFO".to_string(),
                format!("event-{}", i),
                "system info".to_string(),
            )
            .await;
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    let stats = logger.stats();
    println!("  Statistics:");
    println!("    Total events:          {}", stats.total_events());
    println!("    Decision events:       {}", stats.decision_events());
    println!("    System events:         {}", stats.system_events());
    println!("    Batch writes:          {}", stats.batch_writes());
    println!("    Write failures:        {}", stats.write_failures());
    println!("    Signature failures:    {}", stats.signature_failures());
    println!(
        "    Verification failures: {}",
        stats.verification_failures()
    );

    stats.reset();
    println!("\n  After reset:");
    println!("    Total events: {}", stats.total_events());

    logger.shutdown().await;
    println!();
}
