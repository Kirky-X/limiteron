//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 封禁管理器类型定义
//!
//! 提供封禁相关的类型、枚举和配置结构体。

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
use crate::storage::BanTarget;
use crate::storage::{BanRecord, BanStorage};
use chrono::{DateTime, Utc};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::sync::RwLock;

/// 封禁来源
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BanSource {
    /// 自动封禁
    Auto,
    /// 手动封禁
    Manual { operator: String },
}

/// 封禁优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
pub struct BanManager {
    /// 封禁存储
    storage: Arc<dyn BanStorage>,
    /// 配置
    config: Arc<RwLock<BanManagerConfig>>,
    /// 自动解禁任务句柄
    auto_unban_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// 授权提供者（可选）
    authorization_provider: Option<Arc<dyn AuthorizationProvider>>,
    /// 事件发射器（可选，feature-gated）
    #[cfg(feature = "event-system")]
    event_emitter: Option<Arc<crate::events::EventEmitter>>,
}

/// BanManager 构建器
///
/// 用于链式配置 BanManager 实例。
///
/// # 示例
/// ```rust
/// use limiteron::ban::BanManager;
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
#[derive(Default)]
pub struct BanManagerBuilder {
    storage: Option<Arc<dyn BanStorage>>,
    config: Option<BanManagerConfig>,
    authorization_provider: Option<Arc<dyn AuthorizationProvider>>,
    #[cfg(feature = "event-system")]
    event_emitter: Option<Arc<crate::events::EventEmitter>>,
}

impl BanManagerBuilder {
    /// 创建新的 BanManagerBuilder
    pub fn new() -> Self {
        Self {
            storage: None,
            config: None,
            authorization_provider: None,
            #[cfg(feature = "event-system")]
            event_emitter: None,
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
    /// use limiteron::ban::BanManager;
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

    /// 设置事件发射器
    #[cfg(feature = "event-system")]
    pub fn with_event_emitter(mut self, emitter: Arc<crate::events::EventEmitter>) -> Self {
        self.event_emitter = Some(emitter);
        self
    }

    /// 构建 BanManager 实例
    ///
    /// 如果未提供 storage，将使用内存存储作为默认依赖。
    /// 这允许使用 `BanManager::builder().build()` 进行快速原型开发。
    ///
    /// **注意**：默认内存存储不适用于多实例生产环境。
    pub async fn build(self) -> Result<BanManager, FlowGuardError> {
        use crate::storage::MemoryBanStorage;

        let storage = match self.storage {
            Some(s) => s,
            None => Arc::new(MemoryBanStorage::new()),
        };
        let config = self.config.unwrap_or_default();

        BanManager::with_dependencies_and_auth(
            storage,
            config,
            self.authorization_provider,
            #[cfg(feature = "event-system")]
            self.event_emitter,
        )
        .await
    }
}

/// 验证封禁目标
///
/// 使用统一的 validation 模块进行验证。
fn validate_ban_target(target: &BanTarget) -> Result<(), FlowGuardError> {
    match target {
        BanTarget::Ip(ip) => crate::validation::validate_ip_address(ip),
        BanTarget::UserId(user_id) => crate::validation::validate_user_id(user_id),
        BanTarget::Mac(mac) => crate::validation::validate_mac_address(mac),
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
    /// 开箱即用：创建使用默认配置的 BanManager
    ///
    /// 此方法使用内存存储作为默认依赖，无需外部配置即可运行。
    /// 适用于快速原型、测试或独立使用场景。
    ///
    /// **注意**：默认配置使用内存存储，不适用于多实例生产环境。
    /// 对于生产环境，建议使用 `builder()` 或 `with_dependencies()` 方法
    /// 配合持久化存储（如 PostgreSQL）。
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use limiteron::ban_manager::BanManager;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let ban_manager = BanManager::new().await;
    ///     // ban_manager 现在可以用于封禁管理
    /// }
    /// ```
    pub async fn new() -> Result<Self, FlowGuardError> {
        use crate::storage::MemoryBanStorage;

        let storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
        let config = BanManagerConfig::default();

        Self::with_dependencies(storage, config).await
    }

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
        Self::with_dependencies_and_auth(
            storage,
            config,
            None,
            #[cfg(feature = "event-system")]
            None,
        )
        .await
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
        #[cfg(feature = "event-system")] event_emitter: Option<Arc<crate::events::EventEmitter>>,
    ) -> Result<Self, FlowGuardError> {
        let config = Arc::new(RwLock::new(config));

        let ban_manager = Self {
            storage,
            config,
            auto_unban_handle: Arc::new(RwLock::new(None)),
            authorization_provider,
            #[cfg(feature = "event-system")]
            event_emitter,
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
            + chrono::Duration::from_std(duration)
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

        // 发射封禁事件
        #[cfg(feature = "event-system")]
        {
            if let Some(ref emitter) = self.event_emitter {
                let target_str = match detail.target {
                    BanTarget::Ip(ref ip) => ip.clone(),
                    BanTarget::UserId(ref uid) => uid.clone(),
                    BanTarget::Mac(ref mac) => mac.clone(),
                };
                let event = crate::events::Event::new(crate::events::EventType::BanApplied {
                    target: target_str,
                    reason: detail.reason.clone(),
                    duration: detail.duration.as_secs(),
                });
                if let Err(e) = emitter.emit(event).await {
                    error!("Failed to emit ban event: {}", e);
                }
            }
        }

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
                + chrono::Duration::from_std(new_duration)
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
            crate::logging::redact_ban_target(target),
            unbanned_by
        );

        // 检查是否存在封禁
        let record = self.storage.is_banned(target).await?;

        if record.is_none() {
            debug!(
                "No active ban found for target: {}",
                crate::logging::redact_ban_target(target)
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
                    let target_lower = target_type.to_lowercase();
                    let matches = match target_lower.as_str() {
                        "ip" => matches!(&record.target, BanTarget::Ip(_)),
                        "user" => matches!(&record.target, BanTarget::UserId(_)),
                        "mac" => matches!(&record.target, BanTarget::Mac(_)),
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
    use crate::storage::{BanHistory, BanStorage};
    use std::collections::HashMap as StdHashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // ========================================================================
    // 功能完整的 MockBanStorage 实现
    // ========================================================================

    /// Mock 封禁存储行为配置
    #[derive(Clone, Default)]
    struct MockBanBehavior {
        fail_mode: bool,
        force_expired: bool,
        max_entries: Option<usize>,
    }

    /// 功能完整的内存封禁存储（用于测试）
    struct MockBanStorage {
        bans: Arc<RwLock<StdHashMap<BanTarget, BanRecord>>>,
        history: Arc<RwLock<StdHashMap<BanTarget, BanHistory>>>,
        behavior: Arc<RwLock<MockBanBehavior>>,
    }

    impl MockBanStorage {
        fn new() -> Self {
            Self::with_behavior(MockBanBehavior::default())
        }

        fn with_behavior(behavior: MockBanBehavior) -> Self {
            Self {
                bans: Arc::new(RwLock::new(StdHashMap::new())),
                history: Arc::new(RwLock::new(StdHashMap::new())),
                behavior: Arc::new(RwLock::new(behavior)),
            }
        }

        async fn set_behavior(&self, behavior: MockBanBehavior) {
            let mut current = self.behavior.write().await;
            *current = behavior;
        }

        async fn clear(&self) {
            let mut bans = self.bans.write().await;
            let mut history = self.history.write().await;
            bans.clear();
            history.clear();
        }

        async fn should_fail(&self) -> bool {
            self.behavior.read().await.fail_mode
        }

        async fn is_force_expired(&self) -> bool {
            self.behavior.read().await.force_expired
        }

        async fn can_insert(&self, current_len: usize) -> Result<(), crate::error::StorageError> {
            let behavior = self.behavior.read().await;
            if let Some(max_entries) = behavior.max_entries {
                if current_len >= max_entries {
                    return Err(crate::error::StorageError::QueryError(
                        "超过最大封禁条目限制".to_string(),
                    ));
                }
            }
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl BanStorage for MockBanStorage {
        async fn is_banned(
            &self,
            target: &BanTarget,
        ) -> Result<Option<BanRecord>, crate::error::StorageError> {
            if self.should_fail().await {
                return Err(crate::error::StorageError::QueryError(
                    "MockBanStorage is_banned失败".to_string(),
                ));
            }

            if self.is_force_expired().await {
                return Ok(None);
            }

            let mut bans = self.bans.write().await;
            let now = chrono::Utc::now();
            if let Some(record) = bans.get(target) {
                if record.expires_at > now {
                    return Ok(Some(record.clone()));
                }
            }
            bans.remove(target);
            Ok(None)
        }

        async fn save(&self, record: &BanRecord) -> Result<(), crate::error::StorageError> {
            if self.should_fail().await {
                return Err(crate::error::StorageError::QueryError(
                    "MockBanStorage save失败".to_string(),
                ));
            }

            let mut bans = self.bans.write().await;
            self.can_insert(bans.len()).await?;
            bans.insert(record.target.clone(), record.clone());

            let mut history = self.history.write().await;
            let hist = BanHistory {
                ban_times: record.ban_times,
                last_banned_at: record.banned_at,
            };
            history.insert(record.target.clone(), hist);
            Ok(())
        }

        async fn get_history(
            &self,
            target: &BanTarget,
        ) -> Result<Option<BanHistory>, crate::error::StorageError> {
            if self.should_fail().await {
                return Err(crate::error::StorageError::QueryError(
                    "MockBanStorage get_history失败".to_string(),
                ));
            }

            let history = self.history.read().await;
            Ok(history.get(target).cloned())
        }

        async fn increment_ban_times(
            &self,
            target: &BanTarget,
        ) -> Result<u64, crate::error::StorageError> {
            if self.should_fail().await {
                return Err(crate::error::StorageError::QueryError(
                    "MockBanStorage increment_ban_times失败".to_string(),
                ));
            }

            let mut bans = self.bans.write().await;
            if let Some(record) = bans.get_mut(target) {
                record.ban_times += 1;
                Ok(record.ban_times as u64)
            } else {
                Ok(1)
            }
        }

        async fn get_ban_times(
            &self,
            target: &BanTarget,
        ) -> Result<u64, crate::error::StorageError> {
            if self.should_fail().await {
                return Err(crate::error::StorageError::QueryError(
                    "MockBanStorage get_ban_times失败".to_string(),
                ));
            }

            let bans = self.bans.read().await;
            if let Some(record) = bans.get(target) {
                Ok(record.ban_times as u64)
            } else {
                Ok(0)
            }
        }

        async fn remove_ban(&self, target: &BanTarget) -> Result<(), crate::error::StorageError> {
            if self.should_fail().await {
                return Err(crate::error::StorageError::QueryError(
                    "MockBanStorage remove_ban失败".to_string(),
                ));
            }

            let mut bans = self.bans.write().await;
            bans.remove(target);
            Ok(())
        }

        async fn cleanup_expired_bans(&self) -> Result<u64, crate::error::StorageError> {
            if self.should_fail().await {
                return Err(crate::error::StorageError::QueryError(
                    "MockBanStorage cleanup_expired_bans失败".to_string(),
                ));
            }

            let mut bans = self.bans.write().await;
            let now = chrono::Utc::now();
            let mut count = 0;
            bans.retain(|_, record| {
                if record.expires_at <= now {
                    count += 1;
                    false
                } else {
                    true
                }
            });
            Ok(count)
        }

        async fn list_bans(
            &self,
            active_only: bool,
            offset: u64,
            limit: u64,
        ) -> Result<Vec<BanRecord>, crate::error::StorageError> {
            if self.should_fail().await {
                return Err(crate::error::StorageError::QueryError(
                    "MockBanStorage list_bans失败".to_string(),
                ));
            }

            let bans = self.bans.read().await;
            let now = chrono::Utc::now();
            let mut records: Vec<_> = bans.values().cloned().collect();

            if active_only {
                records.retain(|r| r.expires_at > now);
            }

            let total = records.len() as u64;
            let start = offset as usize;
            let end = (offset.saturating_add(limit)) as usize;

            if start >= total as usize {
                return Ok(vec![]);
            }

            Ok(records.into_iter().skip(start).take(end - start).collect())
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    // ========================================================================
    // 测试辅助函数
    // ========================================================================

    /// 创建测试用的 BanManager
    async fn create_test_ban_manager() -> BanManager {
        let storage = Arc::new(MockBanStorage::new());
        BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap()
    }

    /// 创建带有自定义配置的 BanManager
    async fn create_test_ban_manager_with_config(config: BanManagerConfig) -> BanManager {
        let storage = Arc::new(MockBanStorage::new());
        BanManager::with_dependencies(storage, config)
            .await
            .unwrap()
    }

    /// 创建已过期的封禁记录
    fn create_expired_ban_record(target: BanTarget) -> BanRecord {
        let now = chrono::Utc::now();
        BanRecord {
            target,
            ban_times: 1,
            duration: StdDuration::from_secs(0),
            banned_at: now - chrono::Duration::seconds(10),
            expires_at: now - chrono::Duration::seconds(5), // 已过期
            is_manual: false,
            reason: "expired ban".to_string(),
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
        let ban_manager = create_test_ban_manager().await;

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
        let ban_manager = create_test_ban_manager().await;

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
        let ban_manager = create_test_ban_manager().await;

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
        let ban_manager = create_test_ban_manager().await;

        let target = BanTarget::UserId("nonexistent".to_string());
        let result = ban_manager.read_ban(&target).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_update_ban_not_found() {
        let ban_manager = create_test_ban_manager().await;

        let target = BanTarget::UserId("nonexistent".to_string());
        let result = ban_manager
            .update_ban(&target, Some("New reason".to_string()), None, None)
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_ban_not_found() {
        let ban_manager = create_test_ban_manager().await;

        let target = BanTarget::UserId("nonexistent".to_string());
        let result = ban_manager.delete_ban(&target, "admin".to_string()).await;

        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_list_bans_empty() {
        let ban_manager = create_test_ban_manager().await;

        let filter = BanFilter::default();
        let result = ban_manager.list_bans(filter).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_check_ban_priority_empty() {
        let ban_manager = create_test_ban_manager().await;

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
        let ban_manager = create_test_ban_manager().await;

        let config = ban_manager.get_config().await;
        assert!(config.enable_auto_unban);
        assert_eq!(config.auto_unban_interval, 60);
    }

    #[tokio::test]
    async fn test_update_config() {
        let ban_manager = create_test_ban_manager().await;

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
        let ban_manager = create_test_ban_manager().await;

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
    // 基础封禁操作测试
    // ========================================================================

    #[tokio::test]
    async fn test_ip_ban_add_check_remove() {
        let ban_manager = create_test_ban_manager().await;
        let ip = "192.168.1.100";
        let target = BanTarget::Ip(ip.to_string());

        // 1. 添加 IP 封禁
        let detail = ban_manager
            .create_ban(
                target.clone(),
                "IP封禁测试".to_string(),
                BanSource::Auto,
                serde_json::json!({"ip": ip}),
                Some(StdDuration::from_secs(3600)),
            )
            .await
            .unwrap();

        assert_eq!(detail.target, target);
        assert!(!detail.is_manual);
        assert_eq!(detail.ban_times, 1);

        // 2. 检查 IP 是否被封禁
        let banned = ban_manager.read_ban(&target).await.unwrap();
        assert!(banned.is_some());
        let ban_record = banned.unwrap();
        assert_eq!(ban_record.target, target);
        assert_eq!(ban_record.reason, "IP封禁测试");

        // 3. 移除 IP 封禁
        let removed = ban_manager
            .delete_ban(&target, "admin".to_string())
            .await
            .unwrap();
        assert!(removed);

        // 4. 再次检查，应该不再被封禁
        let banned = ban_manager.read_ban(&target).await.unwrap();
        assert!(banned.is_none());
    }

    #[tokio::test]
    async fn test_user_id_ban_add_check_remove() {
        let ban_manager = create_test_ban_manager().await;
        let user_id = "user_test_123";
        let target = BanTarget::UserId(user_id.to_string());

        // 1. 添加用户ID封禁
        let detail = ban_manager
            .create_ban(
                target.clone(),
                "用户违规".to_string(),
                BanSource::Manual {
                    operator: "admin".to_string(),
                },
                serde_json::json!({"user_id": user_id}),
                Some(StdDuration::from_secs(7200)),
            )
            .await
            .unwrap();

        assert_eq!(detail.target, target);
        assert!(detail.is_manual);
        assert_eq!(detail.ban_times, 1);

        // 2. 检查用户是否被封禁
        let banned = ban_manager.read_ban(&target).await.unwrap();
        assert!(banned.is_some());
        let ban_record = banned.unwrap();
        assert_eq!(ban_record.target, target);
        assert!(ban_record.is_manual);

        // 3. 移除用户封禁
        let removed = ban_manager
            .delete_ban(&target, "admin".to_string())
            .await
            .unwrap();
        assert!(removed);

        // 4. 再次检查
        let banned = ban_manager.read_ban(&target).await.unwrap();
        assert!(banned.is_none());
    }

    #[tokio::test]
    async fn test_mac_address_ban_add_check_remove() {
        let ban_manager = create_test_ban_manager().await;
        let mac = "AA:BB:CC:DD:EE:FF";
        let target = BanTarget::Mac(mac.to_string());

        // 1. 添加 MAC 地址封禁
        let detail = ban_manager
            .create_ban(
                target.clone(),
                "MAC地址封禁".to_string(),
                BanSource::Auto,
                serde_json::json!({"mac": mac}),
                Some(StdDuration::from_secs(1800)),
            )
            .await
            .unwrap();

        assert_eq!(detail.target, target);
        assert!(!detail.is_manual);

        // 2. 检查 MAC 是否被封禁
        let banned = ban_manager.read_ban(&target).await.unwrap();
        assert!(banned.is_some());
        let ban_record = banned.unwrap();
        assert_eq!(ban_record.target, target);

        // 3. 移除 MAC 封禁
        let removed = ban_manager
            .delete_ban(&target, "admin".to_string())
            .await
            .unwrap();
        assert!(removed);

        // 4. 再次检查
        let banned = ban_manager.read_ban(&target).await.unwrap();
        assert!(banned.is_none());
    }

    // ========================================================================
    // 高级封禁功能测试
    // ========================================================================

    #[tokio::test]
    async fn test_ban_expiry_auto_unban() {
        let storage = Arc::new(MockBanStorage::new());
        let config = BanManagerConfig {
            enable_auto_unban: false, // 禁用自动解封任务，手动测试
            ..Default::default()
        };
        let ban_manager = BanManager::with_dependencies(storage.clone(), config)
            .await
            .unwrap();

        let target = BanTarget::Ip("10.0.0.1".to_string());

        // 创建一个已过期的封禁记录
        let expired_record = create_expired_ban_record(target.clone());
        storage.save(&expired_record).await.unwrap();

        // 检查过期封禁应该返回 None
        let banned = ban_manager.read_ban(&target).await.unwrap();
        assert!(banned.is_none(), "过期的封禁记录应该自动解除");
    }

    #[tokio::test]
    async fn test_ban_history_tracking() {
        let ban_manager = create_test_ban_manager().await;
        let target = BanTarget::UserId("history_user".to_string());

        // 第一次封禁
        let detail1 = ban_manager
            .create_ban(
                target.clone(),
                "第一次违规".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap();
        assert_eq!(detail1.ban_times, 1);

        // 解封
        ban_manager
            .delete_ban(&target, "admin".to_string())
            .await
            .unwrap();

        // 第二次封禁 - 封禁次数应该递增
        let detail2 = ban_manager
            .create_ban(
                target.clone(),
                "第二次违规".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap();
        assert_eq!(detail2.ban_times, 2);

        // 检查历史记录
        let history = ban_manager.get_history(&target).await.unwrap();
        assert!(history.is_some());
        let history = history.unwrap();
        assert_eq!(history.ban_times, 2);
    }

    #[tokio::test]
    async fn test_incremental_ban_duration() {
        let ban_manager = create_test_ban_manager().await;
        let target = BanTarget::UserId("incremental_test_user".to_string());

        // 第一次封禁 - 应该是 60 秒
        let detail1 = ban_manager
            .create_ban(
                target.clone(),
                "第一次".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap();
        assert_eq!(detail1.duration, StdDuration::from_secs(60));

        // 解封
        ban_manager
            .delete_ban(&target, "admin".to_string())
            .await
            .unwrap();

        // 第二次封禁 - 应该是 300 秒
        let detail2 = ban_manager
            .create_ban(
                target.clone(),
                "第二次".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap();
        assert_eq!(detail2.duration, StdDuration::from_secs(300));

        // 解封
        ban_manager
            .delete_ban(&target, "admin".to_string())
            .await
            .unwrap();

        // 第三次封禁 - 应该是 1800 秒
        let detail3 = ban_manager
            .create_ban(
                target.clone(),
                "第三次".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap();
        assert_eq!(detail3.duration, StdDuration::from_secs(1800));

        // 解封
        ban_manager
            .delete_ban(&target, "admin".to_string())
            .await
            .unwrap();

        // 第四次封禁 - 应该是 7200 秒
        let detail4 = ban_manager
            .create_ban(
                target.clone(),
                "第四次".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap();
        assert_eq!(detail4.duration, StdDuration::from_secs(7200));
    }

    #[tokio::test]
    async fn test_manual_vs_auto_ban() {
        let ban_manager = create_test_ban_manager().await;

        // 自动封禁
        let auto_target = BanTarget::UserId("auto_ban_user".to_string());
        let auto_detail = ban_manager
            .create_ban(
                auto_target.clone(),
                "自动封禁".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap();

        assert!(!auto_detail.is_manual);
        assert_eq!(auto_detail.source, BanSource::Auto);

        // 手动封禁
        let manual_target = BanTarget::UserId("manual_ban_user".to_string());
        let manual_detail = ban_manager
            .create_ban(
                manual_target.clone(),
                "手动封禁".to_string(),
                BanSource::Manual {
                    operator: "admin".to_string(),
                },
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap();

        assert!(manual_detail.is_manual);
        match manual_detail.source {
            BanSource::Manual { operator } => {
                assert_eq!(operator, "admin");
            }
            BanSource::Auto => panic!("期望手动封禁"),
        }
    }

    #[tokio::test]
    async fn test_ban_priority_ordering() {
        // 测试封禁优先级顺序：IP > UserId > MAC
        assert!(BanPriority::Ip < BanPriority::UserId);
        assert!(BanPriority::UserId < BanPriority::Mac);
        assert!(BanPriority::Mac < BanPriority::DeviceId);
        assert!(BanPriority::DeviceId < BanPriority::ApiKey);
    }

    #[tokio::test]
    async fn test_check_ban_priority_with_ip_ban() {
        let storage = Arc::new(MockBanStorage::new());
        let ban_manager =
            BanManager::with_dependencies(storage.clone(), BanManagerConfig::default())
                .await
                .unwrap();

        // 创建 IP 封禁
        let ip_target = BanTarget::Ip("priority.ip.test".to_string());
        let record = BanRecord {
            target: ip_target.clone(),
            ban_times: 1,
            duration: StdDuration::from_secs(3600),
            banned_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            is_manual: false,
            reason: "IP封禁".to_string(),
        };
        storage.save(&record).await.unwrap();

        // 检查多个目标，IP 封禁应该优先返回
        let targets = vec![
            BanTarget::UserId("some_user".to_string()),
            BanTarget::Ip("priority.ip.test".to_string()),
            BanTarget::Mac("AA:BB:CC:DD:EE:FF".to_string()),
        ];

        let result = ban_manager.check_ban_priority(&targets).await.unwrap();
        assert!(result.is_some());
        let detail = result.unwrap();
        assert_eq!(detail.target, ip_target);
    }

    #[tokio::test]
    async fn test_list_bans_with_filter() {
        let ban_manager = create_test_ban_manager().await;

        // 创建多个封禁
        ban_manager
            .create_ban(
                BanTarget::Ip("192.168.1.1".to_string()),
                "IP封禁".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap();

        ban_manager
            .create_ban(
                BanTarget::UserId("user1".to_string()),
                "用户封禁".to_string(),
                BanSource::Manual {
                    operator: "admin".to_string(),
                },
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap();

        // 过滤 IP 类型
        let filter = BanFilter {
            target_type: Some("ip".to_string()),
            ..Default::default()
        };
        let bans = ban_manager.list_bans(filter).await.unwrap();
        assert_eq!(bans.len(), 1);
        assert!(matches!(bans[0].target, BanTarget::Ip(_)));

        // 过滤手动封禁
        let filter = BanFilter {
            manual_only: true,
            ..Default::default()
        };
        let bans = ban_manager.list_bans(filter).await.unwrap();
        assert_eq!(bans.len(), 1);
        assert!(bans[0].is_manual);
    }

    #[tokio::test]
    async fn test_update_ban() {
        let ban_manager = create_test_ban_manager().await;
        let target = BanTarget::UserId("update_test_user".to_string());

        // 创建封禁
        ban_manager
            .create_ban(
                target.clone(),
                "原始原因".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                Some(StdDuration::from_secs(3600)),
            )
            .await
            .unwrap();

        // 更新封禁
        let updated = ban_manager
            .update_ban(
                &target,
                Some("更新后的原因".to_string()),
                Some(StdDuration::from_secs(7200)),
                Some(serde_json::json!({"updated": true})),
            )
            .await
            .unwrap();

        assert!(updated.is_some());
        let detail = updated.unwrap();
        assert_eq!(detail.reason, "更新后的原因");
        assert_eq!(detail.duration, StdDuration::from_secs(7200));
    }

    // ========================================================================
    // 并发封禁测试
    // ========================================================================

    #[tokio::test]
    async fn test_concurrent_ban_operations() {
        let storage = Arc::new(MockBanStorage::new());
        let ban_manager =
            BanManager::with_dependencies(storage.clone(), BanManagerConfig::default())
                .await
                .unwrap();

        let mut handles = vec![];

        // 并发创建 10 个不同的封禁
        for i in 0..10 {
            let bm = ban_manager.clone();
            let handle = tokio::spawn(async move {
                let target = BanTarget::UserId(format!("concurrent_user_{}", i));
                bm.create_ban(
                    target,
                    format!("并发封禁测试 {}", i),
                    BanSource::Auto,
                    serde_json::json!({"index": i}),
                    None,
                )
                .await
            });
            handles.push(handle);
        }

        // 等待所有操作完成
        let results: Vec<_> = futures::future::join_all(handles).await;

        // 验证所有操作都成功
        for result in results {
            assert!(result.unwrap().is_ok());
        }

        // 验证所有封禁都已创建
        let filter = BanFilter::default();
        let bans = ban_manager.list_bans(filter).await.unwrap();
        assert_eq!(bans.len(), 10);
    }

    #[tokio::test]
    async fn test_concurrent_unban_operations() {
        let storage = Arc::new(MockBanStorage::new());
        let ban_manager =
            BanManager::with_dependencies(storage.clone(), BanManagerConfig::default())
                .await
                .unwrap();

        // 先创建多个封禁
        for i in 0..5 {
            let target = BanTarget::Ip(format!("192.168.1.{}", i));
            ban_manager
                .create_ban(
                    target,
                    "测试封禁".to_string(),
                    BanSource::Auto,
                    serde_json::json!({}),
                    None,
                )
                .await
                .unwrap();
        }

        let mut handles = vec![];

        // 并发解封
        for i in 0..5 {
            let bm = ban_manager.clone();
            let handle = tokio::spawn(async move {
                let target = BanTarget::Ip(format!("192.168.1.{}", i));
                bm.delete_ban(&target, "admin".to_string()).await
            });
            handles.push(handle);
        }

        // 等待所有操作完成
        let results: Vec<_> = futures::future::join_all(handles).await;

        // 验证所有操作都成功
        for result in results {
            assert!(result.unwrap().is_ok());
        }

        // 验证所有封禁都已移除
        let filter = BanFilter::default();
        let bans = ban_manager.list_bans(filter).await.unwrap();
        assert_eq!(bans.len(), 0);
    }

    #[tokio::test]
    async fn test_concurrent_ban_state_consistency() {
        let storage = Arc::new(MockBanStorage::new());
        let ban_manager =
            BanManager::with_dependencies(storage.clone(), BanManagerConfig::default())
                .await
                .unwrap();

        let target = BanTarget::UserId("consistency_user".to_string());

        // 并发对同一目标进行封禁操作
        let mut handles = vec![];
        for _ in 0..5 {
            let bm = ban_manager.clone();
            let t = target.clone();
            let handle = tokio::spawn(async move {
                bm.create_ban(
                    t,
                    "一致性测试".to_string(),
                    BanSource::Auto,
                    serde_json::json!({}),
                    None,
                )
                .await
            });
            handles.push(handle);
        }

        let results: Vec<_> = futures::future::join_all(handles).await;

        // 所有操作都应该成功（存储层会处理并发写入）
        for result in results {
            assert!(result.unwrap().is_ok());
        }

        // 最终状态应该是一致的：目标被封禁
        let banned = ban_manager.read_ban(&target).await.unwrap();
        assert!(banned.is_some());
    }

    #[tokio::test]
    async fn test_concurrent_read_write_operations() {
        let storage = Arc::new(MockBanStorage::new());
        let ban_manager =
            BanManager::with_dependencies(storage.clone(), BanManagerConfig::default())
                .await
                .unwrap();

        let target = BanTarget::UserId("rw_test_user".to_string());

        // 先创建一个封禁
        ban_manager
            .create_ban(
                target.clone(),
                "读写测试".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap();

        let mut handles = vec![];

        // 并发读取
        for _ in 0..5 {
            let bm = ban_manager.clone();
            let t = target.clone();
            let handle = tokio::spawn(async move { bm.read_ban(&t).await });
            handles.push(handle);
        }

        // 并发写入（更新）
        for _ in 0..3 {
            let bm = ban_manager.clone();
            let t = target.clone();
            let handle = tokio::spawn(async move {
                bm.update_ban(&t, Some("更新原因".to_string()), None, None)
                    .await
            });
            handles.push(handle);
        }

        let results: Vec<_> = futures::future::join_all(handles).await;

        // 所有操作都应该成功
        for result in results {
            assert!(result.unwrap().is_ok());
        }
    }

    // ========================================================================
    // 授权检查测试
    // ========================================================================

    #[tokio::test]
    async fn test_create_ban_with_authorization_success() {
        use crate::authorization::SimpleAuthorizationProvider;

        let storage = Arc::new(MockBanStorage::new());
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

        let storage = Arc::new(MockBanStorage::new());
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

        let storage = Arc::new(MockBanStorage::new());
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

        let storage = Arc::new(MockBanStorage::new());
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

        let storage = Arc::new(MockBanStorage::new());
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
        let ban_manager = create_test_ban_manager().await;

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

        let storage = Arc::new(MockBanStorage::new());
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

    // ========================================================================
    // 输入验证测试
    // ========================================================================

    #[tokio::test]
    async fn test_validate_ban_reason_empty() {
        let ban_manager = create_test_ban_manager().await;

        let result = ban_manager
            .create_ban(
                BanTarget::Ip("192.168.1.1".to_string()),
                "".to_string(), // 空原因
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await;

        assert!(result.is_err());
        match result {
            Err(FlowGuardError::ValidationError(msg)) => {
                assert!(msg.contains("不能为空"));
            }
            _ => panic!("期望 ValidationError"),
        }
    }

    #[tokio::test]
    async fn test_validate_ban_reason_too_long() {
        let ban_manager = create_test_ban_manager().await;

        let long_reason = "x".repeat(MAX_BAN_REASON_LENGTH + 1);

        let result = ban_manager
            .create_ban(
                BanTarget::Ip("192.168.1.1".to_string()),
                long_reason,
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await;

        assert!(result.is_err());
        match result {
            Err(FlowGuardError::ValidationError(msg)) => {
                assert!(msg.contains("过长"));
            }
            _ => panic!("期望 ValidationError"),
        }
    }

    #[tokio::test]
    async fn test_validate_ban_reason_control_chars() {
        let ban_manager = create_test_ban_manager().await;

        let result = ban_manager
            .create_ban(
                BanTarget::Ip("192.168.1.1".to_string()),
                "包含控制字符\x00的封禁原因".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await;

        assert!(result.is_err());
        match result {
            Err(FlowGuardError::ValidationError(msg)) => {
                assert!(msg.contains("非法字符"));
            }
            _ => panic!("期望 ValidationError"),
        }
    }

    // ========================================================================
    // 存储错误处理测试
    // ========================================================================

    #[tokio::test]
    async fn test_storage_error_handling() {
        let storage = Arc::new(MockBanStorage::with_behavior(MockBanBehavior {
            fail_mode: true,
            ..Default::default()
        }));
        let ban_manager = BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap();

        let result = ban_manager
            .create_ban(
                BanTarget::Ip("192.168.1.1".to_string()),
                "测试".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_max_entries_limit() {
        let storage = Arc::new(MockBanStorage::with_behavior(MockBanBehavior {
            max_entries: Some(2),
            ..Default::default()
        }));
        let ban_manager = BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .unwrap();

        // 创建两个封禁应该成功
        ban_manager
            .create_ban(
                BanTarget::Ip("192.168.1.1".to_string()),
                "测试1".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap();

        ban_manager
            .create_ban(
                BanTarget::Ip("192.168.1.2".to_string()),
                "测试2".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap();

        // 第三个应该失败
        let result = ban_manager
            .create_ban(
                BanTarget::Ip("192.168.1.3".to_string()),
                "测试3".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await;

        assert!(result.is_err());
    }

    // ========================================================================
    // 便捷方法测试
    // ========================================================================

    #[tokio::test]
    async fn test_add_ban_convenience_method() {
        let ban_manager = create_test_ban_manager().await;

        let record = BanRecord {
            target: BanTarget::Ip("192.168.1.50".to_string()),
            ban_times: 1,
            duration: StdDuration::from_secs(1800),
            banned_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(1800),
            is_manual: true,
            reason: "便捷方法测试".to_string(),
        };

        let result = ban_manager.add_ban(record).await;
        assert!(result.is_ok());

        // 验证封禁已创建
        let banned = ban_manager
            .read_ban(&BanTarget::Ip("192.168.1.50".to_string()))
            .await
            .unwrap();
        assert!(banned.is_some());
    }

    #[tokio::test]
    async fn test_get_ban_convenience_method() {
        let ban_manager = create_test_ban_manager().await;

        let target = BanTarget::UserId("get_ban_user".to_string());
        ban_manager
            .create_ban(
                target.clone(),
                "测试".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap();

        let record = ban_manager.get_ban(&target).await.unwrap();
        assert!(record.is_some());
        let record = record.unwrap();
        assert_eq!(record.target, target);
    }

    #[tokio::test]
    async fn test_is_banned_convenience_method() {
        let ban_manager = create_test_ban_manager().await;

        let target = BanTarget::Mac("11:22:33:44:55:66".to_string());

        // 未封禁时
        let result = ban_manager.is_banned(&target).await.unwrap();
        assert!(result.is_none());

        // 封禁后
        ban_manager
            .create_ban(
                target.clone(),
                "测试".to_string(),
                BanSource::Auto,
                serde_json::json!({}),
                None,
            )
            .await
            .unwrap();

        let result = ban_manager.is_banned(&target).await.unwrap();
        assert!(result.is_some());
    }

    // ========================================================================
    // BanManager Construction Patterns Tests
    // ========================================================================

    #[tokio::test]
    async fn test_ban_manager_new_uses_memory_storage() {
        // Test that BanManager::new() uses MemoryBanStorage by default
        let ban_manager = BanManager::new()
            .await
            .expect("BanManager creation should succeed");

        let target = BanTarget::UserId("test_user".to_string());
        let result = ban_manager.is_banned(&target).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ban_manager_builder_with_storage() {
        // Test BanManager::builder() with explicit storage
        use crate::storage::MemoryBanStorage;
        let storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());

        let ban_manager = BanManager::builder()
            .with_storage(storage)
            .build()
            .await
            .expect("BanManager build should succeed");

        let target = BanTarget::Ip("192.168.1.1".to_string());
        let result = ban_manager.is_banned(&target).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ban_manager_builder_without_storage() {
        // Test BanManager::builder() without storage (uses default MemoryBanStorage)
        let ban_manager = BanManager::builder()
            .build()
            .await
            .expect("BanManager build should succeed with default storage");

        let target = BanTarget::UserId("default_test".to_string());
        let result = ban_manager.is_banned(&target).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ban_manager_with_dependencies() {
        // Test BanManager::with_dependencies() with explicit configuration
        use crate::storage::MemoryBanStorage;
        let storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
        let config = BanManagerConfig::default();

        let ban_manager = BanManager::with_dependencies(storage, config)
            .await
            .expect("BanManager creation should succeed");

        let target = BanTarget::Mac("aa:bb:cc:dd:ee:ff".to_string());
        let result = ban_manager.is_banned(&target).await;
        assert!(result.is_ok());
    }
}
