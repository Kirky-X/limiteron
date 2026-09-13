# 🤝 Limiteron 贡献指南

感谢你对 Limiteron 项目的兴趣！本文档描述如何参与开发与提交代码。

## 📋 目录

<details open>
<summary>📑 目录</summary>

- [🛠️ 开发环境](#️-开发环境)
- [🔄 开发工作流](#-开发工作流)
- [🪝 提交前检查](#-提交前检查)
- [📐 代码风格](#-代码风格)
- [🔀 Pull Request 流程](#-pull-request-流程)
- [✉️ 提交信息规范](#️-提交信息规范)

</details>

---

## 🛠️ 开发环境

- **Rust 1.97.1+**（edition 2024，[rust-toolchain.toml](../rust-toolchain.toml) 统一锁定）
- `cargo`、`rustfmt`、`clippy`
- Git 钩子工具（二选一，功能等价）：
  - [lefthook](https://github.com/evilmartians/lefthook)：安装 `lefthook install`
  - [pre-commit](https://pre-commit.com/)：安装 `pip install pre-commit && pre-commit install`
- （可选）PostgreSQL 15+ / Redis 7+：用于持久化与缓存相关的集成测试

```bash
git clone https://github.com/Kirky-X/limiteron
cd limiteron
lefthook install
cargo test --workspace --no-default-features --features full
```

## 🔄 开发工作流

每个开发任务建议遵循以下测试先行循环（Red → Green → Commit → Next）：

1. **定接口**：先定义 trait / API 签名（`trait Xxx { ... }`），不写实现
2. **写测试**：基于接口编写单元测试（`#[cfg(test)] mod tests { ... }`），此时测试应失败（red）
3. **写代码**：实现接口，使测试通过（green）
4. **跑测试**：`cargo test --features <对应特性> --lib`，确保所有测试通过
5. **影响面分析**：检查本次修改对其他模块的影响，识别需联动修改的代码
6. **继续下一个**：基于影响面调整后续任务，再开始下一轮循环

> ⚠️ 注意 `postgres` / `sqlite` / `mysql` 存储驱动互斥，测试与验证请使用显式特性组合，不要使用 `--all-features`。

## 🪝 提交前检查

提交前会自动运行以下检查（见 [.pre-commit-config.yaml](../.pre-commit-config.yaml) 与 [lefthook.yml](../lefthook.yml)）：

| 类别 | 检查 |
|------|------|
| 格式 | `cargo fmt --all -- --check` |
| 静态分析 | `cargo clippy --all-targets --no-default-features --features full -- -D warnings` |
| 编译 | `cargo check --no-default-features --features full` |
| 供应链 | `cargo deny check`（pre-push 另有 `cargo audit` 与覆盖率 ≥80% 门禁） |
| 通用规范 | trailing-whitespace / end-of-file-fixer / check-yaml / check-toml / typos |
| 密钥防护 | 私钥扫描与 detect-secrets（带 `.secrets.baseline` 误报基线） |

> **禁止使用 `--no-verify` 跳过提交钩子。**

## 📐 代码风格

- 遵循现有代码库的命名与架构惯例，惯例优先于新颖
- 依赖必须通过 feature 门控（`optional = true`）
- 100 字符最大行宽（[rustfmt.toml](../rustfmt.toml)），4 空格缩进
- 禁止通配符导入（`warn-on-all-wildcard-imports`）
- 使用 `ahash` + `DashMap`，禁止 `std::collections::HashMap`/`HashSet`
- 使用 `parking_lot` 替代 `std::sync` 原语
- 所有公开 API 必须有文档注释
- 不安全的代码必须注明安全不变式

## 🔀 Pull Request 流程

1. 从 `main` 创建 feature 分支：`git checkout -b feature/<描述>`
2. 编写代码并确保测试通过：`cargo test --workspace --no-default-features --features full`
3. 确保提交钩子全部通过
4. 提交 PR，关联相关 Issue
5. 等待 CI 通过与代码审查

## ✉️ 提交信息规范

使用 Conventional Commits：

- `feat(<模块>): <描述>` — 新功能
- `fix(<模块>): <描述>` — Bug 修复
- `chore(<模块>): <描述>` — 构建/工具/文档
- `refactor(<模块>): <描述>` — 重构
- `docs(<模块>): <描述>` — 文档
