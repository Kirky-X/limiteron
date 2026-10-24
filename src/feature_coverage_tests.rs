// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! feature 覆盖不变式守卫。
//!
//! 不在 `full` 预设内的 feature 若既无 CI 腿、也无 docs.rs 元数据覆盖，
//! 其代码路径既不被编译也不被执行：脱敏引擎（`log-redaction`）曾长期处于
//! 该状态——公开 API 不出现在已发布文档中，缺陷也无法被 CI 捕获。

use std::path::{Path, PathBuf};

/// 脱敏引擎的 feature：不在 `full` 预设内，须由 CI 与 docs.rs 显式覆盖。
const OPT_IN_FEATURE: &str = "log-redaction";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_file(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("读取 {} 失败: {e}", path.display()))
}

/// CI 必须有一条 cargo 腿显式开启该 feature。
#[test]
fn ci_workflow_builds_opt_in_feature() {
    let workflow = read_file(&workspace_root().join(".github/workflows/ci.yml"));
    let covered = workflow.lines().any(|line| {
        // 只认真正的 cargo 命令行，注释行不算覆盖
        let command = line
            .trim()
            .strip_prefix("run:")
            .unwrap_or(line.trim())
            .trim();
        if !command.starts_with("cargo ") {
            return false;
        }
        let Some(features_at) = command.find("--features") else {
            return false;
        };
        command[features_at + "--features".len()..]
            .split([',', ' ', '"', '\''])
            .any(|feature| feature == OPT_IN_FEATURE)
    });
    assert!(
        covered,
        "CI 未在任何 cargo 调用的 --features 列表中启用 `{OPT_IN_FEATURE}`：该 feature 不在 full 预设内，其代码路径不会被编译或执行"
    );
}

/// docs.rs 元数据必须包含该 feature，公开 API 才会出现在已发布文档中。
#[test]
fn docs_rs_metadata_builds_opt_in_feature() {
    let manifest_text = read_file(&workspace_root().join("Cargo.toml"));
    let manifest: toml::Value = toml::from_str(&manifest_text).expect("解析 Cargo.toml 失败");
    let features = manifest
        .get("package")
        .and_then(|package| package.get("metadata"))
        .and_then(|metadata| metadata.get("docs"))
        .and_then(|docs| docs.get("rs"))
        .and_then(|rs| rs.get("features"))
        .and_then(|features| features.as_array())
        .expect("docs.rs 元数据缺少 features");
    assert!(
        features
            .iter()
            .any(|feature| feature.as_str() == Some(OPT_IN_FEATURE)),
        "docs.rs 元数据未启用 `{OPT_IN_FEATURE}`：脱敏引擎的公开 API 不会出现在已发布文档中"
    );
}

/// 脱敏引擎的公开 API 在 feature 开启时可达且生效。
#[cfg(feature = "log-redaction")]
#[test]
fn advanced_redaction_api_is_reachable_and_masks() {
    use crate::logging::{
        RedactionConfig, contains_sensitive_info, redact_advanced, redact_http_content,
    };

    // 敏感字段名 → 整段掩码
    assert_eq!(
        redact_advanced(Some("secret-value"), Some("password")),
        "***"
    );
    // 非敏感字段名 → 原值透传
    assert_eq!(
        redact_advanced(Some("user123"), Some("username")),
        "user123"
    );
    // 敏感信息检出：能掩码的必能检出
    assert!(contains_sensitive_info("password=secret123"));
    assert!(!contains_sensitive_info("username=user123"));
    // HTTP 内容：敏感值掩码，非敏感值保留
    let http = redact_http_content(r#"{"password": "secret123", "username": "user123"}"#);
    assert!(!http.contains("secret123"), "密码值应被脱敏: {http}");
    assert!(http.contains("user123"), "非敏感值应保留: {http}");
    // 脱敏配置：按字段开关决定是否掩码
    let formatted = RedactionConfig::new()
        .add_field("password", true)
        .add_field("username", false)
        .format(|field| match field {
            "password" => Some("secret123".to_string()),
            "username" => Some("user123".to_string()),
            _ => None,
        });
    assert!(
        !formatted.contains("secret123"),
        "敏感字段应被脱敏: {formatted}"
    );
    assert!(
        formatted.contains("user123"),
        "非敏感字段应保留: {formatted}"
    );
}
