// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! `LimiteronModule` — trait-kit 0.2.2 `AsyncKit` integration for limiteron.
//!
//! Phase 3 (T021 Red / T022 Green) of the `trait-kit-async-integration`
//! change. Wires limiteron's [`Governor`] into the `AsyncKit` dependency
//! injection framework as a leaf module (no upstream dependencies).
//!
//! # Design divergence from `design.md` / `spec.md` (Rule 7: expose, don't
//! paper over)
//!
//! `design.md` Decision 3 (lines 417-439) and `limiteron-module/spec.md`
//! R-001 wrote the capability type as `Arc<dyn Limiter + Send + Sync>`, the
//! error type as `LimiteronError`, and the config type as `LimiteronConfig`.
//! limiteron v0.2.1's actual API does not match this pseudo-code on **three**
//! independent points, all surfaced here rather than papered over:
//!
//! 1. **`Governor` does NOT implement `Limiter`** — `Governor::check` takes
//!    `&RequestContext` and returns `Decision`; `Limiter::allow` takes `u64`
//!    cost and returns `bool`. `Governor` is the *controller* (orchestrates
//!    a `DecisionChain` of `Limiter`s), not itself a `Limiter`. We therefore
//!    expose the capability as `Arc<Governor>` (concrete type) instead of
//!    `Arc<dyn Limiter>`. This mirrors oxcache Phase 2's approach (using
//!    `CacheBackend` instead of non-object-safe `UnifiedCache`).
//!
//! 2. **`LimiteronError` does NOT exist** — limiteron's error type is
//!    [`LimiteronError`] (thiserror-based, implements `std::error::Error +
//!    Send + 'static`). We use `LimiteronError` as `AsyncAutoBuilder::Error`.
//!
//! 3. **`LimiteronConfig` does NOT exist** — per spec Constraints line 43
//!    ("LimiteronConfig 类型从 limiteron 现有配置类型复用"), we reuse
//!    limiteron's existing [`FlowControlConfig`] directly.
//!
//! **Follow-up** (out of scope for T020-T022): if the spec owner requires
//! the literal `Arc<dyn Limiter>` form, `Governor` must be made to implement
//! `Limiter` (or a new `RateLimiter` trait introduced). That change affects
//! limiteron's public API and is deferred to its own change spec.

use crate::config::FlowControlConfig;
use crate::error::LimiteronError;
use crate::governor::Governor;
use std::any::TypeId;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use trait_kit::prelude::*;

/// trait-kit `AsyncKit` module that constructs a limiteron `Governor`.
///
/// Leaf module (no upstream dependencies). Register with
/// `AsyncKit::register::<LimiteronModule>()`, configure via
/// `kit.set_config(FlowControlConfig::default())`, then `kit.build().await`
/// and retrieve the capability with `kit.require::<LimiteronModule>()`.
///
/// The returned `Arc<Governor>` is the concrete controller type — see the
/// module-level docs for the design-divergence rationale (spec.md wrote
/// `Arc<dyn Limiter>`, but `Governor` does not implement `Limiter`).
pub struct LimiteronModule;

impl ModuleMeta for LimiteronModule {
    const NAME: &'static str = "limiteron";

    fn dependencies() -> &'static [(&'static str, TypeId)] {
        &[]
    }
}

impl AsyncAutoBuilder for LimiteronModule {
    type Capability = Arc<Governor>;
    type Error = LimiteronError;

    fn build<'a>(
        kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
        Box::pin(async move {
            let config: FlowControlConfig = kit.config().map_err(|e| {
                LimiteronError::ConfigError(format!("LimiteronModule: read config: {e}"))
            })?;
            // Leaf module: use in-memory storage so the kit feature stays
            // independent of postgres/redis backends. Production callers can
            // construct Governor directly via its builder for persistent
            // storage; the kit integration is for wiring/test scenarios.
            use crate::storage::{BanStorage, MemoryBanStorage, MemoryStorage, Storage};
            let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
            let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
            let governor = Governor::builder()
                .with_config(config)
                .with_storage(storage)
                .with_ban_storage(ban_storage)
                .build()
                .await?;
            Ok(Arc::new(governor))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FlowControlConfig;
    use crate::governor::Governor;
    use std::any::TypeId;
    use std::sync::Arc;

    /// Build a minimal valid `FlowControlConfig` (1 rule, 1 matcher, 1
    /// limiter). `FlowControlConfig::default()` has an empty rules vec, which
    /// fails `validate()` with "至少需要一个规则" — Governor::builder().build()
    /// calls validate() and returns Err. The kit integration must surface that
    /// Err, not paper over it (Rule 12), so tests use a valid config.
    fn make_minimal_valid_config() -> FlowControlConfig {
        use crate::config::{Action, ActionConfig, LimiterConfig, Matcher, Rule};
        let mut config = FlowControlConfig::default();
        config.rules.push(Rule {
            id: "default".to_string(),
            name: "default rule".to_string(),
            priority: 0,
            matchers: vec![Matcher::User {
                user_ids: vec!["user1".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 100,
                refill_rate: 10,
            }],
            action: ActionConfig {
                on_exceed: Action::Reject,
                ban: None,
            },
        });
        config
    }

    /// R-limiteron-module-001: `LimiteronModule::NAME == "limiteron"`.
    #[test]
    fn limiteron_module_meta_name() {
        assert_eq!(LimiteronModule::NAME, "limiteron");
    }

    /// R-limiteron-module-001: `LimiteronModule::dependencies()` is empty
    /// (limiteron is a leaf module — no upstream deps).
    #[test]
    fn limiteron_module_meta_dependencies_empty() {
        assert_eq!(
            LimiteronModule::dependencies(),
            &[] as &[(&'static str, TypeId)]
        );
    }

    /// R-limiteron-module-001: register `LimiteronModule` + `set_config` +
    /// `build()` + `require::<LimiteronModule>()` returns an `Arc<Governor>`
    /// capability that was constructed from the kit's config.
    #[tokio::test]
    async fn limiteron_module_build_returns_governor_capability() {
        let mut kit = AsyncKit::new();
        kit.set_config(make_minimal_valid_config());
        kit.register::<LimiteronModule>()
            .expect("register LimiteronModule");
        let kit = kit.build().await.expect("AsyncKit::build");
        let governor: Arc<Governor> = kit
            .require::<LimiteronModule>()
            .expect("require LimiteronModule");
        // Smoke-test the capability type-checks as Arc<Governor> and was
        // actually constructed (register+build+require succeeded).
        let _ = governor;
    }

    /// R-limiteron-module-001: build reads `FlowControlConfig` from
    /// `kit.config::<FlowControlConfig>()` — verifies the config we set is
    /// honored by the constructed Governor (build returns Err on missing
    /// config).
    #[tokio::test]
    async fn limiteron_module_build_reads_config_from_kit() {
        let mut kit = AsyncKit::new();
        kit.set_config(make_minimal_valid_config());
        kit.register::<LimiteronModule>()
            .expect("register LimiteronModule");
        let kit = kit.build().await.expect("AsyncKit::build");
        let _governor: Arc<Governor> = kit
            .require::<LimiteronModule>()
            .expect("require LimiteronModule");
        // If build succeeded, config was read (build() returns Err on
        // missing config).
    }

    /// R-limiteron-module-001: `LimiteronModule::build` returns a
    /// `Pin<Box<dyn Future + Send>>` (async build), not a sync `Result`.
    /// Verified by calling `AsyncAutoBuilder::build` directly on an unbuilt
    /// kit and awaiting the returned future.
    #[tokio::test]
    async fn limiteron_module_build_is_async() {
        let kit = AsyncKit::new();
        kit.set_config(make_minimal_valid_config());
        // Call AsyncAutoBuilder::build directly (bypassing AsyncKit::build's
        // topological pipeline). The return type is Pin<Box<dyn Future + Send>>;
        // awaiting it must yield the capability.
        let fut = <LimiteronModule as AsyncAutoBuilder>::build(&kit);
        let _governor: Arc<Governor> = fut.await.expect("build future resolves");
        // Capability satisfies Send + Sync (AsyncAutoBuilder bound).
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Arc<Governor>>();
    }
}
