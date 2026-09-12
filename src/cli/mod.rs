// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! `limiteron-cli` —— 规则文件配置 CLI。
//!
//! 机器可读契约（与 inklog-cli 口径一致）：全子命令输出单对象 JSON
//! + 稳定退出码 `0/1/2`（0 = 成功；1 = 成功但有告警；2 = 错误）。
//!
//! # 子命令
//!
//! | 子命令 | 说明 | 退出码 |
//! | --- | --- | --- |
//! | `validate <file>` | 解析 + 校验规则文件（YAML/TOML/JSON 按扩展名） | 0 合法 / 1 合法带告警 / 2 非法 |
//! | `export <file>`   | 解析并输出规范化配置 JSON | 0 / 2 |
//! | `apply <file> [--out <file>]` | 校验 + 产出 apply 计划（dry-run 语义；真实下发走 admin API `POST /api/v1/config`） | 0 / 1 / 2 |
//! | `help` / `--help` | 用法说明 | 0 |
//! | `--version` | 版本 JSON | 0 |
//!
//! CLI 逻辑全部收在本模块（纯函数 [`run`]），二进制 `src/bin/limiteron_cli.rs`
//! 仅做参数透传，保证可单测。默认 feature 不编译本模块（决策热路径零开销）。
//!
//! # Example
//!
//! ```rust
//! # #[cfg(feature = "cli")]
//! # {
//! use limiteron::cli::run;
//!
//! let invocation = run(["validate", "/nonexistent/config.toml"]);
//! assert_eq!(invocation.exit_code, 2, "missing file must exit 2");
//! assert_eq!(invocation.output["command"], "validate");
//! # }
//! ```

use crate::config::FlowControlConfig;

/// 退出码：成功
pub const EXIT_OK: i32 = 0;
/// 退出码：成功但有告警
pub const EXIT_WARNINGS: i32 = 1;
/// 退出码：错误（解析失败 / 校验失败 / 用法错误 / IO 错误）
pub const EXIT_ERROR: i32 = 2;

/// 一次 CLI 调用的结果：JSON 输出 + 退出码。
#[derive(Debug, Clone, PartialEq)]
pub struct CliInvocation {
    /// 进程退出码（0/1/2）
    pub exit_code: i32,
    /// stdout 输出的 JSON 对象（机器可读）
    pub output: serde_json::Value,
}

impl CliInvocation {
    fn new(
        exit_code: i32,
        command: &str,
        mut fields: serde_json::Map<String, serde_json::Value>,
    ) -> Self {
        let mut obj = serde_json::Map::new();
        obj.insert("command".to_string(), serde_json::json!(command));
        obj.append(&mut fields);
        Self {
            exit_code,
            output: serde_json::Value::Object(obj),
        }
    }

    fn usage() -> Self {
        let mut fields = serde_json::Map::new();
        fields.insert("ok".to_string(), serde_json::json!(true));
        fields.insert(
            "usage".to_string(),
            serde_json::json!({
                "validate": "validate <config-file>",
                "export": "export <config-file>",
                "apply": "apply <config-file> [--out <output-file>]",
            }),
        );
        Self::new(EXIT_OK, "help", fields)
    }
}

/// 执行一次 CLI 调用（`args` 不含程序名，首元素为子命令）。
pub fn run<I, S>(args: I) -> CliInvocation
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    let Some(command) = args.first() else {
        return CliInvocation::usage();
    };

    match command.as_str() {
        "help" | "--help" | "-h" => CliInvocation::usage(),
        "--version" | "version" => {
            let mut fields = serde_json::Map::new();
            fields.insert("ok".to_string(), serde_json::json!(true));
            fields.insert(
                "version".to_string(),
                serde_json::json!(env!("CARGO_PKG_VERSION")),
            );
            CliInvocation::new(EXIT_OK, "version", fields)
        }
        "validate" => match args.get(1) {
            Some(path) => cmd_validate(path),
            None => usage_error("validate", "missing <config-file> argument"),
        },
        "export" => match args.get(1) {
            Some(path) => cmd_export(path),
            None => usage_error("export", "missing <config-file> argument"),
        },
        "apply" => cmd_apply(&args[1..]),
        other => usage_error(other, "unknown command"),
    }
}

fn usage_error(command: &str, message: &str) -> CliInvocation {
    let mut fields = serde_json::Map::new();
    fields.insert("ok".to_string(), serde_json::json!(false));
    fields.insert("error".to_string(), serde_json::json!(message));
    CliInvocation::new(EXIT_ERROR, command, fields)
}

/// 解析配置文件；任何失败折叠为一条错误消息（供 JSON 诊断输出）。
fn load_config(path: &str) -> Result<FlowControlConfig, String> {
    crate::ConfigLoader::load_from_file(path).map_err(|e| match e {
        crate::LimiteronError::ConfigError(msg) => msg,
        crate::LimiteronError::IoError(io) => format!("IO error: {io}"),
        other => format!("{other}"),
    })
}

/// 告警检查（非致命，不影响合法性判定）：
/// - `version` 不是 `x.y.z` 形态；
/// - 规则间存在重复 `priority`（匹配顺序歧义）。
fn collect_warnings(config: &FlowControlConfig) -> Vec<String> {
    let mut warnings = Vec::new();

    let semver_ok = {
        let parts: Vec<&str> = config.version.split('.').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    };
    if !semver_ok {
        warnings.push(format!(
            "version '{}' 不是 x.y.z 数字形态（语义化版本建议）",
            config.version
        ));
    }

    let mut seen = std::collections::HashSet::new();
    for rule in &config.rules {
        if !seen.insert(rule.priority) {
            warnings.push(format!(
                "规则 '{}' 的 priority {} 与其他规则重复（匹配顺序歧义）",
                rule.id, rule.priority
            ));
        }
    }

    warnings
}

fn cmd_validate(path: &str) -> CliInvocation {
    let mut fields = serde_json::Map::new();
    fields.insert("file".to_string(), serde_json::json!(path));

    let config = match load_config(path) {
        Ok(c) => c,
        Err(e) => {
            fields.insert("ok".to_string(), serde_json::json!(false));
            fields.insert("valid".to_string(), serde_json::json!(false));
            fields.insert("errors".to_string(), serde_json::json!([e]));
            fields.insert("warnings".to_string(), serde_json::json!([]));
            return CliInvocation::new(EXIT_ERROR, "validate", fields);
        }
    };

    let warnings = collect_warnings(&config);
    match config.validate() {
        Ok(()) => {
            fields.insert("ok".to_string(), serde_json::json!(true));
            fields.insert("valid".to_string(), serde_json::json!(true));
            fields.insert(
                "rule_count".to_string(),
                serde_json::json!(config.rules.len()),
            );
            fields.insert(
                "config_hash".to_string(),
                serde_json::json!(config.compute_hash()),
            );
            fields.insert("errors".to_string(), serde_json::json!([]));
            fields.insert("warnings".to_string(), serde_json::json!(warnings));
            let code = if warnings.is_empty() {
                EXIT_OK
            } else {
                EXIT_WARNINGS
            };
            CliInvocation::new(code, "validate", fields)
        }
        Err(msg) => {
            fields.insert("ok".to_string(), serde_json::json!(false));
            fields.insert("valid".to_string(), serde_json::json!(false));
            fields.insert("errors".to_string(), serde_json::json!([msg]));
            fields.insert("warnings".to_string(), serde_json::json!(warnings));
            CliInvocation::new(EXIT_ERROR, "validate", fields)
        }
    }
}

fn cmd_export(path: &str) -> CliInvocation {
    let mut fields = serde_json::Map::new();
    fields.insert("file".to_string(), serde_json::json!(path));

    match load_config(path) {
        Ok(config) => match serde_json::to_value(&config) {
            Ok(value) => {
                fields.insert("ok".to_string(), serde_json::json!(true));
                fields.insert("format".to_string(), serde_json::json!("json"));
                fields.insert("config".to_string(), value);
                CliInvocation::new(EXIT_OK, "export", fields)
            }
            Err(e) => {
                // 序列化失败不得以空对象 + ok:true 伪装成功
                fields.insert("ok".to_string(), serde_json::json!(false));
                fields.insert(
                    "errors".to_string(),
                    serde_json::json!([format!("config serialization failed: {e}")]),
                );
                CliInvocation::new(EXIT_ERROR, "export", fields)
            }
        },
        Err(e) => {
            fields.insert("ok".to_string(), serde_json::json!(false));
            fields.insert("errors".to_string(), serde_json::json!([e]));
            CliInvocation::new(EXIT_ERROR, "export", fields)
        }
    }
}

fn cmd_apply(rest: &[String]) -> CliInvocation {
    let Some(path) = rest.first() else {
        return usage_error("apply", "missing <config-file> argument");
    };

    // 选项解析：目前仅 --out <file>（dry-run 为 apply 的内建语义）
    let mut out_path: Option<&str> = None;
    let mut i = 1;
    while i < rest.len() {
        match rest[i].as_str() {
            "--out" => match rest.get(i + 1) {
                Some(p) => {
                    out_path = Some(p);
                    i += 2;
                }
                None => return usage_error("apply", "--out requires a file argument"),
            },
            other => return usage_error("apply", &format!("unknown option '{other}'")),
        }
    }

    let mut fields = serde_json::Map::new();
    fields.insert("file".to_string(), serde_json::json!(path));

    // 1. 解析 + 校验（失败 → exit 2，不产出计划）
    let config = match load_config(path) {
        Ok(c) => c,
        Err(e) => {
            fields.insert("ok".to_string(), serde_json::json!(false));
            fields.insert("applied".to_string(), serde_json::json!(false));
            fields.insert("errors".to_string(), serde_json::json!([e]));
            return CliInvocation::new(EXIT_ERROR, "apply", fields);
        }
    };
    let warnings = collect_warnings(&config);
    if let Err(msg) = config.validate() {
        fields.insert("ok".to_string(), serde_json::json!(false));
        fields.insert("applied".to_string(), serde_json::json!(false));
        fields.insert("errors".to_string(), serde_json::json!([msg]));
        fields.insert("warnings".to_string(), serde_json::json!(warnings));
        return CliInvocation::new(EXIT_ERROR, "apply", fields);
    }

    // 2. dry-run 计划（真实原子换配置走 admin API `POST /api/v1/config`）
    fields.insert("dry_run".to_string(), serde_json::json!(true));
    fields.insert("valid".to_string(), serde_json::json!(true));
    fields.insert(
        "rules_applied".to_string(),
        serde_json::json!(config.rules.len()),
    );
    fields.insert(
        "config_hash".to_string(),
        serde_json::json!(config.compute_hash()),
    );
    fields.insert("warnings".to_string(), serde_json::json!(warnings));

    // 3. 可选：规范化 JSON 落盘（--out）。先序列化再写入：序列化失败
    // 时不得产出空文件、也不得继续以 applied:true 报告成功
    if let Some(out) = out_path {
        let canonical = match serde_json::to_string_pretty(&config) {
            Ok(s) => s,
            Err(e) => {
                fields.insert("ok".to_string(), serde_json::json!(false));
                fields.insert("applied".to_string(), serde_json::json!(false));
                fields.insert(
                    "errors".to_string(),
                    serde_json::json!([format!("config serialization failed: {e}")]),
                );
                return CliInvocation::new(EXIT_ERROR, "apply", fields);
            }
        };
        match std::fs::write(out, canonical) {
            Ok(()) => {
                fields.insert("written_to".to_string(), serde_json::json!(out));
            }
            Err(e) => {
                fields.insert("ok".to_string(), serde_json::json!(false));
                fields.insert("applied".to_string(), serde_json::json!(false));
                fields.insert(
                    "errors".to_string(),
                    serde_json::json!([format!(
                        "failed to write normalized config to '{out}': {e}"
                    )]),
                );
                return CliInvocation::new(EXIT_ERROR, "apply", fields);
            }
        }
    }

    let code = if warnings.is_empty() {
        EXIT_OK
    } else {
        EXIT_WARNINGS
    };
    fields.insert("ok".to_string(), serde_json::json!(true));
    fields.insert("applied".to_string(), serde_json::json!(true));
    CliInvocation::new(code, "apply", fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 写入临时配置文件；返回 (TempDir, 文件路径)，TempDir 存活至测试结束。
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

    // ========================================================================
    // validate
    // ========================================================================

    #[test]
    fn test_t612_validate_valid_file_exit_0() {
        let (_dir, path) = write_temp_config(&valid_config_json(), "json");
        let inv = run(["validate", path.to_str().unwrap()]);
        assert_eq!(
            inv.exit_code, EXIT_OK,
            "valid config → exit 0: {:?}",
            inv.output
        );
        assert_eq!(inv.output["valid"], serde_json::json!(true));
        assert_eq!(inv.output["rule_count"], serde_json::json!(1));
        assert!(inv.output["config_hash"].is_string());
    }

    #[test]
    fn test_t612_validate_missing_file_exit_2() {
        let inv = run(["validate", "/nonexistent/limiteron.toml"]);
        assert_eq!(inv.exit_code, EXIT_ERROR);
        assert_eq!(inv.output["valid"], serde_json::json!(false));
        assert!(inv.output["errors"].as_array().unwrap().len() == 1);
    }

    #[test]
    fn test_t612_validate_invalid_json_exit_2() {
        let (_dir, path) = write_temp_config("{ not json", "json");
        let inv = run(["validate", path.to_str().unwrap()]);
        assert_eq!(inv.exit_code, EXIT_ERROR, "parse error → exit 2");
        assert_eq!(inv.output["valid"], serde_json::json!(false));
    }

    #[test]
    fn test_t612_validate_rule_violation_exit_2() {
        // 规则缺 limiters → Rule::validate 报错
        let json = r#"{
            "version": "1.0.0",
            "global": { "storage": "memory", "cache": "memory", "metrics": "none" },
            "rules": [
                {
                    "id": "bad",
                    "name": "no limiters",
                    "priority": 1,
                    "matchers": [{ "type": "User", "user_ids": ["u1"] }],
                    "limiters": [],
                    "action": { "on_exceed": "reject" }
                }
            ]
        }"#;
        let (_dir, path) = write_temp_config(json, "json");
        let inv = run(["validate", path.to_str().unwrap()]);
        assert_eq!(inv.exit_code, EXIT_ERROR);
        let errors = inv.output["errors"].as_array().unwrap();
        assert!(
            errors[0].as_str().unwrap().contains("限流器"),
            "rule violation surfaced: {errors:?}"
        );
    }

    #[test]
    fn test_t612_validate_warning_priority_duplicate_exit_1() {
        let json = r#"{
            "version": "1.0.0",
            "global": { "storage": "memory", "cache": "memory", "metrics": "prometheus" },
            "rules": [
                {
                    "id": "a", "name": "rule a", "priority": 5,
                    "matchers": [{ "type": "User", "user_ids": ["u1"] }],
                    "limiters": [{ "type": "TokenBucket", "capacity": 10, "refill_rate": 1 }],
                    "action": { "on_exceed": "reject" }
                },
                {
                    "id": "b", "name": "rule b", "priority": 5,
                    "matchers": [{ "type": "User", "user_ids": ["u2"] }],
                    "limiters": [{ "type": "TokenBucket", "capacity": 10, "refill_rate": 1 }],
                    "action": { "on_exceed": "reject" }
                }
            ]
        }"#;
        let (_dir, path) = write_temp_config(json, "json");
        let inv = run(["validate", path.to_str().unwrap()]);
        assert_eq!(
            inv.exit_code, EXIT_WARNINGS,
            "duplicate priority → warning, exit 1"
        );
        assert_eq!(inv.output["valid"], serde_json::json!(true));
        assert!(inv.output["warnings"].as_array().unwrap().len() >= 1);
    }

    #[test]
    fn test_t612_validate_yaml_input() {
        let yaml = "version: \"1.0.0\"\nglobal:\n  storage: memory\n  cache: memory\n  metrics: prometheus\nrules: []\n";
        let (_dir, path) = write_temp_config(yaml, "yaml");
        let inv = run(["validate", path.to_str().unwrap()]);
        // rules 为空 → validate() 报「至少需要一个规则」→ exit 2（证明 YAML 被解析）
        assert_eq!(inv.exit_code, EXIT_ERROR);
        assert!(
            inv.output["errors"][0]
                .as_str()
                .unwrap()
                .contains("至少需要一个规则"),
            "yaml parsed: {:?}",
            inv.output
        );
    }

    // ========================================================================
    // export
    // ========================================================================

    #[test]
    fn test_t612_export_canonical_json_exit_0() {
        let (_dir, path) = write_temp_config(&valid_config_json(), "json");
        let inv = run(["export", path.to_str().unwrap()]);
        assert_eq!(inv.exit_code, EXIT_OK);
        assert_eq!(inv.output["format"], serde_json::json!("json"));
        assert_eq!(inv.output["config"]["version"], serde_json::json!("1.0.0"));
        assert_eq!(inv.output["config"]["rules"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_t612_export_unparsable_exit_2() {
        let inv = run(["export", "/nonexistent/config.yaml"]);
        assert_eq!(inv.exit_code, EXIT_ERROR);
        assert!(inv.output["config"].is_null());
    }

    // ========================================================================
    // apply（dry-run）
    // ========================================================================

    #[test]
    fn test_t612_apply_dry_run_plan_exit_0() {
        let (_dir, path) = write_temp_config(&valid_config_json(), "json");
        let inv = run(["apply", path.to_str().unwrap()]);
        assert_eq!(inv.exit_code, EXIT_OK, "{:?}", inv.output);
        assert_eq!(inv.output["dry_run"], serde_json::json!(true));
        assert_eq!(inv.output["applied"], serde_json::json!(true));
        assert_eq!(inv.output["rules_applied"], serde_json::json!(1));
        assert!(inv.output["config_hash"].is_string());
    }

    #[test]
    fn test_t612_apply_invalid_config_exit_2_no_plan() {
        let inv = run(["apply", "/nonexistent/config.toml"]);
        assert_eq!(inv.exit_code, EXIT_ERROR);
        assert_eq!(inv.output["applied"], serde_json::json!(false));
        assert!(
            inv.output.get("dry_run").is_none(),
            "invalid config → no plan emitted"
        );
    }

    #[test]
    fn test_t612_apply_out_writes_normalized_json() {
        let (_dir, path) = write_temp_config(&valid_config_json(), "json");
        let out_dir = tempfile::tempdir().unwrap();
        let out = out_dir.path().join("normalized.json");
        let inv = run([
            "apply",
            path.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ]);
        assert_eq!(inv.exit_code, EXIT_OK, "{:?}", inv.output);
        assert_eq!(
            inv.output["written_to"],
            serde_json::json!(out.to_str().unwrap())
        );
        let written = std::fs::read_to_string(&out).unwrap();
        let reparsed: FlowControlConfig = serde_json::from_str(&written).unwrap();
        assert_eq!(reparsed.rules.len(), 1, "normalized output round-trips");
    }

    #[test]
    fn test_t612_apply_out_write_failure_exit_2() {
        let (_dir, path) = write_temp_config(&valid_config_json(), "json");
        let inv = run([
            "apply",
            path.to_str().unwrap(),
            "--out",
            "/nonexistent-dir/x.json",
        ]);
        assert_eq!(inv.exit_code, EXIT_ERROR);
        assert_eq!(inv.output["applied"], serde_json::json!(false));
    }

    // ========================================================================
    // 用法与退出码契约
    // ========================================================================

    #[test]
    fn test_t612_no_args_prints_usage_exit_0() {
        let inv = run(Vec::<String>::new());
        assert_eq!(inv.exit_code, EXIT_OK);
        assert_eq!(inv.output["command"], serde_json::json!("help"));
        assert!(inv.output["usage"].is_object());
    }

    #[test]
    fn test_t612_unknown_command_exit_2() {
        let inv = run(["frobnicate"]);
        assert_eq!(inv.exit_code, EXIT_ERROR);
        assert_eq!(inv.output["command"], serde_json::json!("frobnicate"));
    }

    #[test]
    fn test_t612_version_reports_crate_version() {
        let inv = run(["--version"]);
        assert_eq!(inv.exit_code, EXIT_OK);
        assert_eq!(
            inv.output["version"],
            serde_json::json!(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn test_t612_missing_positional_argument_exit_2() {
        assert_eq!(run(["validate"]).exit_code, EXIT_ERROR);
        assert_eq!(run(["export"]).exit_code, EXIT_ERROR);
        assert_eq!(run(["apply"]).exit_code, EXIT_ERROR);
        assert_eq!(run(["apply", "x.json", "--out"]).exit_code, EXIT_ERROR);
    }
}
