// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! sdforge 适配层：把 [`Guard`] 暴露为
//! `sdforge::domain::ForgeRateLimiter`，供 sdforge 应用从 limiteron 侧
//! 一行接入防护层。
//!
//! 与 sdforge 侧既有 `src/integrations/limiteron_adapter.rs`
//! （`LimiteronForgeAdapter`）的分工：那是 sdforge 侧的消费适配（面向
//! sdforge 内部 trait-kit 装配），本模块是 limiteron 侧提供的独立防护层
//! （面向 limiteron 用户将 sdforge 应用接入限流/熔断/封禁）。两侧实现
//! 各自独立、不互相依赖，仅共享同一 trait 契约与 `record` no-op 口径
//! （`Governor::check` 原子完成 check + consume，无独立 record 步骤）。

use crate::guard::Guard;
use sdforge::domain::{ForgeError, ForgeRateLimiter};
use std::future::Future;
use std::pin::Pin;

/// `ForgeRateLimiter` 实现：判定委托给 [`Guard`]。
///
/// `check` → `Guard::check`（fail-open/close 由 `GuardConfig` 显式裁决，
/// 本适配层不再叠加策略）；`record` → 文档化 no-op（同上：check 原子
/// 提交观察，无独立 record 语义）。
#[derive(Clone)]
pub struct GuardForgeAdapter {
    guard: Guard,
}

impl GuardForgeAdapter {
    /// 包装 guard（复用其身份与故障策略配置）。
    #[must_use]
    pub fn new(guard: Guard) -> Self {
        Self { guard }
    }

    /// 内部 guard（供装配侧复用同一防护配置面）。
    #[must_use]
    pub fn guard(&self) -> &Guard {
        &self.guard
    }
}

impl ForgeRateLimiter for GuardForgeAdapter {
    fn check<'a>(
        &'a self,
        key: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<bool, ForgeError>> + Send + 'a>> {
        Box::pin(async move {
            // Guard::check 已按 fail-open/close 收敛内部错误，不外泄 Err；
            // 决策到 bool 的映射：Allowed → true，Throttled/Banned → false。
            Ok(self.guard.check(key).await.is_allowed())
        })
    }

    fn record<'a>(
        &'a self,
        _key: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), ForgeError>> + Send + 'a>> {
        // 文档化 no-op（非观测丢失）：Governor::check 原子完成 check +
        // consume，观察已在 check 提交；ForgeRateLimiter 的 record 分离
        // 服务于 peek/commit 型后端，本防护层不属此类。
        Box::pin(async move { Ok(()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::{GuardConfig, GuardIdentity};
    use limiteron::config::{
        Action, ActionConfig, CacheBackend, FlowControlConfig, LimiterConfig, Matcher,
        MetricsBackend, Rule, StorageType,
    };
    use limiteron::storage::{MemoryBanStorage, MemoryStorage};
    use limiteron::{Governor, matchers::RequestContext};
    use std::sync::Arc;

    async fn guard_with_capacity(capacity: u64) -> Guard {
        let config = FlowControlConfig {
            version: "1.0".to_string(),
            global: limiteron::config::GlobalConfig {
                storage: StorageType::Memory,
                cache: CacheBackend::Memory,
                metrics: MetricsBackend::Prometheus,
                trusted_proxies: Default::default(),
            },
            rules: vec![Rule {
                id: "sdforge_guard_rule".to_string(),
                name: "Sdforge Guard Rule".to_string(),
                priority: 100,
                matchers: vec![Matcher::User {
                    user_ids: vec!["*".to_string()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity,
                    refill_rate: 1,
                }],
                action: ActionConfig {
                    on_exceed: Action::Reject,
                    ban: None,
                },
            }],
        };
        let governor = Arc::new(
            Governor::builder()
                .with_config(config)
                .with_storage(Arc::new(MemoryStorage::new()))
                .with_ban_storage(Arc::new(MemoryBanStorage::new()))
                .build()
                .await
                .expect("governor"),
        );
        Guard::with_config(governor, GuardConfig::new(GuardIdentity::UserId))
    }

    #[tokio::test]
    async fn check_maps_decision_to_bool() {
        let adapter = GuardForgeAdapter::new(guard_with_capacity(2).await);

        assert!(adapter.check("user-a").await.unwrap());
        assert!(adapter.check("user-a").await.unwrap());
        assert!(!adapter.check("user-a").await.unwrap(), "容量耗尽后应拒绝");
        // 规则链桶为规则级共享：user-a 耗尽后 user-b 同样被拒
        assert!(
            !adapter.check("user-b").await.unwrap(),
            "共享桶耗尽后其它键同被拒"
        );
    }

    #[tokio::test]
    async fn record_is_documented_noop() {
        let adapter = GuardForgeAdapter::new(guard_with_capacity(1).await);
        adapter.record("user-c").await.unwrap();
        // record 后不消耗容量：check 仍放行
        assert!(adapter.check("user-c").await.unwrap());
    }

    #[tokio::test]
    async fn guard_exposes_same_instance_for_reuse() {
        let guard = guard_with_capacity(5).await;
        let adapter = GuardForgeAdapter::new(guard.clone());
        assert!(Arc::ptr_eq(adapter.guard().governor(), guard.governor()));
    }

    /// object-safety 烟测：ForgeRateLimiter 契约要求可作 dyn 分发
    #[tokio::test]
    async fn works_as_dyn_trait_object() {
        let limiter: Arc<dyn ForgeRateLimiter> =
            Arc::new(GuardForgeAdapter::new(guard_with_capacity(1).await));
        assert!(limiter.check("dyn-user").await.unwrap());
        assert!(!limiter.check("dyn-user").await.unwrap());
        limiter.record("dyn-user").await.unwrap();
    }

    /// 空上下文（无可提取身份）显性失败：Governor 判 Err，适配层如实
    /// 上抛 ForgeError 而非静默放行
    #[tokio::test]
    async fn empty_context_fails_loudly() {
        let guard = guard_with_capacity(1).await;
        let ctx = RequestContext::new();
        let result = guard.governor().check(&ctx).await;
        assert!(result.is_err(), "空上下文应显性失败而非误判");
    }
}
