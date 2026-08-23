// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
#![cfg(feature = "macros")]
//! `#[flow_control]` 宏 `on_exceed` 模式集成测试
//!
#![cfg(feature = "macros")]
//! 本测试文件对应 specmark change `patch-macro-extensions` 的 T006 任务。
//!
#![cfg(feature = "macros")]
//! ## 测试目标
//!
#![cfg(feature = "macros")]
//! 验证 `#[flow_control]` 宏的 `on_exceed` 参数行为：
#![cfg(feature = "macros")]
//! - `on_exceed = "reject"`（默认）：超限返回 `RateLimitExceeded`
#![cfg(feature = "macros")]
//! - `on_exceed = "log_only"`：超限不返回错误，原函数继续执行
#![cfg(feature = "macros")]
//! - 默认（未指定）：行为与 `reject` 一致
//!
#![cfg(feature = "macros")]
//! ## 运行时支持
//!
#![cfg(feature = "macros")]
//! `#[flow_control]` 宏生成的代码引用 `limiteron::GLOBAL_LIMITER_MANAGER`，
#![cfg(feature = "macros")]
//! 该符号已在 `limiteron/src/limiters/manager.rs` 中实现（LimiterManager 全局单例，
#![cfg(feature = "macros")]
//! 提供 `get_rate_limiter`/`get_quota_limiter`/`get_concurrency_limiter` 方法）。
//!
#![cfg(feature = "macros")]
//! ## 行为测试位置
//!
#![cfg(feature = "macros")]
//! `on_exceed` 参数解析与代码生成的行为测试位于 macros crate 的单元测试中：
#![cfg(feature = "macros")]
//! - 文件：`limiteron/macros/src/lib.rs` 的 `#[cfg(test)] mod tests`
#![cfg(feature = "macros")]
//! - 测试数量：31 个（含 5 个原有 + 26 个 T006/T007/T008 新增）
#![cfg(feature = "macros")]
//! - 覆盖：
#![cfg(feature = "macros")]
//!   - T006: `on_exceed` 解析（reject/log_only/throttle/invalid）+ 代码生成
#![cfg(feature = "macros")]
//!     （reject 生成 RateLimitExceeded，log_only 不生成错误，throttle 生成 compile_error）
#![cfg(feature = "macros")]
//!   - T007: `key_prefix` 解析 + 代码生成（rate/quota/concurrency key 含前缀）
#![cfg(feature = "macros")]
//!   - T008: `tracing`/`metrics` toggles 解析 + 代码生成（禁用时不生成 span/try_global）
//!
#![cfg(feature = "macros")]
//! 运行单元测试：
#![cfg(feature = "macros")]
//! ```bash
#![cfg(feature = "macros")]
//! cargo test -p limiteron-macros --lib
#![cfg(feature = "macros")]
//! ```

/// Smoke test：验证 `flow_control` 宏符号可被导入
///
/// 此测试验证宏符号可被 `use` 导入（编译时检查）。
/// 实际的宏行为测试在 macros crate 的单元测试中。
#[test]
#[allow(unused_imports)]
fn test_flow_control_macro_is_available() {
    // 使用 use 语句验证宏符号存在（编译时检查）
    // 注意：属性宏不能作为值引用，仅验证 use 语句编译通过
    use limiteron::flow_control;
    // 编译时检查通过即说明宏符号存在
}

// ============================================================================
// on_exceed 行为说明（文档注释，非测试）
// ============================================================================
//
// 实际的 on_exceed 行为验证全部在 macros crate 的单元测试中（见模块级注释）。
// 此处不再保留 `assert!(true)` 形式的占位测试（违反 Rule 9：测试必须验证有意义的属性）。
//
// 行为摘要：
// - `on_exceed = "reject"`（默认）：超限返回 `LimiteronError::RateLimitExceeded`/
//   `QuotaExceeded`/`ConcurrencyLimitExceeded`
// - `on_exceed = "log_only"`：超限不返回错误，继续执行原函数
//   （rate/quota 仍调用 check 记录 metrics；concurrency 不持有 permit）
// - `on_exceed = "throttle"`：当前未实现（`LimiteronError::Throttled` 变体不存在），
//   `generate_flow_control` 生成 `compile_error!`（Rule 12：失败必须显性化）
// - 未知 `on_exceed` 值：在 `FlowControlConfig::parse` 阶段被拒绝（Rule 12）
