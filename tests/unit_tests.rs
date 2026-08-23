// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 单元测试入口（下沉类用例）
//!
//! 集成测试净化时从 tests/integration、tests/e2e 下沉的错误注入 / 故障
//! 模拟用例。单元层允许测试替身（mock 仅存在于单元测试）。

mod unit;

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::unit::*;
}