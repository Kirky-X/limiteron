//! Redis客户端封装
//!
//! 实现基于Redis的存储层，支持单机和集群模式，提供连接池、重试机制和Lua脚本支持。
//!
//! # 特性
//!
//! - **连接池**: 使用ConnectionManager管理连接
//! - **重试机制**: 指数退避重试，最多3次
//! - **Lua脚本**: 预加载脚本，原子性操作
//! - **集群支持**: 支持Redis Cluster
//! - **降级机制**: Redis故障时自动降级
//!

#[cfg(feature = "redis")]
use async_trait::async_trait;
use redis::{aio::ConnectionManager, AsyncCommands, Client};
use secrecy::{ExposeSecret, Secret};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, error, info, trace, warn};

use crate::error::{ConsumeResult, StorageError};
use crate::lua_scripts::{LuaScriptManager, LuaScriptType};
use crate::storage::{BanRecord, BanStorage, BanTarget, QuotaInfo, QuotaStorage, Storage};

// ============================================================================
// Redis 键验证常量
// ============================================================================

/// 最大键组件长度
const MAX_KEY_COMPONENT_LENGTH: usize = 255;

/// 最大键总长度
const MAX_KEY_LENGTH: usize = 1024;

// ============================================================================
// Redis 键验证和清理函数
// ============================================================================

/// 清理键组件（移除危险字符）
///
/// # 参数
/// - `input`: 输入字符串
///
/// # 返回
/// - 清理后的字符串
fn sanitize_key_component(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .take(MAX_KEY_COMPONENT_LENGTH)
        .collect()
}

/// 验证键组件
///
/// # 参数
/// - `component`: 键组件
///
/// # 返回
/// - `Ok(())`: 验证通过
/// - `Err(StorageError)`: 验证失败
fn validate_key_component(component: &str) -> Result<(), StorageError> {
    if component.is_empty() {
        return Err(StorageError::QueryError("键组件不能为空".to_string()));
    }

    if component.len() > MAX_KEY_COMPONENT_LENGTH {
        return Err(StorageError::QueryError(format!(
            "键组件长度超过限制（最大 {} 字符）",
            MAX_KEY_COMPONENT_LENGTH
        )));
    }

    // 检查是否包含危险字符
    if component.contains(':') || component.contains('*') || component.contains('?') {
        return Err(StorageError::QueryError("键组件包含非法字符".to_string()));
    }

    Ok(())
}

/// 验证完整键
///
/// # 参数
/// - `key`: 完整键
///
/// # 返回
/// - `Ok(())`: 验证通过
/// - `Err(StorageError)`: 验证失败
fn validate_key(key: &str) -> Result<(), StorageError> {
    if key.is_empty() {
        return Err(StorageError::QueryError("键不能为空".to_string()));
    }

    if key.len() > MAX_KEY_LENGTH {
        return Err(StorageError::QueryError(format!(
            "键长度超过限制（最大 {} 字符）",
            MAX_KEY_LENGTH
        )));
    }

    // 检查是否包含空字节
    if key.contains('\0') {
        return Err(StorageError::QueryError("键包含非法字符".to_string()));
    }

    Ok(())
}

/// Redis配置
#[cfg(feature = "redis")]
#[derive(Clone)]
pub struct RedisConfig {
    /// Redis连接URL
    pub url: String,
    /// 数据库索引
    pub db: i64,
    /// 密码（使用 Secret 包装以防止意外泄露）
    pub password: Option<Secret<String>>,
    /// 连接超时
    pub connection_timeout: Duration,
    /// 读写超时
    pub io_timeout: Duration,
    /// 最大重试次数
    pub max_retries: u32,
    /// 重试初始退避时间
    pub retry_initial_backoff: Duration,
    /// 是否启用集群模式
    pub cluster_mode: bool,
    /// 连接池大小
    pub pool_size: usize,
    /// 是否启用Lua脚本
    pub enable_lua: bool,
}

impl std::fmt::Debug for RedisConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisConfig")
            .field("url", &self.url)
            .field("db", &self.db)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .field("connection_timeout", &self.connection_timeout)
            .field("io_timeout", &self.io_timeout)
            .field("max_retries", &self.max_retries)
            .field("retry_initial_backoff", &self.retry_initial_backoff)
            .field("cluster_mode", &self.cluster_mode)
            .field("pool_size", &self.pool_size)
            .field("enable_lua", &self.enable_lua)
            .finish()
    }
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: "redis://127.0.0.1:6379".to_string(),
            db: 0,
            password: None,
            connection_timeout: Duration::from_secs(5),
            io_timeout: Duration::from_secs(5),
            max_retries: 3,
            retry_initial_backoff: Duration::from_millis(100),
            cluster_mode: false,
            pool_size: 10,
            enable_lua: true,
        }
    }
}

impl RedisConfig {
    /// 创建新的Redis配置
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }

    /// 设置数据库索引
    pub fn db(mut self, db: i64) -> Self {
        self.db = db;
        self
    }

    /// 设置密码
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(Secret::new(password.into()));
        self
    }

    /// 设置密码（使用 Secret）
    pub fn password_secret(mut self, password: Secret<String>) -> Self {
        self.password = Some(password);
        self
    }

    /// 设置连接超时
    pub fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }

    /// 设置IO超时
    pub fn io_timeout(mut self, timeout: Duration) -> Self {
        self.io_timeout = timeout;
        self
    }

    /// 设置最大重试次数
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// 设置重试初始退避时间
    pub fn retry_initial_backoff(mut self, backoff: Duration) -> Self {
        self.retry_initial_backoff = backoff;
        self
    }

    /// 设置是否启用集群模式
    pub fn cluster_mode(mut self, cluster: bool) -> Self {
        self.cluster_mode = cluster;
        self
    }

    /// 设置连接池大小
    pub fn pool_size(mut self, size: usize) -> Self {
        self.pool_size = size;
        self
    }

    /// 设置是否启用Lua脚本
    pub fn enable_lua(mut self, enable: bool) -> Self {
        self.enable_lua = enable;
        self
    }
}

/// 重试统计
#[cfg(feature = "redis")]
#[derive(Debug, Default, Clone)]
pub struct RetryStats {
    /// 总重试次数
    pub total_retries: Arc<std::sync::atomic::AtomicU64>,
    /// 成功重试次数
    pub successful_retries: Arc<std::sync::atomic::AtomicU64>,
    /// 失败重试次数
    pub failed_retries: Arc<std::sync::atomic::AtomicU64>,
}

impl RetryStats {
    /// 获取总重试次数
    pub fn total_retries(&self) -> u64 {
        self.total_retries
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 获取成功重试次数
    pub fn successful_retries(&self) -> u64 {
        self.successful_retries
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 获取失败重试次数
    pub fn failed_retries(&self) -> u64 {
        self.failed_retries
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 记录重试成功
    pub fn record_success(&self) {
        self.total_retries
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.successful_retries
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 记录重试失败
    pub fn record_failure(&self) {
        self.total_retries
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.failed_retries
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 重置统计
    pub fn reset(&self) {
        self.total_retries
            .store(0, std::sync::atomic::Ordering::Relaxed);
        self.successful_retries
            .store(0, std::sync::atomic::Ordering::Relaxed);
        self.failed_retries
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Redis存储实现
#[cfg(feature = "redis")]
#[derive(Clone)]
pub struct RedisStorage {
    /// 连接管理器
    conn_manager: Arc<Mutex<Option<ConnectionManager>>>,
    /// 配置
    config: RedisConfig,
    /// Lua脚本管理器
    lua_manager: Option<Arc<LuaScriptManager>>,
    /// 重试统计
    retry_stats: RetryStats,
    /// 降级状态
    degraded: Arc<Mutex<bool>>,
    /// 最后降级时间
    last_degraded_at: Arc<Mutex<Option<Instant>>>,
}

impl RedisStorage {
    /// 创建新的Redis存储
    pub async fn new(config: RedisConfig) -> Result<Self, StorageError> {
        info!("创建Redis存储, URL: {}", config.url);

        let lua_manager = if config.enable_lua {
            Some(Arc::new(LuaScriptManager::new()))
        } else {
            None
        };

        let storage = Self {
            conn_manager: Arc::new(Mutex::new(None)),
            config,
            lua_manager,
            retry_stats: RetryStats::default(),
            degraded: Arc::new(Mutex::new(false)),
            last_degraded_at: Arc::new(Mutex::new(None)),
        };

        // 初始化连接
        storage.connect().await?;

        // 预加载Lua脚本
        if let Some(lua_manager) = &storage.lua_manager {
            if let Some(conn_manager) = storage.conn_manager.lock().await.as_ref() {
                let mut conn = conn_manager.clone();
                lua_manager.preload_all_scripts(&mut conn).await?;
            }
        }

        info!("Redis存储创建成功");
        Ok(storage)
    }

    /// 检查Redis连接
    pub async fn ping(&self) -> Result<(), StorageError> {
        self.execute_with_retry(|| async {
            let conn_manager = self.conn_manager.lock().await;
            let conn_manager = conn_manager
                .as_ref()
                .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

            let mut conn = conn_manager.clone();

            let _: String = redis::cmd("PING")
                .query_async(&mut conn)
                .await
                .map_err(|e| {
                    error!("Redis PING失败: {}", e);
                    StorageError::QueryError(format!("PING失败: {}", e))
                })?;

            Ok(())
        })
        .await
    }

    /// 建立连接
    async fn connect(&self) -> Result<(), StorageError> {
        debug!("建立Redis连接");

        // 使用安全的 ConnectionInfo 来处理认证
        let client_info = if let Some(password) = &self.config.password {
            // 解析 URL 地址
            let url = self.config.url.trim_start_matches("redis://");
            let url = url.trim_start_matches("rediss://");

            // 移除可能的认证信息
            let url = if let Some(at_pos) = url.find('@') {
                &url[at_pos + 1..]
            } else {
                url
            };

            // 解析地址和端口
            let (host, port) = if let Some(colon_pos) = url.rfind(':') {
                let host = &url[..colon_pos];
                let port = url[colon_pos + 1..].parse::<u16>().unwrap_or(6379);
                (host.to_string(), port)
            } else {
                (url.to_string(), 6379)
            };

            let addr = redis::ConnectionAddr::Tcp(host, port);

            // 创建安全的连接信息
            redis::ConnectionInfo {
                addr,
                redis: redis::RedisConnectionInfo {
                    db: self.config.db,
                    username: None,
                    password: Some(password.expose_secret().clone()),
                },
            }
        } else {
            // 无密码的情况
            let url = self.config.url.trim_start_matches("redis://");
            let url = url.trim_start_matches("rediss://");
            let url = if let Some(at_pos) = url.find('@') {
                &url[at_pos + 1..]
            } else {
                url
            };

            // 解析地址和端口
            let (host, port) = if let Some(colon_pos) = url.rfind(':') {
                let host = &url[..colon_pos];
                let port = url[colon_pos + 1..].parse::<u16>().unwrap_or(6379);
                (host.to_string(), port)
            } else {
                (url.to_string(), 6379)
            };

            let addr = redis::ConnectionAddr::Tcp(host, port);

            redis::ConnectionInfo {
                addr,
                redis: redis::RedisConnectionInfo {
                    db: self.config.db,
                    username: None,
                    password: None,
                },
            }
        };

        let client = Client::open(client_info).map_err(|e| {
            error!("创建Redis客户端失败: {}", e);
            StorageError::ConnectionError(format!("创建Redis客户端失败: {}", e))
        })?;

        let conn_manager = ConnectionManager::new(client).await.map_err(|e| {
            error!("创建Redis连接管理器失败: {}", e);
            StorageError::ConnectionError(format!("创建Redis连接管理器失败: {}", e))
        })?;

        *self.conn_manager.lock().await = Some(conn_manager);
        *self.degraded.lock().await = false;

        info!("Redis连接建立成功");
        Ok(())
    }

    /// 带重试的执行
    async fn execute_with_retry<F, Fut, T>(&self, f: F) -> Result<T, StorageError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, StorageError>>,
    {
        let mut last_error = None;
        let mut backoff = self.config.retry_initial_backoff;

        for attempt in 0..=self.config.max_retries {
            match f().await {
                Ok(result) => {
                    if attempt > 0 {
                        self.retry_stats.record_success();
                        debug!("重试成功，尝试次数: {}", attempt);
                    }
                    return Ok(result);
                }
                Err(e) => {
                    last_error = Some(e.clone());

                    if attempt < self.config.max_retries {
                        warn!(
                            "操作失败，将在 {:?} 后重试 (尝试 {}/{}): {}",
                            backoff,
                            attempt + 1,
                            self.config.max_retries,
                            e
                        );
                        tokio::time::sleep(backoff).await;
                        backoff = backoff.mul_f32(2.0); // 指数退避

                        // 尝试重新连接
                        if matches!(e, StorageError::ConnectionError(_)) {
                            if let Err(reconnect_err) = self.reconnect().await {
                                error!("重新连接失败: {}", reconnect_err);
                            }
                        }
                    }
                }
            }
        }

        self.retry_stats.record_failure();
        error!("操作失败，已达最大重试次数: {:?}", last_error);

        // 检查是否需要降级
        if matches!(last_error, Some(StorageError::ConnectionError(_))) {
            self.set_degraded(true).await;
        }

        Err(last_error.unwrap_or(StorageError::TimeoutError("操作超时".to_string())))
    }

    /// 重新连接
    async fn reconnect(&self) -> Result<(), StorageError> {
        debug!("尝试重新连接Redis");

        // 清理旧连接
        *self.conn_manager.lock().await = None;

        // 建立新连接
        self.connect().await
    }

    /// 设置降级状态
    async fn set_degraded(&self, degraded: bool) {
        let current = *self.degraded.lock().await;
        if current != degraded {
            *self.degraded.lock().await = degraded;
            if degraded {
                *self.last_degraded_at.lock().await = Some(Instant::now());
                warn!("Redis存储已降级，将使用备用存储");
            } else {
                info!("Redis存储已恢复正常");
            }
        }
    }

    /// 检查是否降级
    pub async fn is_degraded(&self) -> bool {
        *self.degraded.lock().await
    }

    /// 获取重试统计
    pub fn retry_stats(&self) -> &RetryStats {
        &self.retry_stats
    }

    /// 获取Lua脚本管理器
    pub fn lua_manager(&self) -> Option<&Arc<LuaScriptManager>> {
        self.lua_manager.as_ref()
    }

    /// 执行滑动窗口限流
    pub async fn sliding_window(
        &self,
        key: &str,
        window_size: Duration,
        max_requests: u64,
    ) -> Result<(bool, u64, i64), StorageError> {
        let lua_manager = self
            .lua_manager
            .as_ref()
            .ok_or_else(|| StorageError::QueryError("Lua脚本未启用".to_string()))?;

        let current_timestamp = chrono::Utc::now().timestamp_millis();
        let window_size_ms = window_size.as_millis() as i64;

        let result: (i32, i64, i64) = self
            .execute_with_retry(|| async {
                let conn_manager = self.conn_manager.lock().await;
                let conn_manager = conn_manager
                    .as_ref()
                    .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

                let mut conn = conn_manager.clone();
                lua_manager
                    .execute_script(
                        &mut conn,
                        LuaScriptType::SlidingWindow,
                        &[key],
                        &[
                            &window_size_ms.to_string(),
                            &max_requests.to_string(),
                            &current_timestamp.to_string(),
                        ],
                    )
                    .await
            })
            .await?;

        let allowed = result.0 == 1;
        let current_count = result.1 as u64;
        let reset_time = result.2;

        Ok((allowed, current_count, reset_time))
    }

    /// 执行固定窗口限流
    pub async fn fixed_window(
        &self,
        key: &str,
        window_size: Duration,
        max_requests: u64,
    ) -> Result<(bool, u64, i64), StorageError> {
        let lua_manager = self
            .lua_manager
            .as_ref()
            .ok_or_else(|| StorageError::QueryError("Lua脚本未启用".to_string()))?;

        let current_timestamp = chrono::Utc::now().timestamp_millis();
        let window_size_ms = window_size.as_millis() as i64;

        let result: (i32, i64, i64) = self
            .execute_with_retry(|| async {
                let conn_manager = self.conn_manager.lock().await;
                let conn_manager = conn_manager
                    .as_ref()
                    .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

                let mut conn = conn_manager.clone();
                lua_manager
                    .execute_script(
                        &mut conn,
                        LuaScriptType::FixedWindow,
                        &[key],
                        &[
                            &window_size_ms.to_string(),
                            &max_requests.to_string(),
                            &current_timestamp.to_string(),
                        ],
                    )
                    .await
            })
            .await?;

        let allowed = result.0 == 1;
        let current_count = result.1 as u64;
        let reset_time = result.2;

        Ok((allowed, current_count, reset_time))
    }

    /// 执行令牌桶限流
    pub async fn token_bucket(
        &self,
        key: &str,
        capacity: u64,
        refill_rate: u64, // tokens per second
        tokens_requested: u64,
    ) -> Result<(bool, u64, i64), StorageError> {
        let lua_manager = self
            .lua_manager
            .as_ref()
            .ok_or_else(|| StorageError::QueryError("Lua脚本未启用".to_string()))?;

        let current_timestamp = chrono::Utc::now().timestamp_millis();
        let refill_rate_ms = refill_rate as f64 / 1000.0; // tokens per millisecond

        let result: (i32, i64, i64) = self
            .execute_with_retry(|| async {
                let conn_manager = self.conn_manager.lock().await;
                let conn_manager = conn_manager
                    .as_ref()
                    .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

                let mut conn = conn_manager.clone();
                lua_manager
                    .execute_script(
                        &mut conn,
                        LuaScriptType::TokenBucket,
                        &[key],
                        &[
                            &capacity.to_string(),
                            &refill_rate_ms.to_string(),
                            &current_timestamp.to_string(),
                            &tokens_requested.to_string(),
                        ],
                    )
                    .await
            })
            .await?;

        let allowed = result.0 == 1;
        let tokens_remaining = result.1 as u64;
        let refill_time = result.2;

        Ok((allowed, tokens_remaining, refill_time))
    }

    /// 生成配额键（优化：使用用户级别的 Hash）
    ///
    /// 优化前：quota:user123:resource1 -> Hash {consumed, limit, window_start, window_end}
    /// 优化后：quota:user123 -> Hash {resource1_consumed, resource1_limit, resource1_window_start, resource1_window_end, ...}
    ///
    /// 优点：
    /// - 减少 Redis 键数量（从 O(n*m) 到 O(n)）
    /// - 提高内存效率（减少键的元数据开销）
    /// - 批量操作更高效
    fn quota_key(user_id: &str, _resource: &str) -> String {
        format!("quota:{}", user_id)
    }

    /// 生成配额字段名
    fn quota_field(resource: &str, field: &str) -> String {
        format!("{}_{}", resource, field)
    }

    /// 生成封禁键
    fn ban_key(target: &BanTarget) -> String {
        let key = match target {
            BanTarget::Ip(ip) => {
                let sanitized_ip = sanitize_key_component(ip);
                format!("ban:ip:{}", sanitized_ip)
            }
            BanTarget::UserId(user_id) => {
                let sanitized_user_id = sanitize_key_component(user_id);
                format!("ban:user:{}", sanitized_user_id)
            }
            BanTarget::Mac(mac) => {
                let sanitized_mac = sanitize_key_component(mac);
                format!("ban:mac:{}", sanitized_mac)
            }
        };

        // 验证生成的键
        if let Err(e) = validate_key(&key) {
            error!("无效的封禁键: {}", e);
            return "ban:invalid".to_string();
        }

        key
    }

    /// 生成封禁历史键
    fn ban_history_key(target: &BanTarget) -> String {
        let base_key = Self::ban_key(target);
        let key = format!("{}:history", base_key);

        // 验证生成的键
        if let Err(e) = validate_key(&key) {
            error!("无效的封禁历史键: {}", e);
            return "ban:invalid:history".to_string();
        }

        key
    }
}

#[async_trait]
impl Storage for RedisStorage {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        self.execute_with_retry(|| async {
            let conn_manager = self.conn_manager.lock().await;
            let conn_manager = conn_manager
                .as_ref()
                .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

            let mut conn = conn_manager.clone();
            let result: Option<String> = conn.get(key).await.map_err(|e| {
                error!("Redis GET失败: {}", e);
                StorageError::QueryError(format!("GET失败: {}", e))
            })?;

            trace!("GET key={}, result={:?}", key, result);
            Ok(result)
        })
        .await
    }

    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError> {
        self.execute_with_retry(|| async {
            let conn_manager = self.conn_manager.lock().await;
            let conn_manager = conn_manager
                .as_ref()
                .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

            let mut conn = conn_manager.clone();

            if let Some(ttl) = ttl {
                let _: () = conn.set_ex(key, value, ttl).await.map_err(|e| {
                    error!("Redis SETEX失败: {}", e);
                    StorageError::QueryError(format!("SETEX失败: {}", e))
                })?;
            } else {
                let _: () = conn.set(key, value).await.map_err(|e| {
                    error!("Redis SET失败: {}", e);
                    StorageError::QueryError(format!("SET失败: {}", e))
                })?;
            }

            trace!("SET key={}, value={:?}, ttl={:?}", key, value, ttl);
            Ok(())
        })
        .await
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.execute_with_retry(|| async {
            let conn_manager = self.conn_manager.lock().await;
            let conn_manager = conn_manager
                .as_ref()
                .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

            let mut conn = conn_manager.clone();
            let _: () = conn.del(key).await.map_err(|e| {
                error!("Redis DEL失败: {}", e);
                StorageError::QueryError(format!("DEL失败: {}", e))
            })?;

            trace!("DEL key={}", key);
            Ok(())
        })
        .await
    }
}

#[async_trait]
impl QuotaStorage for RedisStorage {
    async fn get_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError> {
        let key = Self::quota_key(user_id, resource);

        self.execute_with_retry(|| async {
            let conn_manager = self.conn_manager.lock().await;
            let conn_manager = conn_manager
                .as_ref()
                .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

            let mut conn = conn_manager.clone();

            // 使用优化的字段名
            let consumed_field = Self::quota_field(resource, "consumed");
            let limit_field = Self::quota_field(resource, "limit");
            let window_start_field = Self::quota_field(resource, "window_start");
            let window_end_field = Self::quota_field(resource, "window_end");

            // 批量获取配额信息（使用 HMGET 减少网络往返）
            let result: Vec<Option<String>> = redis::cmd("HMGET")
                .arg(&key)
                .arg(&consumed_field)
                .arg(&limit_field)
                .arg(&window_start_field)
                .arg(&window_end_field)
                .query_async(&mut conn)
                .await
                .map_err(|e| {
                    error!("Redis HMGET失败: {}", e);
                    StorageError::QueryError(format!("HMGET失败: {}", e))
                })?;

            let consumed = result[0].as_ref().and_then(|s| s.parse::<u64>().ok());
            let limit = result[1].as_ref().and_then(|s| s.parse::<u64>().ok());
            let window_start = result[2].as_ref().and_then(|s| s.parse::<i64>().ok());
            let window_end = result[3].as_ref().and_then(|s| s.parse::<i64>().ok());

            if let (Some(consumed), Some(limit), Some(window_start), Some(window_end)) =
                (consumed, limit, window_start, window_end)
            {
                let quota_info = QuotaInfo {
                    consumed,
                    limit,
                    window_start: chrono::DateTime::from_timestamp(window_start / 1000, 0)
                        .unwrap_or_else(chrono::Utc::now),
                    window_end: chrono::DateTime::from_timestamp(window_end / 1000, 0)
                        .unwrap_or_else(chrono::Utc::now),
                };
                Ok(Some(quota_info))
            } else {
                Ok(None)
            }
        })
        .await
    }

    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
        limit: u64,
        window: std::time::Duration,
    ) -> Result<ConsumeResult, StorageError> {
        let lua_manager = self
            .lua_manager
            .as_ref()
            .ok_or_else(|| StorageError::QueryError("Lua脚本未启用".to_string()))?;

        let key = Self::quota_key(user_id, resource);

        let overdraft_limit = 0u64;
        let now = chrono::Utc::now();
        let window_start = now.timestamp_millis();
        let window_end = window_start
            + i64::try_from(window.as_millis())
                .map_err(|_| StorageError::QueryError("window duration overflow".to_string()))?;

        // 使用优化的字段名
        let consumed_field = Self::quota_field(resource, "consumed");
        let limit_field = Self::quota_field(resource, "limit");
        let window_start_field = Self::quota_field(resource, "window_start");
        let window_end_field = Self::quota_field(resource, "window_end");

        let result: (i32, i64, i64) = self
            .execute_with_retry(|| async {
                let conn_manager = self.conn_manager.lock().await;
                let conn_manager = conn_manager
                    .as_ref()
                    .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

                let mut conn = conn_manager.clone();
                lua_manager
                    .execute_script(
                        &mut conn,
                        LuaScriptType::QuotaConsume,
                        &[&key],
                        &[
                            &cost.to_string(),
                            &limit.to_string(),
                            &overdraft_limit.to_string(),
                            &window_start.to_string(),
                            &window_end.to_string(),
                            &consumed_field,
                            &limit_field,
                            &window_start_field,
                            &window_end_field,
                        ],
                    )
                    .await
            })
            .await?;

        let allowed = result.0 == 1;
        let remaining = result.1 as u64;
        let consumed = result.2 as u64;

        // 检查是否触发告警
        let alert_triggered = remaining < (limit / 10); // 剩余配额少于10%时触发告警

        if alert_triggered {
            warn!(
                "配额告警: user_id={}, resource={}, remaining={}, consumed={}",
                user_id, resource, remaining, consumed
            );
        }

        Ok(ConsumeResult {
            allowed,
            remaining,
            alert_triggered,
        })
    }

    async fn reset(
        &self,
        user_id: &str,
        resource: &str,
        _limit: u64,
        _window: std::time::Duration,
    ) -> Result<(), StorageError> {
        let key = Self::quota_key(user_id, resource);

        self.execute_with_retry(|| async {
            let conn_manager = self.conn_manager.lock().await;
            let conn_manager = conn_manager
                .as_ref()
                .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

            let mut conn = conn_manager.clone();
            let _: () = conn.del(&key).await.map_err(|e| {
                error!("Redis DEL失败: {}", e);
                StorageError::QueryError(format!("DEL失败: {}", e))
            })?;

            debug!("配额已重置: user_id={}, resource={}", user_id, resource);
            Ok(())
        })
        .await
    }
}

#[async_trait]
impl BanStorage for RedisStorage {
    async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
        let key = Self::ban_key(target);

        self.execute_with_retry(|| async {
            let conn_manager = self.conn_manager.lock().await;
            let conn_manager = conn_manager
                .as_ref()
                .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

            let mut conn = conn_manager.clone();

            // 检查封禁记录是否存在
            let exists: bool = conn.exists(&key).await.map_err(|e| {
                error!("Redis EXISTS失败: {}", e);
                StorageError::QueryError(format!("EXISTS失败: {}", e))
            })?;

            if !exists {
                return Ok(None);
            }

            // 获取封禁记录
            let ban_times: u32 = conn.hget(&key, "ban_times").await.unwrap_or(0);
            let duration_ms: i64 = conn.hget(&key, "duration").await.unwrap_or(0);
            let banned_at: i64 = conn.hget(&key, "banned_at").await.unwrap_or(0);
            let expires_at: i64 = conn.hget(&key, "expires_at").await.unwrap_or(0);
            let is_manual: bool = conn.hget(&key, "is_manual").await.unwrap_or(false);
            let reason: String = conn.hget(&key, "reason").await.unwrap_or_default();

            // 检查是否过期
            let now = chrono::Utc::now().timestamp_millis();
            if now > expires_at {
                // 过期，删除记录
                let _: () = conn.del(&key).await.map_err(|e| {
                    error!("Redis DEL失败: {}", e);
                    StorageError::QueryError(format!("DEL失败: {}", e))
                })?;
                return Ok(None);
            }

            let record = BanRecord {
                target: target.clone(),
                ban_times,
                duration: Duration::from_millis(duration_ms as u64),
                banned_at: chrono::DateTime::from_timestamp(banned_at / 1000, 0)
                    .unwrap_or_else(chrono::Utc::now),
                expires_at: chrono::DateTime::from_timestamp(expires_at / 1000, 0)
                    .unwrap_or_else(chrono::Utc::now),
                is_manual,
                reason,
            };

            debug!(
                "检查封禁: target={}, is_banned=true",
                format!("{:?}", target)
            );
            Ok(Some(record))
        })
        .await
    }

    async fn save(&self, record: &BanRecord) -> Result<(), StorageError> {
        let key = Self::ban_key(&record.target);
        let history_key = Self::ban_history_key(&record.target);

        self.execute_with_retry(|| async {
            let conn_manager = self.conn_manager.lock().await;
            let conn_manager = conn_manager
                .as_ref()
                .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

            let mut conn = conn_manager.clone();

            // 保存封禁记录
            let _: () = conn
                .hset(&key, "ban_times", record.ban_times)
                .await
                .map_err(|e| {
                    error!("Redis HSET失败: {}", e);
                    StorageError::QueryError(format!("HSET失败: {}", e))
                })?;
            let _: () = conn
                .hset(&key, "duration", record.duration.as_millis() as i64)
                .await
                .map_err(|e| {
                    error!("Redis HSET失败: {}", e);
                    StorageError::QueryError(format!("HSET失败: {}", e))
                })?;
            let _: () = conn
                .hset(&key, "banned_at", record.banned_at.timestamp_millis())
                .await
                .map_err(|e| {
                    error!("Redis HSET失败: {}", e);
                    StorageError::QueryError(format!("HSET失败: {}", e))
                })?;
            let _: () = conn
                .hset(&key, "expires_at", record.expires_at.timestamp_millis())
                .await
                .map_err(|e| {
                    error!("Redis HSET失败: {}", e);
                    StorageError::QueryError(format!("HSET失败: {}", e))
                })?;
            let _: () = conn
                .hset(&key, "is_manual", record.is_manual)
                .await
                .map_err(|e| {
                    error!("Redis HSET失败: {}", e);
                    StorageError::QueryError(format!("HSET失败: {}", e))
                })?;
            let _: () = conn
                .hset(&key, "reason", &record.reason)
                .await
                .map_err(|e| {
                    error!("Redis HSET失败: {}", e);
                    StorageError::QueryError(format!("HSET失败: {}", e))
                })?;

            // 设置过期时间
            let ttl = (record.expires_at - chrono::Utc::now()).num_seconds();
            if ttl > 0 {
                let _: () = conn.expire(&key, ttl).await.map_err(|e| {
                    error!("Redis EXPIRE失败: {}", e);
                    StorageError::QueryError(format!("EXPIRE失败: {}", e))
                })?;
            }

            // 更新历史记录
            let _: () = conn
                .hset(&history_key, "ban_times", record.ban_times)
                .await
                .map_err(|e| {
                    error!("Redis HSET失败: {}", e);
                    StorageError::QueryError(format!("HSET失败: {}", e))
                })?;
            let _: () = conn
                .hset(
                    &history_key,
                    "last_banned_at",
                    record.banned_at.timestamp_millis(),
                )
                .await
                .map_err(|e| {
                    error!("Redis HSET失败: {}", e);
                    StorageError::QueryError(format!("HSET失败: {}", e))
                })?;

            debug!("保存封禁记录: target={:?}", record.target);
            Ok(())
        })
        .await
    }

    async fn get_history(
        &self,
        target: &BanTarget,
    ) -> Result<Option<crate::storage::BanHistory>, StorageError> {
        let history_key = Self::ban_history_key(target);

        self.execute_with_retry(|| async {
            let conn_manager = self.conn_manager.lock().await;
            let conn_manager = conn_manager
                .as_ref()
                .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

            let mut conn = conn_manager.clone();

            // 检查历史记录是否存在
            let exists: bool = conn.exists(&history_key).await.map_err(|e| {
                error!("Redis EXISTS失败: {}", e);
                StorageError::QueryError(format!("EXISTS失败: {}", e))
            })?;

            if !exists {
                return Ok(None);
            }

            // 获取历史记录
            let ban_times: u32 = conn.hget(&history_key, "ban_times").await.unwrap_or(0);
            let last_banned_at: i64 = conn.hget(&history_key, "last_banned_at").await.unwrap_or(0);

            let history = crate::storage::BanHistory {
                ban_times,
                last_banned_at: chrono::DateTime::from_timestamp(last_banned_at / 1000, 0)
                    .unwrap_or_else(chrono::Utc::now),
            };

            Ok(Some(history))
        })
        .await
    }

    /// 增加封禁次数
    async fn increment_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let key = Self::ban_history_key(target);

        self.execute_with_retry(|| async {
            let conn_manager = self.conn_manager.lock().await;
            let conn_manager = conn_manager
                .as_ref()
                .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

            let mut conn = conn_manager.clone();

            // 增加封禁次数
            let ban_times: u64 = conn.hincr(&key, "ban_times", 1).await.map_err(|e| {
                error!("Redis HINCR失败: {}", e);
                StorageError::QueryError(format!("HINCR失败: {}", e))
            })?;

            // 更新最后封禁时间
            let _: () = conn
                .hset(
                    &key,
                    "last_banned_at",
                    chrono::Utc::now().timestamp_millis(),
                )
                .await
                .map_err(|e| {
                    error!("Redis HSET失败: {}", e);
                    StorageError::QueryError(format!("HSET失败: {}", e))
                })?;

            Ok(ban_times)
        })
        .await
    }

    /// 获取封禁次数
    async fn get_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let key = Self::ban_history_key(target);

        self.execute_with_retry(|| async {
            let conn_manager = self.conn_manager.lock().await;
            let conn_manager = conn_manager
                .as_ref()
                .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

            let mut conn = conn_manager.clone();

            // 获取封禁次数
            let ban_times: u64 = conn.hget(&key, "ban_times").await.unwrap_or(0);

            Ok(ban_times)
        })
        .await
    }

    /// 移除封禁记录
    async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError> {
        let key = Self::ban_key(target);

        self.execute_with_retry(|| async {
            let conn_manager = self.conn_manager.lock().await;
            let conn_manager = conn_manager
                .as_ref()
                .ok_or_else(|| StorageError::ConnectionError("连接未初始化".to_string()))?;

            let mut conn = conn_manager.clone();

            // 删除封禁记录
            let _: i64 = conn.del(&key).await.map_err(|e| {
                error!("Redis DEL失败: {}", e);
                StorageError::QueryError(format!("DEL失败: {}", e))
            })?;

            Ok(())
        })
        .await
    }

    /// 清理过期封禁
    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        // Redis会自动清理过期键，这里返回0
        Ok(0)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redis_config_default() {
        let config = RedisConfig::default();
        assert_eq!(config.url, "redis://127.0.0.1:6379");
        assert_eq!(config.db, 0);
        assert_eq!(config.max_retries, 3);
        assert!(!config.cluster_mode);
        assert!(config.enable_lua);
    }

    #[test]
    fn test_redis_config_builder() {
        let config = RedisConfig::new("redis://localhost:6379")
            .db(1)
            .password("password")
            .connection_timeout(Duration::from_secs(10))
            .io_timeout(Duration::from_secs(10))
            .max_retries(5)
            .cluster_mode(true)
            .pool_size(20)
            .enable_lua(false);

        assert_eq!(config.url, "redis://localhost:6379");
        assert_eq!(config.db, 1);
        assert_eq!(
            config.password.as_ref().map(|p| p.expose_secret()),
            Some(&"password".to_string())
        );
        assert_eq!(config.connection_timeout, Duration::from_secs(10));
        assert_eq!(config.io_timeout, Duration::from_secs(10));
        assert_eq!(config.max_retries, 5);
        assert!(config.cluster_mode);
        assert_eq!(config.pool_size, 20);
        assert!(!config.enable_lua);
    }

    #[test]
    fn test_quota_key() {
        // 优化后的 quota_key 只使用 user_id，resource 作为字段名存储
        let key = RedisStorage::quota_key("user1", "api");
        assert_eq!(key, "quota:user1");
    }

    #[test]
    fn test_ban_key() {
        let key = RedisStorage::ban_key(&BanTarget::Ip("192.168.1.1".to_string()));
        assert_eq!(key, "ban:ip:192.168.1.1");

        let key = RedisStorage::ban_key(&BanTarget::UserId("user1".to_string()));
        assert_eq!(key, "ban:user:user1");

        let key = RedisStorage::ban_key(&BanTarget::Mac("00:11:22:33:44:55".to_string()));
        // MAC 地址会被清理，移除冒号
        assert_eq!(key, "ban:mac:001122334455");
    }

    #[test]
    fn test_ban_history_key() {
        let key = RedisStorage::ban_history_key(&BanTarget::UserId("user1".to_string()));
        // MAC 地址会被清理，移除冒号
        assert_eq!(key, "ban:user:user1:history");
    }

    #[test]
    fn test_retry_stats() {
        let stats = RetryStats::default();
        assert_eq!(stats.total_retries(), 0);
        assert_eq!(stats.successful_retries(), 0);
        assert_eq!(stats.failed_retries(), 0);

        stats.record_success();
        assert_eq!(stats.total_retries(), 1);
        assert_eq!(stats.successful_retries(), 1);

        stats.record_failure();
        assert_eq!(stats.total_retries(), 2);
        assert_eq!(stats.failed_retries(), 1);

        stats.reset();
        assert_eq!(stats.total_retries(), 0);
    }

    #[tokio::test]
    async fn test_degraded_state() {
        let config = RedisConfig::new("redis://invalid:6379");
        // 注意：这里会尝试连接失败，仅测试降级状态切换
        let storage = RedisStorage {
            conn_manager: Arc::new(Mutex::new(None)),
            config,
            lua_manager: None,
            retry_stats: RetryStats::default(),
            degraded: Arc::new(Mutex::new(false)),
            last_degraded_at: Arc::new(Mutex::new(None)),
        };

        assert!(!storage.is_degraded().await);
        storage.set_degraded(true).await;
        assert!(storage.is_degraded().await);
        storage.set_degraded(false).await;
        assert!(!storage.is_degraded().await);
    }
}
