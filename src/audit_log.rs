//! 审计日志模块
//!
//! 提供审计日志功能，记录决策过程、配置变更、封禁操作等。
//!
//! # 特性
//!
//! - **异步写入**: 使用通道缓冲日志，异步写入
//! - **JSON格式**: 支持结构化日志
//! - **批量写入**: 优化性能
//! - **多种事件类型**: 决策、配置变更、封禁操作、系统事件

use crate::error::FlowGuardError;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{self, Sender};
use tokio::sync::RwLock;
use tracing::{debug, error, info, trace, warn};

/// 审计事件类型
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event_type")]
pub enum AuditEvent {
    /// 决策事件
    Decision {
        /// 时间戳
        timestamp: DateTime<Utc>,
        /// 标识符
        identifier: String,
        /// 决策结果
        decision: String,
        /// 原因
        reason: String,
        /// 请求ID
        request_id: Option<String>,
    },
    /// 配置变更事件
    ConfigChange {
        /// 时间戳
        timestamp: DateTime<Utc>,
        /// 旧版本
        old_version: String,
        /// 新版本
        new_version: String,
        /// 变更内容
        changes: Vec<String>,
        /// 操作员
        operator: Option<String>,
    },
    /// 封禁操作事件
    BanOperation {
        /// 时间戳
        timestamp: DateTime<Utc>,
        /// 目标
        target: String,
        /// 操作类型
        action: String,
        /// 原因
        reason: String,
        /// 操作员
        operator: String,
        /// 过期时间
        expires_at: Option<DateTime<Utc>>,
    },
    /// 系统事件
    SystemEvent {
        /// 时间戳
        timestamp: DateTime<Utc>,
        /// 事件级别
        level: String,
        /// 事件名称
        name: String,
        /// 详细信息
        details: String,
    },
    /// 错误事件
    ErrorEvent {
        /// 时间戳
        timestamp: DateTime<Utc>,
        /// 错误类型
        error_type: String,
        /// 错误消息
        message: String,
        /// 堆栈跟踪
        stack_trace: Option<String>,
    },
}

impl AuditEvent {
    /// 获取事件时间戳
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            AuditEvent::Decision { timestamp, .. } => *timestamp,
            AuditEvent::ConfigChange { timestamp, .. } => *timestamp,
            AuditEvent::BanOperation { timestamp, .. } => *timestamp,
            AuditEvent::SystemEvent { timestamp, .. } => *timestamp,
            AuditEvent::ErrorEvent { timestamp, .. } => *timestamp,
        }
    }
}

/// 审计日志统计
#[derive(Debug, Default)]
pub struct AuditLogStats {
    /// 总事件数
    total_events: AtomicU64,
    /// 决策事件数
    decision_events: AtomicU64,
    /// 配置变更事件数
    config_change_events: AtomicU64,
    /// 封禁操作事件数
    ban_operation_events: AtomicU64,
    /// 系统事件数
    system_events: AtomicU64,
    /// 错误事件数
    error_events: AtomicU64,
    /// 批量写入次数
    batch_writes: AtomicU64,
    /// 写入失败次数
    write_failures: AtomicU64,
}

impl AuditLogStats {
    pub fn total_events(&self) -> u64 {
        self.total_events.load(Ordering::Relaxed)
    }

    pub fn decision_events(&self) -> u64 {
        self.decision_events.load(Ordering::Relaxed)
    }

    pub fn config_change_events(&self) -> u64 {
        self.config_change_events.load(Ordering::Relaxed)
    }

    pub fn ban_operation_events(&self) -> u64 {
        self.ban_operation_events.load(Ordering::Relaxed)
    }

    pub fn system_events(&self) -> u64 {
        self.system_events.load(Ordering::Relaxed)
    }

    pub fn error_events(&self) -> u64 {
        self.error_events.load(Ordering::Relaxed)
    }

    pub fn batch_writes(&self) -> u64 {
        self.batch_writes.load(Ordering::Relaxed)
    }

    pub fn write_failures(&self) -> u64 {
        self.write_failures.load(Ordering::Relaxed)
    }

    pub fn reset(&self) {
        self.total_events.store(0, Ordering::Relaxed);
        self.decision_events.store(0, Ordering::Relaxed);
        self.config_change_events.store(0, Ordering::Relaxed);
        self.ban_operation_events.store(0, Ordering::Relaxed);
        self.system_events.store(0, Ordering::Relaxed);
        self.error_events.store(0, Ordering::Relaxed);
        self.batch_writes.store(0, Ordering::Relaxed);
        self.write_failures.store(0, Ordering::Relaxed);
    }
}

/// 审计日志配置
#[derive(Debug, Clone)]
pub struct AuditLogConfig {
    /// 通道容量
    pub channel_capacity: usize,
    /// 批量写入大小
    pub batch_size: usize,
    /// 批量写入超时
    pub batch_timeout: Duration,
    /// 是否启用
    pub enabled: bool,
    /// 输出路径（可选）
    pub output_path: Option<String>,
}

impl Default for AuditLogConfig {
    fn default() -> Self {
        Self {
            channel_capacity: 10000,
            batch_size: 100,
            batch_timeout: Duration::from_secs(5),
            enabled: true,
            output_path: None,
        }
    }
}

impl AuditLogConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn channel_capacity(mut self, capacity: usize) -> Self {
        self.channel_capacity = capacity;
        self
    }

    pub fn batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    pub fn batch_timeout(mut self, timeout: Duration) -> Self {
        self.batch_timeout = timeout;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn output_path(mut self, path: String) -> Self {
        self.output_path = Some(path);
        self
    }
}

/// 审计日志记录器
pub struct AuditLogger {
    /// 事件发送器
    sender: Sender<AuditEvent>,
    /// 统计信息
    stats: Arc<AuditLogStats>,
    /// 配置
    config: AuditLogConfig,
    /// 写入任务句柄
    write_handle: tokio::task::JoinHandle<()>,
}

impl AuditLogger {
    /// 创建新的审计日志记录器
    ///
    /// # 参数
    /// - `config`: 审计日志配置
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::audit_log::{AuditLogger, AuditLogConfig};
    /// use std::time::Duration;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let config = AuditLogConfig::new()
    ///     .channel_capacity(10000)
    ///     .batch_size(100)
    ///     .batch_timeout(Duration::from_secs(5));
    /// let logger = AuditLogger::new(config).await;
    /// # }
    /// ```
    pub async fn new(config: AuditLogConfig) -> Self {
        info!("创建审计日志记录器: enabled={}", config.enabled);

        let (sender, receiver) = mpsc::channel(config.channel_capacity);
        let stats = Arc::new(AuditLogStats::default());

        let write_handle = tokio::spawn(Self::write_task(
            receiver,
            Arc::clone(&stats),
            config.clone(),
        ));

        Self {
            sender,
            stats,
            config,
            write_handle,
        }
    }

    /// 使用默认配置创建审计日志记录器
    pub async fn default() -> Self {
        Self::new(AuditLogConfig::default()).await
    }

    /// 写入任务
    async fn write_task(
        mut receiver: mpsc::Receiver<AuditEvent>,
        stats: Arc<AuditLogStats>,
        config: AuditLogConfig,
    ) {
        let mut batch = Vec::with_capacity(config.batch_size);
        let mut timeout = tokio::time::interval(config.batch_timeout);

        loop {
            tokio::select! {
                // 接收事件
                result = receiver.recv() => {
                    match result {
                        Some(event) => {
                            batch.push(event);
                            stats.total_events.fetch_add(1, Ordering::Relaxed);

                            // 更新事件类型统计
                            match &batch.last().unwrap() {
                                AuditEvent::Decision { .. } => {
                                    stats.decision_events.fetch_add(1, Ordering::Relaxed);
                                }
                                AuditEvent::ConfigChange { .. } => {
                                    stats.config_change_events.fetch_add(1, Ordering::Relaxed);
                                }
                                AuditEvent::BanOperation { .. } => {
                                    stats.ban_operation_events.fetch_add(1, Ordering::Relaxed);
                                }
                                AuditEvent::SystemEvent { .. } => {
                                    stats.system_events.fetch_add(1, Ordering::Relaxed);
                                }
                                AuditEvent::ErrorEvent { .. } => {
                                    stats.error_events.fetch_add(1, Ordering::Relaxed);
                                }
                            }

                            // 达到批量大小，写入
                            if batch.len() >= config.batch_size {
                                Self::write_batch(&batch, &stats);
                                batch.clear();
                            }
                        }
                        None => {
                            // 通道关闭，写入剩余事件
                            if !batch.is_empty() {
                                Self::write_batch(&batch, &stats);
                            }
                            break;
                        }
                    }
                }
                // 超时，写入当前批次
                _ = timeout.tick() => {
                    if !batch.is_empty() {
                        Self::write_batch(&batch, &stats);
                        batch.clear();
                    }
                }
            }
        }

        info!("审计日志写入任务结束");
    }

    /// 写入批次
    fn write_batch(batch: &[AuditEvent], stats: &AuditLogStats) {
        stats.batch_writes.fetch_add(1, Ordering::Relaxed);

        // 序列化为JSON
        for event in batch {
            match serde_json::to_string_pretty(event) {
                Ok(json) => {
                    // 在实际应用中，这里应该写入文件或发送到日志服务
                    // 为了测试，这里只是打印
                    trace!("审计日志: {}", json);
                }
                Err(e) => {
                    stats.write_failures.fetch_add(1, Ordering::Relaxed);
                    error!("序列化审计日志失败: {}", e);
                }
            }
        }
    }

    /// 记录决策事件
    ///
    /// # 参数
    /// - `identifier`: 标识符
    /// - `decision`: 决策结果
    /// - `reason`: 原因
    /// - `request_id`: 请求ID（可选）
    pub async fn log_decision(
        &self,
        identifier: String,
        decision: String,
        reason: String,
        request_id: Option<String>,
    ) {
        if !self.config.enabled {
            return;
        }

        let event = AuditEvent::Decision {
            timestamp: Utc::now(),
            identifier,
            decision,
            reason,
            request_id,
        };

        if let Err(e) = self.sender.send(event).await {
            error!("发送决策事件失败: {}", e);
            self.stats.write_failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// 记录配置变更事件
    ///
    /// # 参数
    /// - `old_version`: 旧版本
    /// - `new_version`: 新版本
    /// - `changes`: 变更内容
    /// - `operator`: 操作员（可选）
    pub async fn log_config_change(
        &self,
        old_version: String,
        new_version: String,
        changes: Vec<String>,
        operator: Option<String>,
    ) {
        if !self.config.enabled {
            return;
        }

        let event = AuditEvent::ConfigChange {
            timestamp: Utc::now(),
            old_version,
            new_version,
            changes,
            operator,
        };

        if let Err(e) = self.sender.send(event).await {
            error!("发送配置变更事件失败: {}", e);
            self.stats.write_failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// 记录封禁操作事件
    ///
    /// # 参数
    /// - `target`: 目标
    /// - `action`: 操作类型
    /// - `reason`: 原因
    /// - `operator`: 操作员
    /// - `expires_at`: 过期时间（可选）
    pub async fn log_ban_operation(
        &self,
        target: String,
        action: String,
        reason: String,
        operator: String,
        expires_at: Option<DateTime<Utc>>,
    ) {
        if !self.config.enabled {
            return;
        }

        let event = AuditEvent::BanOperation {
            timestamp: Utc::now(),
            target,
            action,
            reason,
            operator,
            expires_at,
        };

        if let Err(e) = self.sender.send(event).await {
            error!("发送封禁操作事件失败: {}", e);
            self.stats.write_failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// 记录系统事件
    ///
    /// # 参数
    /// - `level`: 事件级别
    /// - `name`: 事件名称
    /// - `details`: 详细信息
    pub async fn log_system_event(&self, level: String, name: String, details: String) {
        if !self.config.enabled {
            return;
        }

        let event = AuditEvent::SystemEvent {
            timestamp: Utc::now(),
            level,
            name,
            details,
        };

        if let Err(e) = self.sender.send(event).await {
            error!("发送系统事件失败: {}", e);
            self.stats.write_failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// 记录错误事件
    ///
    /// # 参数
    /// - `error_type`: 错误类型
    /// - `message`: 错误消息
    /// - `stack_trace`: 堆栈跟踪（可选）
    pub async fn log_error_event(
        &self,
        error_type: String,
        message: String,
        stack_trace: Option<String>,
    ) {
        if !self.config.enabled {
            return;
        }

        let event = AuditEvent::ErrorEvent {
            timestamp: Utc::now(),
            error_type,
            message,
            stack_trace,
        };

        if let Err(e) = self.sender.send(event).await {
            error!("发送错误事件失败: {}", e);
            self.stats.write_failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// 获取统计信息
    pub fn stats(&self) -> &AuditLogStats {
        &self.stats
    }

    /// 获取配置
    pub fn config(&self) -> &AuditLogConfig {
        &self.config
    }

    /// 停止审计日志记录器
    pub async fn shutdown(mut self) {
        info!("停止审计日志记录器");
        // sender会在drop时自动关闭
        // 等待写入任务完成
        let handle = std::mem::replace(&mut self.write_handle, tokio::spawn(async {}));
        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
    }
}

impl Drop for AuditLogger {
    fn drop(&mut self) {
        self.write_handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_log_config_default() {
        let config = AuditLogConfig::default();
        assert_eq!(config.channel_capacity, 10000);
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.batch_timeout, Duration::from_secs(5));
        assert!(config.enabled);
    }

    #[test]
    fn test_audit_log_config_builder() {
        let config = AuditLogConfig::new()
            .channel_capacity(5000)
            .batch_size(50)
            .batch_timeout(Duration::from_secs(10))
            .enabled(false)
            .output_path("/tmp/audit.log".to_string());

        assert_eq!(config.channel_capacity, 5000);
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.batch_timeout, Duration::from_secs(10));
        assert!(!config.enabled);
        assert_eq!(config.output_path, Some("/tmp/audit.log".to_string()));
    }

    #[test]
    fn test_audit_event_timestamp() {
        let event = AuditEvent::Decision {
            timestamp: Utc::now(),
            identifier: "test".to_string(),
            decision: "allowed".to_string(),
            reason: "test".to_string(),
            request_id: None,
        };

        assert!(event.timestamp() <= Utc::now());
    }

    #[tokio::test]
    async fn test_audit_logger_new() {
        let config = AuditLogConfig::default();
        let logger = AuditLogger::new(config).await;

        assert_eq!(logger.stats().total_events(), 0);
    }

    #[tokio::test]
    async fn test_audit_logger_log_decision() {
        let config = AuditLogConfig::default();
        let logger = AuditLogger::new(config).await;

        logger
            .log_decision(
                "user123".to_string(),
                "allowed".to_string(),
                "within limit".to_string(),
                Some("req-123".to_string()),
            )
            .await;

        // 等待事件被处理
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(logger.stats().decision_events(), 1);
    }

    #[tokio::test]
    async fn test_audit_logger_log_config_change() {
        let config = AuditLogConfig::default();
        let logger = AuditLogger::new(config).await;

        logger
            .log_config_change(
                "1.0".to_string(),
                "2.0".to_string(),
                vec!["updated rate limit".to_string()],
                Some("admin".to_string()),
            )
            .await;

        // 等待事件被处理
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(logger.stats().config_change_events(), 1);
    }

    #[tokio::test]
    async fn test_audit_logger_log_ban_operation() {
        let config = AuditLogConfig::default();
        let logger = AuditLogger::new(config).await;

        logger
            .log_ban_operation(
                "user123".to_string(),
                "ban".to_string(),
                "spam".to_string(),
                "admin".to_string(),
                Some(Utc::now() + chrono::Duration::hours(1)),
            )
            .await;

        // 等待事件被处理
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(logger.stats().ban_operation_events(), 1);
    }

    #[tokio::test]
    async fn test_audit_logger_log_system_event() {
        let config = AuditLogConfig::default();
        let logger = AuditLogger::new(config).await;

        logger
            .log_system_event(
                "info".to_string(),
                "startup".to_string(),
                "system started".to_string(),
            )
            .await;

        // 等待事件被处理
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(logger.stats().system_events(), 1);
    }

    #[tokio::test]
    async fn test_audit_logger_log_error_event() {
        let config = AuditLogConfig::default();
        let logger = AuditLogger::new(config).await;

        logger
            .log_error_event(
                "ConnectionError".to_string(),
                "connection failed".to_string(),
                Some("stack trace".to_string()),
            )
            .await;

        // 等待事件被处理
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(logger.stats().error_events(), 1);
    }

    #[tokio::test]
    async fn test_audit_logger_disabled() {
        let config = AuditLogConfig::new().enabled(false);
        let logger = AuditLogger::new(config).await;

        logger
            .log_decision(
                "user123".to_string(),
                "allowed".to_string(),
                "test".to_string(),
                None,
            )
            .await;

        // 等待
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 事件不应该被记录
        assert_eq!(logger.stats().total_events(), 0);
    }

    #[tokio::test]
    async fn test_audit_logger_stats() {
        let config = AuditLogConfig::default();
        let logger = AuditLogger::new(config).await;

        logger
            .log_decision(
                "user123".to_string(),
                "allowed".to_string(),
                "test".to_string(),
                None,
            )
            .await;

        logger
            .log_config_change("1.0".to_string(), "2.0".to_string(), vec![], None)
            .await;

        logger
            .log_ban_operation(
                "user123".to_string(),
                "ban".to_string(),
                "test".to_string(),
                "admin".to_string(),
                None,
            )
            .await;

        logger
            .log_system_event("info".to_string(), "test".to_string(), "test".to_string())
            .await;

        logger
            .log_error_event("Error".to_string(), "test".to_string(), None)
            .await;

        // 等待事件被处理
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(logger.stats().total_events(), 5);
        assert_eq!(logger.stats().decision_events(), 1);
        assert_eq!(logger.stats().config_change_events(), 1);
        assert_eq!(logger.stats().ban_operation_events(), 1);
        assert_eq!(logger.stats().system_events(), 1);
        assert_eq!(logger.stats().error_events(), 1);
    }

    #[tokio::test]
    async fn test_audit_logger_batch_write() {
        let config = AuditLogConfig::new()
            .batch_size(5)
            .batch_timeout(Duration::from_millis(100));
        let logger = AuditLogger::new(config).await;

        // 发送5个事件
        for i in 0..5 {
            logger
                .log_decision(
                    format!("user{}", i),
                    "allowed".to_string(),
                    "test".to_string(),
                    None,
                )
                .await;
        }

        // 等待批量写入
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert_eq!(logger.stats().batch_writes(), 1);
    }

    #[tokio::test]
    async fn test_audit_logger_shutdown() {
        let config = AuditLogConfig::default();
        let logger = AuditLogger::new(config).await;

        logger
            .log_decision(
                "user123".to_string(),
                "allowed".to_string(),
                "test".to_string(),
                None,
            )
            .await;

        // 等待事件被处理
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 验证事件被正确处理
        assert_eq!(logger.stats().decision_events(), 1);

        logger.shutdown().await;
    }

    #[tokio::test]
    async fn test_audit_stats_reset() {
        let config = AuditLogConfig::default();
        let logger = AuditLogger::new(config).await;

        logger
            .log_decision(
                "user123".to_string(),
                "allowed".to_string(),
                "test".to_string(),
                None,
            )
            .await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        logger.stats().reset();

        assert_eq!(logger.stats().total_events(), 0);
    }
}
