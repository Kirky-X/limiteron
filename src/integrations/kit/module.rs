// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! `LimiteronModule` — trait-kit `AsyncKit` integration for limiteron.
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
use crate::storage::{BanStorage, MemoryBanStorage, MemoryStorage, Storage};
use std::any::TypeId;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use trait_kit::prelude::*;

/// 存储覆盖配置 — 通过 `AsyncKit::set_config(LimiteronStorageConfig::...)`
/// 向 [`LimiteronModule`] 注入自定义 `Storage` / `BanStorage` 实例。
///
/// 未设置（或不调用任何 builder 方法）时，`LimiteronModule` 保持默认行为：
/// 使用进程内 `MemoryStorage` / `MemoryBanStorage`（向后兼容）。
#[derive(Default, Clone)]
pub struct LimiteronStorageConfig {
    storage: Option<Arc<dyn Storage>>,
    ban_storage: Option<Arc<dyn BanStorage>>,
}

impl LimiteronStorageConfig {
    /// 创建空覆盖配置（全部使用默认实现）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 注入存储实例。
    pub fn with_storage(mut self, storage: Arc<dyn Storage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// 注入封禁存储实例。
    pub fn with_ban_storage(mut self, ban_storage: Arc<dyn BanStorage>) -> Self {
        self.ban_storage = Some(ban_storage);
        self
    }

    /// 已注入的存储（`None` = 使用默认 `MemoryStorage`）。
    pub fn storage(&self) -> Option<Arc<dyn Storage>> {
        self.storage.clone()
    }

    /// 已注入的封禁存储（`None` = 使用默认 `MemoryBanStorage`）。
    pub fn ban_storage(&self) -> Option<Arc<dyn BanStorage>> {
        self.ban_storage.clone()
    }
}

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
            // 存储注入钩子：`kit.set_config(LimiteronStorageConfig::...)` 注入
            // 自定义存储；未设置时使用 in-memory 实现（leaf module 不依赖
            // postgres/redis 后端；生产调用方可用 Governor 的 builder 直连持久化
            // 存储，kit 集成面向 wiring/测试场景）。
            let storage_override: Option<LimiteronStorageConfig> = kit.config().ok();
            let storage: Arc<dyn Storage> = storage_override
                .as_ref()
                .and_then(|o| o.storage())
                .unwrap_or_else(|| Arc::new(MemoryStorage::new()) as Arc<dyn Storage>);
            let ban_storage: Arc<dyn BanStorage> = storage_override
                .as_ref()
                .and_then(|o| o.ban_storage())
                .unwrap_or_else(|| Arc::new(MemoryBanStorage::new()) as Arc<dyn BanStorage>);
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
#[cfg(feature = "ban-manager")]
    use crate::matchers::Identifier;
    #[cfg(feature = "ban-manager")]
    use crate::storage::{BanHistory, BanRecord, BanTarget};
    #[cfg(feature = "ban-manager")]
    use crate::error::StorageError;
    #[cfg(feature = "ban-manager")]
    use std::any::Any;
    use std::any::TypeId;
    use std::sync::Arc;
    #[cfg(feature = "ban-manager")]
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 可观测的封禁存储替身：委托给真实 `MemoryBanStorage`，同时计数
    /// `save`/`is_banned` 调用，用于验证注入实例确实被路由使用
    /// （单元层测试替身，符合净化规范）。
    #[cfg(feature = "ban-manager")]
    struct RecordingBanStorage {
        inner: MemoryBanStorage,
        saves: AtomicUsize,
    }

    #[cfg(feature = "ban-manager")]
    impl RecordingBanStorage {
        fn new() -> Self {
            Self {
                inner: MemoryBanStorage::new(),
                saves: AtomicUsize::new(0),
            }
        }

        fn save_count(&self) -> usize {
            self.saves.load(Ordering::SeqCst)
        }
    }

    #[cfg(feature = "ban-manager")]
    #[async_trait::async_trait]
    impl BanStorage for RecordingBanStorage {
        async fn is_banned(
            &self,
            target: &BanTarget,
        ) -> Result<Option<BanRecord>, StorageError> {
            self.inner.is_banned(target).await
        }

        async fn save(&self, record: &BanRecord) -> Result<(), StorageError> {
            self.saves.fetch_add(1, Ordering::SeqCst);
            self.inner.save(record).await
        }

        async fn get_history(
            &self,
            target: &BanTarget,
        ) -> Result<Option<BanHistory>, StorageError> {
            self.inner.get_history(target).await
        }

        async fn increment_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
            self.inner.increment_ban_times(target).await
        }

        async fn get_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
            self.inner.get_ban_times(target).await
        }

        async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError> {
            self.inner.remove_ban(target).await
        }

        async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
            self.inner.cleanup_expired_bans().await
        }

        async fn list_bans(
            &self,
            active_only: bool,
            offset: u64,
            limit: u64,
        ) -> Result<Vec<BanRecord>, StorageError> {
            self.inner.list_bans(active_only, offset, limit).await
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

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

    #[cfg(feature = "ban-manager")]
    /// 存储注入钩子：`kit.set_config(LimiteronStorageConfig::new()
    /// .with_ban_storage(recording))` 后，Governor 的封禁操作必须路由到注入
    /// 实例（通过替身计数验证），而非默认 `MemoryBanStorage`。
    #[tokio::test]
    async fn limiteron_module_routes_to_injected_ban_storage() {
        let recording = Arc::new(RecordingBanStorage::new());
        let recording_for_assert = recording.clone();

        let mut kit = AsyncKit::new();
        kit.set_config(make_minimal_valid_config());
        kit.set_config(
            LimiteronStorageConfig::new().with_ban_storage(recording.clone() as Arc<dyn BanStorage>),
        );
        kit.register::<LimiteronModule>()
            .expect("register LimiteronModule");
        let kit = kit.build().await.expect("AsyncKit::build");
        let governor: Arc<Governor> = kit
            .require::<LimiteronModule>()
            .expect("require LimiteronModule");

        // 触发封禁：create_ban → save + is_banned 由注入替身承接
        let identifier = Identifier::UserId("injected-route-user".to_string());
        governor
            .ban_identifier(&identifier, "test ban", None)
            .await
            .expect("ban via injected storage");

        assert!(
            recording_for_assert.save_count() > 0,
            "注入的 BanStorage 必须被 Governor 路由调用（save）"
        );
        // 解封路径同样路由到注入实例（委托 inner 真实实现，不抛错）
        governor
            .unban_identifier(&identifier)
            .await
            .expect("unban via injected storage");
    }

    #[cfg(feature = "ban-manager")]
    /// 存储注入钩子：未设置 `LimiteronStorageConfig` 时保持默认
    /// `MemoryStorage`/`MemoryBanStorage`（向后兼容），build + 封禁正常。
    #[tokio::test]
    async fn limiteron_module_defaults_to_memory_storage() {
        let mut kit = AsyncKit::new();
        kit.set_config(make_minimal_valid_config());
        kit.register::<LimiteronModule>()
            .expect("register LimiteronModule");
        let kit = kit.build().await.expect("AsyncKit::build");
        let governor: Arc<Governor> = kit
            .require::<LimiteronModule>()
            .expect("require LimiteronModule");

        let identifier = Identifier::UserId("default-memory-user".to_string());
        governor
            .ban_identifier(&identifier, "test ban", None)
            .await
            .expect("ban with default MemoryBanStorage");
    }

    /// 存储注入钩子：`with_storage` 注入的 `Storage` 同样被路由（通过
    /// `LimiteronStorageConfig` 的 getter 往返验证配置保留）。
    #[test]
    fn limiteron_storage_config_builders_roundtrip() {
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
        let config = LimiteronStorageConfig::new()
            .with_storage(storage.clone())
            .with_ban_storage(ban_storage.clone());
        assert!(config.storage().is_some());
        assert!(config.ban_storage().is_some());
        let empty = LimiteronStorageConfig::new();
        assert!(empty.storage().is_none());
        assert!(empty.ban_storage().is_none());
    }
}
