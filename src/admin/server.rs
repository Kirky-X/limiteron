// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
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

        // 启动前强制校验配置（api_key 非空、长度 ≥16），防止以空 key 暴露管理 API
        self.config.validate()?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::config::AdminApiConfig;
    use crate::admin::make_governor;

    #[tokio::test]
    async fn test_admin_server_new() {
        let governor = Arc::new(make_governor().await);
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let server = AdminServer::new(governor, config);
        // 验证 state 字段存在
        assert!(Arc::strong_count(&server.state.governor) >= 1);
        #[cfg(feature = "ban-manager")]
        assert!(server.state.ban_manager.is_none());
        #[cfg(feature = "quota-control")]
        assert!(server.state.quota_controller.is_none());
        #[cfg(feature = "circuit-breaker")]
        assert!(server.state.circuit_breaker.is_none());
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_admin_server_with_ban_manager() {
        use crate::BanManager;
        let governor = Arc::new(make_governor().await);
        let ban_manager = Arc::new(BanManager::new().await.unwrap());
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let server = AdminServer::new(governor, config).with_ban_manager(ban_manager);
        assert!(server.state.ban_manager.is_some());
    }

    #[cfg(feature = "quota-control")]
    #[tokio::test]
    async fn test_admin_server_with_quota_controller() {
        use crate::QuotaController;
        use crate::cache::quota_storage::CacheQuotaStorage;
        use crate::quota::QuotaConfig;
        use crate::storage::QuotaStorage;
        use oxcache::backend::memory::DashMapMemoryBackend;
        let governor = Arc::new(make_governor().await);
        let storage: Arc<dyn QuotaStorage> = Arc::new(CacheQuotaStorage::new(Arc::new(
            DashMapMemoryBackend::new(),
        )));
        let quota_controller = Arc::new(QuotaController::with_dependencies(
            storage,
            QuotaConfig::default(),
        ));
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let server = AdminServer::new(governor, config).with_quota_controller(quota_controller);
        assert!(server.state.quota_controller.is_some());
    }

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test]
    async fn test_admin_server_with_circuit_breaker() {
        use crate::circuit::{CircuitBreaker, CircuitBreakerConfig};
        let governor = Arc::new(make_governor().await);
        let cb = Arc::new(CircuitBreaker::with_dependencies(
            CircuitBreakerConfig::default(),
        ));
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let server = AdminServer::new(governor, config).with_circuit_breaker(cb);
        assert!(server.state.circuit_breaker.is_some());
    }

    #[tokio::test]
    async fn test_admin_server_start_disabled() {
        let governor = Arc::new(make_governor().await);
        // disabled = true 的配置
        let config = AdminApiConfig::default();
        let server = AdminServer::new(governor, config);
        // start() 在 disabled 时应立即返回 Ok(())
        let result = server.start().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_admin_server_into_router() {
        let governor = Arc::new(make_governor().await);
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let server = AdminServer::new(governor, config);
        // into_router 应返回一个 Router 而不启动服务器
        let _router = server.into_router();
        // 如果没有 panic 则说明路由创建成功
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_admin_server_chained_builders() {
        use crate::cache::quota_storage::CacheQuotaStorage;
        use crate::circuit::{CircuitBreaker, CircuitBreakerConfig};
        use crate::quota::QuotaConfig;
        use crate::storage::QuotaStorage;
        use crate::{BanManager, QuotaController};
        use oxcache::backend::memory::DashMapMemoryBackend;
        let governor = Arc::new(make_governor().await);
        let ban_manager = Arc::new(BanManager::new().await.unwrap());
        let storage: Arc<dyn QuotaStorage> = Arc::new(CacheQuotaStorage::new(Arc::new(
            DashMapMemoryBackend::new(),
        )));
        let quota_controller = Arc::new(QuotaController::with_dependencies(
            storage,
            QuotaConfig::default(),
        ));
        let cb = Arc::new(CircuitBreaker::with_dependencies(
            CircuitBreakerConfig::default(),
        ));
        let config = AdminApiConfig::new("test-api-key-16chars!!");
        let server = AdminServer::new(governor, config)
            .with_ban_manager(ban_manager)
            .with_quota_controller(quota_controller)
            .with_circuit_breaker(cb);
        assert!(server.state.ban_manager.is_some());
        assert!(server.state.quota_controller.is_some());
        assert!(server.state.circuit_breaker.is_some());
    }
}
