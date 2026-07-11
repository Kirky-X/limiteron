// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 统一集成测试入口
//!
//! 包含 common 模块和所有子模块测试

mod common;
mod modules;

#[cfg(test)]
mod tests {
    // 集成测试在 modules 子模块中定义
    // 使用 cargo test --test unified_tests 运行
}
