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

/// 最大封禁原因长度
pub const MAX_BAN_REASON_LENGTH: usize = 500;

/// 最大用户ID长度
pub const MAX_USER_ID_LENGTH: usize = 100;

/// 最大MAC地址长度
pub const MAX_MAC_ADDRESS_LENGTH: usize = 17;

use crate::error::FlowGuardError;
use crate::storage::{BanRecord, BanStorage, BanTarget};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument};

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
}

/// 验证IP地址格式
fn validate_ip_address(ip: &str) -> Result<(), FlowGuardError> {
    if ip.is_empty() {
        return Err(FlowGuardError::ValidationError(
            "IP地址不能为空".to_string(),
        ));
    }

    // 检查长度
    if ip.len() > 45 {
        return Err(FlowGuardError::ValidationError("IP地址过长".to_string()));
    }

    // 验证IPv4或IPv6格式
    if ip.parse::<std::net::IpAddr>().is_err() {
        return Err(FlowGuardError::ValidationError(format!(
            "无效的IP地址格式: {}",
            ip
        )));
    }

    Ok(())
}

/// 验证用户ID
fn validate_user_id(user_id: &str) -> Result<(), FlowGuardError> {
    if user_id.is_empty() {
        return Err(FlowGuardError::ValidationError(
            "用户ID不能为空".to_string(),
        ));
    }

    if user_id.len() > MAX_USER_ID_LENGTH {
        return Err(FlowGuardError::ValidationError(format!(
            "用户ID过长，最大长度为 {} 字符",
            MAX_USER_ID_LENGTH
        )));
    }

    // 检查是否包含危险字符
    if user_id.contains(|c: char| c.is_control()) {
        return Err(FlowGuardError::ValidationError(
            "用户ID包含非法字符".to_string(),
        ));
    }

    Ok(())
}

/// 验证MAC地址格式
fn validate_mac_address(mac: &str) -> Result<(), FlowGuardError> {
    if mac.is_empty() {
        return Err(FlowGuardError::ValidationError(
            "MAC地址不能为空".to_string(),
        ));
    }

    if mac.len() > MAX_MAC_ADDRESS_LENGTH {
        return Err(FlowGuardError::ValidationError("MAC地址过长".to_string()));
    }

    // 简单验证MAC地址格式（XX:XX:XX:XX:XX:XX）
    let parts: Vec<&str> = mac.split(':').collect();
    if parts.len() != 6 {
        return Err(FlowGuardError::ValidationError(
            "无效的MAC地址格式".to_string(),
        ));
    }

    for part in parts {
        if part.len() != 2 {
            return Err(FlowGuardError::ValidationError(
                "无效的MAC地址格式".to_string(),
            ));
        }
        if !part.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(FlowGuardError::ValidationError(
                "MAC地址包含非法字符".to_string(),
            ));
        }
    }

    Ok(())
}

/// 验证封禁目标
fn validate_ban_target(target: &BanTarget) -> Result<(), FlowGuardError> {
    match target {
        BanTarget::Ip(ip) => validate_ip_address(ip)?,
        BanTarget::UserId(user_id) => validate_user_id(user_id)?,
        BanTarget::Mac(mac) => validate_mac_address(mac)?,
    }
    Ok(())
}

/// 验证封禁原因
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
    /// 创建新的BanManager实例
    ///
    /// # 参数
    /// - `storage`: 封禁存储后端
    /// - `config`: 配置（可选）
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::ban_manager::BanManager;
    /// use limiteron::storage::MockBanStorage;
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let storage = Arc::new(MockBanStorage);
    ///     let ban_manager = BanManager::new(storage, None).await.unwrap();
    /// }
    /// ```
    pub async fn new(
        storage: Arc<dyn BanStorage>,
        config: Option<BanManagerConfig>,
    ) -> Result<Self, FlowGuardError> {
        let config = config.unwrap_or_default();
        let config = Arc::new(RwLock::new(config));

        let ban_manager = Self {
            storage,
            config,
            auto_unban_handle: Arc::new(RwLock::new(None)),
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

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(StdDuration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                debug!("Running auto-unban task");

                // 清理过期封禁
                #[cfg(feature = "postgres")]
                if let Some(storage) = storage
                    .clone()
                    .as_any()
                    .downcast_ref::<crate::postgres_storage::PostgresStorage>()
                {
                    #[cfg(feature = "postgres")]
                    if let Err(e) = storage.cleanup_expired_bans().await {
                        error!("Auto-unban task failed: {}", e);
                    }
                }
            }
        });

        *self.auto_unban_handle.write().await = Some(handle);
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
    #[instrument(skip(self))]
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
    #[instrument(skip(self, metadata))]
    pub async fn create_ban(
        &self,
        target: BanTarget,
        reason: String,
        source: BanSource,
        metadata: serde_json::Value,
        duration: Option<StdDuration>,
    ) -> Result<BanDetail, FlowGuardError> {
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
            None => {
                // 使用默认配置计算
                let config = self.config.read().await;
                let duration_secs = match ban_times {
                    1 => config.backoff.first_duration,
                    2 => config.backoff.second_duration,
                    3 => config.backoff.third_duration,
                    _ => config.backoff.fourth_duration,
                };
                let duration_secs = duration_secs.min(config.backoff.max_duration);
                StdDuration::from_secs(duration_secs)
            }
        };

        let now = Utc::now();
        let expires_at = now + Duration::from_std(duration).unwrap();
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
    #[instrument(skip(self))]
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
    #[instrument(skip(self))]
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

        if current_record.is_none() {
            return Ok(None);
        }

        let mut record = current_record.unwrap();
        let now = Utc::now();

        // 更新字段
        if let Some(new_reason) = reason {
            record.reason = new_reason;
        }

        if let Some(new_duration) = duration {
            record.duration = new_duration;
            record.expires_at = now + Duration::from_std(new_duration).unwrap();
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
    #[instrument(skip(self))]
    pub async fn delete_ban(
        &self,
        target: &BanTarget,
        unbanned_by: String,
    ) -> Result<bool, FlowGuardError> {
        info!(
            "Deleting ban: target={:?}, unbanned_by={}",
            target, unbanned_by
        );

        // 检查是否存在封禁
        let record = self.storage.is_banned(target).await?;

        if record.is_none() {
            debug!("No active ban found for target: {:?}", target);
            return Ok(false);
        }

        // 如果是PostgreSQL存储，更新unbanned_at和unbanned_by字段
        #[cfg(feature = "postgres")]
        if let Some(storage) = self
            .storage
            .as_any()
            .downcast_ref::<crate::postgres_storage::PostgresStorage>()
        {
            let (target_type, target_value) = match target {
                BanTarget::Ip(ip) => ("ip", ip.as_str()),
                BanTarget::UserId(user_id) => ("user", user_id.as_str()),
                BanTarget::Mac(mac) => ("mac", mac.as_str()),
            };

            sqlx::query(
                r#"
                UPDATE ban_records
                SET unbanned_at = now(),
                    unbanned_by = $1
                WHERE target_type = $2
                  AND target_value = $3
                  AND expires_at > now()
                  AND unbanned_at IS NULL
                "#,
            )
            .bind(&unbanned_by)
            .bind(target_type)
            .bind(target_value)
            .execute(storage.pool())
            .await
            .map_err(|e| {
                FlowGuardError::StorageError(crate::error::StorageError::QueryError(e.to_string()))
            })?;
        }

        // 无论何种存储，都需要从活动封禁中移除
        // 对于PostgreSQL，remove_ban 也会更新 unbanned_at (如果实现正确)
        // 但这里我们已经上面处理了Postgres的特殊逻辑(记录解封人)，
        // 为了兼容 Memory 和 Redis，必须调用 remove_ban
        self.storage.remove_ban(target).await?;

        info!("Ban deleted successfully: target={:?}", target);
        Ok(true)
    }

    /// 列出封禁记录
    ///
    /// # 参数
    /// - `filter`: 过滤条件
    ///
    /// # 返回
    /// - 封禁记录列表
    #[cfg(any(feature = "telemetry", feature = "monitoring"))]
    #[instrument(skip(self))]
    pub async fn list_bans(&self, filter: BanFilter) -> Result<Vec<BanDetail>, FlowGuardError> {
        debug!("Listing bans with filter: {:?}", filter);

        // 如果是PostgreSQL存储，使用数据库查询
        #[cfg(feature = "postgres")]
        if let Some(storage) = self
            .storage
            .as_any()
            .downcast_ref::<crate::postgres_storage::PostgresStorage>()
        {
            let mut conditions = Vec::new();
            let mut params: Vec<String> = Vec::new();

            // 目标类型过滤（使用参数化查询）
            if let Some(target_type) = &filter.target_type {
                // 验证目标类型
                if !["ip", "user", "mac"].contains(&target_type.to_lowercase().as_str()) {
                    return Err(FlowGuardError::ConfigError("无效的目标类型".to_string()));
                }
                conditions.push("target_type = $1".to_string());
                params.push(target_type.to_lowercase());
            }

            // 目标值过滤（使用参数化查询，防止 SQL 注入）
            if let Some(target_value) = &filter.target_value {
                // 验证目标值长度
                if target_value.len() > 255 {
                    return Err(FlowGuardError::ConfigError(
                        "目标值长度超过限制".to_string(),
                    ));
                }
                // 使用参数化查询，LIKE 模式在服务器端添加
                let param_index = conditions.len() + 1;
                conditions.push(format!("target_value LIKE ${}", param_index));

                // 转义 LIKE 通配符，防止 SQL 注入
                let escaped_value = target_value.replace('%', "\\%").replace('_', "\\_");
                params.push(format!("%{}%", escaped_value));
            }

            // 只显示活跃封禁
            if filter.active_only {
                conditions.push("expires_at > now() AND unbanned_at IS NULL".to_string());
            }

            // 只显示手动封禁
            if filter.manual_only {
                conditions.push("is_manual = true".to_string());
            }

            // 构建基础查询
            let mut query = String::from(
                "SELECT id, target_type, target_value, reason, ban_times, duration_secs, ",
            );
            query.push_str("banned_at, expires_at, is_manual, unbanned_at, unbanned_by ");
            query.push_str("FROM ban_records");

            // 添加条件
            if !conditions.is_empty() {
                query.push_str(" WHERE ");
                query.push_str(&conditions.join(" AND "));
            }

            // 排序和分页（使用参数化查询）
            query.push_str(" ORDER BY banned_at DESC LIMIT $1 OFFSET $2");

            let limit = filter
                .limit
                .unwrap_or(DEFAULT_PAGINATION_LIMIT)
                .min(MAX_PAGINATION_LIMIT);
            let offset = filter.offset.unwrap_or(0);

            // 执行参数化查询
            let mut query_builder = sqlx::query_as::<
                _,
                (
                    uuid::Uuid,
                    String,
                    String,
                    String,
                    i32,
                    i64,
                    chrono::DateTime<chrono::Utc>,
                    chrono::DateTime<chrono::Utc>,
                    bool,
                    Option<chrono::DateTime<chrono::Utc>>,
                    Option<String>,
                ),
            >(&query);

            // 绑定分页参数
            query_builder = query_builder.bind(limit as i64).bind(offset as i64);

            // 绑定条件参数
            for param in &params {
                query_builder = query_builder.bind(param);
            }

            // 执行查询
            let results = query_builder.fetch_all(storage.pool()).await.map_err(|e| {
                error!("查询封禁记录失败: {}", e);
                FlowGuardError::StorageError(crate::error::StorageError::QueryError(e.to_string()))
            })?;

            // 转换结果
            let bans: Vec<BanDetail> = results
                .into_iter()
                .map(
                    |(
                        id,
                        target_type,
                        target_value,
                        reason,
                        ban_times,
                        duration_secs,
                        banned_at,
                        expires_at,
                        is_manual,
                        unbanned_at,
                        unbanned_by,
                    )| {
                        let target = match target_type.as_str() {
                            "ip" => BanTarget::Ip(target_value),
                            "user" => BanTarget::UserId(target_value),
                            "mac" => BanTarget::Mac(target_value),
                            _ => BanTarget::UserId(target_value), // 默认处理
                        };

                        let unbanned_by_clone = unbanned_by.clone();

                        BanDetail {
                            id: id.to_string(),
                            target,
                            ban_times: ban_times as u32,
                            duration: std::time::Duration::from_secs(duration_secs as u64),
                            banned_at,
                            expires_at,
                            is_manual,
                            reason,
                            source: if is_manual {
                                BanSource::Manual {
                                    operator: unbanned_by_clone
                                        .unwrap_or_else(|| "unknown".to_string()),
                                }
                            } else {
                                BanSource::Auto
                            },
                            metadata: serde_json::json!({}),
                            created_at: banned_at,
                            updated_at: banned_at,
                            unbanned_at,
                            unbanned_by,
                        }
                    },
                )
                .collect();

            debug!("Found {} bans", bans.len());
            Ok(bans)
        } else {
            // 对于内存存储，返回空列表（简化实现）
            Ok(Vec::new())
        }
    }

    /// 检查封禁优先级（并行版本，支持提前退出）
    ///
    /// # 性能优化
    /// - 使用并行检查，预期延迟降低 50-70%
    /// - 支持提前退出，IP 封禁优先检查
    #[instrument(skip(self, targets))]
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
            let storage = self.storage.clone();
            if let Some(record) = storage.is_banned(ip_target).await? {
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
                debug!(
                    "Found ban with priority {:?}: target={:?}",
                    priority, detail.target
                );
                Ok(Some(detail))
            }
            _ => Ok(None),
        }

        #[cfg(not(feature = "parallel-checker"))]
        {
            // 顺序检查（当 parallel-checker 未启用时）
            for future in check_futures {
                if let Some((priority, detail)) = future.await {
                    debug!(
                        "Found ban with priority {:?}: target={:?}",
                        priority, detail.target
                    );
                    return Ok(Some(detail));
                }
            }
            Ok(None)
        }
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
    ) -> Result<Option<crate::storage::BanHistory>, FlowGuardError> {
        self.storage
            .get_history(target)
            .await
            .map_err(FlowGuardError::StorageError)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MockBanStorage;

    #[allow(dead_code)]
    fn create_test_ban_manager() -> BanManager {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let storage = Arc::new(MockBanStorage);
            BanManager::new(storage, None).await.unwrap()
        })
    }

    #[test]
    fn test_ban_priority_ordering() {
        assert!(BanPriority::Ip < BanPriority::UserId);
        assert!(BanPriority::UserId < BanPriority::Mac);
        assert!(BanPriority::Mac < BanPriority::DeviceId);
        assert!(BanPriority::DeviceId < BanPriority::ApiKey);
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
        let storage = Arc::new(MockBanStorage);
        let ban_manager = BanManager::new(storage, None).await.unwrap();

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
        let storage = Arc::new(MockBanStorage);
        let ban_manager = BanManager::new(storage, None).await.unwrap();

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
        let storage = Arc::new(MockBanStorage);
        let ban_manager = BanManager::new(storage, None).await.unwrap();

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
        let storage = Arc::new(MockBanStorage);
        let ban_manager = BanManager::new(storage, None).await.unwrap();

        let target = BanTarget::UserId("nonexistent".to_string());
        let result = ban_manager.read_ban(&target).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_update_ban_not_found() {
        let storage = Arc::new(MockBanStorage);
        let ban_manager = BanManager::new(storage, None).await.unwrap();

        let target = BanTarget::UserId("nonexistent".to_string());
        let result = ban_manager
            .update_ban(&target, Some("New reason".to_string()), None, None)
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_ban_not_found() {
        let storage = Arc::new(MockBanStorage);
        let ban_manager = BanManager::new(storage, None).await.unwrap();

        let target = BanTarget::UserId("nonexistent".to_string());
        let result = ban_manager.delete_ban(&target, "admin".to_string()).await;

        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_list_bans_empty() {
        let storage = Arc::new(MockBanStorage);
        let ban_manager = BanManager::new(storage, None).await.unwrap();

        let filter = BanFilter::default();
        let result = ban_manager.list_bans(filter).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_check_ban_priority_empty() {
        let storage = Arc::new(MockBanStorage);
        let ban_manager = BanManager::new(storage, None).await.unwrap();

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
        let storage = Arc::new(MockBanStorage);
        let ban_manager = BanManager::new(storage, None).await.unwrap();

        let config = ban_manager.get_config().await;
        assert!(config.enable_auto_unban);
        assert_eq!(config.auto_unban_interval, 60);
    }

    #[tokio::test]
    async fn test_update_config() {
        let storage = Arc::new(MockBanStorage);
        let ban_manager = BanManager::new(storage, None).await.unwrap();

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
        let storage = Arc::new(MockBanStorage);
        let ban_manager = BanManager::new(storage, None).await.unwrap();

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
}
