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
use crate::storage_trait::{BanRecord, BanStorage};
use crate::BanTarget;
use chrono::{DateTime, Utc};
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
}

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

    /// 构建 BanManager 实例
    ///
    /// 如果未提供 storage，将使用内存存储作为默认依赖。
    /// 这允许使用 `BanManager::builder().build()` 进行快速原型开发。
    ///
    /// **注意**：默认内存存储不适用于多实例生产环境。
    pub async fn build(self) -> Result<BanManager, FlowGuardError> {
        use crate::storage_trait::MemoryBanStorage;

        let storage = match self.storage {
            Some(s) => s,
            None => Arc::new(MemoryBanStorage::new()),
        };
        let config = self.config.unwrap_or_default();

        BanManager::with_dependencies_and_auth(storage, config, self.authorization_provider).await
    }
}

// Re-export types for external use
pub use crate::storage_trait::BanTarget;
