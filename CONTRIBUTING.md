# 贡献指南

感谢你对 Limiteron 项目的兴趣！本文档描述如何参与开发与提交代码。

## 开发环境

- **Rust 1.85+**（edition 2024）
- `cargo`、`rustfmt`、`clippy`
- `pre-commit`（安装：`pip install pre-commit && pre-commit install`）
- （可选）PostgreSQL 15+ / Redis 7+ — 用于集成测试

## TDD 工作流

每个开发任务组遵循以下循环（Red → Green → Commit → Analyze → Next）：

1. **定接口**：先定义 trait / API 签名（`trait Xxx { ... }`），不写实现
2. **写测试**：基于接口编写单元测试（`#[cfg(test)] mod tests { ... }`），此时测试应失败（red）
3. **写代码**：实现接口，使测试通过（green）
4. **跑测试**：`cargo test --features <对应特性> --lib`，确保所有测试通过
5. **commit**：通过后执行 `git add . && git commit -m "feat(<模块>): <描述>"`
6. **gitnexus analyze**：用 gitnexus 工具分析本任务对其他模块的影响，识别需联动修改的代码
7. **继续下一个**：基于 analyze 结果调整后续任务，再开始下一轮循环

## Pre-commit Hooks

提交前会自动运行以下检查：

- `cargo fmt --check` — 格式检查
- `cargo clippy -D warnings` — 静态分析
- `cargo check` — 编译检查
- `no-commit-to-branch`（main/master）— 禁止直接提交到受保护分支
- `trailing-whitespace` / `end-of-file-fixer` / `check-yaml` / `check-toml` — 通用规范

> **禁止使用 `--no-verify` 跳过 pre-commit hooks。**

## 代码质量

项目要求在关键节点使用以下工具：

- **diting**：代码简化、架构优化、性能审查（`review`/`audit`/`tech debt`）
- **tiangang**：SAST 安全扫描（发布前 0 CRITICAL 才允许继续）
- **kueiku**：硬性 bug 根因分析与方法论选择

## Pull Request 流程

1. 从 `main` 创建 feature 分支：`git checkout -b feature/<描述>`
2. 编写代码并确保 `cargo test --all-features` 通过
3. 确保 pre-commit hooks 全部通过
4. 提交 PR，关联相关 Issue
5. 等待 CI 通过与代码审查

## 代码风格

- 遵循现有代码库的命名与架构惯例（Rule 11：惯例优先于新颖）
- 依赖必须通过 feature 门控（`optional = true`）
- 100 字符最大行宽（`rustfmt.toml`），4 空格缩进
- 禁止通配符导入（`warn-on-all-wildcard-imports`）
- 使用 `ahash` + `DashMap`，禁止 `std::collections::HashMap/Set`
- 使用 `parking_lot` 替代 `std::sync` 原语
- 所有公开 API 必须有文档注释
- 不安全的代码必须注明安全不变式

## 提交信息规范

使用 Conventional Commits：

- `feat(<模块>): <描述>` — 新功能
- `fix(<模块>): <描述>` — Bug 修复
- `chore(<模块>): <描述>` — 构建/工具/文档
- `refactor(<模块>): <描述>` — 重构
- `docs(<模块>): <描述>` — 文档
