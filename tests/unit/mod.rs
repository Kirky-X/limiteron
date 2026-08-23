// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 单元测试模块（下沉类）
//!
//! 集成测试净化下沉的错误注入 / 故障模拟用例。单元层允许测试替身
//! （mock 仅存在于单元测试，集成与 e2e 禁 mock）。

pub mod advanced_decision_chain;
pub mod resource_exhaustion;
pub mod storage_error_injection;