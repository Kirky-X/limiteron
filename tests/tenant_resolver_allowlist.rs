// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 租户白名单误配的显性留痕测试
//!
//! 空白名单会让所有租户头的流量并入默认命名空间（租户维度隔离失效），
//! 该误配必须以告警形式显性留痕，不得静默生效。
#![cfg(feature = "multi-tenant")]

use limiteron::{HeaderTenantResolver, RequestContext, TenantResolver};
use log::{Log, Metadata, Record};
use std::sync::{Mutex, Once, OnceLock};

/// 捕获 `log` 输出的 logger：`log` 的全局 logger 槽位全进程只能占用一次，
/// 本文件是独立测试二进制，槽位不会被其他测试二进制抢走
static RECORDS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

struct CapturingLogger;

impl Log for CapturingLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.target() == "limiteron"
    }

    fn log(&self, record: &Record) {
        if record.target() != "limiteron" {
            return;
        }
        if let Some(records) = RECORDS.get()
            && let Ok(mut records) = records.lock()
        {
            records.push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

static LOGGER: CapturingLogger = CapturingLogger;

/// 占用全局 logger 槽位并打开告警级别（`log` 的 max_level 默认为 Off，
/// `warn!` 会在构造记录前被跳过）
fn install_capturing_logger() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = RECORDS.set(Mutex::new(Vec::new()));
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(log::LevelFilter::Warn);
    });
}

/// 取走并清空已捕获的告警文本
fn take_messages() -> Vec<String> {
    RECORDS
        .get()
        .and_then(|records| records.lock().ok())
        .map(|mut records| std::mem::take(&mut *records))
        .unwrap_or_default()
}

#[test]
#[serial_test::serial]
fn test_empty_allowlist_warns_and_rejects_every_tenant() {
    install_capturing_logger();
    let _ = take_messages();

    let resolver = HeaderTenantResolver::new("X-Tenant-ID", "production")
        .with_allowed_tenants(Vec::<String>::new());

    let messages = take_messages();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("tenant allowlist") && m.contains("is empty")),
        "empty tenant allowlist must be reported as a misconfiguration, captured: {messages:?}"
    );

    // 空白名单拒绝所有租户头：流量回退默认命名空间，不获得自选命名空间
    let ctx = RequestContext::new().with_header("X-Tenant-ID", "acme-corp");
    assert!(resolver.resolve(&ctx).is_none());
}

#[test]
#[serial_test::serial]
fn test_non_empty_allowlist_does_not_warn() {
    install_capturing_logger();
    let _ = take_messages();

    let resolver =
        HeaderTenantResolver::new("X-Tenant-ID", "production").with_allowed_tenants(["acme-corp"]);

    let messages = take_messages();
    assert!(
        !messages.iter().any(|m| m.contains("tenant allowlist")),
        "a populated allowlist must not be reported as a misconfiguration, captured: {messages:?}"
    );

    let ctx = RequestContext::new().with_header("X-Tenant-ID", "acme-corp");
    let ns = resolver.resolve(&ctx).unwrap();
    assert_eq!(ns.tenant_id(), "acme-corp");
}
