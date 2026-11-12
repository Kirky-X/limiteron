// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Limiteron 主二进制入口（产品二进制形态规范：`src/main.rs`）。
//!
//! 原独立入口 `src/bin/limiteron_cli.rs`（`[[bin]]` 自定义 path 违规形态）
//! 已折叠收敛到本薄壳：二进制名保留 `limiteron-cli`（文档与用户脚本口径
//! 不变），子命令面 `validate` / `export` / `apply` 由 sdforge
//! `CliBuilder` + inventory 注册（`limiteron::cli::forge`）编出，分发回
//! `limiteron::cli::run` 纯函数核心；机器可读 JSON 输出与退出码契约
//! 0/1/2 不变。
//!
//! # Example
//!
//! ```text
//! limiteron-cli validate /etc/limiteron/config.toml   # exit 0/1/2
//! limiteron-cli export  /etc/limiteron/config.yaml    # canonical JSON
//! limiteron-cli apply   /etc/limiteron/config.toml    # dry-run plan
//! ```

fn main() {
    limiteron::cli::forge::run_entry()
}
