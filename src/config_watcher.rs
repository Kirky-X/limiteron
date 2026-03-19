//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 配置监视器
//!
//! 实现配置变更检测功能，支持轮询和Watch两种模式。
//! 统一使用TOML配置文件（config.toml）。

use crate::config::{ChangeSource, ConfigChangeRecord, ConfigHistory, FlowControlConfig};
use crate::config_loader::ConfigLoader;
use crate::error::FlowGuardError;
use crate::storage_trait::Storage;
use log::{debug, error, info};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task;
use tokio::time::sleep;

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
/// 支持从TOML配置文件读取配置，并检测配置变更。
pub struct ConfigWatcher {
    /// 存储后端
    storage: Arc<dyn Storage>,
    /// 配置文件路径
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
}

/// 监视模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WatchMode {
    /// 轮询模式
    #[default]
    Poll,
    /// Watch模式（文件系统事件）
    Watch,
    /// 混合模式（轮询 + Watch）
    Hybrid,
}

impl ConfigWatcher {
    /// 创建新的配置监视器
    ///
    /// # 参数
    /// - `storage`: 存储后端
    /// - `config_path`: 配置文件路径
    /// - `poll_interval`: 轮询间隔
    /// - `callback`: 配置变更回调
    /// - `watch_mode`: 监视模式
    pub fn new(
        storage: Arc<dyn Storage>,
        config_path: Option<PathBuf>,
        poll_interval: Duration,
        callback: ConfigChangeCallback,
        watch_mode: WatchMode,
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
        }
    }

    /// 启动配置监视器
    ///
    /// # 参数
    /// - `self`: self reference
    ///
    /// # 返回
    /// - `Ok(())`: 启动成功
    /// - `Err(FlowGuardError)`: 启动失败
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
                let watcher = self.clone_for_polling();
                tokio::spawn(async move {
                    if let Err(e) = watcher.start_polling().await {
                        error!("Polling watcher error: {:?}", e);
                    }
                });
            }
            WatchMode::Watch => {
                let watcher = self.clone_for_watching();
                tokio::spawn(async move {
                    if let Err(e) = watcher.start_watching().await {
                        error!("File watcher error: {:?}", e);
                    }
                });
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
    ///
    /// # 返回
    /// - `Ok(())`: 停止成功
    /// - `Err(FlowGuardError)`: 停止失败
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
        .map_err(|e| FlowGuardError::IoError(std::io::Error::other(e)))?;

        watcher
            .watch(config_path, RecursiveMode::NonRecursive)
            .map_err(|e| FlowGuardError::IoError(std::io::Error::other(e)))?;

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
        debug!("File event: {:?}", event);

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
    ///
    /// # 返回
    /// - `Ok(true)`: 配置已变更
    /// - `Ok(false)`: 配置未变更
    /// - `Err(FlowGuardError)`: 检查失败
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

        Ok(has_changed)
    }

    /// 加载配置
    async fn load_config(&self) -> Result<FlowControlConfig, FlowGuardError> {
        // 从文件加载
        if let Some(ref config_path) = self.config_path {
            if config_path.exists() {
                return self.load_config_from_file(config_path).await;
            }
        }

        Err(FlowGuardError::ConfigError(
            "无法加载配置：未指定配置文件路径".to_string(),
        ))
    }

    /// 从文件加载配置
    ///
    /// 使用ConfigLoader加载配置文件，支持TOML/YAML/JSON格式，
    /// 自动处理环境变量覆盖。
    async fn load_config_from_file(
        &self,
        path: &Path,
    ) -> Result<FlowControlConfig, FlowGuardError> {
        // 使用confers的ConfigLoader进行配置加载
        // ConfigLoader::load_from_file是同步方法，需要在阻塞线程池中执行
        let path = path.to_path_buf();
        task::spawn_blocking(move || ConfigLoader::load_from_file(&path))
            .await
            .map_err(|e| FlowGuardError::ConfigError(format!("配置加载任务失败: {}", e)))?
    }

    /// 加载当前配置（用于比较）
    async fn load_current_config(&self) -> Result<FlowControlConfig, FlowGuardError> {
        self.load_config().await
    }

    /// 手动触发配置检查
    ///
    /// # 返回
    /// - `Ok(true)`: 配置已变更
    /// - `Ok(false)`: 配置未变更
    /// - `Err(FlowGuardError)`: 检查失败
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
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GlobalConfig, Matcher, Rule};
    use crate::error::StorageError;
    use crate::storage_trait::Storage;
    use async_trait::async_trait;
    use chrono::Utc;
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::fs;

    // Simple in-memory storage for testing
    struct TestStorage {
        data: Mutex<HashMap<String, String>>,
    }

    impl TestStorage {
        fn new() -> Self {
            Self {
                data: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl Storage for TestStorage {
        async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
            let data = self.data.lock();
            Ok(data.get(key).cloned())
        }

        async fn set(&self, key: &str, value: &str, _ttl: Option<u64>) -> Result<(), StorageError> {
            let mut data = self.data.lock();
            data.insert(key.to_string(), value.to_string());
            Ok(())
        }

        async fn delete(&self, key: &str) -> Result<(), StorageError> {
            let mut data = self.data.lock();
            data.remove(key);
            Ok(())
        }
    }

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
        let storage = Arc::new(TestStorage::new());
        let callback: ConfigChangeCallback = Arc::new(|config, source| {
            Box::pin(async move {
                println!("Config changed: {:?} - {}", source, config.version);
                Ok(())
            })
        });
        let watcher = ConfigWatcher::new(
            storage,
            Some(PathBuf::from("config.toml")),
            Duration::from_secs(60),
            callback,
            WatchMode::Poll,
        );

        assert!(watcher.config_path.is_some());
        assert_eq!(watcher.watch_mode, WatchMode::Poll);
    }

    #[tokio::test]
    async fn test_config_watcher_start_stop() {
        let storage = Arc::new(TestStorage::new());
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();
        let callback: ConfigChangeCallback = Arc::new(move |_config, _source| {
            called_clone.store(true, Ordering::SeqCst);
            Box::pin(async move { Ok(()) })
        });

        let watcher = ConfigWatcher::new(
            storage,
            None,
            Duration::from_secs(1),
            callback,
            WatchMode::Poll,
        );

        // 启动
        let start_result = watcher.start().await;
        assert!(start_result.is_ok());

        // 停止
        let stop_result = watcher.stop().await;
        assert!(stop_result.is_ok());

        // 验证回调未被调用（因为没有配置文件）
        assert!(!called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_config_watcher_invalid_path() {
        let storage = Arc::new(TestStorage::new());
        let callback: ConfigChangeCallback =
            Arc::new(|_config, _source| Box::pin(async move { Ok(()) }));

        let watcher = ConfigWatcher::new(
            storage,
            Some(PathBuf::from("/nonexistent/config.toml")),
            Duration::from_secs(60),
            callback,
            WatchMode::Poll,
        );

        // 尝试加载不存在的配置
        let result = watcher.load_config().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_config_watcher_history() {
        let storage = Arc::new(TestStorage::new());
        let callback: ConfigChangeCallback =
            Arc::new(|_config, _source| Box::pin(async move { Ok(()) }));

        let watcher = ConfigWatcher::new(
            storage,
            None,
            Duration::from_secs(60),
            callback,
            WatchMode::Poll,
        );

        // 验证历史记录为空
        let history = watcher.get_history().await;
        assert!(history.is_empty());

        // 验证版本和哈希为空
        let version = watcher.get_current_version().await;
        assert!(version.is_empty());

        let hash = watcher.get_current_hash().await;
        assert!(hash.is_empty());
    }

    #[tokio::test]
    async fn test_config_watcher_watch_mode() {
        let storage = Arc::new(TestStorage::new());
        let callback: ConfigChangeCallback =
            Arc::new(|_config, _source| Box::pin(async move { Ok(()) }));

        let watcher_poll = ConfigWatcher::new(
            storage.clone(),
            None,
            Duration::from_secs(60),
            callback.clone(),
            WatchMode::Poll,
        );

        let watcher_watch = ConfigWatcher::new(
            storage.clone(),
            None,
            Duration::from_secs(60),
            callback.clone(),
            WatchMode::Watch,
        );

        let watcher_hybrid = ConfigWatcher::new(
            storage,
            None,
            Duration::from_secs(60),
            callback,
            WatchMode::Hybrid,
        );

        assert_eq!(watcher_poll.watch_mode, WatchMode::Poll);
        assert_eq!(watcher_watch.watch_mode, WatchMode::Watch);
        assert_eq!(watcher_hybrid.watch_mode, WatchMode::Hybrid);
    }

    #[tokio::test]
    async fn test_config_watcher_manual_check() {
        let storage = Arc::new(TestStorage::new());
        let callback: ConfigChangeCallback =
            Arc::new(|_config, _source| Box::pin(async move { Ok(()) }));

        let watcher = ConfigWatcher::new(
            storage,
            Some(PathBuf::from("/nonexistent/config.toml")),
            Duration::from_secs(60),
            callback,
            WatchMode::Poll,
        );

        // 手动检查应该返回错误（因为配置文件不存在）
        let result = watcher.manual_check().await;
        assert!(result.is_err());
    }
}
