// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 决策链模块
//!
//! 使用责任链模式实现多限流器组合决策。
//!
//! # 特性
//!
//! - 责任链模式：支持链式调用多个限流器
//! - 短路逻辑：任一拒绝则立即返回拒绝
//! - 优先级排序：按优先级顺序执行限流器
//! - 决策聚合：聚合所有限流器的决策结果
//! - 可扩展：易于添加新的限流器类型

pub mod types;

pub use types::{ChainStats, DecisionChain, DecisionChainBuilder, DecisionNode};
