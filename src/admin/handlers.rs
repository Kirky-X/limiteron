//! HTTP处理器

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use super::server::AppState;
#[cfg(feature = "ban-manager")]
use crate::ban::{BanFilter, BanTarget};

// ==================== 响应类型 ====================

/// 通用响应
#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            message: "OK".to_string(),
            data: Some(data),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
        }
    }
}

// ==================== 系统状态 ====================

/// 系统整体状态
#[derive(Serialize)]
pub struct SystemStatus {
    pub total_requests: u64,
    pub blocked_requests: u64,
    pub success_rate: f64,
    #[cfg(feature = "ban-manager")]
    pub active_bans: usize,
    #[cfg(feature = "circuit-breaker")]
    pub circuit_breaker: String,
}

/// GET /api/v1/status
pub async fn get_status(State(state): State<AppState>) -> Json<ApiResponse<SystemStatus>> {
    let stats = state.governor.stats().await;

    let blocked = stats.rejected_requests + stats.banned_requests;
    let total = stats.total_requests;
    let success_rate = if total > 0 {
        (total - blocked) as f64 / total as f64
    } else {
        1.0
    };

    #[cfg(feature = "ban-manager")]
    let active_bans: usize = if let Some(ref bm) = state.ban_manager {
        bm.list_bans(BanFilter {
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
    let cb_state = if let Some(ref cb) = state.circuit_breaker {
        cb.get_state().await.to_string()
    } else {
        "disabled".to_string()
    };

    Json(ApiResponse::ok(SystemStatus {
        total_requests: total,
        blocked_requests: blocked,
        success_rate,
        #[cfg(feature = "ban-manager")]
        active_bans,
        #[cfg(feature = "circuit-breaker")]
        circuit_breaker: cb_state,
    }))
}

// ==================== 限流器状态 ====================

/// 限流器状态响应
#[derive(Serialize)]
pub struct LimiterStatus {
    pub key: String,
    pub limit: u64,
    pub remaining: u64,
    pub reset_at: u64,
}

/// GET /api/v1/status/limiter/{key}
///
/// 注意：此端点尚未实现真实存储查询，返回 501 Not Implemented。
/// 禁止用 200 OK 掩盖未完成功能（Rule 12: 失败必须显性化）。
pub async fn get_limiter_status(
    State(_state): State<AppState>,
    Path(key): Path<String>,
) -> (StatusCode, Json<ApiResponse<LimiterStatus>>) {
    let _ = key;
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiResponse::error("limiter status query not implemented")),
    )
}

// ==================== 封禁管理 ====================

/// 解除封禁请求
#[derive(Deserialize)]
pub struct UnbanRequest {
    pub reason: Option<String>,
    /// 操作者标识（用于授权检查与审计）
    pub operator: Option<String>,
}

/// DELETE /api/v1/ban/{target}
///
/// 路径 `target` 优先尝试解析为 IP，否则视为 UserId。
/// 状态码：200=成功, 404=未找到, 503=未配置, 500=内部错误
#[cfg(feature = "ban-manager")]
pub async fn delete_ban(
    State(state): State<AppState>,
    Path(target): Path<String>,
    Json(req): Json<UnbanRequest>,
) -> (StatusCode, Json<ApiResponse<()>>) {
    let Some(ref ban_manager) = state.ban_manager else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::error("Ban manager not configured")),
        );
    };
    // 路径 target 优先按 IP 解析，回退为 UserId
    let ban_target = if target.parse::<std::net::IpAddr>().is_ok() {
        BanTarget::Ip(target)
    } else {
        BanTarget::UserId(target)
    };
    let operator = req.operator.unwrap_or_else(|| "admin-api".to_string());
    match ban_manager.delete_ban(&ban_target, operator).await {
        Ok(true) => (
            StatusCode::OK,
            Json(ApiResponse {
                success: true,
                message: req.reason.unwrap_or_else(|| "Ban removed".to_string()),
                data: Some(()),
            }),
        ),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error("Ban not found")),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(format!("Failed to remove ban: {}", e))),
        ),
    }
}

/// DELETE /api/v1/ban/{target} (无ban-manager特性)
#[cfg(not(feature = "ban-manager"))]
pub async fn delete_ban(Path(_target): Path<String>) -> (StatusCode, Json<ApiResponse<()>>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiResponse::error("Ban manager not configured")),
    )
}

/// 创建封禁请求
#[derive(Deserialize)]
#[cfg(feature = "ban-manager")]
pub struct CreateBanRequest {
    /// 封禁目标（与 BanTarget serde 格式一致：{"type":"ip","value":"1.2.3.4"}）
    pub target: crate::storage::BanTarget,
    /// 封禁原因
    pub reason: String,
    /// 操作者标识（用于授权检查与审计），默认 "admin-api"
    #[serde(default)]
    pub operator: Option<String>,
    /// 封禁时长（秒），None = 使用退避算法自动计算
    #[serde(default)]
    pub duration_secs: Option<u64>,
}

/// 创建封禁响应
#[derive(Serialize)]
#[cfg(feature = "ban-manager")]
pub struct BanResponse {
    pub id: String,
    pub ban_times: u32,
    pub expires_at: i64,
    pub is_manual: bool,
}

/// POST /api/v1/ban
///
/// 创建封禁。支持 ip/user/mac/geo 四种 target 类型。
/// 错误映射：ValidationError→400, AuthorizationError→403, 其他→500。
#[cfg(feature = "ban-manager")]
pub async fn create_ban(
    State(state): State<AppState>,
    Json(req): Json<CreateBanRequest>,
) -> (StatusCode, Json<ApiResponse<BanResponse>>) {
    use crate::ban::BanSource;
    use std::time::Duration;

    let Some(ref ban_manager) = state.ban_manager else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::error("Ban manager not configured")),
        );
    };

    let operator = req.operator.unwrap_or_else(|| "admin-api".to_string());
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
        Ok(detail) => (
            StatusCode::CREATED,
            Json(ApiResponse::ok(BanResponse {
                id: detail.id,
                ban_times: detail.ban_times,
                expires_at: detail.expires_at.timestamp(),
                is_manual: detail.is_manual,
            })),
        ),
        Err(e) => {
            let status = match &e {
                crate::error::FlowGuardError::ValidationError(_) => StatusCode::BAD_REQUEST,
                crate::error::FlowGuardError::AuthorizationError(_) => StatusCode::FORBIDDEN,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(ApiResponse::error(format!("{}", e))))
        }
    }
}

/// POST /api/v1/ban (无ban-manager特性)
#[cfg(not(feature = "ban-manager"))]
pub async fn create_ban() -> (StatusCode, Json<ApiResponse<()>>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiResponse::error("Ban manager not configured")),
    )
}

// ==================== 配额管理 ====================

/// 更新配额请求
#[derive(Deserialize)]
pub struct UpdateQuotaRequest {
    pub resource: String,
    pub new_limit: u64,
    #[serde(default)]
    pub duration_secs: Option<u64>,
}

/// 更新配额响应
#[derive(Serialize)]
pub struct UpdateQuotaResponse {
    pub success: bool,
    pub expires_at: Option<u64>,
}

/// PUT /api/v1/quota/{tenant_id}
///
/// 状态码：200=成功, 400=不支持的操作, 503=未配置, 500=内部错误
#[cfg(feature = "quota-control")]
pub async fn update_quota(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Json(req): Json<UpdateQuotaRequest>,
) -> (StatusCode, Json<ApiResponse<UpdateQuotaResponse>>) {
    let Some(ref quota_controller) = state.quota_controller else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::error("Quota controller not configured")),
        );
    };
    // QuotaController 当前不支持 per-tenant 配额上限更新（配额上限为全局 QuotaConfig）
    // 提供重置配额使用量作为最接近的操作
    if req.new_limit == 0 {
        // new_limit=0 视为重置信号
        match quota_controller
            .reset_quota(&tenant_id, &req.resource)
            .await
        {
            Ok(_) => (
                StatusCode::OK,
                Json(ApiResponse::ok(UpdateQuotaResponse {
                    success: true,
                    expires_at: req.duration_secs.map(|d| {
                        use std::time::{SystemTime, UNIX_EPOCH};
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|t| t.as_secs() + d)
                            .unwrap_or(0)
                    }),
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to reset quota: {}", e))),
            ),
        }
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(
                "Per-tenant quota limit update is not supported; use global QuotaConfig update_config instead",
            )),
        )
    }
}

/// PUT /api/v1/quota/{tenant_id} (无quota-control特性)
#[cfg(not(feature = "quota-control"))]
pub async fn update_quota(
    Path(_tenant_id): Path<String>,
) -> (StatusCode, Json<ApiResponse<UpdateQuotaResponse>>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiResponse::error("Quota controller not configured")),
    )
}

// ==================== 熔断器状态 ====================

/// 熔断器状态响应
#[derive(Serialize)]
pub struct CircuitBreakerStatus {
    pub state: String,
    pub failure_rate: f64,
    pub slow_call_rate: f64,
}

/// GET /api/v1/status/circuit-breaker
///
/// 状态码：200=成功, 503=未配置
#[cfg(feature = "circuit-breaker")]
pub async fn get_circuit_breaker_status(
    State(state): State<AppState>,
) -> (StatusCode, Json<ApiResponse<CircuitBreakerStatus>>) {
    if let Some(ref cb) = state.circuit_breaker {
        let stats = cb.get_stats().await;
        let total = stats.total_calls as f64;
        // CircuitBreakerStats 不跟踪 slow_call_rate，置为 0.0
        let failure_rate = if total > 0.0 {
            stats.failure_count as f64 / total
        } else {
            0.0
        };
        (
            StatusCode::OK,
            Json(ApiResponse::ok(CircuitBreakerStatus {
                state: stats.state.to_string(),
                failure_rate,
                slow_call_rate: 0.0,
            })),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::error("Circuit breaker not configured")),
        )
    }
}

/// GET /api/v1/status/circuit-breaker (无circuit-breaker特性)
#[cfg(not(feature = "circuit-breaker"))]
pub async fn get_circuit_breaker_status() -> (StatusCode, Json<ApiResponse<()>>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiResponse::error("Circuit breaker not configured")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::server::AppState;
    use crate::config::types::{
        Action, ActionConfig, FlowControlConfig, LimiterConfig, Matcher, Rule,
    };
    use crate::storage::{BanStorage, Storage};
    use crate::storage::{MemoryBanStorage, MemoryStorage};
    use crate::Governor;
    use std::sync::Arc;

    /// 构造包含至少一条规则的合法 FlowControlConfig（Governor::new() 默认配置无规则会 panic）
    fn make_valid_config() -> FlowControlConfig {
        FlowControlConfig {
            version: "0.1.0".to_string(),
            global: crate::config::types::GlobalConfig::default(),
            rules: vec![Rule {
                id: "test_rule".to_string(),
                name: "Test Rule".to_string(),
                priority: 100,
                matchers: vec![Matcher::User {
                    user_ids: vec!["*".to_string()],
                }],
                limiters: vec![LimiterConfig::TokenBucket {
                    capacity: 100,
                    refill_rate: 10,
                }],
                action: ActionConfig {
                    on_exceed: Action::Reject,
                    ban: None,
                },
            }],
        }
    }

    /// 构造可用的 Governor 实例（避免 Governor::new() 的空配置 panic）
    async fn make_governor() -> Governor {
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let ban_storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
        Governor::builder()
            .with_config(make_valid_config())
            .with_storage(storage)
            .with_ban_storage(ban_storage)
            .build()
            .await
            .expect("Governor build should succeed with valid config")
    }

    #[test]
    fn test_api_response_ok_contains_data() {
        let resp: ApiResponse<i32> = ApiResponse::ok(42);
        assert!(resp.success);
        assert_eq!(resp.message, "OK");
        assert_eq!(resp.data, Some(42));
    }

    #[test]
    fn test_api_response_error_has_no_data() {
        let resp: ApiResponse<i32> = ApiResponse::error("something failed");
        assert!(!resp.success);
        assert_eq!(resp.message, "something failed");
        assert_eq!(resp.data, None);
    }

    #[test]
    fn test_api_response_error_accepts_string_and_str() {
        let owned = String::from("owned error");
        let resp1: ApiResponse<i32> = ApiResponse::error(owned);
        assert_eq!(resp1.message, "owned error");

        let resp2: ApiResponse<i32> = ApiResponse::error("borrowed error");
        assert_eq!(resp2.message, "borrowed error");
    }

    // ========================================================================
    // Handler 集成测试
    // ========================================================================

    /// 构造最小可用 AppState（仅 Governor，可选组件为 None）
    async fn make_minimal_state() -> AppState {
        let governor = Arc::new(make_governor().await);
        AppState {
            governor,
            #[cfg(feature = "ban-manager")]
            ban_manager: None,
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
        }
    }

    #[tokio::test]
    async fn test_get_status_empty_governor() {
        let state = make_minimal_state().await;
        let resp = get_status(State(state)).await;
        assert!(resp.0.success);
        let data = resp.0.data.unwrap();
        assert_eq!(data.total_requests, 0);
        assert_eq!(data.blocked_requests, 0);
        assert_eq!(data.success_rate, 1.0);
    }

    #[tokio::test]
    async fn test_get_limiter_status_returns_key() {
        let state = make_minimal_state().await;
        let (status, resp) = get_limiter_status(State(state), Path("my-key".to_string())).await;
        // 端点尚未实现，返回 501
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
        assert!(!resp.0.success);
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_delete_ban_no_manager() {
        let state = make_minimal_state().await;
        let req = UnbanRequest {
            reason: None,
            operator: None,
        };
        let resp = delete_ban(State(state), Path("192.168.1.1".to_string()), Json(req)).await;
        assert!(!resp.1 .0.success);
        assert_eq!(resp.1 .0.message, "Ban manager not configured");
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_delete_ban_with_manager_ip_target() {
        use crate::BanManager;
        let ban_manager = Arc::new(BanManager::new().await.unwrap());
        let governor = Arc::new(make_governor().await);
        let state = AppState {
            governor,
            ban_manager: Some(ban_manager),
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
        };
        // 未封禁的 IP → Ban not found
        let req = UnbanRequest {
            reason: Some("test".to_string()),
            operator: Some("tester".to_string()),
        };
        let resp = delete_ban(State(state), Path("192.168.1.1".to_string()), Json(req)).await;
        assert!(!resp.1 .0.success);
        assert_eq!(resp.1 .0.message, "Ban not found");
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_delete_ban_with_manager_userid_target() {
        use crate::BanManager;
        let ban_manager = Arc::new(BanManager::new().await.unwrap());
        let governor = Arc::new(make_governor().await);
        let state = AppState {
            governor,
            ban_manager: Some(ban_manager),
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
        };
        // 非 IP 字符串 → UserId 目标
        let req = UnbanRequest {
            reason: None,
            operator: None,
        };
        let resp = delete_ban(State(state), Path("user-123".to_string()), Json(req)).await;
        assert!(!resp.1 .0.success);
        assert_eq!(resp.1 .0.message, "Ban not found");
    }

    #[cfg(feature = "quota-control")]
    #[tokio::test]
    async fn test_update_quota_no_controller() {
        let state = make_minimal_state().await;
        let req = UpdateQuotaRequest {
            resource: "api".to_string(),
            new_limit: 0,
            duration_secs: None,
        };
        let resp = update_quota(State(state), Path("tenant-1".to_string()), Json(req)).await;
        assert!(!resp.1 .0.success);
        assert_eq!(resp.1 .0.message, "Quota controller not configured");
    }

    #[cfg(feature = "quota-control")]
    #[tokio::test]
    async fn test_update_quota_unsupported_limit() {
        use crate::cache::quota_storage::CacheQuotaStorage;
        use crate::quota::QuotaConfig;
        use crate::storage::QuotaStorage;
        use crate::QuotaController;
        use oxcache::backend::memory::DashMapMemoryBackend;
        let storage: Arc<dyn QuotaStorage> = Arc::new(CacheQuotaStorage::new(Arc::new(
            DashMapMemoryBackend::new(),
        )));
        let quota_controller = Arc::new(QuotaController::with_dependencies(
            storage,
            QuotaConfig::default(),
        ));
        let governor = Arc::new(make_governor().await);
        let state = AppState {
            governor,
            #[cfg(feature = "ban-manager")]
            ban_manager: None,
            quota_controller: Some(quota_controller),
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
        };
        // new_limit > 0 → 不支持
        let req = UpdateQuotaRequest {
            resource: "api".to_string(),
            new_limit: 100,
            duration_secs: None,
        };
        let resp = update_quota(State(state), Path("tenant-1".to_string()), Json(req)).await;
        assert!(!resp.1 .0.success);
        assert!(resp.1 .0.message.contains("not supported"));
    }

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test]
    async fn test_get_circuit_breaker_status_no_cb() {
        let state = make_minimal_state().await;
        let resp = get_circuit_breaker_status(State(state)).await;
        assert!(!resp.1 .0.success);
        assert_eq!(resp.1 .0.message, "Circuit breaker not configured");
    }

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test]
    async fn test_get_circuit_breaker_status_with_cb() {
        use crate::circuit::types::{CircuitBreaker, CircuitBreakerConfig};
        let cb = Arc::new(CircuitBreaker::with_dependencies(
            CircuitBreakerConfig::default(),
        ));
        let governor = Arc::new(make_governor().await);
        let state = AppState {
            governor,
            #[cfg(feature = "ban-manager")]
            ban_manager: None,
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            circuit_breaker: Some(cb),
        };
        let resp = get_circuit_breaker_status(State(state)).await;
        assert!(resp.1 .0.success);
        let data = resp.1 .0.data.unwrap();
        assert_eq!(data.failure_rate, 0.0);
        assert_eq!(data.slow_call_rate, 0.0);
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_get_status_with_ban_manager_and_cb() {
        use crate::circuit::types::{CircuitBreaker, CircuitBreakerConfig};
        use crate::BanManager;
        let ban_manager = Arc::new(BanManager::new().await.unwrap());
        let cb = Arc::new(CircuitBreaker::with_dependencies(
            CircuitBreakerConfig::default(),
        ));
        let governor = Arc::new(make_governor().await);
        let state = AppState {
            governor,
            ban_manager: Some(ban_manager),
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            circuit_breaker: Some(cb),
        };
        let resp = get_status(State(state)).await;
        assert!(resp.0.success);
        let data = resp.0.data.unwrap();
        assert_eq!(data.active_bans, 0);
        assert!(!data.circuit_breaker.is_empty());
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_delete_ban_success() {
        use crate::ban::BanTarget;
        use crate::storage::BanRecord;
        use crate::BanManager;
        let ban_manager = Arc::new(BanManager::new().await.unwrap());
        // 先添加一条封禁
        let target = BanTarget::Ip("10.0.0.1".to_string());
        let now = chrono::Utc::now();
        ban_manager
            .add_ban(BanRecord {
                target: target.clone(),
                ban_times: 1,
                duration: std::time::Duration::from_secs(3600),
                banned_at: now,
                expires_at: now + chrono::Duration::seconds(3600),
                is_manual: true,
                reason: "test ban".to_string(),
            })
            .await
            .unwrap();
        let governor = Arc::new(make_governor().await);
        let state = AppState {
            governor,
            ban_manager: Some(ban_manager),
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
        };
        let req = UnbanRequest {
            reason: Some("manual unban".to_string()),
            operator: Some("admin".to_string()),
        };
        let resp = delete_ban(State(state), Path("10.0.0.1".to_string()), Json(req)).await;
        assert!(resp.1 .0.success);
        assert_eq!(resp.1 .0.message, "manual unban");
    }

    #[cfg(feature = "quota-control")]
    #[tokio::test]
    async fn test_update_quota_reset_success() {
        use crate::cache::quota_storage::CacheQuotaStorage;
        use crate::quota::QuotaConfig;
        use crate::storage::QuotaStorage;
        use crate::QuotaController;
        use oxcache::backend::memory::DashMapMemoryBackend;
        let storage: Arc<dyn QuotaStorage> = Arc::new(CacheQuotaStorage::new(Arc::new(
            DashMapMemoryBackend::new(),
        )));
        let quota_controller = Arc::new(QuotaController::with_dependencies(
            storage,
            QuotaConfig::default(),
        ));
        let governor = Arc::new(make_governor().await);
        let state = AppState {
            governor,
            #[cfg(feature = "ban-manager")]
            ban_manager: None,
            quota_controller: Some(quota_controller),
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: None,
        };
        let req = UpdateQuotaRequest {
            resource: "api".to_string(),
            new_limit: 0,
            duration_secs: Some(3600),
        };
        let resp = update_quota(State(state), Path("tenant-1".to_string()), Json(req)).await;
        assert!(resp.1 .0.success);
        let data = resp.1 .0.data.unwrap();
        assert!(data.expires_at.is_some());
    }

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test]
    async fn test_get_circuit_breaker_status_with_failures() {
        use crate::circuit::types::{CircuitBreaker, CircuitBreakerConfig};
        use crate::error::{FlowGuardError, StorageError};
        let cb = Arc::new(CircuitBreaker::with_dependencies(
            CircuitBreakerConfig::default(),
        ));
        // 通过 execute 触发一次失败以覆盖 failure_rate > 0 分支
        // 使用 ConnectionError（transient）才会被 DefaultErrorClassifier 计为失败
        let _ = cb
            .execute(|| async {
                Err::<(), FlowGuardError>(FlowGuardError::StorageError(
                    StorageError::ConnectionError("test".to_string()),
                ))
            })
            .await;
        let governor = Arc::new(make_governor().await);
        let state = AppState {
            governor,
            #[cfg(feature = "ban-manager")]
            ban_manager: None,
            #[cfg(feature = "quota-control")]
            quota_controller: None,
            circuit_breaker: Some(cb),
        };
        let resp = get_circuit_breaker_status(State(state)).await;
        assert!(resp.1 .0.success);
        let data = resp.1 .0.data.unwrap();
        assert!(data.failure_rate > 0.0);
    }
}
