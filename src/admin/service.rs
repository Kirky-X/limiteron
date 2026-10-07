// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 协议无关的管理服务面
//!
//! [`AdminService`] 以纯 Rust 类型定义管理操作的完整契约（与 HTTP 无关），
//! 供三面共享：
//! - `handlers`：axum 薄壳（解析请求 → service 调用 → 状态码映射）；
//! - `openapi`：OpenAPI 文档的操作清单与 schema 面（单一事实来源）；
//! - `client`：薄客户端的类型化方法（同一契约的消费端）。
//!
//! [`AdminServiceError`] 与 HTTP 状态码的映射由 HTTP 适配层决定（见
//! `handlers`），service 层只表达错误类别。

use crate::Governor;
use crate::config::FlowControlConfig;
use async_trait::async_trait;
use std::sync::Arc;

#[cfg(feature = "circuit-breaker")]
use super::handlers::CircuitBreakerStatus;
#[cfg(feature = "ban-manager")]
use super::handlers::{BanResponse, BanTargetQuery, CreateBanRequest, UnbanRequest};
use super::handlers::{BatchCheckBody, SystemStatus, TokenPrefetchBody};
#[cfg(feature = "quota-control")]
use super::handlers::{UpdateQuotaRequest, UpdateQuotaResponse};
#[cfg(feature = "ban-manager")]
use crate::error::LimiteronError;

/// 管理操作错误（类别即协议适配层的语义映射依据）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminServiceError {
    /// 依赖组件未配置（→ 503）
    NotConfigured(&'static str),
    /// 请求不合法（→ 400）
    Invalid(String),
    /// 目标不存在（→ 404）
    NotFound(String),
    /// 无权限（→ 403；消息由构造方给定——create_ban 保持旧版响应体
    /// 契约 "Authorization error: {msg}"）
    Forbidden(String),
    /// 内部错误（→ 500）
    Internal(String),
}

impl std::fmt::Display for AdminServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured(what) => write!(f, "{what} not configured"),
            Self::Invalid(m) => write!(f, "{m}"),
            Self::NotFound(m) => write!(f, "{m}"),
            Self::Forbidden(m) => write!(f, "{m}"),
            Self::Internal(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for AdminServiceError {}

/// 协议无关管理服务契约
///
/// 方法与 `POST/PUT/DELETE /api/v1/*` 端点一一对应；只读端点
/// （status/introspect/circuit-breaker）同样入面以保持契约完整。
/// 健康探针（healthz/readyz/metrics）为协议特定运维面（Prometheus 文本、
/// 503 语义），不入本 trait，仅在 OpenAPI 文档中描述。
#[async_trait]
pub trait AdminService: Send + Sync {
    /// GET /api/v1/status —— 系统整体状态
    async fn status(&self) -> SystemStatus;

    /// GET /api/v1/introspect —— 运行时自省快照
    async fn introspect(&self) -> serde_json::Value;

    /// POST /api/v1/config —— 规则热更新（校验失败返回 [`AdminServiceError::Invalid`]，
    /// 旧配置原样保留）
    async fn apply_config(
        &self,
        config: FlowControlConfig,
    ) -> Result<serde_json::Value, AdminServiceError>;

    /// POST /api/v1/check/batch —— 批量检查（空批/超上限返回
    /// [`AdminServiceError::Invalid`]）
    async fn check_batch(
        &self,
        body: BatchCheckBody,
    ) -> Result<(usize, Vec<serde_json::Value>), AdminServiceError>;

    /// POST /api/v1/tokens/prefetch —— 批量令牌预取，返回
    /// `(granted 数, 逐项结果)`
    ///
    /// 预取器为进程级单例、不依赖组件引用，逻辑在自由函数
    /// [`prefetch_tokens_detached`]，trait 方法仅为契约完整性代理。
    async fn prefetch_tokens(
        &self,
        body: TokenPrefetchBody,
    ) -> Result<(usize, Vec<serde_json::Value>), AdminServiceError>;

    /// POST /api/v1/ban —— 创建封禁
    #[cfg(feature = "ban-manager")]
    async fn create_ban(
        &self,
        operator: String,
        req: CreateBanRequest,
    ) -> Result<BanResponse, AdminServiceError>;

    /// DELETE /api/v1/ban/{target} —— 解除封禁；不存在时返回
    /// [`AdminServiceError::NotFound`]，成功返回响应消息
    #[cfg(feature = "ban-manager")]
    async fn delete_ban(
        &self,
        operator: String,
        target: String,
        query: BanTargetQuery,
        req: UnbanRequest,
    ) -> Result<String, AdminServiceError>;

    /// PUT /api/v1/quota/{user_id} —— 重置该 user 的配额使用量（new_limit=0）
    #[cfg(feature = "quota-control")]
    async fn update_quota(
        &self,
        user_id: &str,
        req: UpdateQuotaRequest,
    ) -> Result<UpdateQuotaResponse, AdminServiceError>;

    /// GET /api/v1/status/circuit-breaker —— 熔断器状态
    #[cfg(feature = "circuit-breaker")]
    async fn circuit_breaker_status(&self) -> Result<CircuitBreakerStatus, AdminServiceError>;
}

/// 无状态批量令牌预取（预取器为进程级单例、不依赖组件引用；
/// trait 方法与 HTTP 壳共用此实现）
pub async fn prefetch_tokens_detached(
    body: TokenPrefetchBody,
) -> Result<(usize, Vec<serde_json::Value>), AdminServiceError> {
    use super::handlers::{BATCH_MAX_ITEMS, token_prefetcher};

    if body.items.is_empty() {
        return Err(AdminServiceError::Invalid(
            "token prefetch requires at least one item".to_string(),
        ));
    }
    if body.items.len() > BATCH_MAX_ITEMS {
        return Err(AdminServiceError::Invalid(format!(
            "token prefetch limited to {BATCH_MAX_ITEMS} items per call"
        )));
    }

    let pairs: Vec<(String, u64)> = body.items.into_iter().map(|i| (i.key, i.tokens)).collect();
    let results = token_prefetcher().prefetch_batch(&pairs).await;
    let granted_count = results.iter().filter(|r| r.granted).count();
    let values = serde_json::to_value(&results)
        .unwrap_or_else(|_| serde_json::json!([]))
        .as_array()
        .cloned()
        .unwrap_or_default();
    Ok((granted_count, values))
}

/// [`AdminService`] 的 Governor 实现：包装 `LimiteronState` 的组件引用
///
/// 逻辑自 `handlers` 原地迁入（行为回归基线），handlers 仅保留
/// HTTP 解析与状态码映射。
pub struct GovernorAdminService {
    governor: Arc<Governor>,
    #[cfg(feature = "ban-manager")]
    ban_manager: Option<Arc<crate::BanManager>>,
    #[cfg(feature = "quota-control")]
    quota_controller: Option<Arc<crate::QuotaController>>,
    #[cfg(feature = "circuit-breaker")]
    circuit_breaker: Option<Arc<crate::CircuitBreaker>>,
}

impl GovernorAdminService {
    /// 从应用状态构造（组件引用为廉价的 `Arc` 克隆）
    pub fn from_state(state: &super::server::LimiteronState) -> Self {
        Self {
            governor: state.governor.clone(),
            #[cfg(feature = "ban-manager")]
            ban_manager: state.ban_manager.clone(),
            #[cfg(feature = "quota-control")]
            quota_controller: state.quota_controller.clone(),
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: state.circuit_breaker.clone(),
        }
    }
}

#[async_trait]
impl AdminService for GovernorAdminService {
    async fn status(&self) -> SystemStatus {
        let stats = self.governor.stats().await;

        // 饱和运算：计数器持续累加可能接近 u64::MAX，且回退场景下
        // blocked 可能超过 total，裸加减会 panic（debug）或回绕（release）
        let blocked = stats
            .rejected_requests
            .saturating_add(stats.banned_requests);
        let total = stats.total_requests;
        let success_rate = if total > 0 {
            (total.saturating_sub(blocked)) as f64 / total as f64
        } else {
            1.0
        };

        #[cfg(feature = "ban-manager")]
        let active_bans: usize = if let Some(ref bm) = self.ban_manager {
            bm.list_bans(crate::ban::BanFilter {
                active_only: true,
                ..Default::default()
            })
            .await
            .map(|v| v.len())
            .unwrap_or(0)
        } else {
            0
        };

        #[cfg(feature = "circuit-breaker")]
        let cb_state = if let Some(ref cb) = self.circuit_breaker {
            cb.get_state().await.to_string()
        } else {
            "disabled".to_string()
        };

        SystemStatus {
            total_requests: total,
            blocked_requests: blocked,
            success_rate,
            #[cfg(feature = "ban-manager")]
            active_bans,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: cb_state,
        }
    }

    async fn introspect(&self) -> serde_json::Value {
        let snapshot = self.governor.introspect().await;
        let body = serde_json::to_value(&snapshot).unwrap_or_else(|_| serde_json::json!({}));

        // feature 叠加经 shadowing 逐步增强：每个 feature 只在自己编译时
        // 取得可变访问，避免无 feature 组合下的 unused_mut
        #[cfg(feature = "ban-manager")]
        let body = {
            let mut body = body;
            if let Some(ref bm) = self.ban_manager
                && let Ok(bans) = bm
                    .list_bans(crate::ban::BanFilter {
                        active_only: true,
                        ..Default::default()
                    })
                    .await
            {
                let items: Vec<serde_json::Value> = bans
                    .iter()
                    .map(|b| {
                        serde_json::json!({
                            "target": b.target,
                            "ban_times": b.ban_times,
                            "is_manual": b.is_manual,
                            "reason": b.reason,
                            "expires_at": b.expires_at.to_rfc3339(),
                        })
                    })
                    .collect();
                body["active_bans"] = serde_json::Value::Array(items);
            }
            body
        };

        #[cfg(feature = "circuit-breaker")]
        let body = {
            let mut body = body;
            if let Some(ref cb) = self.circuit_breaker {
                body["circuit_breaker_state"] = serde_json::json!(cb.get_state().await.to_string());
            }
            body
        };

        body
    }

    async fn apply_config(
        &self,
        config: FlowControlConfig,
    ) -> Result<serde_json::Value, AdminServiceError> {
        match self.governor.apply_config(config).await {
            Ok(report) => {
                Ok(serde_json::to_value(&report).unwrap_or_else(|_| serde_json::json!({})))
            }
            Err(e) => Err(AdminServiceError::Invalid(format!("config rejected: {e}"))),
        }
    }

    async fn check_batch(
        &self,
        body: BatchCheckBody,
    ) -> Result<(usize, Vec<serde_json::Value>), AdminServiceError> {
        use super::handlers::BATCH_MAX_ITEMS;

        if body.requests.is_empty() {
            return Err(AdminServiceError::Invalid(
                "batch check requires at least one request".to_string(),
            ));
        }
        if body.requests.len() > BATCH_MAX_ITEMS {
            return Err(AdminServiceError::Invalid(format!(
                "batch check limited to {BATCH_MAX_ITEMS} requests per call"
            )));
        }

        let mut results = Vec::with_capacity(body.requests.len());
        for (index, item) in body.requests.into_iter().enumerate() {
            let mut ctx = crate::matchers::RequestContext::new();
            // 默认 CompositeExtractor 从 X-User-Id 头 / 客户端 IP / X-API-Key
            // 提取标识符；批量端点将 body 字段映射到对应提取源。
            if let Some(user_id) = item.user_id {
                ctx.headers.insert("x-user-id".to_string(), user_id);
            }
            ctx.ip = item.ip.clone();
            ctx.client_ip = item.ip;
            ctx.path = item.path.unwrap_or_default();
            ctx.method = item.method.unwrap_or_else(|| "GET".to_string());

            match self.governor.check(&ctx).await {
                Ok(decision) => {
                    let (kind, allowed) = match &decision {
                        crate::error::Decision::Allowed(_) => ("Allowed", true),
                        crate::error::Decision::Rejected(_) => ("Rejected", false),
                        crate::error::Decision::Banned(_) => ("Banned", false),
                    };
                    results.push(serde_json::json!({
                        "index": index,
                        "allowed": allowed,
                        "decision": kind,
                    }));
                }
                Err(e) => {
                    results.push(serde_json::json!({
                        "index": index,
                        "allowed": false,
                        "decision": "Error",
                        "error": e.to_string(),
                    }));
                }
            }
        }

        let allowed_count = results.iter().filter(|r| r["allowed"] == true).count();
        Ok((allowed_count, results))
    }

    async fn prefetch_tokens(
        &self,
        body: TokenPrefetchBody,
    ) -> Result<(usize, Vec<serde_json::Value>), AdminServiceError> {
        prefetch_tokens_detached(body).await
    }

    #[cfg(feature = "ban-manager")]
    async fn create_ban(
        &self,
        operator: String,
        req: CreateBanRequest,
    ) -> Result<BanResponse, AdminServiceError> {
        use crate::ban::BanSource;
        use std::time::Duration;

        let Some(ref ban_manager) = self.ban_manager else {
            return Err(AdminServiceError::NotConfigured("Ban manager"));
        };

        let source = BanSource::Manual { operator };
        let duration = req.duration_secs.map(Duration::from_secs);

        match ban_manager
            .create_ban(
                req.target,
                req.reason,
                source,
                serde_json::json!({"source": "http-api"}),
                duration,
            )
            .await
        {
            Ok(detail) => Ok(BanResponse {
                id: detail.id,
                ban_times: detail.ban_times,
                expires_at: detail.expires_at.timestamp(),
                is_manual: detail.is_manual,
            }),
            Err(e) => Err(match &e {
                LimiteronError::ValidationError(_) => AdminServiceError::Invalid(e.to_string()),
                // 消息透传 LimiteronError 的 Display（旧 handlers 实现的
                // 响应体契约 "Authorization error: {msg}"，逐位一致）
                LimiteronError::AuthorizationError(_) => {
                    AdminServiceError::Forbidden(e.to_string())
                }
                _ => AdminServiceError::Internal(e.to_string()),
            }),
        }
    }

    #[cfg(feature = "ban-manager")]
    async fn delete_ban(
        &self,
        operator: String,
        target: String,
        query: BanTargetQuery,
        req: UnbanRequest,
    ) -> Result<String, AdminServiceError> {
        use crate::storage::BanTarget;

        let Some(ref ban_manager) = self.ban_manager else {
            return Err(AdminServiceError::NotConfigured("Ban manager"));
        };
        let ban_target = match query.target_type.as_deref() {
            Some("ip") => BanTarget::Ip(target),
            Some("user") => BanTarget::UserId(target),
            Some("mac") => BanTarget::Mac(target),
            Some("geo") => BanTarget::Geo {
                country_code: target,
            },
            Some("cidr") => BanTarget::Cidr(target),
            Some(other) => {
                return Err(AdminServiceError::Invalid(format!(
                    "unsupported target type: {other}"
                )));
            }
            None => {
                // 自动推断：IP 优先，回退 UserId
                if target.parse::<std::net::IpAddr>().is_ok() {
                    BanTarget::Ip(target)
                } else {
                    BanTarget::UserId(target)
                }
            }
        };
        match ban_manager.delete_ban(&ban_target, operator).await {
            Ok(true) => Ok(req.reason.unwrap_or_else(|| "Ban removed".to_string())),
            Ok(false) => Err(AdminServiceError::NotFound("Ban not found".to_string())),
            Err(e) => Err(AdminServiceError::Internal(format!(
                "Failed to remove ban: {e}"
            ))),
        }
    }

    #[cfg(feature = "quota-control")]
    async fn update_quota(
        &self,
        user_id: &str,
        req: UpdateQuotaRequest,
    ) -> Result<UpdateQuotaResponse, AdminServiceError> {
        let Some(ref quota_controller) = self.quota_controller else {
            return Err(AdminServiceError::NotConfigured("Quota controller"));
        };
        // QuotaController 当前不支持 per-tenant 配额上限更新（配额上限为全局
        // QuotaConfig）；本端点为 user 维度：路径段即 reset_quota 的 user_id，
        // 租户维度配额请用 QuotaController::reset_quota_for_tenant。
        if req.new_limit == 0 {
            // new_limit=0 视为重置信号
            match quota_controller.reset_quota(user_id, &req.resource).await {
                Ok(_) => Ok(UpdateQuotaResponse {
                    success: true,
                    expires_at: req.duration_secs.map(|d| {
                        use std::time::{SystemTime, UNIX_EPOCH};
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|t| t.as_secs() + d)
                            .unwrap_or(0)
                    }),
                }),
                Err(e) => Err(AdminServiceError::Internal(format!(
                    "Failed to reset quota: {e}"
                ))),
            }
        } else {
            Err(AdminServiceError::Invalid(
                "Per-tenant quota limit update is not supported; use global QuotaConfig update_config instead"
                    .to_string(),
            ))
        }
    }

    #[cfg(feature = "circuit-breaker")]
    async fn circuit_breaker_status(&self) -> Result<CircuitBreakerStatus, AdminServiceError> {
        if let Some(ref cb) = self.circuit_breaker {
            let stats = cb.get_stats().await;
            let total = stats.total_calls as f64;
            // CircuitBreakerStats 不跟踪 slow_call_rate，置为 0.0
            let failure_rate = if total > 0.0 {
                stats.failure_count as f64 / total
            } else {
                0.0
            };
            Ok(CircuitBreakerStatus {
                state: stats.state.to_string(),
                failure_rate,
                slow_call_rate: 0.0,
            })
        } else {
            Err(AdminServiceError::NotConfigured("Circuit breaker"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::{LimiteronState, make_state};

    fn service_from(state: &LimiteronState) -> GovernorAdminService {
        GovernorAdminService::from_state(state)
    }

    #[tokio::test]
    async fn status_reports_fresh_governor_counters() {
        let state = make_state().await;
        let status = service_from(&state).status().await;
        assert_eq!(status.total_requests, 0);
        assert_eq!(status.blocked_requests, 0);
        assert_eq!(status.success_rate, 1.0);
    }

    #[tokio::test]
    async fn apply_config_rejects_invalid_and_keeps_old() {
        use crate::config::{Action, ActionConfig, Rule};

        let state = make_state().await;
        let service = service_from(&state);

        // 重复 rule id 触发校验失败
        let config = FlowControlConfig {
            version: "1.0".to_string(),
            global: Default::default(),
            rules: vec![
                Rule {
                    id: "dup".to_string(),
                    name: "a".to_string(),
                    priority: 1,
                    matchers: vec![],
                    limiters: vec![],
                    action: ActionConfig {
                        on_exceed: Action::Reject,
                        ban: None,
                    },
                },
                Rule {
                    id: "dup".to_string(),
                    name: "b".to_string(),
                    priority: 2,
                    matchers: vec![],
                    limiters: vec![],
                    action: ActionConfig {
                        on_exceed: Action::Reject,
                        ban: None,
                    },
                },
            ],
        };
        let result = service.apply_config(config).await;
        assert!(
            matches!(result, Err(AdminServiceError::Invalid(_))),
            "重复 rule id 应被拒绝: {result:?}"
        );
    }

    #[tokio::test]
    async fn check_batch_rejects_empty() {
        let state = make_state().await;
        let err = service_from(&state)
            .check_batch(BatchCheckBody { requests: vec![] })
            .await
            .unwrap_err();
        assert!(matches!(err, AdminServiceError::Invalid(_)));
    }

    #[tokio::test]
    async fn prefetch_tokens_rejects_empty() {
        let state = make_state().await;
        let err = service_from(&state)
            .prefetch_tokens(super::super::handlers::TokenPrefetchBody { items: vec![] })
            .await
            .unwrap_err();
        assert!(matches!(err, AdminServiceError::Invalid(_)));
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn create_ban_authorization_error_keeps_legacy_message() {
        use super::super::handlers::CreateBanRequest;
        use crate::authorization::SimpleAuthorizationProvider;
        use crate::ban::BanManager;
        use crate::storage::BanTarget;

        // 仅授权 admin 的授权提供者：operator "intruder" 触发 AuthorizationError
        let governor = Arc::new(crate::admin::make_governor().await);
        let ban_manager = Arc::new(
            BanManager::builder()
                .with_authorization_provider(Arc::new(SimpleAuthorizationProvider::new(vec![
                    "admin".to_string(),
                ])))
                .build()
                .await
                .expect("ban manager with authorization provider"),
        );
        let service = GovernorAdminService {
            governor,
            ban_manager: Some(ban_manager),
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
        };

        let result = service
            .create_ban(
                "intruder".to_string(),
                CreateBanRequest {
                    target: BanTarget::Ip("10.0.0.9".to_string()),
                    reason: "test".to_string(),
                    operator: None,
                    duration_secs: None,
                },
            )
            .await;
        let Err(err) = result else {
            panic!("未授权 operator 的 create_ban 应失败");
        };

        // 响应体契约逐位一致：消息保持旧版 "Authorization error: {msg}"
        // （旧 handlers 实现直接透传 LimiteronError 的 Display）
        let message = err.to_string();
        assert!(
            message.starts_with("Authorization error: "),
            "403 响应体消息应保持旧版契约，got: {message}"
        );
        assert!(
            message.contains("intruder"),
            "消息应携带 operator 明细: {message}"
        );
        assert!(
            matches!(err, AdminServiceError::Forbidden { .. }),
            "AuthorizationError 应映射为 Forbidden(403): {err:?}"
        );
    }

    #[test]
    fn error_display_is_stable() {
        assert_eq!(
            AdminServiceError::NotConfigured("Ban manager").to_string(),
            "Ban manager not configured"
        );
        assert_eq!(
            AdminServiceError::NotFound("Ban not found".to_string()).to_string(),
            "Ban not found"
        );
    }
}
