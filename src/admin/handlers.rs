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
pub async fn get_limiter_status(
    State(_state): State<AppState>,
    Path(key): Path<String>,
) -> (StatusCode, Json<ApiResponse<LimiterStatus>>) {
    // TODO: 实现从存储中获取限流状态
    let status = LimiterStatus {
        key: key.clone(),
        limit: 0,
        remaining: 0,
        reset_at: 0,
    };

    (StatusCode::OK, Json(ApiResponse::ok(status)))
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
#[cfg(feature = "ban-manager")]
pub async fn delete_ban(
    State(state): State<AppState>,
    Path(target): Path<String>,
    Json(req): Json<UnbanRequest>,
) -> Json<ApiResponse<()>> {
    if let Some(ref ban_manager) = state.ban_manager {
        // 路径 target 优先按 IP 解析，回退为 UserId
        let ban_target = if target.parse::<std::net::IpAddr>().is_ok() {
            BanTarget::Ip(target)
        } else {
            BanTarget::UserId(target)
        };
        let operator = req.operator.unwrap_or_else(|| "admin-api".to_string());
        match ban_manager.delete_ban(&ban_target, operator).await {
            Ok(true) => Json(ApiResponse {
                success: true,
                message: req.reason.unwrap_or_else(|| "Ban removed".to_string()),
                data: Some(()),
            }),
            Ok(false) => Json(ApiResponse::error("Ban not found")),
            Err(e) => Json(ApiResponse::error(format!("Failed to remove ban: {}", e))),
        }
    } else {
        Json(ApiResponse::error("Ban manager not configured"))
    }
}

/// DELETE /api/v1/ban/{target} (无ban-manager特性)
#[cfg(not(feature = "ban-manager"))]
pub async fn delete_ban(Path(_target): Path<String>) -> Json<ApiResponse<()>> {
    Json(ApiResponse::error("Ban manager not configured"))
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
#[cfg(feature = "quota-control")]
pub async fn update_quota(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Json(req): Json<UpdateQuotaRequest>,
) -> Json<ApiResponse<UpdateQuotaResponse>> {
    if let Some(ref quota_controller) = state.quota_controller {
        // QuotaController 当前不支持 per-tenant 配额上限更新（配额上限为全局 QuotaConfig）
        // 提供重置配额使用量作为最接近的操作
        if req.new_limit == 0 {
            // new_limit=0 视为重置信号
            match quota_controller
                .reset_quota(&tenant_id, &req.resource)
                .await
            {
                Ok(_) => Json(ApiResponse::ok(UpdateQuotaResponse {
                    success: true,
                    expires_at: req.duration_secs.map(|d| {
                        use std::time::{SystemTime, UNIX_EPOCH};
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|t| t.as_secs() + d)
                            .unwrap_or(0)
                    }),
                })),
                Err(e) => Json(ApiResponse::error(format!("Failed to reset quota: {}", e))),
            }
        } else {
            Json(ApiResponse::error(
                "Per-tenant quota limit update is not supported; use global QuotaConfig update_config instead",
            ))
        }
    } else {
        Json(ApiResponse::error("Quota controller not configured"))
    }
}

/// PUT /api/v1/quota/{tenant_id} (无quota-control特性)
#[cfg(not(feature = "quota-control"))]
pub async fn update_quota(
    Path(_tenant_id): Path<String>,
) -> Json<ApiResponse<UpdateQuotaResponse>> {
    Json(ApiResponse::error("Quota controller not configured"))
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
#[cfg(feature = "circuit-breaker")]
pub async fn get_circuit_breaker_status(
    State(state): State<AppState>,
) -> Json<ApiResponse<CircuitBreakerStatus>> {
    if let Some(ref cb) = state.circuit_breaker {
        let stats = cb.get_stats().await;
        let total = stats.total_calls as f64;
        // CircuitBreakerStats 不跟踪 slow_call_rate，置为 0.0
        let failure_rate = if total > 0.0 {
            stats.failure_count as f64 / total
        } else {
            0.0
        };
        Json(ApiResponse::ok(CircuitBreakerStatus {
            state: stats.state.to_string(),
            failure_rate,
            slow_call_rate: 0.0,
        }))
    } else {
        Json(ApiResponse::error("Circuit breaker not configured"))
    }
}

/// GET /api/v1/status/circuit-breaker (无circuit-breaker特性)
#[cfg(not(feature = "circuit-breaker"))]
pub async fn get_circuit_breaker_status() -> Json<ApiResponse<()>> {
    Json(ApiResponse::error("Circuit breaker not configured"))
}
