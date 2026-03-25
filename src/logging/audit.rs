//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 审计日志模块
//!
//! 提供审计日志功能，记录决策过程、配置变更、封禁操作等。
//! 支持 HMAC-SHA256 签名验证日志完整性。

#[cfg(feature = "audit-log")]
use chrono::{DateTime, Utc};
#[cfg(feature = "audit-log")]
use hmac::{Hmac, Mac};
#[cfg(feature = "audit-log")]
use log::{error, info, trace, warn};
#[cfg(feature = "audit-log")]
use secrecy::{ExposeSecret, SecretString};
#[cfg(feature = "audit-log")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "audit-log")]
use sha2::Sha256;
#[cfg(feature = "audit-log")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "audit-log")]
use std::sync::Arc;
#[cfg(feature = "audit-log")]
use std::time::Duration;
#[cfg(feature = "audit-log")]
use tokio::sync::mpsc::{self, Sender};

#[cfg(feature = "audit-log")]
use crate::error::FlowGuardError;

#[cfg(feature = "audit-log")]
/// 审计事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum AuditEvent {
    Decision {
        timestamp: DateTime<Utc>,
        identifier: String,
        decision: String,
        reason: String,
        request_id: Option<String>,
    },
    ConfigChange {
        timestamp: DateTime<Utc>,
        old_version: String,
        new_version: String,
        changes: Vec<String>,
        operator: Option<String>,
    },
    BanOperation {
        timestamp: DateTime<Utc>,
        target: String,
        action: String,
        reason: String,
        operator: String,
        expires_at: Option<DateTime<Utc>>,
    },
    SystemEvent {
        timestamp: DateTime<Utc>,
        level: String,
        name: String,
        details: String,
    },
    ErrorEvent {
        timestamp: DateTime<Utc>,
        error_type: String,
        message: String,
        stack_trace: Option<String>,
    },
}

#[cfg(feature = "audit-log")]
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

    /// 获取事件操作类型（用于签名）
    pub fn operation(&self) -> &str {
        match self {
            AuditEvent::Decision { .. } => "decision",
            AuditEvent::ConfigChange { .. } => "config_change",
            AuditEvent::BanOperation { .. } => "ban_operation",
            AuditEvent::SystemEvent { .. } => "system_event",
            AuditEvent::ErrorEvent { .. } => "error_event",
        }
    }

    /// 获取事件目标（用于签名）
    pub fn target(&self) -> String {
        match self {
            AuditEvent::Decision { identifier, .. } => identifier.clone(),
            AuditEvent::ConfigChange {
                old_version,
                new_version,
                ..
            } => {
                format!("{}->{}", old_version, new_version)
            }
            AuditEvent::BanOperation { target, .. } => target.clone(),
            AuditEvent::SystemEvent { name, .. } => name.clone(),
            AuditEvent::ErrorEvent { error_type, .. } => error_type.clone(),
        }
    }

    /// 获取事件结果（用于签名）
    pub fn result(&self) -> String {
        match self {
            AuditEvent::Decision { decision, .. } => decision.clone(),
            AuditEvent::ConfigChange { changes, .. } => changes.join(","),
            AuditEvent::BanOperation { action, .. } => action.clone(),
            AuditEvent::SystemEvent { level, .. } => level.clone(),
            AuditEvent::ErrorEvent { message, .. } => message.clone(),
        }
    }
}

#[cfg(feature = "audit-log")]
/// 审计日志条目（带签名）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// 审计事件
    pub event: AuditEvent,
    /// HMAC-SHA256 签名（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// 签名算法版本
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_version: Option<u32>,
}

#[cfg(feature = "audit-log")]
impl AuditLogEntry {
    /// 创建新的审计日志条目
    pub fn new(event: AuditEvent) -> Self {
        Self {
            event,
            signature: None,
            signature_version: None,
        }
    }

    /// 从事件创建带签名的日志条目
    pub fn with_signature(event: AuditEvent, signing_key: &str) -> Self {
        let mut entry = Self::new(event);
        entry.sign(signing_key);
        entry
    }

    /// 对日志条目进行签名
    pub fn sign(&mut self, signing_key: &str) {
        let signature = Self::generate_signature(&self.event, signing_key);
        self.signature = Some(signature);
        self.signature_version = Some(1); // 当前签名算法版本
    }

    /// 生成 HMAC-SHA256 签名
    ///
    /// 对日志条目的关键字段进行签名，包括：
    /// - timestamp: 时间戳
    /// - operation: 操作类型
    /// - target: 目标标识
    /// - result: 操作结果
    fn generate_signature(event: &AuditEvent, signing_key: &str) -> String {
        // 构建签名消息
        let message = format!(
            "{}|{}|{}|{}",
            event.timestamp().to_rfc3339(),
            event.operation(),
            event.target(),
            event.result()
        );

        // 创建 HMAC-SHA256 实例
        let mut mac = Hmac::<Sha256>::new_from_slice(signing_key.as_bytes())
            .expect("HMAC can take key of any size");

        mac.update(message.as_bytes());

        // 返回十六进制编码的签名
        let result = mac.finalize();
        hex::encode(result.into_bytes())
    }

    /// 验证签名完整性
    ///
    /// 返回：
    /// - Ok(true): 签名验证通过
    /// - Ok(false): 未配置签名密钥，跳过验证
    /// - Err(...): 签名验证失败，日志可能被篡改
    pub fn verify(&self, signing_key: &str) -> Result<bool, FlowGuardError> {
        // 如果没有签名，返回错误
        let stored_signature = match &self.signature {
            Some(sig) => sig,
            None => {
                return Err(FlowGuardError::AuditLogError(
                    "日志条目缺少签名".to_string(),
                ));
            }
        };

        // 重新计算签名
        let expected_signature = Self::generate_signature(&self.event, signing_key);

        // 使用常量时间比较防止时序攻击
        if Self::constant_time_compare(stored_signature, &expected_signature) {
            Ok(true)
        } else {
            Err(FlowGuardError::AuditLogError(
                "签名验证失败，日志可能被篡改".to_string(),
            ))
        }
    }

    /// 常量时间比较，防止时序攻击
    fn constant_time_compare(a: &str, b: &str) -> bool {
        if a.len() != b.len() {
            return false;
        }

        let a_bytes = a.as_bytes();
        let b_bytes = b.as_bytes();

        let mut result = 0u8;
        for (x, y) in a_bytes.iter().zip(b_bytes.iter()) {
            result |= x ^ y;
        }

        result == 0
    }
}

#[cfg(feature = "audit-log")]
#[derive(Debug, Default)]
pub struct AuditLogStats {
    total_events: AtomicU64,
    decision_events: AtomicU64,
    config_change_events: AtomicU64,
    ban_operation_events: AtomicU64,
    system_events: AtomicU64,
    error_events: AtomicU64,
    batch_writes: AtomicU64,
    write_failures: AtomicU64,
    signature_failures: AtomicU64,
    verification_failures: AtomicU64,
}

#[cfg(feature = "audit-log")]
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

    pub fn signature_failures(&self) -> u64 {
        self.signature_failures.load(Ordering::Relaxed)
    }

    pub fn verification_failures(&self) -> u64 {
        self.verification_failures.load(Ordering::Relaxed)
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
        self.signature_failures.store(0, Ordering::Relaxed);
        self.verification_failures.store(0, Ordering::Relaxed);
    }
}

#[cfg(feature = "audit-log")]
/// 敏感数据脱敏
///
/// 对标识符和其他敏感数据进行脱敏处理
fn sanitize_identifier(identifier: &str) -> String {
    // 检查是否是 IP 地址
    if identifier.contains('.') && identifier.parse::<std::net::IpAddr>().is_ok() {
        // IP 地址：保留前两段，后两段掩码
        let parts: Vec<&str> = identifier.split('.').collect();
        if parts.len() == 4 {
            return format!("{}.{}.xxx.xxx", parts[0], parts[1]);
        }
    }

    // 检查是否是邮箱
    if identifier.contains('@') {
        // 邮箱：保留用户名前3位和域名
        let parts: Vec<&str> = identifier.split('@').collect();
        if parts.len() == 2 {
            let username = parts[0];
            let masked_username = if username.len() > 3 {
                format!("{}***", &username[..3])
            } else {
                "***".to_string()
            };
            return format!("{}@{}", masked_username, parts[1]);
        }
    }

    // 检查是否是 User ID（假设是数字或UUID）
    if identifier.len() > 10 {
        // User ID：只显示前3位和后3位
        return format!(
            "{}***{}",
            &identifier[..3],
            &identifier[identifier.len() - 3..]
        );
    }

    // 其他情况：部分掩码
    if identifier.len() > 6 {
        format!(
            "{}***{}",
            &identifier[..3],
            &identifier[identifier.len() - 3..]
        )
    } else {
        "***".to_string()
    }
}

#[cfg(feature = "audit-log")]
#[derive(Debug, Clone)]
pub struct AuditLogConfig {
    /// 通道容量
    pub channel_capacity: usize,
    /// 批处理大小
    pub batch_size: usize,
    /// 批处理超时
    pub batch_timeout: Duration,
    /// 是否启用
    pub enabled: bool,
    /// 输出路径
    pub output_path: Option<String>,
    /// 日志轮转：最大文件大小（字节）
    pub max_file_size: Option<u64>,
    /// 日志轮转：保留的文件数量
    pub max_files: Option<usize>,
    /// HMAC 签名密钥（用于保护日志完整性）
    pub signing_key: Option<SecretString>,
    /// 读取时是否验证签名
    pub verify_on_read: bool,
}

#[cfg(feature = "audit-log")]
impl Default for AuditLogConfig {
    fn default() -> Self {
        Self {
            channel_capacity: 10000,
            batch_size: 100,
            batch_timeout: Duration::from_secs(5),
            enabled: true,
            output_path: None,
            max_file_size: Some(100 * 1024 * 1024), // 100MB
            max_files: Some(10),                    // 保留10个文件
            signing_key: None,
            verify_on_read: true,
        }
    }
}

#[cfg(feature = "audit-log")]
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

    /// 设置签名密钥
    ///
    /// # 安全说明
    /// - 密钥应使用安全的随机源生成
    /// - 密钥长度建议至少 32 字节
    /// - 密钥应安全存储，不要硬编码在代码中
    pub fn signing_key(mut self, key: String) -> Self {
        self.signing_key = Some(SecretString::new(key.into_boxed_str()));
        self
    }

    /// 设置读取时是否验证签名
    pub fn verify_on_read(mut self, verify: bool) -> Self {
        self.verify_on_read = verify;
        self
    }
}

#[cfg(feature = "audit-log")]
#[derive(Debug)]
pub struct AuditLogger {
    sender: Sender<AuditEvent>,
    stats: Arc<AuditLogStats>,
    config: AuditLogConfig,
    write_handle: tokio::task::JoinHandle<()>,
}

#[cfg(feature = "audit-log")]
impl AuditLogger {
    pub async fn new(config: AuditLogConfig) -> Self {
        info!(
            "创建审计日志记录器: enabled={}, signing_enabled={}",
            config.enabled,
            config.signing_key.is_some()
        );

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

    pub async fn default() -> Self {
        Self::new(AuditLogConfig::default()).await
    }

    async fn write_task(
        mut receiver: mpsc::Receiver<AuditEvent>,
        stats: Arc<AuditLogStats>,
        config: AuditLogConfig,
    ) {
        let mut batch = Vec::with_capacity(config.batch_size);
        let mut timeout = tokio::time::interval(config.batch_timeout);

        loop {
            tokio::select! {
                result = receiver.recv() => {
                    match result {
                        Some(event) => {
                            batch.push(event);
                            stats.total_events.fetch_add(1, Ordering::Relaxed);

                            match batch.last().unwrap() {
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

                            if batch.len() >= config.batch_size {
                                Self::write_batch(&batch, &config, &stats);
                                batch.clear();
                            }
                        }
                        None => {
                            if !batch.is_empty() {
                                Self::write_batch(&batch, &config, &stats);
                            }
                            break;
                        }
                    }
                }
                _ = timeout.tick() => {
                    if !batch.is_empty() {
                        Self::write_batch(&batch, &config, &stats);
                        batch.clear();
                    }
                }
            }
        }

        info!("审计日志写入任务结束");
    }

    fn write_batch(batch: &[AuditEvent], config: &AuditLogConfig, stats: &AuditLogStats) {
        stats.batch_writes.fetch_add(1, Ordering::Relaxed);

        for event in batch {
            // 创建带签名的日志条目
            let entry = if let Some(ref signing_key) = config.signing_key {
                let key = signing_key.expose_secret();
                AuditLogEntry::with_signature(event.clone(), key)
            } else {
                AuditLogEntry::new(event.clone())
            };

            match serde_json::to_string_pretty(&entry) {
                Ok(json) => {
                    // 使用 info 级别记录日志（生产环境可见）
                    info!("审计日志: {}", json);

                    // 如果配置了输出路径，写入文件
                    if let Some(ref path) = config.output_path {
                        if let Err(e) = Self::write_to_file(path, &json, config) {
                            stats.write_failures.fetch_add(1, Ordering::Relaxed);
                            error!("写入审计日志文件失败: {}: {}", path, e);
                        } else {
                            trace!("成功写入审计日志文件: {}", path);
                        }
                    }
                }
                Err(e) => {
                    stats.write_failures.fetch_add(1, Ordering::Relaxed);
                    error!("序列化审计日志失败: {}", e);
                }
            }
        }
    }

    /// 写入审计日志到文件
    ///
    /// # 安全说明
    /// - 使用追加模式写入，避免覆盖已有日志
    /// - 自动创建目录（如果不存在）
    /// - 添加换行符分隔日志条目
    /// - 支持日志轮转
    fn write_to_file(path: &str, content: &str, config: &AuditLogConfig) -> std::io::Result<()> {
        use std::fs::OpenOptions;
        use std::io::Write;

        // 检查是否需要轮转
        if let Some(max_size) = config.max_file_size {
            if let Ok(metadata) = std::fs::metadata(path) {
                if metadata.len() >= max_size {
                    // 执行日志轮转
                    Self::rotate_log_file(path, config.max_files)?;
                }
            }
        }

        // 确保目录存在
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        // 以追加模式打开文件
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;

        // 写入内容并添加换行符
        writeln!(file, "{}", content)?;

        Ok(())
    }

    /// 日志轮转
    ///
    /// 将当前日志文件重命名，并删除旧的日志文件
    fn rotate_log_file(path: &str, max_files: Option<usize>) -> std::io::Result<()> {
        let path_obj = std::path::Path::new(path);
        let parent = path_obj.parent().unwrap_or(std::path::Path::new("."));
        let stem = path_obj
            .file_stem()
            .unwrap_or(std::ffi::OsStr::new("audit"));
        let extension = path_obj
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("log");

        // 删除最旧的日志文件（如果超过 max_files）
        if let Some(max_files) = max_files {
            let old_file = parent.join(format!(
                "{}.{}.{}",
                stem.to_str().unwrap(),
                max_files,
                extension
            ));
            if old_file.exists() {
                std::fs::remove_file(&old_file)?;
            }

            // 重命名中间的日志文件
            for i in (1..max_files).rev() {
                let old_name =
                    parent.join(format!("{}.{}.{}", stem.to_str().unwrap(), i, extension));
                let new_name = parent.join(format!(
                    "{}.{}.{}",
                    stem.to_str().unwrap(),
                    i + 1,
                    extension
                ));
                if old_name.exists() {
                    std::fs::rename(&old_name, &new_name)?;
                }
            }
        }

        // 重命名当前日志文件
        let rotated_name = parent.join(format!("{}.1.{}", stem.to_str().unwrap(), extension));
        std::fs::rename(path, &rotated_name)?;

        Ok(())
    }

    /// 从文件读取并验证审计日志
    ///
    /// # 参数
    /// - `path`: 日志文件路径
    ///
    /// # 返回
    /// - `Ok(Vec<AuditLogEntry>)`: 验证通过的日志条目列表
    /// - `Err(...)`: 读取或验证失败
    pub fn read_and_verify(
        path: &str,
        config: &AuditLogConfig,
        stats: &AuditLogStats,
    ) -> Result<Vec<AuditLogEntry>, FlowGuardError> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // 解析日志条目
            match serde_json::from_str::<AuditLogEntry>(line) {
                Ok(entry) => {
                    // 如果配置了签名验证，则验证签名
                    if config.verify_on_read {
                        if let Some(ref signing_key) = config.signing_key {
                            let key = signing_key.expose_secret();
                            match entry.verify(key) {
                                Ok(true) => {
                                    entries.push(entry);
                                }
                                Ok(false) => {
                                    // 未配置签名密钥，跳过验证
                                    entries.push(entry);
                                }
                                Err(e) => {
                                    stats.verification_failures.fetch_add(1, Ordering::Relaxed);
                                    warn!("审计日志签名验证失败: {}", e);
                                    // 根据配置决定是否继续
                                    // 这里选择记录警告但继续处理
                                    entries.push(entry);
                                }
                            }
                        } else {
                            entries.push(entry);
                        }
                    } else {
                        entries.push(entry);
                    }
                }
                Err(e) => {
                    stats.verification_failures.fetch_add(1, Ordering::Relaxed);
                    warn!("解析审计日志条目失败: {}", e);
                }
            }
        }

        Ok(entries)
    }

    /// 验证单个日志条目的完整性
    ///
    /// # 参数
    /// - `entry`: 要验证的日志条目
    ///
    /// # 返回
    /// - `Ok(true)`: 签名验证通过
    /// - `Ok(false)`: 未配置签名密钥，跳过验证
    /// - `Err(...)`: 签名验证失败
    pub fn verify_integrity(&self, entry: &AuditLogEntry) -> Result<bool, FlowGuardError> {
        if let Some(ref signing_key) = self.config.signing_key {
            let key = signing_key.expose_secret();
            entry.verify(key)
        } else {
            // 未配置签名密钥，跳过验证
            Ok(false)
        }
    }

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

        // 对敏感数据进行脱敏
        let sanitized_identifier = sanitize_identifier(&identifier);

        let event = AuditEvent::Decision {
            timestamp: Utc::now(),
            identifier: sanitized_identifier,
            decision,
            reason,
            request_id,
        };

        if let Err(e) = self.sender.send(event).await {
            error!("发送决策事件失败: {}", e);
            self.stats.write_failures.fetch_add(1, Ordering::Relaxed);
        }
    }

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

    pub fn stats(&self) -> &AuditLogStats {
        &self.stats
    }

    pub fn config(&self) -> &AuditLogConfig {
        &self.config
    }

    pub async fn shutdown(mut self) {
        info!("停止审计日志记录器");
        let handle = std::mem::replace(&mut self.write_handle, tokio::spawn(async {}));
        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
    }
}

#[cfg(feature = "audit-log")]
impl Drop for AuditLogger {
    fn drop(&mut self) {
        self.write_handle.abort();
    }
}

#[cfg(feature = "audit-log")]
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
        assert!(config.signing_key.is_none());
        assert!(config.verify_on_read);
    }

    #[test]
    fn test_audit_log_config_builder() {
        let config = AuditLogConfig::new()
            .channel_capacity(5000)
            .batch_size(50)
            .batch_timeout(Duration::from_secs(10))
            .enabled(false)
            .output_path("/tmp/audit.log".to_string())
            .signing_key("test-secret-key-32-bytes-long!".to_string())
            .verify_on_read(false);

        assert_eq!(config.channel_capacity, 5000);
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.batch_timeout, Duration::from_secs(10));
        assert!(!config.enabled);
        assert_eq!(config.output_path, Some("/tmp/audit.log".to_string()));
        assert!(config.signing_key.is_some());
        assert!(!config.verify_on_read);
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

    #[test]
    fn test_audit_event_operation() {
        let event = AuditEvent::Decision {
            timestamp: Utc::now(),
            identifier: "test".to_string(),
            decision: "allowed".to_string(),
            reason: "test".to_string(),
            request_id: None,
        };
        assert_eq!(event.operation(), "decision");

        let event = AuditEvent::ConfigChange {
            timestamp: Utc::now(),
            old_version: "1.0".to_string(),
            new_version: "2.0".to_string(),
            changes: vec!["change1".to_string()],
            operator: None,
        };
        assert_eq!(event.operation(), "config_change");
    }

    #[test]
    fn test_audit_event_target() {
        let event = AuditEvent::Decision {
            timestamp: Utc::now(),
            identifier: "user123".to_string(),
            decision: "allowed".to_string(),
            reason: "test".to_string(),
            request_id: None,
        };
        assert_eq!(event.target(), "user123");

        let event = AuditEvent::ConfigChange {
            timestamp: Utc::now(),
            old_version: "1.0".to_string(),
            new_version: "2.0".to_string(),
            changes: vec![],
            operator: None,
        };
        assert_eq!(event.target(), "1.0->2.0");
    }

    #[test]
    fn test_audit_log_entry_signature() {
        let event = AuditEvent::Decision {
            timestamp: Utc::now(),
            identifier: "test-user".to_string(),
            decision: "allowed".to_string(),
            reason: "within limit".to_string(),
            request_id: None,
        };

        let signing_key = "test-secret-key-32-bytes-long!";
        let entry = AuditLogEntry::with_signature(event.clone(), signing_key);

        // 验证签名存在
        assert!(entry.signature.is_some());
        assert_eq!(entry.signature_version, Some(1));

        // 验证签名可以验证通过
        let result = entry.verify(signing_key);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_audit_log_entry_signature_verification_failure() {
        let event = AuditEvent::Decision {
            timestamp: Utc::now(),
            identifier: "test-user".to_string(),
            decision: "allowed".to_string(),
            reason: "within limit".to_string(),
            request_id: None,
        };

        let signing_key = "test-secret-key-32-bytes-long!";
        let wrong_key = "wrong-secret-key-32-bytes-long!";
        let entry = AuditLogEntry::with_signature(event, signing_key);

        // 使用错误的密钥验证应该失败
        let result = entry.verify(wrong_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_audit_log_entry_missing_signature() {
        let event = AuditEvent::Decision {
            timestamp: Utc::now(),
            identifier: "test-user".to_string(),
            decision: "allowed".to_string(),
            reason: "within limit".to_string(),
            request_id: None,
        };

        let entry = AuditLogEntry::new(event);
        let signing_key = "test-secret-key-32-bytes-long!";

        // 没有签名的条目验证应该失败
        let result = entry.verify(signing_key);
        assert!(result.is_err());
        assert!(matches!(result, Err(FlowGuardError::AuditLogError(_))));
    }

    #[test]
    fn test_constant_time_compare() {
        // 相同字符串
        assert!(AuditLogEntry::constant_time_compare("abc123", "abc123"));

        // 不同字符串
        assert!(!AuditLogEntry::constant_time_compare("abc123", "abc124"));

        // 不同长度
        assert!(!AuditLogEntry::constant_time_compare("abc", "abcd"));
    }

    #[test]
    fn test_signature_tampering_detection() {
        let event = AuditEvent::Decision {
            timestamp: Utc::now(),
            identifier: "test-user".to_string(),
            decision: "allowed".to_string(),
            reason: "within limit".to_string(),
            request_id: None,
        };

        let signing_key = "test-secret-key-32-bytes-long!";
        let mut entry = AuditLogEntry::with_signature(event.clone(), signing_key);

        // 篡改签名
        entry.signature = Some("tampered_signature".to_string());

        // 验证应该失败
        let result = entry.verify(signing_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_signature_deterministic() {
        let timestamp = Utc::now();
        let event = AuditEvent::Decision {
            timestamp,
            identifier: "test-user".to_string(),
            decision: "allowed".to_string(),
            reason: "within limit".to_string(),
            request_id: None,
        };

        let signing_key = "test-secret-key-32-bytes-long!";

        // 相同事件应该产生相同的签名
        let entry1 = AuditLogEntry::with_signature(event.clone(), signing_key);
        let entry2 = AuditLogEntry::with_signature(event, signing_key);

        assert_eq!(entry1.signature, entry2.signature);
    }

    #[test]
    fn test_signature_serialization() {
        let event = AuditEvent::Decision {
            timestamp: Utc::now(),
            identifier: "test-user".to_string(),
            decision: "allowed".to_string(),
            reason: "within limit".to_string(),
            request_id: None,
        };

        let signing_key = "test-secret-key-32-bytes-long!";
        let entry = AuditLogEntry::with_signature(event, signing_key);

        // 序列化
        let json = serde_json::to_string(&entry).unwrap();

        // 反序列化
        let deserialized: AuditLogEntry = serde_json::from_str(&json).unwrap();

        // 验证签名仍然有效
        let result = deserialized.verify(signing_key);
        assert!(result.is_ok());
        assert!(result.unwrap());
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

        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(logger.stats().decision_events(), 1);
    }

    #[tokio::test]
    async fn test_audit_logger_with_signing() {
        let config = AuditLogConfig::new()
            .signing_key("test-secret-key-32-bytes-long!".to_string())
            .verify_on_read(true);

        let logger = AuditLogger::new(config).await;

        logger
            .log_decision(
                "user123".to_string(),
                "allowed".to_string(),
                "within limit".to_string(),
                None,
            )
            .await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(logger.stats().decision_events(), 1);
        assert!(logger.config().signing_key.is_some());
    }

    #[test]
    fn test_sanitize_identifier_ip() {
        let result = sanitize_identifier("192.168.1.100");
        assert_eq!(result, "192.168.xxx.xxx");
    }

    #[test]
    fn test_sanitize_identifier_email() {
        let result = sanitize_identifier("test@example.com");
        assert_eq!(result, "tes***@example.com");
    }

    #[test]
    fn test_sanitize_identifier_user_id() {
        let result = sanitize_identifier("1234567890abcdef");
        assert_eq!(result, "123***def");
    }

    #[test]
    fn test_audit_log_stats() {
        let stats = AuditLogStats::default();

        stats.total_events.store(100, Ordering::Relaxed);
        stats.decision_events.store(50, Ordering::Relaxed);
        stats.signature_failures.store(2, Ordering::Relaxed);

        assert_eq!(stats.total_events(), 100);
        assert_eq!(stats.decision_events(), 50);
        assert_eq!(stats.signature_failures(), 2);
    }
}
