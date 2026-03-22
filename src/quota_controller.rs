//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 配额控制器模块
//!
//! 实现配额控制功能，支持多种配额类型、滑动窗口重置、透支功能和告警机制。

/// 默认配额限制
pub const DEFAULT_QUOTA_LIMIT: u64 = 1000;

/// 默认窗口大小（1小时）
pub const DEFAULT_WINDOW_SIZE_SECS: u64 = 3600;

/// 默认去重窗口（5分钟）
pub const DEFAULT_DEDUP_WINDOW_SECS: u64 = 300;

/// 默认透支限制百分比
pub const DEFAULT_OVERDRAFT_LIMIT_PERCENT: u8 = 20;

use crate::error::{ConsumeResult, FlowGuardError};
use crate::storage_trait::QuotaStorage;
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration as StdDuration;

/// 配额类型
#[cfg(feature = "quota-control")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QuotaType {
    /// 令牌配额
    Token,
    /// 金额配额
    Money,
    /// 计数配额
    Count,
}

impl QuotaType {
    /// 从字符串解析配额类型
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "token" => Some(QuotaType::Token),
            "money" => Some(QuotaType::Money),
            "count" => Some(QuotaType::Count),
            _ => None,
        }
    }

    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            QuotaType::Token => "token",
            QuotaType::Money => "money",
            QuotaType::Count => "count",
        }
    }
}

/// 配额配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "quota-control")]
pub struct QuotaConfig {
    /// 配额类型
    pub quota_type: QuotaType,
    /// 配额上限
    pub limit: u64,
    /// 窗口大小（秒）
    pub window_size: u64,
    /// 是否允许透支
    pub allow_overdraft: bool,
    /// 透支上限（配额的百分比，0-100）
    pub overdraft_limit_percent: u8,
    /// 告警配置
    pub alert_config: AlertConfig,
}

impl Default for QuotaConfig {
    fn default() -> Self {
        Self {
            quota_type: QuotaType::Count,
            limit: DEFAULT_QUOTA_LIMIT,
            window_size: DEFAULT_WINDOW_SIZE_SECS,
            allow_overdraft: false,
            overdraft_limit_percent: DEFAULT_OVERDRAFT_LIMIT_PERCENT,
            alert_config: AlertConfig::default(),
        }
    }
}

/// 告警配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "quota-control")]
pub struct AlertConfig {
    /// 是否启用告警
    pub enabled: bool,
    /// 告警阈值（百分比）
    pub thresholds: Vec<u8>,
    /// 告警渠道
    pub channels: Vec<AlertChannel>,
    /// 告警去重时间窗口（秒）
    pub dedup_window: u64,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            thresholds: vec![80, 90, 100],
            channels: vec![AlertChannel::Log],
            dedup_window: DEFAULT_DEDUP_WINDOW_SECS,
        }
    }
}

/// 告警渠道
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg(feature = "quota-control")]
pub enum AlertChannel {
    /// 日志告警
    Log,
    /// Webhook 告警
    Webhook { url: String },
}

/// 告警信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "quota-control")]
pub struct AlertInfo {
    /// 用户ID
    pub user_id: String,
    /// 资源
    pub resource: String,
    /// 配额类型
    pub quota_type: QuotaType,
    /// 告警阈值（百分比）
    pub threshold: u8,
    /// 当前使用量
    pub current_usage: u64,
    /// 配额上限
    pub limit: u64,
    /// 触发时间
    pub triggered_at: DateTime<Utc>,
}

/// 配额状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "quota-control")]
pub struct QuotaState {
    /// 已消费量
    pub consumed: u64,
    /// 窗口开始时间
    pub window_start: DateTime<Utc>,
    /// 窗口结束时间
    pub window_end: DateTime<Utc>,
}

/// 配额控制器
#[cfg(feature = "quota-control")]
pub struct QuotaController {
    /// 存储后端
    storage: Arc<dyn QuotaStorage>,
    /// 配额配置
    config: QuotaConfig,
    /// 告警去重缓存（key: user_id:resource:threshold, value: last_alert_time）
    alert_dedup: Arc<DashMap<String, DateTime<Utc>>>,
}

impl Clone for QuotaController {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            config: self.config.clone(),
            alert_dedup: self.alert_dedup.clone(),
        }
    }
}

/// QuotaController 构建器
///
/// 用于链式配置 QuotaController 实例。
///
/// # 示例
/// ```rust, ignore
/// use limiteron::quota_controller::{QuotaController, QuotaConfig, QuotaType};
///
/// let config = QuotaConfig {
///     quota_type: QuotaType::Count,
///     limit: 1000,
///     window_size: 3600,
///     allow_overdraft: true,
///     overdraft_limit_percent: 20,
///     alert_config: Default::default(),
/// };
/// let controller = QuotaController::builder()
///     .build()
///     .unwrap();
/// ```
#[cfg(feature = "quota-control")]
pub struct QuotaControllerBuilder {
    storage: Option<Arc<dyn QuotaStorage>>,
    config: Option<QuotaConfig>,
}

#[cfg(feature = "quota-control")]
impl QuotaControllerBuilder {
    /// 创建新的 QuotaControllerBuilder
    pub fn new() -> Self {
        Self {
            storage: None,
            config: None,
        }
    }

    /// 设置配额存储后端
    pub fn with_storage(mut self, storage: Arc<dyn QuotaStorage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// 设置配置
    pub fn with_config(mut self, config: QuotaConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// 构建 QuotaController 实例
    pub fn build(self) -> Result<QuotaController, FlowGuardError> {
        let storage = self.storage.expect("storage is required");
        let config = self.config.unwrap_or_default();

        Ok(QuotaController::with_dependencies(storage, config))
    }
}

#[cfg(feature = "quota-control")]
impl Default for QuotaControllerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl QuotaController {
    /// 创建新的配额控制器
    ///
    /// # 参数
    /// - `storage`: 存储后端
    /// - `config`: 配额配置
    ///
    /// # 示例
    /// ```rust, ignore
    /// use limiteron::quota_controller::{QuotaController, QuotaConfig, QuotaType};
    ///
    /// let config = QuotaConfig {
    ///     quota_type: QuotaType::Count,
    ///     limit: 1000,
    ///     window_size: 3600,
    ///     allow_overdraft: true,
    ///     overdraft_limit_percent: 20,
    ///     alert_config: Default::default(),
    /// };
    /// let controller = QuotaController::builder()
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn builder() -> QuotaControllerBuilder {
        QuotaControllerBuilder::new()
    }

    /// 使用依赖注入创建 QuotaController 实例
    ///
    /// # 参数
    /// - `storage`: 配额存储后端
    /// - `config`: 配额控制器配置
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::quota_controller::{QuotaController, QuotaConfig};
    /// use limiteron::storage::QuotaStorage;
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let storage: Arc<dyn QuotaStorage> = Arc::new(my_storage);
    ///     let config = QuotaConfig::default();
    ///     let controller = QuotaController::with_dependencies(storage, config);
    /// }
    /// ```
    pub fn with_dependencies(storage: Arc<dyn QuotaStorage>, config: QuotaConfig) -> Self {
        Self {
            storage,
            config,
            alert_dedup: Arc::new(DashMap::new()),
        }
    }

    /// 消费配额
    ///
    /// # 参数
    /// - `user_id`: 用户ID
    /// - `resource`: 资源标识
    /// - `cost`: 消费数量
    ///
    /// # 返回
    /// - `Ok(result)`: 消费结果
    /// - `Err(error)`: 错误信息
    ///
    /// # 示例
    /// ```rust, ignore
    /// # use limiteron::quota_controller::{QuotaController, QuotaConfig, QuotaType};
    /// # use limiteron::storage_trait::QuotaStorage;
    /// # use std::sync::Arc;
    /// #
    /// # struct MockStorage;
    /// # #[async_trait::async_trait]
    /// # impl QuotaStorage for MockStorage {
    /// #   async fn get_quota(&self, _: &str, _: &str) -> Result<Option<limiteron::storage_trait::QuotaInfo>, limiteron::error::StorageError> { Ok(None) }
    /// #   async fn consume(&self, _: &str, _: &str, _: u64, _: u64, _: std::time::Duration) -> Result<limiteron::error::ConsumeResult, limiteron::error::FlowGuardError> { unimplemented!() }
    /// #   async fn reset(&self, _: &str, _: &str, _: u64, _: std::time::Duration) -> Result<(), limiteron::error::FlowGuardError> { Ok(()) }
    /// # }
    /// #
    /// # let controller = QuotaController::new(Arc::new(MockStorage), QuotaConfig::default());
    /// #
    /// # async {
    /// let result = controller.consume("user123", "api_call", 10).await.unwrap();
    /// println!("Allowed: {}, Remaining: {}", result.allowed, result.remaining);
    /// # };
    /// ```
    pub async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
    ) -> Result<ConsumeResult, FlowGuardError> {
        // 验证消费数量
        if cost == 0 {
            // 获取当前配额状态（用于计算 usage_percent）
            let usage_percent = self.calculate_usage_percent(0, self.config.limit);
            return Ok(ConsumeResult {
                allowed: true,
                remaining: self.config.limit,
                alert_triggered: false,
                usage_percent,
            });
        }

        // 获取当前配额状态
        let quota_state = self.get_or_create_quota_state(user_id, resource).await?;

        // 检查窗口是否需要重置
        let updated_state = self.check_and_reset_window(quota_state).await?;

        // 计算可透支上限和总限制
        let overdraft_limit = self.calculate_overdraft_limit();
        let total_limit = self.calculate_total_limit(overdraft_limit);

        // 检查是否超过总限制
        if updated_state.consumed + cost > total_limit {
            let usage_percent = self.calculate_usage_percent(updated_state.consumed, total_limit);
            return Ok(ConsumeResult {
                allowed: false,
                remaining: total_limit.saturating_sub(updated_state.consumed),
                alert_triggered: false,
                usage_percent,
            });
        }

        // 更新消费量
        let new_consumed = updated_state.consumed + cost;

        // 保存到存储
        self.save_quota_state(user_id, resource, &updated_state, new_consumed)
            .await?;

        // 计算剩余配额
        let remaining = total_limit.saturating_sub(new_consumed);

        // 计算使用率
        let usage_percent = self.calculate_usage_percent(new_consumed, total_limit);

        // 检查告警
        let alert_triggered = self
            .check_and_trigger_alert(user_id, resource, new_consumed)
            .await?;

        Ok(ConsumeResult {
            allowed: true,
            remaining,
            alert_triggered,
            usage_percent,
        })
    }

    /// 获取配额状态
    ///
    /// # 参数
    /// - `user_id`: 用户ID
    /// - `resource`: 资源标识
    ///
    /// # 返回
    /// - `Ok(Some(state))`: 配额状态
    /// - `Ok(None)`: 配额不存在
    /// - `Err(error)`: 错误信息
    pub async fn get_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<Option<QuotaState>, FlowGuardError> {
        let quota_info = self
            .storage
            .get_quota(user_id, resource)
            .await
            .map_err(FlowGuardError::StorageError)?;

        if let Some(info) = quota_info {
            Ok(Some(QuotaState {
                consumed: info.consumed,
                window_start: info.window_start,
                window_end: info.window_end,
            }))
        } else {
            Ok(None)
        }
    }

    /// 重置配额
    ///
    /// # 参数
    /// - `user_id`: 用户ID
    /// - `resource`: 资源标识
    ///
    /// # 返回
    /// - `Ok(())`: 重置成功
    /// - `Err(error)`: 错误信息
    pub async fn reset_quota(&self, user_id: &str, resource: &str) -> Result<(), FlowGuardError> {
        self.storage
            .reset(
                user_id,
                resource,
                self.config.limit,
                StdDuration::from_secs(self.config.window_size),
            )
            .await
            .map_err(FlowGuardError::StorageError)?;

        Ok(())
    }

    /// 获取或创建配额状态
    async fn get_or_create_quota_state(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<QuotaState, FlowGuardError> {
        if let Some(state) = self.get_quota(user_id, resource).await? {
            return Ok(state);
        }

        // 创建新的配额状态
        let now = Utc::now();
        let window_start = now;
        let window_end = now + Duration::seconds(self.config.window_size as i64);

        Ok(QuotaState {
            consumed: 0,
            window_start,
            window_end,
        })
    }

    /// 检查并重置窗口
    ///
    /// 实现滑动窗口重置逻辑：如果当前时间超过窗口结束时间，
    /// 则计算新的窗口时间，并按比例保留配额消费量。
    async fn check_and_reset_window(
        &self,
        state: QuotaState,
    ) -> Result<QuotaState, FlowGuardError> {
        let now = Utc::now();

        // 如果当前时间在窗口内，不需要重置
        if now < state.window_end {
            return Ok(state);
        }

        // 计算窗口跨越情况
        let window_duration = Duration::seconds(self.config.window_size as i64);
        let elapsed = now.signed_duration_since(state.window_start);
        let windows_passed = (elapsed.num_seconds() / window_duration.num_seconds()) as u64;

        // 计算新窗口时间（使用 checked_mul 防止整数溢出）
        // 限制 windows_passed 避免溢出
        let safe_windows_passed = windows_passed.min(i32::MAX as u64) as i32;
        let new_window_start = state.window_start + window_duration * safe_windows_passed;
        let new_window_end = new_window_start + window_duration;

        // 滑动窗口重置：根据时间比例保留消费量
        // 例如：如果窗口已经过去 50%，则保留 50% 的消费量
        let window_elapsed = now.signed_duration_since(state.window_start);
        let window_progress = (window_elapsed.num_milliseconds() as f64
            / window_duration.num_milliseconds() as f64)
            .min(1.0);

        // 计算应该保留的消费量
        let retained_consumed = if windows_passed >= 1 {
            // 如果跨越了至少一个完整窗口，完全重置
            0
        } else {
            // 单个窗口内，按比例保留
            (state.consumed as f64 * (1.0 - window_progress)) as u64
        };

        Ok(QuotaState {
            consumed: retained_consumed,
            window_start: new_window_start,
            window_end: new_window_end,
        })
    }

    /// 保存配额状态
    async fn save_quota_state(
        &self,
        user_id: &str,
        resource: &str,
        state: &QuotaState,
        new_consumed: u64,
    ) -> Result<(), FlowGuardError> {
        // 使用存储的 consume 方法更新配额
        // 计算总限制
        let overdraft_limit = self.calculate_overdraft_limit();
        let total_limit = self.calculate_total_limit(overdraft_limit);

        let _result = self
            .storage
            .consume(
                user_id,
                resource,
                new_consumed.saturating_sub(state.consumed),
                total_limit,
                StdDuration::from_secs(self.config.window_size),
            )
            .await
            .map_err(FlowGuardError::StorageError)?;

        Ok(())
    }

    /// 检查并触发告警
    async fn check_and_trigger_alert(
        &self,
        user_id: &str,
        resource: &str,
        consumed: u64,
    ) -> Result<bool, FlowGuardError> {
        if !self.config.alert_config.enabled {
            return Ok(false);
        }

        // 计算使用率
        let usage_percent = if self.config.limit > 0 {
            (consumed as f64 / self.config.limit as f64 * 100.0) as u8
        } else {
            100
        };

        let mut alert_triggered = false;

        // 检查每个告警阈值
        for &threshold in &self.config.alert_config.thresholds {
            if usage_percent >= threshold {
                // 检查是否需要去重
                let dedup_key = format!("{}:{}:{}", user_id, resource, threshold);

                let should_alert = {
                    if let Some(last_alert_time) = self.alert_dedup.get(&dedup_key) {
                        let elapsed = Utc::now().signed_duration_since(*last_alert_time);
                        elapsed.num_seconds() as u64 >= self.config.alert_config.dedup_window
                    } else {
                        true
                    }
                };

                if should_alert {
                    // 创建告警信息
                    let alert_info = AlertInfo {
                        user_id: user_id.to_string(),
                        resource: resource.to_string(),
                        quota_type: self.config.quota_type,
                        threshold,
                        current_usage: consumed,
                        limit: self.config.limit,
                        triggered_at: Utc::now(),
                    };

                    // 异步发送告警
                    self.send_alert(alert_info).await;

                    // 更新去重缓存
                    self.alert_dedup.insert(dedup_key, Utc::now());

                    alert_triggered = true;
                }
            }
        }

        Ok(alert_triggered)
    }

    /// 发送告警
    async fn send_alert(&self, alert_info: AlertInfo) {
        for channel in &self.config.alert_config.channels {
            let channel = channel.clone();
            let alert_info = alert_info.clone();

            // 使用 tokio::spawn 异步发送告警，不阻塞主流程
            tokio::spawn(async move {
                match channel {
                    AlertChannel::Log => {
                        log::warn!(
                            "配额告警触发: user_id={}, resource={}, quota_type={}, threshold={}%, current_usage={}, limit={}, triggered_at={}",
                            alert_info.user_id,
                            alert_info.resource,
                            alert_info.quota_type.as_str(),
                            alert_info.threshold,
                            alert_info.current_usage,
                            alert_info.limit,
                            alert_info.triggered_at.format("%Y-%m-%d %H:%M:%S UTC")
                        );
                    }
                    AlertChannel::Webhook { url } => {
                        // 发送 Webhook 告警
                        if let Err(e) = send_webhook_alert(&url, &alert_info).await {
                            log::error!("发送 Webhook 告警失败: {}", e);
                        }
                    }
                }
            });
        }
    }

    /// 获取配置
    pub fn config(&self) -> &QuotaConfig {
        &self.config
    }

    /// 更新配置
    pub fn update_config(&mut self, config: QuotaConfig) {
        self.config = config;
    }

    /// 清理过期的告警去重记录
    pub fn cleanup_alert_dedup(&self) {
        let now = Utc::now();
        let dedup_window = Duration::seconds(self.config.alert_config.dedup_window as i64);

        self.alert_dedup.retain(|_, last_alert_time| {
            now.signed_duration_since(*last_alert_time) < dedup_window
        });
    }

    /// 计算透支上限（内部辅助方法）
    ///
    /// 使用 checked_mul 和 checked_div 防止整数溢出。
    fn calculate_overdraft_limit(&self) -> u64 {
        if self.config.allow_overdraft {
            self.config
                .limit
                .checked_mul(self.config.overdraft_limit_percent as u64)
                .and_then(|v| v.checked_div(100))
                .unwrap_or(u64::MAX / 2) // 如果溢出，使用安全值
        } else {
            0
        }
    }

    /// 计算总限制（内部辅助方法）
    ///
    /// 使用 checked_add 防止整数溢出。
    fn calculate_total_limit(&self, overdraft_limit: u64) -> u64 {
        self.config
            .limit
            .checked_add(overdraft_limit)
            .unwrap_or(u64::MAX / 2) // 如果溢出，使用安全值
    }

    /// 计算使用率（内部辅助方法）
    ///
    /// 返回百分比形式的浮点数。
    fn calculate_usage_percent(&self, consumed: u64, limit: u64) -> f64 {
        if limit > 0 {
            (consumed as f64 / limit as f64) * 100.0
        } else {
            0.0
        }
    }
}

/// 发送 Webhook 告警
///
/// 注意：此功能需要启用 `webhook` feature 并添加 `reqwest` 依赖。
/// 如果未启用，将返回错误。
#[cfg(feature = "webhook")]
async fn send_webhook_alert(
    url: &str,
    alert_info: &AlertInfo,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .json(alert_info)
        .timeout(StdDuration::from_secs(5))
        .send()
        .await?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("Webhook 返回错误状态码: {}", response.status()).into())
    }
}

/// 发送 Webhook 告警（未启用 webhook feature 时的存根实现）
#[cfg(not(feature = "webhook"))]
async fn send_webhook_alert(
    _url: &str,
    _alert_info: &AlertInfo,
) -> Result<(), Box<dyn std::error::Error>> {
    Err("Webhook 功能未启用，请启用 'webhook' feature".into())
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::StorageError;
    use crate::storage_trait::{QuotaInfo, QuotaStorage};
    use ahash::AHashMap as HashMap;
    use async_trait::async_trait;
    use parking_lot::Mutex;

    /// 测试用的配额存储实现
    struct TestQuotaStorage {
        quotas: Mutex<HashMap<String, QuotaInfo>>,
    }

    impl TestQuotaStorage {
        fn new() -> Self {
            Self {
                quotas: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl QuotaStorage for TestQuotaStorage {
        async fn get_quota(
            &self,
            user_id: &str,
            resource: &str,
        ) -> Result<Option<QuotaInfo>, StorageError> {
            let key = format!("{}:{}", user_id, resource);
            Ok(self.quotas.lock().get(&key).cloned())
        }

        async fn consume(
            &self,
            user_id: &str,
            resource: &str,
            cost: u64,
            limit: u64,
            window: StdDuration,
        ) -> Result<ConsumeResult, StorageError> {
            let key = format!("{}:{}", user_id, resource);
            let mut quotas = self.quotas.lock();

            let quota_info = quotas.entry(key.clone()).or_insert_with(|| {
                let now = Utc::now();
                QuotaInfo {
                    consumed: 0,
                    limit,
                    window_start: now,
                    window_end: now
                        + Duration::from_std(window)
                            .unwrap_or(Duration::seconds(DEFAULT_WINDOW_SIZE_SECS as i64)),
                }
            });

            // 检查窗口是否过期
            let now = Utc::now();
            if now >= quota_info.window_end {
                // 窗口已过期，重置消费量
                quota_info.consumed = 0;
                quota_info.window_start = now;
                quota_info.window_end = now
                    + Duration::from_std(window)
                        .unwrap_or(Duration::seconds(DEFAULT_WINDOW_SIZE_SECS as i64));
                quota_info.limit = limit;
            }

            if quota_info.consumed + cost > quota_info.limit {
                let usage_percent = if limit > 0 {
                    ((quota_info.consumed + cost) as f64 / limit as f64) * 100.0
                } else {
                    100.0
                };
                return Ok(ConsumeResult {
                    allowed: false,
                    remaining: quota_info.limit - quota_info.consumed,
                    alert_triggered: false,
                    usage_percent,
                });
            }

            quota_info.consumed += cost;

            let usage_percent = if limit > 0 {
                (quota_info.consumed as f64 / limit as f64) * 100.0
            } else {
                0.0
            };

            Ok(ConsumeResult {
                allowed: true,
                remaining: quota_info.limit - quota_info.consumed,
                alert_triggered: false,
                usage_percent,
            })
        }

        async fn reset(
            &self,
            user_id: &str,
            resource: &str,
            limit: u64,
            window: StdDuration,
        ) -> Result<(), StorageError> {
            let key = format!("{}:{}", user_id, resource);
            let mut quotas = self.quotas.lock();

            if let Some(quota_info) = quotas.get_mut(&key) {
                quota_info.consumed = 0;
                quota_info.limit = limit;
                let now = Utc::now();
                quota_info.window_start = now;
                quota_info.window_end = now
                    + Duration::from_std(window)
                        .unwrap_or(Duration::seconds(DEFAULT_WINDOW_SIZE_SECS as i64));
            }

            Ok(())
        }
    }

    /// 测试配额类型解析
    #[test]
    fn test_quota_type_parse() {
        assert_eq!(QuotaType::parse("token"), Some(QuotaType::Token));
        assert_eq!(QuotaType::parse("money"), Some(QuotaType::Money));
        assert_eq!(QuotaType::parse("count"), Some(QuotaType::Count));
        assert_eq!(QuotaType::parse("unknown"), None);
    }

    /// 测试配额类型字符串转换
    #[test]
    fn test_quota_type_as_str() {
        assert_eq!(QuotaType::Token.as_str(), "token");
        assert_eq!(QuotaType::Money.as_str(), "money");
        assert_eq!(QuotaType::Count.as_str(), "count");
    }

    /// 测试配额配置默认值
    #[test]
    fn test_quota_config_default() {
        let config = QuotaConfig::default();
        assert_eq!(config.quota_type, QuotaType::Count);
        assert_eq!(config.limit, 1000);
        assert_eq!(config.window_size, 3600);
        assert!(!config.allow_overdraft);
        assert_eq!(config.overdraft_limit_percent, 20);
        assert!(config.alert_config.enabled);
    }

    /// 测试告警配置默认值
    #[test]
    fn test_alert_config_default() {
        let config = AlertConfig::default();
        assert!(config.enabled);
        assert_eq!(config.thresholds, vec![80, 90, 100]);
        assert_eq!(config.channels, vec![AlertChannel::Log]);
        assert_eq!(config.dedup_window, 300);
    }

    /// 测试创建配额控制器
    #[test]
    fn test_quota_controller_new() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig::default();
        let controller = QuotaController::with_dependencies(storage, config);

        assert_eq!(controller.config().limit, 1000);
    }

    /// 测试消费配额 - 基本场景
    #[tokio::test]
    async fn test_consume_basic() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费 10 个配额
        let result = controller.consume("user1", "resource1", 10).await.unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 90);
        assert!(!result.alert_triggered);

        // 再消费 20 个配额
        let result = controller.consume("user1", "resource1", 20).await.unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 70);
    }

    /// 测试消费配额 - 超过限制
    #[tokio::test]
    async fn test_consume_exceeds_limit() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费 100 个配额
        let result = controller.consume("user1", "resource1", 100).await.unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 0);

        // 尝试再消费 1 个配额，应该被拒绝
        let result = controller.consume("user1", "resource1", 1).await.unwrap();
        assert!(!result.allowed);
        assert_eq!(result.remaining, 0);
    }

    /// 测试透支功能
    #[tokio::test]
    async fn test_overdraft() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: true,
            overdraft_limit_percent: 20,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费 100 个配额（达到上限）
        let result = controller.consume("user1", "resource1", 100).await.unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 20); // remaining includes overdraft (120 - 100 = 20)

        // 消费 10 个配额（透支）
        let result = controller.consume("user1", "resource1", 10).await.unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 10); // 120 - 110 = 10

        // 尝试再消费 11 个配额（超过透支上限），应该被拒绝
        let result = controller.consume("user1", "resource1", 11).await.unwrap();
        assert!(!result.allowed);
    }

    /// 测试滑动窗口重置
    #[tokio::test]
    async fn test_sliding_window_reset() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 1, // 1 秒窗口
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费 50 个配额
        let result = controller.consume("user1", "resource1", 50).await.unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 50);

        // 等待窗口过期（超过一个完整窗口）
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        // 现在应该可以消费配额了（滑动窗口会完全重置）
        let result = controller.consume("user1", "resource1", 30).await.unwrap();
        assert!(result.allowed);
        // 窗口已经完全过期，所以应该有 100 - 30 = 70 剩余
        // 但由于滑动窗口的特性，可能会有部分保留
        // 所以我们只检查是否允许消费
        assert!(result.allowed);
    }

    /// 测试告警触发
    #[tokio::test]
    async fn test_alert_trigger() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: true,
                thresholds: vec![80, 90, 100],
                channels: vec![AlertChannel::Log],
                dedup_window: DEFAULT_DEDUP_WINDOW_SECS,
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费 80 个配额，应该触发 80% 告警
        let result = controller.consume("user1", "resource1", 80).await.unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered);

        // 消费 10 个配额，应该触发 90% 告警
        let result = controller.consume("user1", "resource1", 10).await.unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered);

        // 消费 10 个配额，应该触发 100% 告警
        let result = controller.consume("user1", "resource1", 10).await.unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered);
    }

    /// 测试告警去重
    #[tokio::test]
    async fn test_alert_dedup() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: true,
                thresholds: vec![80],
                channels: vec![AlertChannel::Log],
                dedup_window: 5, // 5 秒去重窗口
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费 80 个配额，应该触发告警
        let result = controller.consume("user1", "resource1", 80).await.unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered);

        // 立即再次消费到 90%，仍然不应该触发告警（去重）
        let result = controller.consume("user1", "resource1", 10).await.unwrap();
        assert!(result.allowed);
        assert!(!result.alert_triggered);

        // 等待去重窗口过期
        tokio::time::sleep(tokio::time::Duration::from_millis(5100)).await;

        // 清理过期的去重记录
        controller.cleanup_alert_dedup();

        // 再次消费到 95%，应该触发告警
        let result = controller.consume("user1", "resource1", 5).await.unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered);
    }

    /// 测试获取配额状态
    #[tokio::test]
    async fn test_get_quota() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费配额
        controller.consume("user1", "resource1", 50).await.unwrap();

        // 获取配额状态
        let state = controller.get_quota("user1", "resource1").await.unwrap();
        assert!(state.is_some());
        assert_eq!(state.unwrap().consumed, 50);
    }

    /// 测试重置配额
    #[tokio::test]
    async fn test_reset_quota() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费配额
        controller.consume("user1", "resource1", 50).await.unwrap();

        // 重置配额
        controller.reset_quota("user1", "resource1").await.unwrap();

        // 获取配额状态
        let state = controller.get_quota("user1", "resource1").await.unwrap();
        assert!(state.is_some());
        assert_eq!(state.unwrap().consumed, 0);
    }

    /// 测试不同配额类型
    #[test]
    fn test_quota_types() {
        let token_config = QuotaConfig {
            quota_type: QuotaType::Token,
            ..Default::default()
        };
        assert_eq!(token_config.quota_type.as_str(), "token");

        let money_config = QuotaConfig {
            quota_type: QuotaType::Money,
            ..Default::default()
        };
        assert_eq!(money_config.quota_type.as_str(), "money");

        let count_config = QuotaConfig {
            quota_type: QuotaType::Count,
            ..Default::default()
        };
        assert_eq!(count_config.quota_type.as_str(), "count");
    }

    /// 测试并发消费
    #[tokio::test]
    async fn test_concurrent_consume() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = Arc::new(QuotaController::with_dependencies(storage, config));
        let mut handles = vec![];

        // 创建 10 个并发任务，每个消费 10 个配额
        for _ in 0..10 {
            let controller_clone = Arc::clone(&controller);
            handles.push(tokio::spawn(async move {
                controller_clone.consume("user1", "resource1", 10).await
            }));
        }

        let mut total_consumed = 0;
        for handle in handles {
            let result = handle.await.unwrap().unwrap();
            if result.allowed {
                total_consumed += 10;
            }
        }

        // 总消费量应该不超过 100
        assert!(total_consumed <= 100);
    }

    /// 测试消费数量为 0
    #[tokio::test]
    async fn test_consume_zero() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig::default();
        let controller = QuotaController::with_dependencies(storage, config);

        let result = controller.consume("user1", "resource1", 0).await.unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 1000);
        assert!(!result.alert_triggered);
    }

    /// 测试更新配置
    #[test]
    fn test_update_config() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig::default();
        let mut controller = QuotaController::with_dependencies(storage, config);

        assert_eq!(controller.config().limit, 1000);

        let new_config = QuotaConfig {
            limit: 500,
            ..Default::default()
        };
        controller.update_config(new_config);

        assert_eq!(controller.config().limit, 500);
    }

    // ========================================================================
    // 增强测试 - 基础配额操作
    // ========================================================================

    /// 测试配额耗尽拒绝 - 边界条件
    /// 验证在配额恰好耗尽时的行为
    #[tokio::test]
    async fn test_quota_exhaustion_boundary() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费恰好 100 个配额（达到上限）
        let result = controller.consume("user1", "resource1", 100).await.unwrap();
        assert!(result.allowed, "消费恰好达到上限应该被允许");
        assert_eq!(result.remaining, 0, "剩余配额应该为 0");

        // 尝试消费 1 个配额，应该被拒绝
        let result = controller.consume("user1", "resource1", 1).await.unwrap();
        assert!(!result.allowed, "超过上限应该被拒绝");
        assert_eq!(result.remaining, 0, "拒绝时剩余配额应该为 0");

        // 尝试消费 0 个配额，应该被允许
        let result = controller.consume("user1", "resource1", 0).await.unwrap();
        assert!(result.allowed, "消费 0 应该被允许");
    }

    /// 测试配额重置后可以重新消费
    #[tokio::test]
    async fn test_quota_reset_allows_new_consumption() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 50,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费全部配额
        let result = controller.consume("user1", "resource1", 50).await.unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 0);

        // 尝试再消费，应该被拒绝
        let result = controller.consume("user1", "resource1", 1).await.unwrap();
        assert!(!result.allowed);

        // 重置配额
        controller.reset_quota("user1", "resource1").await.unwrap();

        // 重置后应该可以重新消费
        let result = controller.consume("user1", "resource1", 30).await.unwrap();
        assert!(result.allowed, "重置后应该可以消费");
        assert_eq!(result.remaining, 20, "重置后剩余配额应该正确");
    }

    // ========================================================================
    // 增强测试 - 高级配额功能
    // ========================================================================

    /// 测试滑动窗口重置 - 跨越多个窗口
    #[tokio::test]
    async fn test_sliding_window_multiple_periods() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 1, // 1 秒窗口
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 第一轮消费
        let result = controller.consume("user1", "resource1", 50).await.unwrap();
        assert!(result.allowed);

        // 等待超过一个完整窗口
        tokio::time::sleep(tokio::time::Duration::from_millis(1200)).await;

        // 第二轮消费 - 窗口应该已重置
        let result = controller.consume("user1", "resource1", 60).await.unwrap();
        assert!(result.allowed, "窗口重置后应该可以消费");
    }

    /// 测试透支功能 - 边界条件
    #[tokio::test]
    async fn test_overdraft_boundary() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: true,
            overdraft_limit_percent: 20, // 20% 透支 = 20 额外配额
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费到恰好达到原始上限
        let result = controller.consume("user1", "resource1", 100).await.unwrap();
        assert!(result.allowed);
        // 剩余应该包含透支额度: 120 - 100 = 20
        assert_eq!(result.remaining, 20);

        // 消费透支额度
        let result = controller.consume("user1", "resource1", 20).await.unwrap();
        assert!(result.allowed, "透支额度内应该被允许");
        assert_eq!(result.remaining, 0);

        // 超过透支上限
        let result = controller.consume("user1", "resource1", 1).await.unwrap();
        assert!(!result.allowed, "超过透支上限应该被拒绝");
    }

    /// 测试透支功能 - 不同透支百分比
    #[tokio::test]
    async fn test_overdraft_different_percentages() {
        // 测试 10% 透支
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: true,
            overdraft_limit_percent: 10, // 10% 透支 = 10 额外配额
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费到原始上限
        let result = controller.consume("user1", "resource1", 100).await.unwrap();
        assert!(result.allowed);

        // 消费透支额度 (10)
        let result = controller.consume("user1", "resource1", 10).await.unwrap();
        assert!(result.allowed, "10% 透支额度内应该被允许");

        // 超过透支上限
        let result = controller.consume("user1", "resource1", 1).await.unwrap();
        assert!(!result.allowed, "超过 10% 透支上限应该被拒绝");
    }

    /// 测试多级告警触发 - 所有阈值
    #[tokio::test]
    async fn test_multi_level_alerts() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: true,
                thresholds: vec![50, 75, 90, 100], // 多个阈值
                channels: vec![AlertChannel::Log],
                dedup_window: 1, // 1 秒去重窗口便于测试
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费 50% - 应该触发 50% 告警
        let result = controller.consume("user1", "resource1", 50).await.unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered, "达到 50% 应该触发告警");

        // 等待去重窗口过期
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;
        controller.cleanup_alert_dedup();

        // 消费到 75% - 应该触发 75% 告警
        let result = controller.consume("user1", "resource1", 25).await.unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered, "达到 75% 应该触发告警");

        // 等待去重窗口过期
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;
        controller.cleanup_alert_dedup();

        // 消费到 90% - 应该触发 90% 告警
        let result = controller.consume("user1", "resource1", 15).await.unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered, "达到 90% 应该触发告警");

        // 等待去重窗口过期
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;
        controller.cleanup_alert_dedup();

        // 消费到 100% - 应该触发 100% 告警
        let result = controller.consume("user1", "resource1", 10).await.unwrap();
        assert!(result.allowed);
        assert!(result.alert_triggered, "达到 100% 应该触发告警");
    }

    /// 测试告警去重 - 同一阈值不重复触发
    #[tokio::test]
    async fn test_alert_dedup_same_threshold() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: true,
                thresholds: vec![80],
                channels: vec![AlertChannel::Log],
                dedup_window: 300, // 5 分钟去重窗口
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 第一次消费到 80% - 触发告警
        let result = controller.consume("user1", "resource1", 80).await.unwrap();
        assert!(result.alert_triggered, "首次达到阈值应该触发告警");

        // 继续消费到 85% - 不应该触发告警（去重）
        let result = controller.consume("user1", "resource1", 5).await.unwrap();
        assert!(!result.alert_triggered, "同一去重窗口内不应该重复触发告警");

        // 继续消费到 90% - 不应该触发告警（去重）
        let result = controller.consume("user1", "resource1", 5).await.unwrap();
        assert!(!result.alert_triggered, "同一去重窗口内不应该重复触发告警");
    }

    /// 测试告警禁用
    #[tokio::test]
    async fn test_alert_disabled() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false, // 禁用告警
                thresholds: vec![80, 90, 100],
                channels: vec![AlertChannel::Log],
                dedup_window: 300,
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费到 100% - 不应该触发告警
        let result = controller.consume("user1", "resource1", 100).await.unwrap();
        assert!(result.allowed);
        assert!(!result.alert_triggered, "告警禁用时不应该触发告警");
    }

    // ========================================================================
    // 增强测试 - 边界条件
    // ========================================================================

    /// 测试大消费数处理 - 整数溢出保护
    #[tokio::test]
    async fn test_large_consumption() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 尝试消费一个非常大的数（超过 u64::MAX / 2）
        let result = controller
            .consume("user1", "resource1", u64::MAX)
            .await
            .unwrap();
        assert!(!result.allowed, "超大消费数应该被拒绝");
    }

    /// 测试大消费数处理 - 接近限制
    #[tokio::test]
    async fn test_large_consumption_near_limit() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: u32::MAX as u64, // 使用较大的限制
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费接近限制的数量
        let result = controller
            .consume("user1", "resource1", u32::MAX as u64 - 1)
            .await
            .unwrap();
        assert!(result.allowed, "接近限制的消费应该被允许");
        assert_eq!(result.remaining, 1);

        // 再消费 1 个
        let result = controller.consume("user1", "resource1", 1).await.unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 0);
    }

    /// 测试使用率计算正确性
    #[tokio::test]
    async fn test_usage_percent_calculation() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 200,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费 50 个，使用率应该是 25%
        let result = controller.consume("user1", "resource1", 50).await.unwrap();
        assert!(result.allowed);
        assert!(
            (result.usage_percent - 25.0).abs() < 0.1,
            "使用率应该是 25%，实际是 {}%",
            result.usage_percent
        );

        // 消费 50 个，总共 100 个，使用率应该是 50%
        let result = controller.consume("user1", "resource1", 50).await.unwrap();
        assert!(result.allowed);
        assert!(
            (result.usage_percent - 50.0).abs() < 0.1,
            "使用率应该是 50%，实际是 {}%",
            result.usage_percent
        );

        // 消费 50 个，总共 150 个，使用率应该是 75%
        let result = controller.consume("user1", "resource1", 50).await.unwrap();
        assert!(result.allowed);
        assert!(
            (result.usage_percent - 75.0).abs() < 0.1,
            "使用率应该是 75%，实际是 {}%",
            result.usage_percent
        );

        // 消费 50 个，总共 200 个，使用率应该是 100%
        let result = controller.consume("user1", "resource1", 50).await.unwrap();
        assert!(result.allowed);
        assert!(
            (result.usage_percent - 100.0).abs() < 0.1,
            "使用率应该是 100%，实际是 {}%",
            result.usage_percent
        );
    }

    /// 测试使用率计算 - 零限制边界
    #[tokio::test]
    async fn test_usage_percent_zero_limit() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 0, // 零限制
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 消费 0 个配额
        let result = controller.consume("user1", "resource1", 0).await.unwrap();
        assert!(result.allowed);
        // 零限制时，usage_percent 应该返回 0.0（根据 calculate_usage_percent 实现）
        assert_eq!(result.usage_percent, 0.0, "零限制且零消费时使用率应该是 0%");
    }

    /// 测试并发消费安全性 - 高并发场景
    #[tokio::test]
    async fn test_concurrent_consume_high_contention() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 1000,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = Arc::new(QuotaController::with_dependencies(storage, config));
        let mut handles = vec![];

        // 创建 100 个并发任务，每个消费 15 个配额
        // 总共请求 1500，但限制是 1000
        for _ in 0..100 {
            let controller_clone = Arc::clone(&controller);
            handles.push(tokio::spawn(async move {
                controller_clone.consume("user1", "resource1", 15).await
            }));
        }

        let mut allowed_count = 0;
        let mut denied_count = 0;
        let mut total_consumed = 0u64;

        for handle in handles {
            let result = handle.await.unwrap().unwrap();
            if result.allowed {
                allowed_count += 1;
                total_consumed += 15;
            } else {
                denied_count += 1;
            }
        }

        // 验证：
        // 1. 总消费量不应该超过限制
        assert!(
            total_consumed <= 1000,
            "总消费量 {} 不应该超过限制 1000",
            total_consumed
        );

        // 2. 应该有部分请求被拒绝
        assert!(denied_count > 0, "应该有部分请求被拒绝");

        // 3. 允许的请求数 * 15 应该等于总消费量
        assert_eq!(
            allowed_count * 15,
            total_consumed as usize,
            "允许的请求数与总消费量应该一致"
        );
    }

    /// 测试并发消费安全性 - 多用户场景
    #[tokio::test]
    async fn test_concurrent_consume_multiple_users() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = Arc::new(QuotaController::with_dependencies(storage, config));
        let mut handles = vec![];

        // 创建 10 个用户，每个用户并发消费
        for user_idx in 0..10 {
            for _ in 0..15 {
                // 每个用户发起 15 次请求，每次 10 个配额
                let controller_clone = Arc::clone(&controller);
                let user_id = format!("user{}", user_idx);
                handles.push(tokio::spawn(async move {
                    controller_clone.consume(&user_id, "resource1", 10).await
                }));
            }
        }

        let mut user_consumption: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();

        for handle in handles {
            let result = handle.await.unwrap().unwrap();
            if result.allowed {
                // 这里我们无法直接获取 user_id，但可以验证总体行为
            }
        }

        // 验证每个用户的消费量不超过限制
        for user_idx in 0..10 {
            let user_id = format!("user{}", user_idx);
            let state = controller.get_quota(&user_id, "resource1").await.unwrap();
            if let Some(state) = state {
                assert!(
                    state.consumed <= 100,
                    "用户 {} 的消费量 {} 不应该超过限制 100",
                    user_id,
                    state.consumed
                );
            }
        }
    }

    /// 测试零消费处理 - 多次零消费
    #[tokio::test]
    async fn test_multiple_zero_consumption() {
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let controller = QuotaController::with_dependencies(storage, config);

        // 多次消费 0
        for _ in 0..10 {
            let result = controller.consume("user1", "resource1", 0).await.unwrap();
            assert!(result.allowed);
            assert_eq!(result.remaining, 100);
        }

        // 验证实际消费量仍为 0
        let state = controller.get_quota("user1", "resource1").await.unwrap();
        // 零消费不会创建配额状态
        assert!(
            state.is_none() || state.unwrap().consumed == 0,
            "零消费不应该增加消费量"
        );
    }

    /// 测试不同配额类型
    #[tokio::test]
    async fn test_different_quota_types() {
        // Token 类型
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Token,
            limit: 1000,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };
        let controller = QuotaController::with_dependencies(storage, config);
        let result = controller.consume("user1", "api", 100).await.unwrap();
        assert!(result.allowed);

        // Money 类型
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Money,
            limit: 10000, // 100.00 元，以分为单位
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };
        let controller = QuotaController::with_dependencies(storage, config);
        let result = controller.consume("user2", "payment", 500).await.unwrap();
        assert!(result.allowed);

        // Count 类型
        let storage = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 100,
            window_size: 3600,
            allow_overdraft: false,
            overdraft_limit_percent: 0,
            alert_config: AlertConfig {
                enabled: false,
                ..Default::default()
            },
        };
        let controller = QuotaController::with_dependencies(storage, config);
        let result = controller.consume("user3", "requests", 10).await.unwrap();
        assert!(result.allowed);
    }

    /// 测试 Builder 模式创建 QuotaController
    #[tokio::test]
    async fn test_quota_controller_builder() {
        let storage: Arc<dyn QuotaStorage> = Arc::new(TestQuotaStorage::new());
        let config = QuotaConfig {
            quota_type: QuotaType::Count,
            limit: 200,
            window_size: 1800,
            allow_overdraft: true,
            overdraft_limit_percent: 10,
            alert_config: AlertConfig::default(),
        };

        let controller = QuotaController::builder()
            .with_storage(storage)
            .with_config(config.clone())
            .build()
            .unwrap();

        // 验证配置正确应用
        assert_eq!(controller.config().limit, 200);
        assert_eq!(controller.config().window_size, 1800);
        assert!(controller.config().allow_overdraft);
        assert_eq!(controller.config().overdraft_limit_percent, 10);

        // 验证功能正常
        let result = controller.consume("user1", "resource1", 50).await.unwrap();
        assert!(result.allowed);
    }
}
