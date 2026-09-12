// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! `limiteron-cli` —— limiteron 规则文件配置 CLI。
//!
//! 子命令：`validate` / `export` / `apply`（dry-run）/ `help` / `--version`。
//! 全子命令输出机器可读 JSON，退出码契约：0 成功 / 1 成功带告警 / 2 错误。
//! 核心逻辑在 `limiteron::cli`（`cli` feature），此二进制仅做参数透传。
//!
//! # Example
//!
//! ```text
//! limiteron-cli validate /etc/limiteron/config.toml   # exit 0/1/2
//! limiteron-cli export  /etc/limiteron/config.yaml    # canonical JSON
//! limiteron-cli apply   /etc/limiteron/config.toml    # dry-run plan
//! ```

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let invocation = limiteron::cli::run(args);
    // 序列化失败必须显性报错：静默输出空串 + 退出码 0 会让下游把
    // 失败当作合法的空结果消费
    match serde_json::to_string_pretty(&invocation.output) {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("limiteron-cli: failed to serialize output: {e}");
            std::process::exit(2);
        }
    }
    std::process::exit(invocation.exit_code);
}
