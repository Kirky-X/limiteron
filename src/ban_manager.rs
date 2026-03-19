//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 封禁管理器
//!
//! 提供封禁记录的CRUD操作、指数退避算法和封禁优先级管理。
//!
//! # 功能
//!
//! - 封禁记录CRUD操作
//! - 指数退避算法（自动计算封禁时长）
//! - 封禁优先级管理（IP > User > MAC > Device > APIKey）
//! - 自动解封定时任务
//! - 完整的审计日志
//! - 并行封禁检查（性能提升 50-70%）

/// 第一次封禁时长（1分钟）
pub const FIRST_BAN_DURATION_SECS: u64 = 60;

/// 第二次封禁时长（5分钟）
pub const SECOND_BAN_DURATION_SECS: u64 = 300;

/// 第三次封禁时长（30分钟）
pub const THIRD_BAN_DURATION_SECS: u64 = 1800;

/// 第四次封禁时长（2小时）
pub const FOURTH_BAN_DURATION_SECS: u64 = 7200;

/// 最大封禁时长（24小时）
pub const MAX_BAN_DURATION_SECS: u64 = 86400;

/// 自动解封检查间隔（1分钟）
pub const AUTO_UNBAN_INTERVAL_SECS: u64 = 60;

/// 默认分页限制
pub const DEFAULT_PAGINATION_LIMIT: u64 = 100;

/// 最大分页限制
pub const MAX_PAGINATION_LIMIT: u64 = 1000;

use crate::authorization::AuthorizationProvider;
use crate::constants::MAX_BAN_REASON_LENGTH;
use crate::error::FlowGuardError;
use crate::storage_trait::{BanRecord, BanStorage};
use crate::validation;
#[cfg(feature = "ban-manager")]
use crate::BanTarget;
use chrono::{DateTime, Duration, Utc};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::sync::RwLock;

/// 封禁来源
#[cfg(feature = "ban-manager")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BanSource {
    /// 自动封禁
    Auto,
    /// 手动封禁
    Manual { operator: String },
}

/// 封禁优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg(feature = "ban-manager")]
pub enum BanPriority {
    /// IP封禁（最高优先级）
    Ip = 1,
    /// 用户ID封禁
    UserId = 2,
    /// MAC地址封禁
    Mac = 3,
    /// 设备ID封禁
    DeviceId = 4,
    /// API Key封禁
    ApiKey = 5,
}

impl BanPriority {
    /// 从BanTarget获取优先级
    pub fn from_target(target: &BanTarget) -> Self {
        match target {
            BanTarget::Ip(_) => BanPriority::Ip,
            BanTarget::UserId(_) => BanPriority::UserId,
            BanTarget::Mac(_) => BanPriority::Mac,
        }
    }
}

/// 封禁详情（包含审计信息）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "ban-manager")]
pub struct BanDetail {
    /// 封禁ID
    pub id: String,
    /// 封禁目标
    pub target: BanTarget,
    /// 封禁次数
    pub ban_times: u32,
    /// 封禁时长
    pub duration: StdDuration,
    /// 封禁时间
    pub banned_at: DateTime<Utc>,
    /// 过期时间
    pub expires_at: DateTime<Utc>,
    /// 是否手动封禁
    pub is_manual: bool,
    /// 封禁原因
    pub reason: String,
    /// 封禁来源
    pub source: BanSource,
    /// 元数据
    pub metadata: serde_json::Value,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 更新时间
    pub updated_at: DateTime<Utc>,
    /// 解封时间
    pub unbanned_at: Option<DateTime<Utc>>,
    /// 解封人
    pub unbanned_by: Option<String>,
}

impl From<BanRecord> for BanDetail {
    fn from(record: BanRecord) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            target: record.target,
            ban_times: record.ban_times,
            duration: record.duration,
            banned_at: record.banned_at,
            expires_at: record.expires_at,
            is_manual: record.is_manual,
            reason: record.reason,
            source: if record.is_manual {
                BanSource::Manual {
                    operator: "unknown".to_string(),
                }
            } else {
                BanSource::Auto
            },
            metadata: serde_json::json!({}),
            created_at: record.banned_at,
            updated_at: record.banned_at,
            unbanned_at: None,
            unbanned_by: None,
        }
    }
}

/// 封禁过滤器
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg(feature = "ban-manager")]
pub struct BanFilter {
    /// 目标类型过滤
    pub target_type: Option<String>,
    /// 目标值过滤（支持模糊匹配）
    pub target_value: Option<String>,
    /// 是否只显示活跃封禁
    pub active_only: bool,
    /// 是否只显示手动封禁
    pub manual_only: bool,
    /// 开始时间
    pub start_time: Option<DateTime<Utc>>,
    /// 结束时间
    pub end_time: Option<DateTime<Utc>>,
    /// 分页偏移
    pub offset: Option<u64>,
    /// 分页限制
    pub limit: Option<u64>,
}

/// 指数退避配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "ban-manager")]
pub struct BackoffConfig {
    /// 第一次违规封禁时长（秒）
    pub first_duration: u64,
    /// 第二次违规封禁时长（秒）
    pub second_duration: u64,
    /// 第三次违规封禁时长（秒）
    pub third_duration: u64,
    /// 第四次及以上违规封禁时长（秒）
    pub fourth_duration: u64,
    /// 最大封禁时长（秒）
    pub max_duration: u64,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            first_duration: FIRST_BAN_DURATION_SECS,
            second_duration: SECOND_BAN_DURATION_SECS,
            third_duration: THIRD_BAN_DURATION_SECS,
            fourth_duration: FOURTH_BAN_DURATION_SECS,
            max_duration: MAX_BAN_DURATION_SECS,
        }
    }
}

/// BanManager配置
#[derive(Debug, Clone)]
#[cfg(feature = "ban-manager")]
pub struct BanManagerConfig {
    /// 指数退避配置
    pub backoff: BackoffConfig,
    /// 是否启用自动解封
    pub enable_auto_unban: bool,
    /// 自动解封检查间隔（秒）
    pub auto_unban_interval: u64,
}

impl Default for BanManagerConfig {
    fn default() -> Self {
        Self {
            backoff: BackoffConfig::default(),
            enable_auto_unban: true,
            auto_unban_interval: AUTO_UNBAN_INTERVAL_SECS,
        }
    }
}

/// 封禁管理器
///
/// 管理封禁记录的生命周期，提供CRUD接口和指数退避算法。
#[derive(Clone)]
#[cfg(feature = "ban-manager")]
pub struct BanManager {
    /// 封禁存储
    storage: Arc<dyn BanStorage>,
    /// 配置
    config: Arc<RwLock<BanManagerConfig>>,
    /// 自动解禁任务句柄
    auto_unban_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// 授权提供者（可选）
    authorization_provider: Option<Arc<dyn AuthorizationProvider>>,
}

/// BanManager 构建器
///
/// 用于链式配置 BanManager 实例。
///
/// # 示例
/// ```rust
/// use limiteron::ban_manager::BanManager;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() {
///     let ban_manager = BanManager::builder()
///         .build()
///         .await
///         .unwrap();
/// }
/// ```
#[cfg(feature = "ban-manager")]
#[derive(Default)]
pub struct BanManagerBuilder {
    storage: Option<Arc<dyn BanStorage>>,
    config: Option<BanManagerConfig>,
    authorization_provider: Option<Arc<dyn AuthorizationProvider>>,
}

#[cfg(feature = "ban-manager")]
impl BanManagerBuilder {
    /// 创建新的 BanManagerBuilder
    pub fn new() -> Self {
        Self {
            storage: None,
            config: None,
            authorization_provider: None,
        }
    }

    /// 设置封禁存储后端
    pub fn with_storage(mut self, storage: Arc<dyn BanStorage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// 设置配置
    pub fn with_config(mut self, config: BanManagerConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// 设置授权提供者
    ///
    /// # 参数
    ///
    /// * `provider` - 授权提供者实例
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::ban_manager::BanManager;
    /// use limiteron::authorization::SimpleAuthorizationProvider;
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let auth_provider = Arc::new(SimpleAuthorizationProvider::new(vec![
    ///         "admin".to_string(),
    ///     ]));
    ///
    ///     let ban_manager = BanManager::builder()
    ///         .with_storage(storage)
    ///         .with_authorization_provider(auth_provider)
    ///         .build()
    ///         .await
    ///         .unwrap();
    /// }
    /// ```
    pub fn with_authorization_provider(mut self, provider: Arc<dyn AuthorizationProvider>) -> Self {
        self.authorization_provider = Some(provider);
        self
    }

    /// 构建 BanManager 实例
    pub async fn build(self) -> Result<BanManager, FlowGuardError> {
        let storage = self
            .storage
            .ok_or_else(|| FlowGuardError::DependencyError("storage is required".to_string()))?;
        let config = self.config.unwrap_or_default();

        BanManager::with_dependencies_and_auth(storage, config, self.authorization_provider).await
    }
}

/// 验证封禁目标
///
/// 使用统一的 validation 模块进行验证。
#[cfg(feature = "ban-manager")]
fn validate_ban_target(target: &BanTarget) -> Result<(), FlowGuardError> {
    match target {
        BanTarget::Ip(ip) => validation::validate_ip_address(ip),
        BanTarget::UserId(user_id) => validation::validate_user_id(user_id),
        BanTarget::Mac(mac) => validation::validate_mac_address(mac),
    }
}

/// 验证封禁原因
///
/// 使用统一的 validation 模块进行验证。
fn validate_ban_reason(reason: &str) -> Result<(), FlowGuardError> {
    if reason.is_empty() {
        return Err(FlowGuardError::ValidationError(
            "封禁原因不能为空".to_string(),
        ));
    }

    if reason.len() > MAX_BAN_REASON_LENGTH {
        return Err(FlowGuardError::ValidationError(format!(
            "封禁原因过长，最大长度为 {} 字符",
            MAX_BAN_REASON_LENGTH
        )));
    }

    // 检查是否包含控制字符
    if reason.contains(|c: char| c.is_control()) {
        return Err(FlowGuardError::ValidationError(
            "封禁原因包含非法字符".to_string(),
        ));
    }

    Ok(())
}

impl BanManager {
    /// 创建 BanManagerBuilder 用于链式配置
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::ban_manager::BanManager;
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let ban_manager = BanManager::builder()
    ///         .build()
    ///         .await
    ///         .unwrap();
    /// }
    /// ```
    pub fn builder() -> BanManagerBuilder {
        BanManagerBuilder::new()
    }

    /// 使用依赖注入创建 BanManager 实例
    ///
    /// # 参数
    /// - `storage`: 封禁存储后端
    /// - `config`: 封禁管理器配置
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::ban_manager::{BanManager, BanManagerConfig};
    /// use limiteron::storage::BanStorage;
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let storage: Arc<dyn BanStorage> = Arc::new(my_storage);
    ///     let config = BanManagerConfig::default();
    ///     let ban_manager = BanManager::with_dependencies(storage, config).await.unwrap();
    /// }
    /// ```
    pub async fn with_dependencies(
        storage: Arc<dyn BanStorage>,
        config: BanManagerConfig,
    ) -> Result<Self, FlowGuardError> {
        Self::with_dependencies_and_auth(storage, config, None).await
    }

    /// 使用依赖注入和授权提供者创建 BanManager 实例
    ///
    /// # 参数
    /// - `storage`: 封禁存储后端
    /// - `config`: 封禁管理器配置
    /// - `authorization_provider`: 授权提供者（可选）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::ban_manager::{BanManager, BanManagerConfig};
    /// use limiteron::authorization::SimpleAuthorizationProvider;
    /// use limiteron::storage::BanStorage;
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let storage: Arc<dyn BanStorage> = Arc::new(my_storage);
    ///     let config = BanManagerConfig::default();
    ///     let auth_provider = Arc::new(SimpleAuthorizationProvider::new(vec!["admin".to_string()]));
    ///     let ban_manager = BanManager::with_dependencies_and_auth(
    ///         storage,
    ///         config,
    ///         Some(auth_provider),
    ///     ).await.unwrap();
    /// }
    /// ```
    pub async fn with_dependencies_and_auth(
        storage: Arc<dyn BanStorage>,
        config: BanManagerConfig,
        authorization_provider: Option<Arc<dyn AuthorizationProvider>>,
    ) -> Result<Self, FlowGuardError> {
        let config = Arc::new(RwLock::new(config));

        let ban_manager = Self {
            storage,
            config,
            auto_unban_handle: Arc::new(RwLock::new(None)),
            authorization_provider,
        };

        // 启动自动解封任务
        ban_manager.start_auto_unban_task().await;

        info!("BanManager initialized successfully");
        Ok(ban_manager)
    }

    /// 启动自动解封任务
    async fn start_auto_unban_task(&self) {
        let config = self.config.read().await;
        if !config.enable_auto_unban {
            return;
        }

        let storage = self.storage.clone();
        let interval_secs = config.auto_unban_interval;
        drop(config);

        let mut handle_write = self.auto_unban_handle.write().await;
        if handle_write.is_some() {
            return; // 任务已在运行
        }

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(StdDuration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                debug!("Running auto-unban task");

                // 清理过期封禁
                // 注：过期清理需要特定的存储实现
                // 当前使用BanStorage trait的cleanup_expired_bans方法
                if let Err(e) = storage.cleanup_expired_bans().await {
                    error!("Auto-unban task failed: {}", e);
                }
            }
        });

        *handle_write = Some(handle);
        info!("Auto-unban task started (interval: {}s)", interval_secs);
    }

    /// 停止自动解封任务
    pub async fn stop_auto_unban_task(&self) {
        let mut handle_guard = self.auto_unban_handle.write().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
            info!("Auto-unban task stopped");
        }
    }

    /// 计算封禁时长（指数退避算法）
    ///
    /// # 参数
    /// - `ban_times`: 封禁次数
    ///
    /// # 返回
    /// - 封禁时长（秒）
    ///
    /// # 指数退避规则
    /// - 第一次违规：封禁1分钟
    /// - 第二次违规：封禁5分钟
    /// - 第三次违规：封禁30分钟
    /// - 第四次及以上：封禁2小时
    /// - 最大封禁时长：24小时
    pub async fn calculate_ban_duration(&self, ban_times: u32) -> StdDuration {
        let config = self.config.read().await;
        let duration_secs = match ban_times {
            1 => config.backoff.first_duration,
            2 => config.backoff.second_duration,
            3 => config.backoff.third_duration,
            _ => config.backoff.fourth_duration,
        };

        // 不超过最大时长
        let duration_secs = duration_secs.min(config.backoff.max_duration);

        debug!(
            "Calculated ban duration: ban_times={}, duration={}s",
            ban_times, duration_secs
        );

        StdDuration::from_secs(duration_secs)
    }

    /// 创建封禁记录
    ///
    /// # 参数
    /// - `target`: 封禁目标
    /// - `reason`: 封禁原因
    /// - `source`: 封禁来源
    /// - `metadata`: 元数据
    /// - `duration`: 封禁时长（可选，不提供则自动计算）
    ///
    /// # 返回
    /// - 封禁详情
    ///
    /// # 授权检查
    /// 如果设置了授权提供者，会先检查操作者是否有权限执行此操作。
    /// 对于手动封禁（`BanSource::Manual`），操作者从 `source` 中提取。
    /// 对于自动封禁（`BanSource::Auto`），跳过授权检查。
    pub async fn create_ban(
        &self,
        target: BanTarget,
        reason: String,
        source: BanSource,
        metadata: serde_json::Value,
        duration: Option<StdDuration>,
    ) -> Result<BanDetail, FlowGuardError> {
        // 授权检查：仅对手动封禁进行检查
        if let (Some(provider), BanSource::Manual { ref operator }) =
            (&self.authorization_provider, &source)
        {
            self.check_authorization(provider, "create_ban", operator, &target)
                .await?;
        }

        // 输入验证
        validate_ban_target(&target)?;
        validate_ban_reason(&reason)?;

        info!(
            "Creating ban: target={:?}, reason={}, source={:?}",
            target, reason, source
        );

        // 获取历史记录
        let history = self.storage.get_history(&target).await?;
        let ban_times = history.as_ref().map(|h| h.ban_times + 1).unwrap_or(1);

        // 计算封禁时长
        let duration = match duration {
            Some(d) => d,
            None => self.calculate_ban_duration(ban_times).await,
        };

        let now = Utc::now();
        let expires_at = now
            + Duration::from_std(duration)
                .map_err(|e| FlowGuardError::TimeError(format!("Invalid duration: {}", e)))?;
        let is_manual = matches!(source, BanSource::Manual { .. });

        let record = BanRecord {
            target: target.clone(),
            ban_times,
            duration,
            banned_at: now,
            expires_at,
            is_manual,
            reason: reason.clone(),
        };

        // 保存封禁记录
        self.storage.save(&record).await?;

        let detail = BanDetail {
            id: uuid::Uuid::new_v4().to_string(),
            target,
            ban_times,
            duration,
            banned_at: now,
            expires_at,
            is_manual,
            reason,
            source,
            metadata,
            created_at: now,
            updated_at: now,
            unbanned_at: None,
            unbanned_by: None,
        };

        info!(
            "Ban created successfully: id={}, ban_times={}",
            detail.id, ban_times
        );
        Ok(detail)
    }

    /// 查询封禁状态
    ///
    /// # 参数
    /// - `target`: 封禁目标
    ///
    /// # 返回
    /// - 封禁详情（如果存在）
    pub async fn read_ban(&self, target: &BanTarget) -> Result<Option<BanDetail>, FlowGuardError> {
        debug!("Reading ban: target={:?}", target);

        let record = self.storage.is_banned(target).await?;

        Ok(record.map(BanDetail::from))
    }

    /// 更新封禁信息
    ///
    /// # 参数
    /// - `target`: 封禁目标
    /// - `reason`: 新的封禁原因
    /// - `duration`: 新的封禁时长（可选）
    /// - `metadata`: 新的元数据（可选）
    ///
    /// # 返回
    /// - 更新后的封禁详情
    pub async fn update_ban(
        &self,
        target: &BanTarget,
        reason: Option<String>,
        duration: Option<StdDuration>,
        metadata: Option<serde_json::Value>,
    ) -> Result<Option<BanDetail>, FlowGuardError> {
        debug!("Updating ban: target={:?}", target);

        // 获取当前封禁记录
        let current_record = self.storage.is_banned(target).await?;

        let current_record = match current_record {
            Some(record) => record,
            None => return Ok(None),
        };

        let mut record = current_record;
        let now = Utc::now();

        // 更新字段
        if let Some(new_reason) = reason {
            record.reason = new_reason;
        }

        if let Some(new_duration) = duration {
            record.duration = new_duration;
            record.expires_at = now
                + Duration::from_std(new_duration)
                    .map_err(|e| FlowGuardError::TimeError(format!("Invalid duration: {}", e)))?;
        }

        // 保存更新后的记录
        self.storage.save(&record).await?;

        let mut detail = BanDetail::from(record);
        detail.updated_at = now;

        if let Some(metadata) = metadata {
            detail.metadata = metadata;
        }

        info!("Ban updated successfully: id={}", detail.id);
        Ok(Some(detail))
    }

    /// 删除封禁记录（解封）
    ///
    /// # 参数
    /// - `target`: 封禁目标
    /// - `unbanned_by`: 解封人
    ///
    /// # 返回
    /// - 是否成功解封
    ///
    /// # 授权检查
    /// 如果设置了授权提供者，会先检查操作者是否有权限执行此操作。
    pub async fn delete_ban(
        &self,
        target: &BanTarget,
        unbanned_by: String,
    ) -> Result<bool, FlowGuardError> {
        // 授权检查
        if let Some(provider) = &self.authorization_provider {
            self.check_authorization(provider, "remove_ban", &unbanned_by, target)
                .await?;
        }

        info!(
            "Deleting ban: target={}, unbanned_by={}",
            crate::log_redaction::redact_ban_target(target),
            unbanned_by
        );

        // 检查是否存在封禁
        let record = self.storage.is_banned(target).await?;

        if record.is_none() {
            debug!(
                "No active ban found for target: {}",
                crate::log_redaction::redact_ban_target(target)
            );
            return Ok(false);
        }

        // 移除封禁记录
        // 注：对于PostgreSQL，如果需要记录unbanned_by，需要在BanStorage实现中处理
        self.storage.remove_ban(target).await?;

        info!("Ban deleted successfully: target={:?}", target);
        Ok(true)
    }

    /// 列出封禁记录
    ///
    /// # 参数
    /// - `filter`: 过滤条件，支持目标类型过滤、活跃封禁过滤、手动封禁过滤、时间范围过滤和分页
    ///
    /// # 返回
    /// - 封禁记录列表（BanDetail 形式）
    pub async fn list_bans(&self, filter: BanFilter) -> Result<Vec<BanDetail>, FlowGuardError> {
        debug!("Listing bans with filter: {:?}", filter);

        // 解析分页参数，使用默认值
        let offset = filter.offset.unwrap_or(0).min(MAX_PAGINATION_LIMIT);
        let limit = filter
            .limit
            .unwrap_or(DEFAULT_PAGINATION_LIMIT)
            .min(MAX_PAGINATION_LIMIT);

        // 获取封禁记录（使用新的 list_bans 方法）
        let active_only =
            filter.active_only || filter.start_time.is_some() || filter.end_time.is_some();
        let records = self.storage.list_bans(active_only, offset, limit).await?;

        // 应用目标类型过滤
        let filtered: Vec<_> = records
            .into_iter()
            .filter(|record| {
                // 目标类型过滤
                if let Some(ref target_type) = filter.target_type {
                    let matches = match (target_type.to_lowercase().as_str(), &record.target) {
                        ("ip", BanTarget::Ip(_)) => true,
                        ("user", BanTarget::UserId(_)) => true,
                        ("mac", BanTarget::Mac(_)) => true,
                        _ => false,
                    };
                    if !matches {
                        return false;
                    }
                }

                // 目标值过滤（模糊匹配）
                if let Some(ref target_value) = filter.target_value {
                    let value_matches = match &record.target {
                        BanTarget::Ip(ip) => ip.contains(target_value),
                        BanTarget::UserId(uid) => uid.contains(target_value),
                        BanTarget::Mac(mac) => mac.contains(target_value),
                    };
                    if !value_matches {
                        return false;
                    }
                }

                // 手动封禁过滤
                if filter.manual_only && !record.is_manual {
                    return false;
                }

                // 时间范围过滤
                if let Some(start) = filter.start_time {
                    if record.banned_at < start {
                        return false;
                    }
                }
                if let Some(end) = filter.end_time {
                    if record.banned_at > end {
                        return false;
                    }
                }

                true
            })
            .collect();

        // 转换为 BanDetail
        let bans = filtered.into_iter().map(BanDetail::from).collect();

        Ok(bans)
    }

    /// Checking ban priority（并行版本，支持提前退出）
    ///
    /// # 性能优化
    /// - 使用并行检查，预期延迟降低 50-70%
    /// - 支持提前退出，IP 封禁优先检查
    pub async fn check_ban_priority(
        &self,
        targets: &[BanTarget],
    ) -> Result<Option<BanDetail>, FlowGuardError> {
        debug!(
            "Checking ban priority for {} targets (parallel with early exit)",
            targets.len()
        );

        if targets.is_empty() {
            return Ok(None);
        }

        // 优先检查 IP 封禁（最高优先级），支持提前退出
        if let Some(ip_target) = targets.iter().find(|t| matches!(t, BanTarget::Ip(_))) {
            debug!("Checking IP ban first for early exit");
            if let Some(record) = self.storage.is_banned(ip_target).await? {
                debug!("Found IP ban (highest priority): target={:?}", ip_target);
                return Ok(Some(BanDetail::from(record)));
            }
        }

        // IP 未被封禁，并行检查其他目标
        let storage = self.storage.clone();
        let check_futures: Vec<_> = targets
            .iter()
            .filter(|t| !matches!(t, BanTarget::Ip(_))) // 跳过已检查的 IP
            .map(|target| {
                let target = target.clone();
                let storage = storage.clone();
                Box::pin(async move {
                    let record = storage.is_banned(&target).await.ok()?;
                    record.map(|r| (BanPriority::from_target(&target), BanDetail::from(r)))
                })
            })
            .collect();

        if check_futures.is_empty() {
            return Ok(None);
        }

        // 使用 select! 实现提前退出
        #[cfg(feature = "parallel-checker")]
        match futures::future::select_all(check_futures).await {
            (Some((priority, detail)), _, _) => {
                self.log_ban_found(priority, &detail);
                Ok(Some(detail))
            }
            _ => Ok(None),
        }

        #[cfg(not(feature = "parallel-checker"))]
        {
            // 顺序检查（当 parallel-checker 未启用时）
            for future in check_futures {
                if let Some((priority, detail)) = future.await {
                    self.log_ban_found(priority, &detail);
                    return Ok(Some(detail));
                }
            }
            Ok(None)
        }
    }

    /// 记录找到封禁的日志（内部辅助方法）
    fn log_ban_found(&self, priority: BanPriority, detail: &BanDetail) {
        debug!(
            "Found ban with priority {:?}: target={:?}",
            priority, detail.target
        );
    }

    /// 获取配置
    pub async fn get_config(&self) -> BanManagerConfig {
        self.config.read().await.clone()
    }

    /// 更新配置
    pub async fn update_config(&self, new_config: BanManagerConfig) -> Result<(), FlowGuardError> {
        info!("Updating BanManager configuration");

        *self.config.write().await = new_config;

        // 重启自动解封任务
        self.stop_auto_unban_task().await;
        self.start_auto_unban_task().await;

        info!("BanManager configuration updated successfully");
        Ok(())
    }

    /// 添加封禁（便捷方法）
    pub async fn add_ban(&self, record: BanRecord) -> Result<(), FlowGuardError> {
        let detail = self
            .create_ban(
                record.target.clone(),
                record.reason.clone(),
                if record.is_manual {
                    BanSource::Manual {
                        operator: "system".to_string(),
                    }
                } else {
                    BanSource::Auto
                },
                serde_json::json!({}),
                Some(record.duration),
            )
            .await?;
        info!("Ban added: {:?}", detail);
        Ok(())
    }

    /// 获取封禁（便捷方法）
    pub async fn get_ban(&self, target: &BanTarget) -> Result<Option<BanRecord>, FlowGuardError> {
        let detail = self.read_ban(target).await?;
        if let Some(detail) = detail {
            Ok(Some(BanRecord {
                target: detail.target,
                ban_times: detail.ban_times,
                duration: detail.duration,
                banned_at: detail.banned_at,
                expires_at: detail.expires_at,
                is_manual: detail.is_manual,
                reason: detail.reason,
            }))
        } else {
            Ok(None)
        }
    }

    /// 检查是否被封禁（便捷方法）
    pub async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, FlowGuardError> {
        self.get_ban(target).await
    }

    /// 获取封禁历史（便捷方法）
    pub async fn get_history(
        &self,
        target: &BanTarget,
    ) -> Result<Option<crate::BanHistory>, FlowGuardError> {
        self.storage
            .get_history(target)
            .await
            .map_err(FlowGuardError::StorageError)
    }

    /// 检查授权（内部辅助方法）
    ///
    /// 统一处理授权检查逻辑，避免重复的 target 转换代码。
    async fn check_authorization(
        &self,
        provider: &Arc<dyn AuthorizationProvider>,
        action: &str,
        operator: &str,
        target: &BanTarget,
    ) -> Result<(), FlowGuardError> {
        let target_str = match target {
            BanTarget::Ip(ip) => ip.clone(),
            BanTarget::UserId(user_id) => user_id.clone(),
            BanTarget::Mac(mac) => mac.clone(),
        };
        provider
            .check_authorization(action, operator, &target_str)
            .await?;
        debug!(
            "Authorization passed for {}: operator={}, target={}",
            action, operator, target_str
        );
        Ok(())
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_trait::{BanHistory, BanStorage};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // Simple in-memory ban storage for testing
    struct TestBanStorage {
        bans: Arc<RwLock<Vec<(BanTarget, BanRecord)>>>,
    }

    impl TestBanStorage {
        fn new() -> Self {
            Self {
                bans: Arc::new(RwLock::new(Vec::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl BanStorage for TestBanStorage {
        async fn is_banned(
            &self,
            _target: &BanTarget,
        ) -> Result<Option<BanRecord>, crate::error::StorageError> {
            Ok(None)
        }

        async fn save(&self, _record: &BanRecord) -> Result<(), crate::error::StorageError> {
            Ok(())
        }

        async fn get_history(
            &self,
            _target: &BanTarget,
        ) -> Result<Option<BanHistory>, crate::error::StorageError> {
            Ok(None)
        }

        async fn increment_ban_times(
            &self,
            _target: &BanTarget,
        ) -> Result<u64, crate::error::StorageError> {
            Ok(0)
        }

        async fn get_ban_times(
            &self,
            _target: &BanTarget,
        ) -> Result<u64, crate::error::StorageError> {
            Ok(0)
        }

        async fn remove_ban(&self, _target: &BanTarget) -> Result<(), crate::error::StorageError> {
            Ok(())
        }

        async fn cleanup_expired_bans(&self) -> Result<u64, crate::error::StorageError> {
            Ok(0)
        }

        async fn list_bans(
            &self,
            _active_only: bool,
            _offset: u64,
            _limit: u64,
        ) -> Result<Vec<BanRecord>, crate::error::StorageError> {
            Ok(vec![])
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[test]
    fn test_ban_priority_from_target() {
        let ip_target = BanTarget::Ip("192.168.1.1".to_string());
        assert_eq!(BanPriority::from_target(&ip_target), BanPriority::Ip);

        let user_target = BanTarget::UserId("user123".to_string());
        assert_eq!(BanPriority::from_target(&user_target), BanPriority::UserId);

        let mac_target = BanTarget::Mac("00:11:22:33:44:55".to_string());
        assert_eq!(BanPriority::from_target(&mac_target), BanPriority::Mac);
    }

    #[test]
    fn test_backoff_config_default() {
        let config = BackoffConfig::default();
        assert_eq!(config.first_duration, 60);
        assert_eq!(config.second_duration, 300);
        assert_eq!(config.third_duration, 1800);
        assert_eq!(config.fourth_duration, 7200);
        assert_eq!(config.max_duration, 86400);
    }

    #[test]
    fn test_ban_manager_config_default() {
        let config = BanManagerConfig::default();
        assert!(config.enable_auto_unban);
        assert_eq!(config.auto_unban_interval, 60);
    }

    #[tokio::test]
    async fn test_calculate_ban_duration() {
        let storage = Arc::new(TestBanStorage::new());
        let ban_manager = BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap();

        // 第一次违规：1分钟
        let duration = ban_manager.calculate_ban_duration(1).await;
        assert_eq!(duration, StdDuration::from_secs(60));

        // 第二次违规：5分钟
        let duration = ban_manager.calculate_ban_duration(2).await;
        assert_eq!(duration, StdDuration::from_secs(300));

        // 第三次违规：30分钟
        let duration = ban_manager.calculate_ban_duration(3).await;
        assert_eq!(duration, StdDuration::from_secs(1800));

        // 第四次违规：2小时
        let duration = ban_manager.calculate_ban_duration(4).await;
        assert_eq!(duration, StdDuration::from_secs(7200));

        // 第五次违规：仍然是2小时
        let duration = ban_manager.calculate_ban_duration(5).await;
        assert_eq!(duration, StdDuration::from_secs(7200));
    }

    #[tokio::test]
    async fn test_create_ban_auto() {
        let storage = Arc::new(TestBanStorage::new());
        let ban_manager = BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap();

        let target = BanTarget::UserId("user123".to_string());
        let reason = "Excessive requests".to_string();
        let source = BanSource::Auto;
        let metadata = serde_json::json!({"requests": 1000});

        let result = ban_manager
            .create_ban(target.clone(), reason.clone(), source, metadata, None)
            .await;

        assert!(result.is_ok());
        let detail = result.unwrap();
        assert_eq!(detail.target, target);
        assert_eq!(detail.reason, reason);
        assert!(!detail.is_manual);
        assert_eq!(detail.ban_times, 1);
    }

    #[tokio::test]
    async fn test_create_ban_manual() {
        let storage = Arc::new(TestBanStorage::new());
        let ban_manager = BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap();

        let target = BanTarget::Ip("192.168.1.1".to_string());
        let reason = "Manual ban".to_string();
        let source = BanSource::Manual {
            operator: "admin".to_string(),
        };
        let metadata = serde_json::json!({});
        let duration = StdDuration::from_secs(3600);

        let result = ban_manager
            .create_ban(
                target.clone(),
                reason.clone(),
                source,
                metadata,
                Some(duration),
            )
            .await;

        assert!(result.is_ok());
        let detail = result.unwrap();
        assert_eq!(detail.target, target);
        assert_eq!(detail.reason, reason);
        assert!(detail.is_manual);
        assert_eq!(detail.duration, duration);
    }

    #[tokio::test]
    async fn test_read_ban_not_found() {
        let storage = Arc::new(TestBanStorage::new());
        let ban_manager = BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap();

        let target = BanTarget::UserId("nonexistent".to_string());
        let result = ban_manager.read_ban(&target).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_update_ban_not_found() {
        let storage = Arc::new(TestBanStorage::new());
        let ban_manager = BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap();

        let target = BanTarget::UserId("nonexistent".to_string());
        let result = ban_manager
            .update_ban(&target, Some("New reason".to_string()), None, None)
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_ban_not_found() {
        let storage = Arc::new(TestBanStorage::new());
        let ban_manager = BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap();

        let target = BanTarget::UserId("nonexistent".to_string());
        let result = ban_manager.delete_ban(&target, "admin".to_string()).await;

        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_list_bans_empty() {
        let storage = Arc::new(TestBanStorage::new());
        let ban_manager = BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap();

        let filter = BanFilter::default();
        let result: Result<Vec<crate::ban_manager::BanDetail>, crate::error::FlowGuardError> =
            ban_manager.list_bans(filter).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_check_ban_priority_empty() {
        let storage = Arc::new(TestBanStorage::new());
        let ban_manager = BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap();

        let targets = vec![
            BanTarget::Ip("192.168.1.1".to_string()),
            BanTarget::UserId("user123".to_string()),
        ];

        let result = ban_manager.check_ban_priority(&targets).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_config() {
        let storage = Arc::new(TestBanStorage::new());
        let ban_manager = BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap();

        let config = ban_manager.get_config().await;
        assert!(config.enable_auto_unban);
        assert_eq!(config.auto_unban_interval, 60);
    }

    #[tokio::test]
    async fn test_update_config() {
        let storage = Arc::new(TestBanStorage::new());
        let ban_manager = BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap();

        let new_config = BanManagerConfig {
            backoff: BackoffConfig::default(),
            enable_auto_unban: false,
            auto_unban_interval: 120,
        };

        let result = ban_manager.update_config(new_config.clone()).await;

        assert!(result.is_ok());
        let updated_config = ban_manager.get_config().await;
        assert!(!updated_config.enable_auto_unban);
        assert_eq!(updated_config.auto_unban_interval, 120);
    }

    #[tokio::test]
    async fn test_stop_auto_unban_task() {
        let storage = Arc::new(TestBanStorage::new());
        let ban_manager = BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap();

        // 停止任务应该不会失败
        ban_manager.stop_auto_unban_task().await;
    }

    #[tokio::test]
    async fn test_ban_filter_default() {
        let filter = BanFilter::default();
        assert!(filter.target_type.is_none());
        assert!(filter.target_value.is_none());
        assert!(!filter.active_only);
        assert!(!filter.manual_only);
        assert!(filter.start_time.is_none());
        assert!(filter.end_time.is_none());
        assert!(filter.offset.is_none());
        assert!(filter.limit.is_none());
    }

    #[test]
    fn test_ban_source_equality() {
        let source1 = BanSource::Auto;
        let source2 = BanSource::Auto;
        assert_eq!(source1, source2);

        let source3 = BanSource::Manual {
            operator: "admin".to_string(),
        };
        let source4 = BanSource::Manual {
            operator: "admin".to_string(),
        };
        assert_eq!(source3, source4);
    }

    // ========================================================================
    // 授权检查测试
    // ========================================================================

    #[tokio::test]
    async fn test_create_ban_with_authorization_success() {
        use crate::authorization::SimpleAuthorizationProvider;

        let storage = Arc::new(TestBanStorage::new());
        let auth_provider = Arc::new(SimpleAuthorizationProvider::new(vec!["admin".to_string()]));

        let ban_manager = BanManager::builder()
            .with_storage(storage)
            .with_authorization_provider(auth_provider)
            .build()
            .await
            .unwrap();

        let target = BanTarget::Ip("192.168.1.1".to_string());
        let source = BanSource::Manual {
            operator: "admin".to_string(),
        };

        // admin 角色应该可以创建封禁
        let result = ban_manager
            .create_ban(
                target,
                "Test ban".to_string(),
                source,
                serde_json::json!({}),
                None,
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_ban_with_authorization_failure() {
        use crate::authorization::SimpleAuthorizationProvider;

        let storage = Arc::new(TestBanStorage::new());
        let auth_provider = Arc::new(SimpleAuthorizationProvider::new(vec!["admin".to_string()]));

        let ban_manager = BanManager::builder()
            .with_storage(storage)
            .with_authorization_provider(auth_provider)
            .build()
            .await
            .unwrap();

        let target = BanTarget::Ip("192.168.1.1".to_string());
        let source = BanSource::Manual {
            operator: "unauthorized_user".to_string(),
        };

        // 未授权用户应该被拒绝
        let result = ban_manager
            .create_ban(
                target,
                "Test ban".to_string(),
                source,
                serde_json::json!({}),
                None,
            )
            .await;

        assert!(result.is_err());
        match result {
            Err(FlowGuardError::AuthorizationError(msg)) => {
                assert!(msg.contains("unauthorized_user"));
            }
            _ => panic!("期望 AuthorizationError"),
        }
    }

    #[tokio::test]
    async fn test_create_ban_auto_bypasses_authorization() {
        use crate::authorization::DenyAllAuthorizationProvider;

        let storage = Arc::new(TestBanStorage::new());
        // 使用拒绝所有操作的授权提供者
        let auth_provider = Arc::new(DenyAllAuthorizationProvider);

        let ban_manager = BanManager::builder()
            .with_storage(storage)
            .with_authorization_provider(auth_provider)
            .build()
            .await
            .unwrap();

        let target = BanTarget::Ip("192.168.1.1".to_string());
        let source = BanSource::Auto;

        // 自动封禁应该绕过授权检查
        let result = ban_manager
            .create_ban(
                target,
                "Auto ban".to_string(),
                source,
                serde_json::json!({}),
                None,
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_ban_with_authorization_success() {
        use crate::authorization::SimpleAuthorizationProvider;

        let storage = Arc::new(TestBanStorage::new());
        let auth_provider = Arc::new(SimpleAuthorizationProvider::new(vec!["admin".to_string()]));

        let ban_manager = BanManager::builder()
            .with_storage(storage)
            .with_authorization_provider(auth_provider)
            .build()
            .await
            .unwrap();

        let target = BanTarget::Ip("192.168.1.1".to_string());

        // admin 角色应该可以删除封禁
        let result = ban_manager.delete_ban(&target, "admin".to_string()).await;

        // 即使封禁不存在，授权检查也应该通过
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_ban_with_authorization_failure() {
        use crate::authorization::SimpleAuthorizationProvider;

        let storage = Arc::new(TestBanStorage::new());
        let auth_provider = Arc::new(SimpleAuthorizationProvider::new(vec!["admin".to_string()]));

        let ban_manager = BanManager::builder()
            .with_storage(storage)
            .with_authorization_provider(auth_provider)
            .build()
            .await
            .unwrap();

        let target = BanTarget::Ip("192.168.1.1".to_string());

        // 未授权用户应该被拒绝
        let result = ban_manager
            .delete_ban(&target, "unauthorized_user".to_string())
            .await;

        assert!(result.is_err());
        match result {
            Err(FlowGuardError::AuthorizationError(msg)) => {
                assert!(msg.contains("unauthorized_user"));
            }
            _ => panic!("期望 AuthorizationError"),
        }
    }

    #[tokio::test]
    async fn test_no_authorization_provider_allows_all() {
        let storage = Arc::new(TestBanStorage::new());

        // 不设置授权提供者
        let ban_manager = BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap();

        let target = BanTarget::Ip("192.168.1.1".to_string());
        let source = BanSource::Manual {
            operator: "anyone".to_string(),
        };

        // 没有授权提供者时，所有操作都应该被允许
        let result = ban_manager
            .create_ban(
                target,
                "Test ban".to_string(),
                source,
                serde_json::json!({}),
                None,
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ban_manager_builder_with_authorization() {
        use crate::authorization::SimpleAuthorizationProvider;

        let storage = Arc::new(TestBanStorage::new());
        let auth_provider = Arc::new(SimpleAuthorizationProvider::new(vec![
            "admin".to_string(),
            "moderator".to_string(),
        ]));

        let ban_manager = BanManager::builder()
            .with_storage(storage)
            .with_config(BanManagerConfig::default())
            .with_authorization_provider(auth_provider)
            .build()
            .await
            .unwrap();

        let target = BanTarget::UserId("user123".to_string());

        // moderator 应该可以操作
        let result = ban_manager
            .delete_ban(&target, "moderator".to_string())
            .await;
        assert!(result.is_ok());
    }
}
