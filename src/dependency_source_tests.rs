// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 依赖来源不变式守卫。
//!
//! 跨仓自研依赖必须经 crates.io registry 引入：CI 与发布流程只检出本仓，
//! 指向仓外兄弟目录的 `path` 依赖会让清单在依赖装载阶段直接失败
//! （`failed to load source for dependency`），构建 / 测试 / 文档 / 发布全断。
//! 本地联调走工作区外的路径覆盖（工作区根 `.cargo/config.toml` 的 `paths` 项），
//! 仓内清单不得出现越出工作区的 path。

use std::path::{Path, PathBuf};

use toml::Value;

/// 必须经 crates.io registry 引入的跨仓自研依赖（crates.io 均有已发布版本）。
const CROSS_REPO_CRATES: [&str; 5] = ["oxcache", "dbnexus", "trait-kit", "inklog", "confers"];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_manifest(manifest_path: &Path) -> Value {
    let text = std::fs::read_to_string(manifest_path)
        .unwrap_or_else(|e| panic!("读取 {} 失败: {e}", manifest_path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("解析 {} 失败: {e}", manifest_path.display()))
}

/// 清单中的依赖表：工作区级 + 包级（常规 / 开发 / 构建）。
fn dependency_tables(manifest: &Value) -> Vec<(&'static str, &Value)> {
    let mut tables = Vec::new();
    if let Some(table) = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
    {
        tables.push(("workspace.dependencies", table));
    }
    for name in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = manifest.get(name) {
            tables.push((name, table));
        }
    }
    tables
}

/// 依赖表内声明了 path 的条目：(依赖名, path 原值)。
fn path_entries(table: &Value) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    let Some(map) = table.as_table() else {
        return entries;
    };
    for (name, value) in map {
        if let Some(path) = value.get("path").and_then(|path| path.as_str()) {
            entries.push((name.clone(), path.to_string()));
        }
    }
    entries
}

/// 工作区成员清单：与根清单同受 path 越界约束。
fn workspace_member_manifests(root: &Path, manifest: &Value) -> Vec<PathBuf> {
    let mut manifests = Vec::new();
    let Some(members) = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(|members| members.as_array())
    else {
        return manifests;
    };
    for member in members.iter().filter_map(|member| member.as_str()) {
        let candidate = root.join(member).join("Cargo.toml");
        if candidate.is_file() {
            manifests.push(candidate);
        }
    }
    manifests
}

/// 工作区级依赖条目。
fn workspace_entry<'a>(manifest: &'a Value, crate_name: &str) -> Option<&'a Value> {
    manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(|table| table.get(crate_name))
}

/// 依赖的有效声明：`workspace = true` 时回落到工作区依赖表。
fn effective_entry<'a>(manifest: &'a Value, crate_name: &str) -> Option<&'a Value> {
    match manifest
        .get("dependencies")
        .and_then(|table| table.get(crate_name))
    {
        Some(entry) if entry.get("workspace").and_then(|flag| flag.as_bool()) == Some(true) => {
            workspace_entry(manifest, crate_name)
        }
        Some(entry) => Some(entry),
        None => workspace_entry(manifest, crate_name),
    }
}

/// 依赖的有效 path（`workspace = true` 时回落到工作区依赖表）。
fn effective_path(manifest: &Value, crate_name: &str) -> Option<String> {
    effective_entry(manifest, crate_name)
        .and_then(|entry| entry.get("path"))
        .and_then(|path| path.as_str())
        .map(str::to_string)
}

/// 依赖表根名。
fn is_dependency_root(table: &str) -> bool {
    matches!(
        table,
        "dependencies" | "dev-dependencies" | "build-dependencies"
    )
}

/// 把依赖声明与其紧邻上方的注释块关联起来：(依赖名, 注释文本)。
fn dependency_comments(manifest: &str) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    let mut table = String::new();
    let mut pending: Vec<&str> = Vec::new();

    for line in manifest.lines() {
        let trimmed = line.trim();
        if let Some(comment) = trimmed.strip_prefix('#') {
            pending.push(comment.trim());
            continue;
        }
        if trimmed.is_empty() {
            pending.clear();
            continue;
        }
        if let Some(header) = trimmed
            .strip_prefix('[')
            .and_then(|header| header.strip_suffix(']'))
        {
            table = header.trim().to_string();
            // `[dependencies.foo]` 形态：依赖名取表头末段
            if let Some((root, name)) = table.split_once('.')
                && is_dependency_root(root)
            {
                entries.push((name.to_string(), pending.join("\n")));
            }
            pending.clear();
            continue;
        }
        if is_dependency_root(&table)
            && let Some((key, _)) = trimmed.split_once('=')
        {
            // `foo.workspace = true` 形态：依赖名取点号前段
            let key = key.trim();
            let name = key.strip_suffix(".workspace").unwrap_or(key);
            entries.push((name.to_string(), pending.join("\n")));
        }
        pending.clear();
    }
    entries
}

/// 全部 path 依赖都必须落在工作区根内：只检出本仓的 CI 才装载得起来。
#[test]
fn path_dependencies_stay_inside_workspace_root() {
    let root = workspace_root();
    let root_canonical = root
        .canonicalize()
        .unwrap_or_else(|e| panic!("工作区根 {} 不可规范化: {e}", root.display()));
    let root_manifest_path = root.join("Cargo.toml");
    let root_manifest = read_manifest(&root_manifest_path);

    let mut manifest_paths = vec![root_manifest_path];
    manifest_paths.extend(workspace_member_manifests(&root, &root_manifest));

    let mut inspected = 0usize;
    for manifest_path in &manifest_paths {
        let manifest = read_manifest(manifest_path);
        let base = manifest_path.parent().expect("清单有父目录");
        for (table_name, table) in dependency_tables(&manifest) {
            for (dep_name, path) in path_entries(table) {
                inspected += 1;
                let target = base.join(&path);
                let target_canonical = target.canonicalize().unwrap_or_else(|e| {
                    panic!(
                        "{}[{}].{} 的 path 目标不存在: {} ({e})——只检出本仓的 CI 装载不了该依赖",
                        manifest_path.display(),
                        table_name,
                        dep_name,
                        target.display()
                    )
                });
                assert!(
                    target_canonical.starts_with(&root_canonical),
                    "{}[{}].{} 的 path 越出工作区根: {}（工作区根 {}）——跨仓依赖必须经 crates.io registry 引入",
                    manifest_path.display(),
                    table_name,
                    dep_name,
                    target_canonical.display(),
                    root_canonical.display()
                );
            }
        }
    }
    assert!(inspected > 0, "未发现任何 path 依赖，守卫失去判据");
}

/// 跨仓自研依赖必须经 registry 引入：带 version 约束、无 path。
///
/// 清单未声明的名单项直接跳过（如 inklog 已随掩码引擎换源从本仓清单移除，
/// 仅存于 integrations/ 适配器清单，由 path 守卫覆盖），不变式约束的是
/// 「已声明者的来源」，不是「必须声明」。
#[test]
fn cross_repo_crates_are_registry_sourced() {
    let manifest = read_manifest(&workspace_root().join("Cargo.toml"));
    for crate_name in CROSS_REPO_CRATES {
        let Some(entry) = effective_entry(&manifest, crate_name) else {
            continue;
        };
        assert!(
            entry.get("path").is_none(),
            "{crate_name} 声明了 path: {:?}——跨仓依赖必须经 crates.io registry 引入，只检出本仓的 CI 装载不了仓外 path",
            entry.get("path")
        );
        assert!(
            entry.get("version").is_some(),
            "{crate_name} 缺少 version 约束：registry 依赖须声明版本区间"
        );
    }
}

/// 注释声称经 registry 引入时，声明不得同时带 path。
#[test]
fn registry_comments_match_dependency_declarations() {
    let manifest_path = workspace_root().join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("读取 {} 失败: {e}", manifest_path.display()));
    let manifest = read_manifest(&manifest_path);

    let mut checked = 0usize;
    for (dep_name, comment) in dependency_comments(&text) {
        if !comment.to_lowercase().contains("registry") {
            continue;
        }
        checked += 1;
        let declared_path = effective_path(&manifest, &dep_name);
        assert!(
            declared_path.is_none(),
            "{dep_name} 的注释声称经 crates.io registry 引入，实际声明带 path: {declared_path:?}——注释与声明矛盾"
        );
    }
    assert!(
        checked > 0,
        "未找到任何声称 registry 来源的依赖注释，守卫失去判据"
    );
}
