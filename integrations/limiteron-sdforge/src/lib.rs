// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! # limiteron-sdforge — sdforge 防护桥接 crate
//!
//! limiteron 侧提供的独立防护层：把 limiteron 的限流 / 熔断 / 封禁能力
//! 以最小依赖面暴露给 sdforge 应用。核心 [`guard`] 协议无关、不依赖
//! sdforge；启用 `sdforge` feature 后，`sdforge_adapter` 模块把 guard 适配
//! 为 `sdforge::domain::ForgeRateLimiter`。
//!
//! ## 与 sdforge 侧 `LimiteronForgeAdapter` 的分工（两仓 CHANGELOG 同步留痕）
//!
//! | | sdforge 侧 `integrations/limiteron_adapter.rs` | 本 crate |
//! |---|---|---|
//! | 视角 | sdforge 侧消费适配（面向 sdforge 内部 trait-kit 装配） | limiteron 侧独立防护层（面向 limiteron 用户接入 sdforge 应用） |
//! | 依赖方向 | sdforge → limiteron（optional `limiteron-integration`） | limiteron-sdforge → limiteron（本体）+ optional sdforge |
//! | 发布节奏 | 随 sdforge 发版 | 随 limiteron 发版 |
//!
//! 依赖方向与发布节奏相互独立，故不合并为单侧实现。sdforge 仓侧的
//! 分工记录由本仓 CHANGELOG 代持留痕（sdforge 侧同步义务见
//! limiteron `docs/CHANGELOG.md` 对应条目）。
//!
//! ## 菱形依赖与钉版统一策略（两处 optional 钉版对称）
//!
//! ```text
//! limiteron-sdforge ──► sdforge ──► limiteron
//!        │                                  ▲
//!        └──────────────────────────────────┘
//! ```
//!
//! 菱形合法：Cargo 对同一 semver 兼容区间合并为单一构建（本 workspace
//! 的 limiteron `0.3.0-rc.6` 满足 sdforge 声明的 `0.3.0-rc.4` 下界）。
//! 两侧 optional 钉版统一为同一策略——**钉对方已发布的 rc 版本下界 +
//! `default-features = false`**：
//!
//! - sdforge 侧（既有）：`limiteron = { version = "0.3.0-rc.4", default-features = false }`
//! - 本 crate（新增）：`sdforge = { version = "0.5.0-rc.5", default-features = false, optional = true }`
//!
//! rc.5 的存在性证据：crates.io 解包验证 `sdforge::domain::{ForgeError,
//! ForgeRateLimiter}` 已存在、`domain` 模块无条件编译（无 feature 门控）、
//! `ForgeError::internal(impl Display)` 构造器可用。

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod guard;

#[cfg(feature = "sdforge")]
pub mod sdforge_adapter;

pub use guard::{Guard, GuardConfig, GuardDecision, GuardIdentity};

#[cfg(feature = "sdforge")]
pub use sdforge_adapter::GuardForgeAdapter;
