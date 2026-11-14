// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! sdforge CLI 面装载器 —— 规则文件配置子命令的 inventory 注册与分发。
//!
//! 把 `limiteron-cli` 独立二进制的能力折叠为主二进制 `limiteron` 的子命令
//! 集（`validate` / `export` / `apply`），复用 `limiteron::cli::run` 纯函数
//! 核心（机器可读 JSON + 退出码 0/1/2 契约不变）。本模块只做三件事：
//!
//! 1. `inventory::submit!` 注册命令面（`CliCommandRegistration` +
//!    `CliHandlerRegistration`），经 sdforge `CliBuilder` 编进主 CLI；
//! 2. handler 把 CLI 位置参数透传给 [`crate::cli::run`]；
//! 3. `run_entry` 薄壳：i18n 初始化 → clap 解析 → dispatch → 输出 JSON
//!    并按退出码契约退出（原 `src/bin/limiteron_cli.rs` 的 main 逻辑）。
//!
//! # Example
//!
//! ```text
//! limiteron-cli validate /etc/limiteron/config.toml   # exit 0/1/2
//! limiteron-cli export  /etc/limiteron/config.yaml    # canonical JSON
//! limiteron-cli apply   /etc/limiteron/config.toml    # dry-run plan
//! ```

use sdforge::cli::{
    CliArgInfo, CliArgType, CliBuilder, CliCommandRegistration, CliHandlerRegistration,
};
use sdforge::core::{ApiError, HandlerArgs, HandlerFuture, HandlerState};
use sdforge::inventory;

/// 子命令描述（注册面单点维护；本地化经 `cli-*-about` FTL 键——
/// `crate::i18n::catalog::ensure_registered` 已把 en/zh 两个 locale 的
/// 全量键注册进 sdforge 宿主注册表，`CliBuilder::build` 在构建期经
/// `translate_or_fallback` 解析）。
const VALIDATE_ABOUT: &str = "Validate a limiteron rule config file (YAML/TOML/JSON)";
const EXPORT_ABOUT: &str = "Parse a rule config file and export canonical JSON";
const APPLY_ABOUT: &str = "Validate a rule config file and emit an apply plan (dry-run)";

/// 构造带全量注册子命令的主 CLI `clap::Command`（薄壳入口专用）。
pub fn build_cli() -> sdforge::clap::Command {
    // 同步活动 locale 到 sdforge 注册表（catalog 注册已含 en/zh 全量键；
    // 检测链 LIMITERON_LANG → LC_ALL → … 在 i18n::init 固定，此处仅透传）。
    sdforge::i18n::set_locale(&crate::i18n::current_locale().to_string());
    CliBuilder::new()
        .with_name("limiteron-cli")
        .build()
        // sdforge 的 CliBuilder::build 写死 env!("CARGO_PKG_VERSION")（解析到
        // sdforge 自己的版本）；clap 后调覆盖先调，重链一次让 `--version`
        // 自报 limiteron 版本。
        .version(env!("CARGO_PKG_VERSION"))
        .about("Limiteron — unified flow control CLI")
}

/// handler 通用骨架：组装参数 → 调 [`crate::cli::run`] → 取回 JSON 输出。
///
/// 失败分支（exit 2 / 退出码 1）不在这里映射为 `Err`：机器可读契约的
/// 错误体（`ok:false` + `errors` 数组）本身就在返回的 JSON 里，由
/// [`run_entry`] 按 `exit_code` 统一退出，语义与原二进制逐字节一致。
fn invoke_cli_command(
    command: &'static str,
    args: HandlerArgs,
    _state: HandlerState,
) -> HandlerFuture {
    Box::pin(async move {
        // `file` 与 `out` 都是 Body 形参；`apply --out` 缺省时 map 无该键
        // （clap 只呈现用户提供的参数 + 注册默认值）。
        let mut argv: Vec<String> = vec![command.to_string()];
        if let Some(file) = args.get("file") {
            argv.push(file.clone());
        }
        if let Some(out) = args.get("out") {
            argv.push("--out".to_string());
            argv.push(out.clone());
        }
        let invocation = crate::cli::run(argv);
        serde_json::to_value(&invocation.output).map_err(|e| {
            ApiError::internal_error(
                crate::i18n::t("cli-error-serialize-output", &[("reason", e.to_string())]),
                "limiteron.cli.serialize_output",
            )
        })
    })
}

// —— handler 包装（inventory 注册要求独立具名函数）——

fn handle_validate_cmd(args: HandlerArgs, state: HandlerState) -> HandlerFuture {
    invoke_cli_command("validate", args, state)
}

fn handle_export_cmd(args: HandlerArgs, state: HandlerState) -> HandlerFuture {
    invoke_cli_command("export", args, state)
}

fn handle_apply_cmd(args: HandlerArgs, state: HandlerState) -> HandlerFuture {
    invoke_cli_command("apply", args, state)
}

// —— inventory 注册面 ——

// validate <config-file>
inventory::submit! {
    CliCommandRegistration::new("validate", "1", VALIDATE_ABOUT, "handle_validate_cmd")
        .with_i18n_key(Some("cli-validate-about"))
        .with_args(&[
            CliArgInfo::new("file", "Path to the rule file", CliArgType::Path, true, None),
        ])
}
inventory::submit! { CliHandlerRegistration { name: "validate", handler: handle_validate_cmd } }

// export <config-file>
inventory::submit! {
    CliCommandRegistration::new("export", "1", EXPORT_ABOUT, "handle_export_cmd")
        .with_i18n_key(Some("cli-export-about"))
        .with_args(&[
            CliArgInfo::new("file", "Path to the rule file", CliArgType::Path, true, None),
        ])
}
inventory::submit! { CliHandlerRegistration { name: "export", handler: handle_export_cmd } }

// apply <config-file> [--out <output-file>]
inventory::submit! {
    CliCommandRegistration::new("apply", "1", APPLY_ABOUT, "handle_apply_cmd")
        .with_i18n_key(Some("cli-apply-about"))
        .with_args(&[
            CliArgInfo::new("file", "Path to the rule file", CliArgType::Path, true, None),
            CliArgInfo::new("out", "Write normalized JSON to this file", CliArgType::Body, false, None),
        ])
}
inventory::submit! { CliHandlerRegistration { name: "apply", handler: handle_apply_cmd } }

/// 薄壳入口：i18n 初始化 → clap 解析 → dispatch → JSON 输出 → 退出码。
///
/// 原 `src/bin/limiteron_cli.rs` main 的等价迁移：全子命令输出机器可读
/// JSON（pretty），退出码契约 0 成功 / 1 成功带告警 / 2 错误。
pub fn run_entry() -> ! {
    // i18n 启动初始化：按检测链 LIMITERON_LANG → LC_ALL → LC_MESSAGES →
    // LANG → sys-locale → en 固定本次调用的输出语言。
    crate::i18n::init();

    let cli = build_cli();
    let matches = cli.get_matches();
    // 裸调用（无子命令）：与原二进制契约一致——打印 usage JSON 并以 0 退出
    // （clap 默认不强制子命令，落到 dispatch 会变成 Internal 错误）。
    if matches.subcommand_name().is_none() {
        let invocation = crate::cli::run(Vec::<String>::new());
        match serde_json::to_string_pretty(&invocation.output) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!(
                    "{}",
                    crate::i18n::t("cli-error-serialize-output", &[("reason", e.to_string())],)
                );
                std::process::exit(2);
            }
        }
        std::process::exit(invocation.exit_code);
    }
    // 序列化/分发失败必须显性报错：静默输出空串 + 退出码 0 会让下游把
    // 失败当作合法的空结果消费。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("CLI runtime must build");
    match runtime.block_on(sdforge::cli::dispatch::dispatch(&matches, None)) {
        Ok((_name, value)) => {
            match serde_json::to_string_pretty(&value) {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    eprintln!(
                        "{}",
                        crate::i18n::t("cli-error-serialize-output", &[("reason", e.to_string())],)
                    );
                    std::process::exit(2);
                }
            }
            // dispatch 成功路径的退出码由 handler 业务契约决定：查询原
            // invocation 的退出码（0/1/2）——handler 已把它编码进 JSON 的
            // `ok`/`errors`/`warnings` 字段，这里重放同一判定。
            std::process::exit(invocation_exit_code(&value));
        }
        Err(e) => {
            eprintln!(
                "{}",
                crate::i18n::t("cli-error-dispatch-failed", &[("reason", e.to_string())],)
            );
            std::process::exit(2);
        }
    }
}

/// 从 handler 返回的 JSON 重放原退出码契约（0/1/2）。
///
/// `validate`/`apply` 的成功带告警 → 1；`ok:false` 或 `errors` 非空 → 2；
/// 其余（`export`/`help`/`version`）→ 0。与 [`crate::cli::run`] 的
/// `exit_code` 字段判定完全同构（该字段不进 JSON 输出，故按字段重放）。
fn invocation_exit_code(output: &serde_json::Value) -> i32 {
    if output.get("ok").and_then(|v| v.as_bool()) == Some(false) {
        return crate::cli::EXIT_ERROR;
    }
    let has_errors = output
        .get("errors")
        .and_then(|v| v.as_array())
        .is_some_and(|a| !a.is_empty());
    if has_errors {
        return crate::cli::EXIT_ERROR;
    }
    let has_warnings = output
        .get("warnings")
        .and_then(|v| v.as_array())
        .is_some_and(|a| !a.is_empty());
    if has_warnings {
        return crate::cli::EXIT_WARNINGS;
    }
    crate::cli::EXIT_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp_config(content: &str, ext: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("config.{ext}"));
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        (dir, path)
    }

    fn valid_config_json() -> String {
        r#"{
            "version": "1.0.0",
            "global": { "storage": "memory", "cache": "memory", "metrics": "prometheus" },
            "rules": [
                {
                    "id": "r1",
                    "name": "api rule",
                    "priority": 10,
                    "matchers": [{ "type": "User", "user_ids": ["u1"] }],
                    "limiters": [{ "type": "TokenBucket", "capacity": 100, "refill_rate": 10 }],
                    "action": { "on_exceed": "reject" }
                }
            ]
        }"#
        .to_string()
    }

    #[test]
    fn all_subcommands_registered_via_inventory() {
        let names: Vec<&str> = sdforge::inventory::iter::<CliCommandRegistration>()
            .filter(|r| {
                r.handler_fn_name.starts_with("handle_")
                    && ["validate", "export", "apply"].contains(&r.name)
            })
            .map(|r| r.name)
            .collect();
        assert_eq!(names.len(), 3, "validate/export/apply must be registered");
    }

    // 宿主注册是 i18n-host feature 的契约（cli 单开时查找走自持面，
    // 不注册进 sdforge 全局注册表）。
    #[cfg(feature = "i18n-host")]
    #[test]
    fn cli_about_keys_registered_into_sdforge_registry() {
        // 先经 limiteron 自身的 translate 链触发 catalog 的
        // ensure_registered（宿主注册只在该路径惰性发生），再断言
        // sdforge 注册表能直接解析 cli-*-about。
        let _ = crate::i18n::t_simple("cli-validate-about");
        for key in ["cli-validate-about", "cli-export-about", "cli-apply-about"] {
            for locale in ["en", "zh"] {
                assert_ne!(
                    sdforge::i18n::translate_for(locale, key, &[]),
                    key,
                    "{key} not registered for {locale}"
                );
            }
        }
    }

    #[test]
    fn build_cli_lists_all_subcommands() {
        crate::i18n::init();
        let cmd = build_cli();
        let names: Vec<&str> = cmd.get_subcommands().map(|c| c.get_name()).collect();
        assert!(names.contains(&"validate"), "validate listed: {names:?}");
        assert!(names.contains(&"export"), "export listed: {names:?}");
        assert!(names.contains(&"apply"), "apply listed: {names:?}");
        // 主命令版本必须自报 limiteron 版本（覆盖 sdforge 注入的 sdforge 版本）。
        assert_eq!(cmd.get_version().unwrap(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn build_cli_translates_about_via_registry() {
        // sdforge 在 build 时按活动 locale 解析描述；注册表已含
        // cli-*-about 键，构建出的子命令 about 不等于回退英文注册串。
        crate::i18n::init();
        sdforge::i18n::set_locale(&crate::i18n::current_locale().to_string());
        let cmd = build_cli();
        let validate = cmd
            .get_subcommands()
            .find(|c| c.get_name() == "validate")
            .expect("validate subcommand must exist");
        let about = validate.get_about().unwrap().to_string();
        let expected_en = "Validate a limiteron rule config file (YAML/TOML/JSON)";
        assert!(
            about == expected_en || about.contains("校验"),
            "about must resolve from the registry (en or zh): {about}"
        );
    }

    #[test]
    fn invocation_exit_code_contract() {
        let code = |json: serde_json::Value| invocation_exit_code(&json);
        assert_eq!(code(serde_json::json!({"ok": true})), crate::cli::EXIT_OK);
        assert_eq!(
            code(serde_json::json!({"ok": true, "warnings": ["w1"]})),
            crate::cli::EXIT_WARNINGS
        );
        assert_eq!(
            code(serde_json::json!({"ok": false, "errors": ["e1"]})),
            crate::cli::EXIT_ERROR
        );
        assert_eq!(
            code(serde_json::json!({"ok": true, "errors": ["e1"]})),
            crate::cli::EXIT_ERROR
        );
        assert_eq!(
            code(serde_json::json!({"ok": true, "warnings": [], "errors": []})),
            crate::cli::EXIT_OK
        );
    }

    #[tokio::test]
    async fn handler_validate_routes_through_core_run() {
        let (_dir, path) = write_temp_config(&valid_config_json(), "json");
        let args: HandlerArgs = [("file".to_string(), path.to_str().unwrap().to_string())]
            .into_iter()
            .collect();
        let fut = invoke_cli_command("validate", args, None);
        let value = fut.await.unwrap();
        assert_eq!(value["command"], serde_json::json!("validate"));
        assert_eq!(value["valid"], serde_json::json!(true));
        assert_eq!(value["rule_count"], serde_json::json!(1));
    }

    #[tokio::test]
    async fn handler_missing_file_returns_machine_error_json() {
        let args: HandlerArgs = [("file".to_string(), "/nonexistent/config.toml".to_string())]
            .into_iter()
            .collect();
        let fut = invoke_cli_command("validate", args, None);
        let value = fut.await.unwrap();
        assert_eq!(value["command"], serde_json::json!("validate"));
        assert_eq!(value["ok"], serde_json::json!(false));
        assert_eq!(value["valid"], serde_json::json!(false));
        assert_eq!(
            invocation_exit_code(&value),
            crate::cli::EXIT_ERROR,
            "missing file must map to exit 2"
        );
    }

    #[tokio::test]
    async fn handler_apply_passes_out_option() {
        let (_dir, path) = write_temp_config(&valid_config_json(), "json");
        let out_dir = tempfile::tempdir().unwrap();
        let out = out_dir.path().join("normalized.json");
        let args: HandlerArgs = [
            ("file".to_string(), path.to_str().unwrap().to_string()),
            ("out".to_string(), out.to_str().unwrap().to_string()),
        ]
        .into_iter()
        .collect();
        let fut = invoke_cli_command("apply", args, None);
        let value = fut.await.unwrap();
        assert_eq!(value["applied"], serde_json::json!(true));
        assert_eq!(
            value["written_to"],
            serde_json::json!(out.to_str().unwrap())
        );
        assert!(out.is_file(), "--out must round-trip through the core run");
    }

    #[tokio::test]
    async fn dispatch_end_to_end_matches_core_run_output() {
        // 真实子命令级等价性：build_cli → clap 解析 → sdforge dispatch →
        // handler 透传 run() 的输出必须与直调 run() 的 JSON 逐字段一致。
        crate::i18n::init();
        let (_dir, path) = write_temp_config(&valid_config_json(), "json");
        let cmd = build_cli();
        let matches = cmd
            .try_get_matches_from(["limiteron-cli", "validate", path.to_str().unwrap()])
            .expect("valid argv must parse");
        let (name, value) = sdforge::cli::dispatch::dispatch(&matches, None)
            .await
            .expect("dispatch must succeed for a registered subcommand");
        assert_eq!(name, "validate");
        let expected = crate::cli::run(["validate", path.to_str().unwrap()]).output;
        assert_eq!(
            value, expected,
            "dispatch output must equal the core run() output"
        );
    }

    #[tokio::test]
    async fn dispatch_end_to_end_export_subcommand() {
        crate::i18n::init();
        let (_dir, path) = write_temp_config(&valid_config_json(), "json");
        let cmd = build_cli();
        let matches = cmd
            .try_get_matches_from(["limiteron-cli", "export", path.to_str().unwrap()])
            .expect("valid argv must parse");
        let (name, value) = sdforge::cli::dispatch::dispatch(&matches, None)
            .await
            .expect("dispatch must succeed for a registered subcommand");
        assert_eq!(name, "export");
        assert_eq!(value["command"], serde_json::json!("export"));
        assert_eq!(value["config"]["version"], serde_json::json!("1.0.0"));
    }

    #[test]
    fn clap_rejects_missing_positional_argument() {
        // 原二进制对缺位置参数返回 JSON usage 错误 + exit 2；clap 层改以
        // usage 错误呈现（同样 exit 2，由 clap 进程退出保证）。
        crate::i18n::init();
        let cmd = build_cli();
        assert!(
            cmd.clone()
                .try_get_matches_from(["limiteron-cli", "validate"])
                .is_err()
        );
        assert!(
            cmd.clone()
                .try_get_matches_from(["limiteron-cli", "export"])
                .is_err()
        );
        assert!(
            cmd.try_get_matches_from(["limiteron-cli", "apply"])
                .is_err()
        );
    }

    #[test]
    fn clap_rejects_unknown_subcommand() {
        crate::i18n::init();
        let cmd = build_cli();
        assert!(
            cmd.try_get_matches_from(["limiteron-cli", "frobnicate"])
                .is_err()
        );
    }
}
