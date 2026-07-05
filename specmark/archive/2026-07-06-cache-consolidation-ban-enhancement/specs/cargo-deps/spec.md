# Spec — cargo-deps

> Delta spec for change `cache-consolidation-ban-enhancement`. 覆盖此变更引入/修改的依赖治理能力域需求。

## Requirements

### R-cargo-deps-001: 移除 redis 直接依赖

Cargo.toml 不再包含 `redis` crate 作为 workspace 依赖或包依赖。所有 Redis 后端能力通过 `oxcache/redis` feature 提供。

**验收标准：**
- `Cargo.toml` 中无 `redis = ` workspace 依赖行
- `Cargo.toml` 中无 `redis = { workspace = true, optional = true }` 包依赖行
- `cargo tree -e features | grep redis` 仅显示 oxcache 的 redis feature 传递依赖
- 项目编译不依赖 `redis` crate 直接导入

### R-cargo-deps-002: 移除 redis-storage feature

`redis-storage` feature 完全移除，`full` feature 不再包含它。

**验收标准：**
- `Cargo.toml` `[features]` 节无 `redis-storage = ` 行
- `Cargo.toml` `full = [...]` 数组中无 `redis-storage` 字符串
- `cargo build --features redis-storage` 报错"unknown feature"
- `cargo build --features full` 编译通过且不启用 redis-storage

### R-cargo-deps-003: 升级依赖到最新稳定版本

所有可升级的 workspace 依赖升级到最新稳定版本，保持 edition = "2021"。

**验收标准：**
- `cargo update --dry-run` 输出无可用升级（或仅 patch 级别）
- `tokio` / `serde` / `axum` / `sqlx` / `sea-orm` / `oxcache` / `notify` 等核心依赖版本号 ≥ 当前最新稳定版
- `cargo build --features full` 编译通过
- `cargo test --features full --lib` 全部测试通过

### R-cargo-deps-004: 移除未使用依赖

通过 `cargo machete` 检测，移除所有编译时未使用的依赖。

**验收标准：**
- `cargo machete` 输出"No unused dependencies"或仅 feature-gated 依赖（有注释说明）
- 移除的依赖列表记录到 specmark/changes/cache-consolidation-ban-enhancement/machete-report.txt

## Constraints

- 保持 edition = "2021"，不升级 edition
- 不修改 external dependencies（oxcache/dbnexus/confers）源码（AGENTS.md 规则）
- dbnexus 0.2 vs oxcache 0.3 版本冲突若未解决，在 Cargo.toml 注释说明原因

## Out of Scope

- 不升级 Rust edition 到 2024
- 不替换 oxcache/dbnexus 为其他库
- 不引入新的核心依赖（仅升级现有）
