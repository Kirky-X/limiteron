// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! trait-kit `AsyncKit` integration for limiteron.
//!
//! Enable via the `kit` cargo feature. Provides [`LimiteronModule`] — a leaf
//! module (no upstream dependencies) that constructs a limiteron
//! [`Governor`](crate::Governor) capability during
//! [`AsyncKit::build`](trait_kit::AsyncKit::build).
//!
//! See `specmark/changes/trait-kit-async-integration/specs/limiteron-module/spec.md`
//! for the acceptance criteria driving this module.

pub mod module;
pub use module::LimiteronModule;
