# Change: 添加独立 Examples 目录

## Why
当前项目没有独立的 examples 目录来演示所有功能特性的使用方式。用户难以快速理解和学习框架的各种功能。需要创建一个独立的 Rust 目录，包含按模块划分的示例代码，独立于 main crate，不被 cargo.toml 引用。

## What Changes
- 创建独立的 `examples/` 目录（独立 Rust 项目，非 workspace）
- 每个功能模块一个单独的 example 文件
- 不被 `Cargo.toml` workspace 引用
- 演示框架所有核心功能的使用方式

## Impact
- Affected specs: examples (新增 capability)
- Affected code: 新增 `examples/` 目录及多个示例文件
- 无breaking changes，纯新增功能
