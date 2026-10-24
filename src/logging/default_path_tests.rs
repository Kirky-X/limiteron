// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 日志默认路径冒烟测试。
//!
//! 默认构建下不安装任何日志后端，裸 `log`/`tracing` 宏必须仍能编译并执行
//! 且不 panic——防止默认日志路径被意外破坏。

/// Smoke test: on the default logging path, the bare `log` and
/// `tracing` macros must still compile and execute without panic. This
/// guards against accidental breakage of the default logging path.
#[test]
fn log_and_tracing_macros_work_on_default_path() {
    log::info!("default log path");
    log::warn!("default log warn");
    log::error!("default log error");
    tracing::info!(target: "limiteron", "default tracing path");
    tracing::warn!(target: "limiteron", "default tracing warn");
}
