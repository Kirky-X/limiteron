// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 协议无关的防护核心：限流 + 熔断 + 封禁的统一入口。
//!
//! [`Guard`] 包装一个已装配的 `limiteron::Governor`（限流规则、熔断器、
//! 封禁存储均在 Governor 侧装配，本层不重复实现），向应用提供单一
//! `check(key)` 判定面与三态 [`GuardDecision`]。
//!
//! 防护语义：
//! - **限流**：Governor 规则链裁决（按 [`GuardIdentity`] 写入的键匹配）；
//! - **熔断**：Governor 装配 `CircuitBreaker`（limiteron `circuit-breaker`
//!   feature）后自动生效——下游故障时 Governor 切入降级判定，本层透传；
//! - **封禁**：触发封禁规则的键由 Governor 升级为 `Decision::Banned`；
//! - **故障策略**：Governor 内部错误（存储不可达等）按
//!   [`GuardConfig::fail_open`] 显式裁决——默认 fail-close（拒绝，安全
//!   优先），`fail_open = true` 时放行并以 `tracing::warn!` 留痕。

use limiteron::{Decision, Governor, LimiteronError, matchers::RequestContext};
use std::sync::Arc;

/// 请求身份键类型：决定 `check(key)` 的键写入 `RequestContext` 的哪个字段。
///
/// Governor 的规则匹配器按对应身份维度（`Matcher::Ip` / `Matcher::User` /
/// API Key 提取器）命中规则，两侧类型必须一致，否则规则永不命中。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardIdentity {
    /// 客户端 IP（写入 `RequestContext.ip` 与 `client_ip`）
    Ip,
    /// 用户 ID（写入 `RequestContext.user_id`）
    UserId,
    /// API Key（写入 `RequestContext.api_key`）
    ApiKey,
}

/// 防护配置。
///
/// 字段全显式（无隐藏默认行为分歧）：身份类型必须声明；故障策略必须
/// 有意选择（默认 fail-close）。
#[derive(Debug, Clone)]
pub struct GuardConfig {
    identity: GuardIdentity,
    fail_open: bool,
}

impl GuardConfig {
    /// 以指定身份类型构造配置，故障策略默认 fail-close。
    #[must_use]
    pub fn new(identity: GuardIdentity) -> Self {
        Self {
            identity,
            fail_open: false,
        }
    }

    /// 设置故障策略（`true` = 存储等内部错误时放行并告警）。
    #[must_use]
    pub fn with_fail_open(mut self, fail_open: bool) -> Self {
        self.fail_open = fail_open;
        self
    }

    /// 身份键类型。
    #[must_use]
    pub fn identity(&self) -> GuardIdentity {
        self.identity
    }

    /// 故障策略：`true` = fail-open（错误时放行）。
    #[must_use]
    pub fn fail_open(&self) -> bool {
        self.fail_open
    }
}

/// 三态防护裁决（limiteron `Decision` 的收窄投影：保留裁决语义与关键
/// 元数据，剥离限流算法内部细节）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardDecision {
    /// 放行。
    Allowed,
    /// 限流拒绝。
    Throttled {
        /// 拒绝原因。
        reason: String,
        /// 建议重试等待秒数（limiteron reset 语义透传）。
        retry_after_secs: u64,
    },
    /// 封禁。
    Banned {
        /// 封禁原因。
        reason: String,
        /// 封禁到期时刻。
        banned_until: chrono::DateTime<chrono::Utc>,
        /// 历史封禁次数。
        ban_times: u32,
    },
}

impl GuardDecision {
    /// 是否放行。
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        matches!(self, GuardDecision::Allowed)
    }
}

/// 防护入口：限流 / 熔断 / 封禁的统一 `check` 面。
///
/// 熔断能力来自 Governor 装配（`GovernorBuilder::with_circuit_breaker`，
/// 需 limiteron `circuit-breaker` feature），本层透传其裁决，不重复实现。
#[derive(Clone)]
pub struct Guard {
    governor: Arc<Governor>,
    config: GuardConfig,
}

impl std::fmt::Debug for Guard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Governor 无 Debug 面（内部并发原语），仅记录实例地址与配置
        f.debug_struct("Guard")
            .field("governor", &Arc::as_ptr(&self.governor))
            .field("config", &self.config)
            .finish()
    }
}

impl Guard {
    /// 以默认配置（IP 身份、fail-close）包装 Governor。
    #[must_use]
    pub fn new(governor: Arc<Governor>) -> Self {
        Self::with_config(governor, GuardConfig::new(GuardIdentity::Ip))
    }

    /// 以显式配置包装 Governor。
    #[must_use]
    pub fn with_config(governor: Arc<Governor>, config: GuardConfig) -> Self {
        Self { governor, config }
    }

    /// 防护配置。
    #[must_use]
    pub fn config(&self) -> &GuardConfig {
        &self.config
    }

    /// 底层 Governor（供装配侧挂事件/指标等扩展时复用同一实例）。
    #[must_use]
    pub fn governor(&self) -> &Arc<Governor> {
        &self.governor
    }

    /// 判定请求是否放行；内部错误按 fail-open/close 策略收敛为三态。
    ///
    /// 这是应用请求路径的便捷入口：错误不会外泄（fail-open 时以
    /// `tracing::warn!` 留痕），需要显式错误面请用 [`Guard::try_check`]。
    pub async fn check(&self, key: &str) -> GuardDecision {
        match self.try_check(key).await {
            Ok(decision) => decision,
            Err(e) => {
                if self.config.fail_open {
                    tracing::warn!(
                        key = %key,
                        error = %e,
                        "guard fail-open: governor error, allowing request"
                    );
                    GuardDecision::Allowed
                } else {
                    tracing::warn!(
                        key = %key,
                        error = %e,
                        "guard fail-close: governor error, rejecting request"
                    );
                    GuardDecision::Throttled {
                        reason: format!("guard internal error: {e}"),
                        retry_after_secs: 0,
                    }
                }
            }
        }
    }

    /// 显式错误面：Governor 内部错误原样上抛，不应用 fail-open/close。
    ///
    /// 键写入 `RequestContext` 的语义维度（字段 + 默认 `CompositeExtractor`
    /// 读取的 header 通道，二者同步写入以覆盖字段匹配与标识符提取两条
    /// 路径）；规则链的限流桶为规则级共享（`chain.check()` 无键语义），
    /// 键的隔离作用面是规则匹配、封禁与事件归因。
    pub async fn try_check(&self, key: &str) -> Result<GuardDecision, LimiteronError> {
        let mut ctx = RequestContext::new();
        match self.config.identity {
            GuardIdentity::Ip => {
                ctx.ip = Some(key.to_string());
                ctx.client_ip = Some(key.to_string());
            }
            GuardIdentity::UserId => {
                ctx.user_id = Some(key.to_string());
                ctx.headers.insert("x-user-id".to_string(), key.to_string());
            }
            GuardIdentity::ApiKey => {
                ctx.api_key = Some(key.to_string());
                ctx.headers.insert("x-api-key".to_string(), key.to_string());
            }
        }
        let decision = self.governor.check(&ctx).await?;
        Ok(match decision {
            Decision::Allowed(_) => GuardDecision::Allowed,
            Decision::Rejected(m) => GuardDecision::Throttled {
                reason: m.reason,
                retry_after_secs: m.retry_after,
            },
            Decision::Banned(b) => GuardDecision::Banned {
                reason: b.reason().to_string(),
                banned_until: b.banned_until(),
                ban_times: b.ban_times(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use limiteron::config::{
        Action, ActionConfig, CacheBackend, FlowControlConfig, LimiterConfig, Matcher,
        MetricsBackend, Rule, StorageType,
    };
    use limiteron::matchers::{Identifier, IdentifierExtractor};
    use limiteron::storage::{BanRecord, BanStorage, BanTarget, MemoryBanStorage, MemoryStorage};

    /// 恒不识别的提取器：注入 Governor 触发 check 内部错误以测 fail-open/close
    struct FailingExtractor;

    impl IdentifierExtractor for FailingExtractor {
        fn extract(&self, _context: &RequestContext) -> Option<Identifier> {
            None
        }
        fn name(&self) -> &str {
            "FailingExtractor"
        }
    }

    fn config_with(capacity: u64, matcher: Matcher) -> FlowControlConfig {
        FlowControlConfig {
            version: "1.0".to_string(),
            global: limiteron::config::GlobalConfig {
                storage: StorageType::Memory,
                cache: CacheBackend::Memory,
                metrics: MetricsBackend::Prometheus,
                trusted_proxies: Default::default(),
            },
            rules: vec![Rule {
                id: "guard_rule".to_string(),
                name: "Guard Rule".to_string(),
                priority: 100,
                matchers: vec![matcher],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity,
                    refill_rate: 1,
                }],
                action: ActionConfig {
                    on_exceed: Action::Reject,
                    ban: None,
                },
            }],
        }
    }

    async fn governor_with_bans(
        config: FlowControlConfig,
        ban_storage: Arc<dyn limiteron::storage::BanStorage>,
    ) -> Arc<Governor> {
        Arc::new(
            Governor::builder()
                .with_config(config)
                .with_storage(Arc::new(MemoryStorage::new()))
                .with_ban_storage(ban_storage)
                .build()
                .await
                .expect("governor"),
        )
    }

    async fn governor_for(config: FlowControlConfig) -> Arc<Governor> {
        governor_with_bans(config, Arc::new(MemoryBanStorage::new())).await
    }

    async fn governor_failing_extractor() -> Arc<Governor> {
        Arc::new(
            Governor::builder()
                .with_config(config_with(10, user_matcher()))
                .with_storage(Arc::new(MemoryStorage::new()))
                .with_ban_storage(Arc::new(MemoryBanStorage::new()))
                .with_identifier_extractor(Arc::new(FailingExtractor))
                .build()
                .await
                .expect("governor"),
        )
    }

    fn user_matcher() -> Matcher {
        Matcher::User {
            user_ids: vec!["*".to_string()],
        }
    }

    fn ban_record(user: &str) -> BanRecord {
        let now = chrono::Utc::now();
        BanRecord {
            target: BanTarget::UserId(user.to_string()),
            ban_times: 2,
            duration: std::time::Duration::from_secs(3600),
            banned_at: now,
            expires_at: now + chrono::Duration::hours(1),
            is_manual: true,
            reason: "abuse detected".to_string(),
        }
    }

    #[tokio::test]
    async fn allowed_then_throttled_with_metadata() {
        let governor = governor_for(config_with(2, user_matcher())).await;
        let guard = Guard::with_config(governor, GuardConfig::new(GuardIdentity::UserId));

        assert_eq!(guard.check("alice").await, GuardDecision::Allowed);
        assert_eq!(guard.check("alice").await, GuardDecision::Allowed);

        let third = guard.check("alice").await;
        let GuardDecision::Throttled {
            reason,
            retry_after_secs,
        } = third
        else {
            panic!("第三次请求应被限流，got {third:?}");
        };
        assert!(!reason.is_empty(), "限流裁决应携带原因");
        // retry_after 为 limiteron 内部 reset 语义的透传（秒），桥接层不钉值
        let _ = retry_after_secs;

        // 规则链桶为规则级共享（chain.check() 无键语义）：alice 耗尽桶后
        // 同规则的 bob 同样被拒——键的隔离作用面是标识符（封禁/事件），
        // 不是限流桶；per-key 限额须按身份维度拆多条规则实现
        assert!(!guard.check("bob").await.is_allowed());
    }

    #[tokio::test]
    async fn pre_banned_identifier_returns_banned_with_metadata() {
        let bans = Arc::new(MemoryBanStorage::new());
        bans.save(&ban_record("carol")).await.unwrap();

        let governor = governor_with_bans(config_with(10, user_matcher()), bans).await;
        let guard = Guard::with_config(governor, GuardConfig::new(GuardIdentity::UserId));

        let banned = guard.check("carol").await;
        let GuardDecision::Banned {
            reason,
            banned_until,
            ban_times,
        } = banned
        else {
            panic!("已封禁标识符应命中 Banned，got {banned:?}");
        };
        assert_eq!(reason, "abuse detected", "封禁原因应透传");
        assert!(banned_until > chrono::Utc::now(), "封禁应设置未来到期时刻");
        assert_eq!(ban_times, 2, "封禁次数应透传");

        // 未被封禁的键正常走规则链
        assert_eq!(guard.check("dave").await, GuardDecision::Allowed);
    }

    #[tokio::test]
    async fn identity_key_selects_request_context_field() {
        // IP 身份 + IP 匹配规则：键写入 ip/client_ip，被 IpExtractor
        // （回退 client_ip 通道）提取为标识符且规则命中
        let ip_rule = config_with(
            2,
            Matcher::Ip {
                ip_ranges: vec!["0.0.0.0/0".to_string()],
            },
        );
        let governor = governor_for(ip_rule).await;
        let guard = Guard::with_config(governor, GuardConfig::new(GuardIdentity::Ip));

        // 标识符随键变化（封禁/事件按 IP 归因），规则桶共享：
        // 两次请求耗尽桶后第三个 IP 也被拒（同 alice/bob 语义）
        assert_eq!(guard.check("10.1.1.1").await, GuardDecision::Allowed);
        assert_eq!(guard.check("10.1.1.2").await, GuardDecision::Allowed);
        assert!(!guard.check("10.1.1.3").await.is_allowed());

        // UserId 身份：键经 X-User-Id header 命中默认 CompositeExtractor
        // 的 UserIdExtractor 通道
        let governor2 = governor_for(config_with(2, user_matcher())).await;
        let user_guard = Guard::with_config(governor2, GuardConfig::new(GuardIdentity::UserId));
        assert!(user_guard.check("erin").await.is_allowed());

        // ApiKey 身份：键经 X-API-Key header 命中 ApiKeyExtractor 通道
        let governor3 = governor_for(config_with(2, user_matcher())).await;
        let key_guard = Guard::with_config(governor3, GuardConfig::new(GuardIdentity::ApiKey));
        assert!(key_guard.check("sk-123").await.is_allowed());
    }

    #[tokio::test]
    async fn fail_close_rejects_on_governor_error() {
        let guard = Guard::with_config(
            governor_failing_extractor().await,
            GuardConfig::new(GuardIdentity::UserId),
        );

        let decision = guard.check("frank").await;
        let GuardDecision::Throttled { reason, .. } = decision else {
            panic!("fail-close 应把内部错误收敛为拒绝，got {decision:?}");
        };
        assert!(reason.contains("guard internal error"));

        // try_check 显式错误面：原样上抛
        let err = guard.try_check("frank").await.unwrap_err();
        assert!(err.to_string().contains("Failed to extract identifier"));
    }

    #[tokio::test]
    async fn fail_open_allows_on_governor_error() {
        let guard = Guard::with_config(
            governor_failing_extractor().await,
            GuardConfig::new(GuardIdentity::UserId).with_fail_open(true),
        );

        assert_eq!(guard.check("grace").await, GuardDecision::Allowed);
        // fail-open 仅收敛错误面；显式错误面不受影响
        assert!(guard.try_check("grace").await.is_err());
    }

    #[test]
    fn config_accessors_expose_explicit_fields() {
        let config = GuardConfig::new(GuardIdentity::ApiKey).with_fail_open(true);
        assert_eq!(config.identity(), GuardIdentity::ApiKey);
        assert!(config.fail_open());

        let default_config = GuardConfig::new(GuardIdentity::Ip);
        assert!(!default_config.fail_open(), "默认必须 fail-close");
    }
}
