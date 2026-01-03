//! 配置监视器
//!
//! 实现配置变更检测功能，支持轮询和Watch两种模式。

use crate::config::{ChangeSource, ConfigChangeRecord, ConfigHistory, FlowControlConfig};
use crate::error::{FlowGuardError, StorageError};
use crate::storage::Storage;
use chrono::Utc;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use serde::Deserialize;
use sqlx::Row;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, error, info, instrument};

/// 配置监视器回调类型
pub type ConfigChangeCallback = Arc<
    dyn Fn(
            FlowControlConfig,
            ChangeSource,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), FlowGuardError>> + Send>,
        > + Send
        + Sync,
>;

/// 配置监视器
///
/// 支持从PostgreSQL、文件系统（YAML/TOML）读取配置，并检测配置变更。
pub struct ConfigWatcher {
    /// 存储后端
    storage: Arc<dyn Storage>,
    /// 配置文件路径（可选）
    config_path: Option<PathBuf>,
    /// 轮询间隔
    poll_interval: Duration,
    /// 当前配置版本
    current_version: Arc<RwLock<String>>,
    /// 当前配置哈希
    current_hash: Arc<RwLock<String>>,
    /// 配置变更回调
    callback: ConfigChangeCallback,
    /// 配置变更历史
    history: Arc<RwLock<ConfigHistory>>,
    /// 运行状态
    running: Arc<RwLock<bool>>,
    /// 监视模式
    watch_mode: WatchMode,
    /// 数据库配置键
    db_config_key: Option<String>,
}

/// 监视模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchMode {
    /// 轮询模式
    Poll,
    /// Watch模式（文件系统事件）
    Watch,
    /// 混合模式（轮询 + Watch）
    Hybrid,
}

impl Default for WatchMode {
    fn default() -> Self {
        Self::Poll
    }
}

impl ConfigWatcher {
    /// 创建新的配置监视器
    ///
    /// # 参数
    /// - `storage`: 存储后端
    /// - `config_path`: 配置文件路径（可选）
    /// - `poll_interval`: 轮询间隔
    /// - `callback`: 配置变更回调
    /// - `watch_mode`: 监视模式
    /// - `db_config_key`: 数据库配置键（可选）
    pub fn new(
        storage: Arc<dyn Storage>,
        config_path: Option<PathBuf>,
        poll_interval: Duration,
        callback: ConfigChangeCallback,
        watch_mode: WatchMode,
        db_config_key: Option<String>,
    ) -> Self {
        Self {
            storage,
            config_path,
            poll_interval,
            current_version: Arc::new(RwLock::new(String::new())),
            current_hash: Arc::new(RwLock::new(String::new())),
            callback,
            history: Arc::new(RwLock::new(ConfigHistory::new(100))),
            running: Arc::new(RwLock::new(false)),
            watch_mode,
            db_config_key,
        }
    }

    /// 启动配置监视器
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<(), FlowGuardError> {
        let mut running = self.running.write().await;
        if *running {
            return Err(FlowGuardError::ConfigError(
                "配置监视器已在运行".to_string(),
            ));
        }
        *running = true;
        drop(running);

        info!("Starting config watcher with mode: {:?}", self.watch_mode);

        match self.watch_mode {
            WatchMode::Poll => {
                self.start_polling().await?;
            }
            WatchMode::Watch => {
                self.start_watching().await?;
            }
            WatchMode::Hybrid => {
                // 启动轮询和Watch两个任务
                let poll_watcher = self.clone_for_polling();
                let file_watcher = self.clone_for_watching();

                tokio::spawn(async move {
                    if let Err(e) = poll_watcher.start_polling().await {
                        error!("Polling watcher error: {:?}", e);
                    }
                });

                tokio::spawn(async move {
                    if let Err(e) = file_watcher.start_watching().await {
                        error!("File watcher error: {:?}", e);
                    }
                });
            }
        }

        Ok(())
    }

    /// 停止配置监视器
    #[instrument(skip(self))]
    pub async fn stop(&self) -> Result<(), FlowGuardError> {
        let mut running = self.running.write().await;
        *running = false;
        info!("Config watcher stopped");
        Ok(())
    }

    /// 启动轮询模式
    async fn start_polling(&self) -> Result<(), FlowGuardError> {
        info!(
            "Starting polling mode with interval: {:?}",
            self.poll_interval
        );

        while *self.running.read().await {
            if let Err(e) = self.check_config_change().await {
                error!("Config change check failed: {:?}", e);
            }

            sleep(self.poll_interval).await;
        }

        Ok(())
    }

    /// 启动Watch模式
    async fn start_watching(&self) -> Result<(), FlowGuardError> {
        let config_path = self
            .config_path
            .as_ref()
            .ok_or_else(|| FlowGuardError::ConfigError("配置文件路径未指定".to_string()))?;

        info!("Starting watch mode for path: {:?}", config_path);

        let (tx, mut rx) = tokio::sync::mpsc::channel(100);

        // 创建文件系统监视器
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
            if let Ok(event) = res {
                if let Err(e) = tx.blocking_send(event) {
                    error!("Failed to send file event: {:?}", e);
                }
            }
        })
        .map_err(|e| FlowGuardError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        watcher
            .watch(config_path, RecursiveMode::NonRecursive)
            .map_err(|e| {
                FlowGuardError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e))
            })?;

        // 处理文件系统事件
        while *self.running.read().await {
            tokio::select! {
                event = rx.recv() => {
                    if let Some(event) = event {
                        self.handle_file_event(event).await?;
                    } else {
                        break;
                    }
                }
                _ = sleep(Duration::from_secs(1)) => {
                    // 定期检查运行状态
                }
            }
        }

        Ok(())
    }

    /// 处理文件系统事件
    async fn handle_file_event(&self, event: Event) -> Result<(), FlowGuardError> {
        debug!("Received file event: {:?}", event.kind);

        // 只处理修改和创建事件
        match event.kind {
            EventKind::Modify(_) | EventKind::Create(_) => {
                // 等待一小段时间，确保文件写入完成
                sleep(Duration::from_millis(100)).await;

                if let Err(e) = self.check_config_change().await {
                    error!("Config change check failed: {:?}", e);
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// 检查配置变更
    #[instrument(skip(self))]
    pub async fn check_config_change(&self) -> Result<bool, FlowGuardError> {
        // 加载新配置
        let new_config = self.load_config().await?;

        // 计算新配置哈希
        let new_hash = new_config.compute_hash();

        // 比较哈希值
        let current_hash = self.current_hash.read().await;
        let has_changed = *current_hash != new_hash;
        drop(current_hash);

        if has_changed {
            info!("Config change detected, hash: {}", new_hash);

            // 更新当前哈希和版本
            {
                let mut current_hash = self.current_hash.write().await;
                *current_hash = new_hash.clone();
            }
            {
                let mut current_version = self.current_version.write().await;
                *current_version = new_config.version.clone();
            }

            // 记录变更历史
            let old_config = self.load_current_config().await.ok();
            let change_record = new_config.create_change_record(
                old_config.as_ref(),
                if self.watch_mode == WatchMode::Watch {
                    ChangeSource::Watch
                } else {
                    ChangeSource::Poll
                },
            );
            self.history.write().await.add_record(change_record);

            // 调用回调函数
            let callback = self.callback.clone();
            let config_clone = new_config.clone();
            let source = if self.watch_mode == WatchMode::Watch {
                ChangeSource::Watch
            } else {
                ChangeSource::Poll
            };

            tokio::spawn(async move {
                if let Err(e) = callback(config_clone, source).await {
                    error!("Config change callback failed: {:?}", e);
                }
            });
        }

        Ok(false)
    }

    /// 加载配置
    async fn load_config(&self) -> Result<FlowControlConfig, FlowGuardError> {
        // 优先从文件加载
        if let Some(ref config_path) = self.config_path {
            if config_path.exists() {
                return self.load_config_from_file(config_path).await;
            }
        }

        // 从数据库加载
        if let Some(ref db_key) = self.db_config_key {
            return self.load_config_from_db(db_key).await;
        }

        Err(FlowGuardError::ConfigError(
            "无法加载配置：未指定配置文件路径或数据库键".to_string(),
        ))
    }

    /// 从文件加载配置
    async fn load_config_from_file(
        &self,
        path: &Path,
    ) -> Result<FlowControlConfig, FlowGuardError> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| FlowGuardError::IoError(e))?;

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .ok_or_else(|| FlowGuardError::ConfigError("无法确定配置文件类型".to_string()))?;

        match extension {
            "yaml" | "yml" => {
                let config: FlowControlConfig = serde_yaml::from_str(&content)
                    .map_err(|e| FlowGuardError::ConfigError(format!("YAML解析错误: {}", e)))?;
                Ok(config)
            }
            "toml" => {
                let config: FlowControlConfig = toml::from_str(&content)
                    .map_err(|e| FlowGuardError::ConfigError(format!("TOML解析错误: {}", e)))?;
                Ok(config)
            }
            "json" => {
                let config: FlowControlConfig = serde_json::from_str(&content)
                    .map_err(|e| FlowGuardError::ConfigError(format!("JSON解析错误: {}", e)))?;
                Ok(config)
            }
            _ => Err(FlowGuardError::ConfigError(format!(
                "不支持的配置文件类型: {}",
                extension
            ))),
        }
    }

    /// 从数据库加载配置
    async fn load_config_from_db(&self, key: &str) -> Result<FlowControlConfig, FlowGuardError> {
        let value = self
            .storage
            .get(key)
            .await
            .map_err(|e| FlowGuardError::StorageError(e))?
            .ok_or_else(|| FlowGuardError::StorageError(StorageError::NotFound(key.to_string())))?;

        let config: FlowControlConfig = serde_json::from_str(&value)
            .map_err(|e| FlowGuardError::ConfigError(format!("JSON解析错误: {}", e)))?;

        Ok(config)
    }

    /// 加载当前配置（用于比较）
    async fn load_current_config(&self) -> Result<FlowControlConfig, FlowGuardError> {
        self.load_config().await
    }

    /// 手动触发配置检查
    #[instrument(skip(self))]
    pub async fn manual_check(&self) -> Result<bool, FlowGuardError> {
        info!("Manual config check triggered");
        self.check_config_change().await
    }

    /// 获取配置变更历史
    pub async fn get_history(&self) -> Vec<ConfigChangeRecord> {
        self.history.read().await.get_records().to_vec()
    }

    /// 获取当前版本
    pub async fn get_current_version(&self) -> String {
        self.current_version.read().await.clone()
    }

    /// 获取当前哈希
    pub async fn get_current_hash(&self) -> String {
        self.current_hash.read().await.clone()
    }

    /// 克隆用于轮询
    fn clone_for_polling(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            config_path: self.config_path.clone(),
            poll_interval: self.poll_interval,
            current_version: self.current_version.clone(),
            current_hash: self.current_hash.clone(),
            callback: self.callback.clone(),
            history: self.history.clone(),
            running: self.running.clone(),
            watch_mode: WatchMode::Poll,
            db_config_key: self.db_config_key.clone(),
        }
    }

    /// 克隆用于Watch
    fn clone_for_watching(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            config_path: self.config_path.clone(),
            poll_interval: self.poll_interval,
            current_version: self.current_version.clone(),
            current_hash: self.current_hash.clone(),
            callback: self.callback.clone(),
            history: self.history.clone(),
            running: self.running.clone(),
            watch_mode: WatchMode::Watch,
            db_config_key: self.db_config_key.clone(),
        }
    }
}

/// PostgreSQL配置存储
#[derive(Debug, Deserialize)]
pub struct PostgresConfigStorage {
    pub connection_string: String,
    pub table_name: String,
    pub key_column: String,
    pub value_column: String,
}

impl PostgresConfigStorage {
    pub async fn load_config(&self, key: &str) -> Result<FlowControlConfig, FlowGuardError> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect(&self.connection_string)
            .await
            .map_err(|e| {
                FlowGuardError::StorageError(StorageError::ConnectionError(e.to_string()))
            })?;

        let row = sqlx::query(&format!(
            "SELECT {} FROM {} WHERE {} = $1",
            self.value_column, self.table_name, self.key_column
        ))
        .bind(key)
        .fetch_one(&pool)
        .await
        .map_err(|e| FlowGuardError::StorageError(StorageError::QueryError(e.to_string())))?;

        let value: String = row.get::<String, _>(self.value_column.as_str());

        let config: FlowControlConfig = serde_json::from_str(&value)
            .map_err(|e| FlowGuardError::ConfigError(format!("JSON解析错误: {}", e)))?;

        Ok(config)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GlobalConfig, Matcher, Rule};
    use crate::storage::MemoryStorage;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use tempfile::NamedTempFile;
    use tokio::fs;

    fn create_test_config(version: &str) -> FlowControlConfig {
        FlowControlConfig {
            version: version.to_string(),
            global: GlobalConfig {
                storage: "memory".to_string(),
                cache: "memory".to_string(),
                metrics: "prometheus".to_string(),
            },
            rules: vec![Rule {
                id: "test_rule".to_string(),
                name: "Test Rule".to_string(),
                priority: 100,
                matchers: vec![Matcher::User {
                    user_ids: vec!["*".to_string()],
                }],
                limiters: vec![crate::config::LimiterConfig::TokenBucket {
                    capacity: 1000,
                    refill_rate: 100,
                }],
                action: crate::config::ActionConfig {
                    on_exceed: "reject".to_string(),
                    ban: None,
                },
            }],
        }
    }

    #[tokio::test]
    async fn test_config_watcher_creation() {
        let storage = Arc::new(MemoryStorage::new());
        let callback: ConfigChangeCallback = Arc::new(|config, source| {
            Box::pin(async move {
                info!(
                    "Config changed: version={}, source={:?}",
                    config.version, source
                );
                Ok(())
            })
        });

        let watcher = ConfigWatcher::new(
            storage.clone(),
            None,
            Duration::from_secs(5),
            callback,
            WatchMode::Poll,
            Some("config_key".to_string()),
        );

        assert_eq!(watcher.get_current_version().await, "");
    }

    #[tokio::test]
    async fn test_config_hash_computation() {
        let config1 = create_test_config("1.0");
        let config2 = create_test_config("1.0");
        let config3 = create_test_config("2.0");

        assert_eq!(config1.compute_hash(), config2.compute_hash());
        assert_ne!(config1.compute_hash(), config3.compute_hash());
    }

    #[tokio::test]
    async fn test_config_comparison() {
        let config1 = create_test_config("1.0");
        let config2 = create_test_config("1.0");
        let config3 = create_test_config("2.0");

        assert!(config1.is_same_as(&config2));
        assert!(!config1.is_same_as(&config3));
    }

    #[tokio::test]
    async fn test_config_version_comparison() {
        let config1 = create_test_config("1.0");
        let config2 = create_test_config("1.1");
        let config3 = create_test_config("2.0");

        use std::cmp::Ordering;
        assert_eq!(config1.compare_version(&config2), Ordering::Less);
        assert_eq!(config2.compare_version(&config1), Ordering::Greater);
        assert_eq!(config1.compare_version(&config1), Ordering::Equal);
    }

    #[tokio::test]
    async fn test_config_change_record() {
        let old_config = create_test_config("1.0");
        let new_config = create_test_config("2.0");

        let record = new_config.create_change_record(Some(&old_config), ChangeSource::Manual);

        assert_eq!(record.old_version, Some("1.0".to_string()));
        assert_eq!(record.new_version, "2.0".to_string());
        assert_eq!(record.source, ChangeSource::Manual);
        assert!(!record.changes.is_empty());
    }

    #[tokio::test]
    async fn test_config_history() {
        let mut history = ConfigHistory::new(10);

        let record = ConfigChangeRecord {
            timestamp: Utc::now(),
            old_version: Some("1.0".to_string()),
            new_version: "2.0".to_string(),
            old_hash: Some("hash1".to_string()),
            new_hash: "hash2".to_string(),
            source: ChangeSource::Manual,
            changes: vec!["版本变更".to_string()],
        };

        history.add_record(record.clone());

        assert_eq!(history.get_records().len(), 1);
        assert_eq!(history.get_latest().unwrap().new_version, "2.0");

        history.clear();
        assert_eq!(history.get_records().len(), 0);
    }

    #[tokio::test]
    async fn test_config_history_max_records() {
        let mut history = ConfigHistory::new(3);

        for i in 1..=5 {
            let record = ConfigChangeRecord {
                timestamp: Utc::now(),
                old_version: Some(format!("{}.0", i)),
                new_version: format!("{}.0", i + 1),
                old_hash: Some(format!("hash{}", i)),
                new_hash: format!("hash{}", i + 1),
                source: ChangeSource::Manual,
                changes: vec![format!("变更{}", i)],
            };
            history.add_record(record);
        }

        // 应该只保留最后3条记录
        assert_eq!(history.get_records().len(), 3);
        assert_eq!(
            history.get_records()[0].old_version,
            Some("3.0".to_string())
        );
    }

    #[tokio::test]
    async fn test_load_config_from_yaml_file() {
        let storage = Arc::new(MemoryStorage::new());
        let callback: ConfigChangeCallback = Arc::new(|config, source| {
            Box::pin(async move {
                info!(
                    "Config changed: version={}, source={:?}",
                    config.version, source
                );
                Ok(())
            })
        });

        let temp_file = NamedTempFile::new().unwrap();
        let yaml_content = r#"
version: "1.0"
global:
  storage: "memory"
  cache: "memory"
  metrics: "prometheus"
rules:
  - id: "test_rule"
    name: "Test Rule"
    priority: 100
    matchers:
      - type: User
        user_ids: ["*"]
    limiters:
      - type: TokenBucket
        capacity: 1000
        refill_rate: 100
    action:
      on_exceed: "reject"
"#;
        fs::write(temp_file.path(), yaml_content).await.unwrap();

        let watcher = ConfigWatcher::new(
            storage.clone(),
            Some(temp_file.path().to_path_buf()),
            Duration::from_secs(5),
            callback,
            WatchMode::Poll,
            None,
        );

        let config = watcher
            .load_config_from_file(temp_file.path())
            .await
            .unwrap();
        assert_eq!(config.version, "1.0");
        assert_eq!(config.rules.len(), 1);
    }

    #[tokio::test]
    async fn test_load_config_from_toml_file() {
        let storage = Arc::new(MemoryStorage::new());
        let callback: ConfigChangeCallback = Arc::new(|config, source| {
            Box::pin(async move {
                info!(
                    "Config changed: version={}, source={:?}",
                    config.version, source
                );
                Ok(())
            })
        });

        let temp_file = NamedTempFile::new().unwrap();
        let toml_content = r#"
version = "1.0"

[global]
storage = "memory"
cache = "memory"
metrics = "prometheus"

[[rules]]
id = "test_rule"
name = "Test Rule"
priority = 100

[[rules.matchers]]
type = "User"
user_ids = ["*"]

[[rules.limiters]]
type = "TokenBucket"
capacity = 1000
refill_rate = 100

[rules.action]
on_exceed = "reject"
"#;
        fs::write(temp_file.path(), toml_content).await.unwrap();

        let watcher = ConfigWatcher::new(
            storage.clone(),
            Some(temp_file.path().to_path_buf()),
            Duration::from_secs(5),
            callback,
            WatchMode::Poll,
            None,
        );

        let config = watcher
            .load_config_from_file(temp_file.path())
            .await
            .unwrap();
        assert_eq!(config.version, "1.0");
        assert_eq!(config.rules.len(), 1);
    }

    #[tokio::test]
    async fn test_load_config_from_db() {
        let storage = Arc::new(MemoryStorage::new());
        let config = create_test_config("1.0");
        let config_json = serde_json::to_string(&config).unwrap();

        storage.set("config_key", &config_json, None).await.unwrap();

        let callback: ConfigChangeCallback = Arc::new(|config, source| {
            Box::pin(async move {
                info!(
                    "Config changed: version={}, source={:?}",
                    config.version, source
                );
                Ok(())
            })
        });

        let watcher = ConfigWatcher::new(
            storage.clone(),
            None,
            Duration::from_secs(5),
            callback,
            WatchMode::Poll,
            Some("config_key".to_string()),
        );

        let loaded_config = watcher.load_config_from_db("config_key").await.unwrap();
        assert_eq!(loaded_config.version, "1.0");
        assert_eq!(loaded_config.rules.len(), 1);
    }

    #[tokio::test]
    async fn test_config_change_detection() {
        let storage = Arc::new(MemoryStorage::new());
        let config1 = create_test_config("1.0");
        let config2 = create_test_config("2.0");

        storage
            .set(
                "config_key",
                &serde_json::to_string(&config1).unwrap(),
                None,
            )
            .await
            .unwrap();

        let callback_called = Arc::new(AtomicBool::new(false));
        let callback_called_clone = callback_called.clone();

        let callback: ConfigChangeCallback = Arc::new(move |config, source| {
            let callback_called = callback_called_clone.clone();
            Box::pin(async move {
                info!(
                    "Config changed: version={}, source={:?}",
                    config.version, source
                );
                callback_called.store(true, Ordering::SeqCst);
                Ok(())
            })
        });

        let watcher = ConfigWatcher::new(
            storage.clone(),
            None,
            Duration::from_secs(5),
            callback,
            WatchMode::Poll,
            Some("config_key".to_string()),
        );

        // 初始加载
        let changed = watcher.check_config_change().await.unwrap();
        assert!(!changed);

        // 更新配置
        storage
            .set(
                "config_key",
                &serde_json::to_string(&config2).unwrap(),
                None,
            )
            .await
            .unwrap();

        // 检测变更
        let changed = watcher.check_config_change().await.unwrap();
        assert!(changed);

        // 等待回调执行
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(callback_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_manual_check() {
        let storage = Arc::new(MemoryStorage::new());
        let config1 = create_test_config("1.0");
        let config2 = create_test_config("2.0");

        storage
            .set(
                "config_key",
                &serde_json::to_string(&config1).unwrap(),
                None,
            )
            .await
            .unwrap();

        let callback: ConfigChangeCallback = Arc::new(|config, source| {
            Box::pin(async move {
                info!(
                    "Config changed: version={}, source={:?}",
                    config.version, source
                );
                Ok(())
            })
        });

        let watcher = ConfigWatcher::new(
            storage.clone(),
            None,
            Duration::from_secs(5),
            callback,
            WatchMode::Poll,
            Some("config_key".to_string()),
        );

        // 初始加载
        watcher.check_config_change().await.unwrap();

        // 更新配置
        storage
            .set(
                "config_key",
                &serde_json::to_string(&config2).unwrap(),
                None,
            )
            .await
            .unwrap();

        // 手动检查
        let changed = watcher.manual_check().await.unwrap();
        assert!(changed);
    }

    #[tokio::test]
    async fn test_get_current_version_and_hash() {
        let storage = Arc::new(MemoryStorage::new());
        let config = create_test_config("1.0");

        storage
            .set("config_key", &serde_json::to_string(&config).unwrap(), None)
            .await
            .unwrap();

        let callback: ConfigChangeCallback = Arc::new(|config, source| {
            Box::pin(async move {
                info!(
                    "Config changed: version={}, source={:?}",
                    config.version, source
                );
                Ok(())
            })
        });

        let watcher = ConfigWatcher::new(
            storage.clone(),
            None,
            Duration::from_secs(5),
            callback,
            WatchMode::Poll,
            Some("config_key".to_string()),
        );

        // 初始状态
        assert_eq!(watcher.get_current_version().await, "");
        assert_eq!(watcher.get_current_hash().await, "");

        // 检测变更
        watcher.check_config_change().await.unwrap();

        // 更新后状态
        assert_eq!(watcher.get_current_version().await, "1.0");
        assert!(!watcher.get_current_hash().await.is_empty());
    }

    #[tokio::test]
    async fn test_start_stop_watcher() {
        let storage = Arc::new(MemoryStorage::new());
        let config = create_test_config("1.0");

        storage
            .set("config_key", &serde_json::to_string(&config).unwrap(), None)
            .await
            .unwrap();

        let callback: ConfigChangeCallback = Arc::new(|config, source| {
            Box::pin(async move {
                info!(
                    "Config changed: version={}, source={:?}",
                    config.version, source
                );
                Ok(())
            })
        });

        let watcher = ConfigWatcher::new(
            storage.clone(),
            None,
            Duration::from_millis(100),
            callback,
            WatchMode::Poll,
            Some("config_key".to_string()),
        );

        // 启动监视器
        watcher.start().await.unwrap();

        // 等待一段时间
        tokio::time::sleep(Duration::from_millis(200)).await;

        // 停止监视器
        watcher.stop().await.unwrap();

        // 再次停止应该失败
        let result = watcher.stop().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_double_start_fails() {
        let storage = Arc::new(MemoryStorage::new());
        let callback: ConfigChangeCallback = Arc::new(|config, source| {
            Box::pin(async move {
                info!(
                    "Config changed: version={}, source={:?}",
                    config.version, source
                );
                Ok(())
            })
        });

        let watcher = ConfigWatcher::new(
            storage.clone(),
            None,
            Duration::from_secs(5),
            callback,
            WatchMode::Poll,
            Some("config_key".to_string()),
        );

        // 第一次启动
        watcher.start().await.unwrap();

        // 第二次启动应该失败
        let result = watcher.start().await;
        assert!(result.is_err());

        // 清理
        watcher.stop().await.unwrap();
    }
}
