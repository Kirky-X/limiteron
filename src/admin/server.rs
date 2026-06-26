//! HTTP服务器启动

#[cfg(feature = "ban-manager")]
use crate::BanManager;
#[cfg(feature = "circuit-breaker")]
use crate::CircuitBreaker;
use crate::Governor;
#[cfg(feature = "quota-control")]
use crate::QuotaController;

use super::config::AdminApiConfig;
use super::routes;
use axum::Router;
use std::sync::Arc;
use tokio::net::TcpListener;

/// 应用状态
#[derive(Clone)]
pub struct AppState {
    pub governor: Arc<Governor>,
    #[cfg(feature = "ban-manager")]
    pub ban_manager: Option<Arc<BanManager>>,
    #[cfg(feature = "quota-control")]
    pub quota_controller: Option<Arc<QuotaController>>,
    #[cfg(feature = "circuit-breaker")]
    pub circuit_breaker: Option<Arc<CircuitBreaker>>,
}

/// 管理API服务器
pub struct AdminServer {
    state: AppState,
    config: AdminApiConfig,
}

impl AdminServer {
    /// 创建新服务器
    pub fn new(governor: Arc<Governor>, config: AdminApiConfig) -> Self {
        Self {
            state: AppState {
                governor,
                #[cfg(feature = "ban-manager")]
                ban_manager: None,
                #[cfg(feature = "quota-control")]
                quota_controller: None,
                #[cfg(feature = "circuit-breaker")]
                circuit_breaker: None,
            },
            config,
        }
    }

    /// 设置封禁管理器
    #[cfg(feature = "ban-manager")]
    pub fn with_ban_manager(mut self, ban_manager: Arc<BanManager>) -> Self {
        self.state.ban_manager = Some(ban_manager);
        self
    }

    /// 设置配额控制器
    #[cfg(feature = "quota-control")]
    pub fn with_quota_controller(mut self, quota_controller: Arc<QuotaController>) -> Self {
        self.state.quota_controller = Some(quota_controller);
        self
    }

    /// 设置熔断器
    #[cfg(feature = "circuit-breaker")]
    pub fn with_circuit_breaker(mut self, circuit_breaker: Arc<CircuitBreaker>) -> Self {
        self.state.circuit_breaker = Some(circuit_breaker);
        self
    }

    /// 启动服务器
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.config.enabled {
            log::info!(target: "admin-api", "管理API已禁用");
            return Ok(());
        }

        let router = routes::create_router(self.state.clone(), &self.config);

        let listener = TcpListener::bind(&self.config.address()).await?;
        let address = listener.local_addr()?;

        log::info!(
            target: "admin-api",
            "管理API服务器已启动: http://{}",
            address
        );

        axum::serve(listener, router).await?;

        Ok(())
    }

    /// 创建Router(不启动服务器)
    pub fn into_router(self) -> Router {
        routes::create_router(self.state.clone(), &self.config)
    }
}
