// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 日志安全模块
//!
//! 提供审计日志和日志脱敏功能，保护敏感信息不被泄露。

// 子模块
#[cfg(feature = "audit-log")]
pub mod audit;

pub mod redaction;

// 重新导出 audit 模块的公共类型
#[cfg(feature = "audit-log")]
pub use audit::{AuditEvent, AuditLogConfig, AuditLogEntry, AuditLogStats, AuditLogger};

// 重新导出 redaction 模块的公共类型
pub use redaction::{redact_basic, redact_email, redact_ip, redact_user_id};

#[cfg(feature = "log-redaction")]
pub use redaction::{
    RedactionConfig, contains_sensitive_info, redact_advanced, redact_http_content,
};

#[cfg(feature = "ban-manager")]
pub use redaction::redact_ban_target;

#[cfg(test)]
mod default_path_tests;
