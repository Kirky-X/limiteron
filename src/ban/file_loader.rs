//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 文件封禁加载器模块
//!
//! 从 YAML 文件加载封禁规则到 BanManager，支持热重载（文件变更自动重载）。
//!
//! # YAML 格式
//!
//! ```yaml
//! bans:
//!   - target:
//!       type: ip
//!       value: "192.168.1.1"
//!     reason: "恶意请求"
//!     duration_secs: 3600  # 可选，null = 使用退避算法
//!   - target:
//!       type: geo
//!       value:
//!         country_code: "CN"
//!     reason: "地区封禁"
//! ```

use crate::ban::{BanManager, BanSource};
use crate::error::FlowGuardError;
use crate::storage::BanTarget;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// 单条封禁文件条目
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BanFileEntry {
    /// 封禁目标
    pub target: BanTarget,
    /// 封禁原因
    pub reason: String,
    /// 封禁时长（秒），None = 使用退避算法自动计算
    #[serde(default)]
    pub duration_secs: Option<u64>,
}

/// 封禁文件根结构
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BanFile {
    /// 封禁条目列表
    #[serde(default)]
    pub bans: Vec<BanFileEntry>,
}

/// 加载结果
#[derive(Debug, Clone, Default)]
pub struct LoadResult {
    /// 成功加载的封禁数量
    pub success_count: usize,
    /// 失败的封禁数量
    pub failure_count: usize,
    /// 失败详情
    pub errors: Vec<BanLoadError>,
}

/// 单条封禁加载失败信息
#[derive(Debug, Clone)]
pub struct BanLoadError {
    /// 目标描述（用于日志定位）
    pub target_desc: String,
    /// 错误信息
    pub error: String,
}

/// 文件封禁加载器
///
/// 从 YAML 文件加载封禁规则，可选支持文件变更热重载。
pub struct BanFileLoader {
    path: PathBuf,
    #[cfg(feature = "config-watcher")]
    watch_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl BanFileLoader {
    /// 创建新的文件加载器
    ///
    /// # 参数
    /// - `path`: YAML 文件路径
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            #[cfg(feature = "config-watcher")]
            watch_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// 获取文件路径
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 一次性加载文件中的封禁规则到 BanManager
    ///
    /// 单条加载失败不会中断整体加载，失败详情记录在 `LoadResult.errors` 中。
    /// 文件不存在或 YAML 解析失败会返回 `Err`。
    ///
    /// # 返回
    /// - `Ok(LoadResult)`: 加载完成（可能含部分失败）
    /// - `Err(FlowGuardError)`: 文件读取或 YAML 解析失败
    pub async fn load_once(&self, manager: &BanManager) -> Result<LoadResult, FlowGuardError> {
        // 文件大小预检查：防止 YAML 炸弹（billion laughs attack）导致 OOM
        const MAX_BAN_FILE_SIZE: u64 = 2 * 1024 * 1024; // 2 MB
        let file_meta = std::fs::metadata(&self.path).map_err(|e| {
            FlowGuardError::ConfigError(format!(
                "读取封禁文件元数据失败 {}: {}",
                self.path.display(),
                e
            ))
        })?;
        if file_meta.len() > MAX_BAN_FILE_SIZE {
            return Err(FlowGuardError::ConfigError(format!(
                "封禁文件过大: {} ({} bytes, 上限 {} bytes)",
                self.path.display(),
                file_meta.len(),
                MAX_BAN_FILE_SIZE
            )));
        }

        let content = std::fs::read_to_string(&self.path).map_err(|e| {
            FlowGuardError::ConfigError(format!("读取封禁文件失败 {}: {}", self.path.display(), e))
        })?;

        let ban_file: BanFile = serde_yaml::from_str(&content).map_err(|e| {
            FlowGuardError::ConfigError(format!(
                "解析封禁文件 YAML 失败 {}: {}",
                self.path.display(),
                e
            ))
        })?;

        let mut result = LoadResult::default();

        for entry in &ban_file.bans {
            let target_desc = format!("{:?}", entry.target);
            let duration = entry.duration_secs.map(Duration::from_secs);
            let source = BanSource::Manual {
                operator: "file_loader".to_string(),
            };

            match manager
                .create_ban(
                    entry.target.clone(),
                    entry.reason.clone(),
                    source,
                    serde_json::json!({"source": "file", "path": self.path.display().to_string()}),
                    duration,
                )
                .await
            {
                Ok(_) => {
                    result.success_count += 1;
                }
                Err(e) => {
                    log::warn!("文件加载封禁失败: target={:?}, error={}", entry.target, e);
                    result.errors.push(BanLoadError {
                        target_desc,
                        error: e.to_string(),
                    });
                    result.failure_count += 1;
                }
            }
        }

        Ok(result)
    }

    /// 启动文件变更热重载
    ///
    /// 当文件被修改时，自动重新加载封禁规则。
    /// 使用 notify crate 监听文件系统事件。
    ///
    /// # 参数
    /// - `manager`: BanManager 实例（会被 clone 到后台任务中）
    #[cfg(feature = "config-watcher")]
    pub async fn start_watching(&self, manager: BanManager) -> Result<(), FlowGuardError> {
        use notify::{RecommendedWatcher, RecursiveMode, Watcher};
        use tokio::sync::mpsc;

        // 如果已有监听任务，先停止
        self.stop_watching().await;

        let path = self.path.clone();
        let (tx, mut rx) = mpsc::channel::<()>(16);

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    if let notify::EventKind::Modify(_) | notify::EventKind::Create(_) = event.kind
                    {
                        // 文件变更，发送信号（忽略发送失败，说明接收端已关闭）
                        let _ = tx.blocking_send(());
                    }
                }
            },
            notify::Config::default().with_poll_interval(Duration::from_secs(2)),
        )
        .map_err(|e| FlowGuardError::ConfigError(format!("启动文件监听失败: {}", e)))?;

        // 监听文件所在目录（监听文件本身在某些编辑器下会丢失事件）
        let watch_dir = path.parent().unwrap_or(Path::new("."));
        watcher
            .watch(watch_dir, RecursiveMode::NonRecursive)
            .map_err(|e| FlowGuardError::ConfigError(format!("注册文件监听失败: {}", e)))?;

        let manager_clone = manager.clone();
        let loader_path = path.clone();

        let handle = tokio::spawn(async move {
            // watcher 必须在任务中保持存活
            let _watcher = watcher;
            let loader = BanFileLoader::new(loader_path);

            while rx.recv().await.is_some() {
                // debounce: 收集 500ms 窗口内的所有事件，只触发一次重载
                // 编辑器原子写入（写临时文件 + 重命名）会短时间内触发多个事件，
                // 防抖避免每次事件都同步读文件 + 解析 YAML + 写存储造成的性能抖动
                let debounce = tokio::time::sleep(Duration::from_millis(500));
                tokio::pin!(debounce);
                loop {
                    tokio::select! {
                        _ = &mut debounce => break,
                        recv = rx.recv() => match recv {
                            Some(_) => continue,
                            None => break,
                        },
                    }
                }
                log::info!("封禁文件变更，触发重载: {}", loader.path().display());
                match loader.load_once(&manager_clone).await {
                    Ok(r) => {
                        log::info!(
                            "封禁文件重载完成: 成功 {} 条, 失败 {} 条",
                            r.success_count,
                            r.failure_count
                        );
                    }
                    Err(e) => {
                        log::error!("封禁文件重载失败: {}", e);
                    }
                }
            }
        });

        *self.watch_handle.write() = Some(handle);
        Ok(())
    }

    /// 停止文件变更热重载
    #[cfg(feature = "config-watcher")]
    pub async fn stop_watching(&self) {
        if let Some(handle) = self.watch_handle.write().take() {
            handle.abort();
        }
    }
}

#[cfg(feature = "config-watcher")]
impl Drop for BanFileLoader {
    fn drop(&mut self) {
        if let Some(handle) = self.watch_handle.write().take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ban::BanManagerConfig;
    use crate::storage::MemoryBanStorage;
    use std::io::Write;

    /// 创建临时 YAML 文件
    fn write_temp_yaml(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().expect("创建临时文件失败");
        f.write_all(content.as_bytes()).expect("写入临时文件失败");
        f
    }

    /// 创建测试用 BanManager（内存存储）
    async fn make_manager() -> BanManager {
        let storage = MemoryBanStorage::create_ban_storage();
        BanManager::with_dependencies(storage, BanManagerConfig::default())
            .await
            .expect("创建 BanManager 失败")
    }

    #[test]
    fn test_ban_file_entry_deserialize_ip() {
        let yaml = r#"
target:
  type: ip
  value: "192.168.1.1"
reason: "恶意请求"
duration_secs: 3600
"#;
        let entry: BanFileEntry = serde_yaml::from_str(yaml).expect("解析失败");
        assert_eq!(entry.target, BanTarget::Ip("192.168.1.1".to_string()));
        assert_eq!(entry.reason, "恶意请求");
        assert_eq!(entry.duration_secs, Some(3600));
    }

    #[test]
    fn test_ban_file_entry_deserialize_geo() {
        let yaml = r#"
target:
  type: geo
  value:
    country_code: "CN"
reason: "地区封禁"
"#;
        let entry: BanFileEntry = serde_yaml::from_str(yaml).expect("解析失败");
        assert_eq!(
            entry.target,
            BanTarget::Geo {
                country_code: "CN".to_string()
            }
        );
        assert_eq!(entry.reason, "地区封禁");
        assert_eq!(entry.duration_secs, None);
    }

    #[test]
    fn test_ban_file_entry_deserialize_user() {
        let yaml = r#"
target:
  type: user
  value: "user123"
reason: "违规用户"
duration_secs: 7200
"#;
        let entry: BanFileEntry = serde_yaml::from_str(yaml).expect("解析失败");
        assert_eq!(entry.target, BanTarget::UserId("user123".to_string()));
        assert_eq!(entry.duration_secs, Some(7200));
    }

    #[test]
    fn test_ban_file_entry_deserialize_mac() {
        let yaml = r#"
target:
  type: mac
  value: "00:1A:2B:3C:4D:5E"
reason: "MAC 封禁"
"#;
        let entry: BanFileEntry = serde_yaml::from_str(yaml).expect("解析失败");
        assert_eq!(
            entry.target,
            BanTarget::Mac("00:1A:2B:3C:4D:5E".to_string())
        );
        assert_eq!(entry.duration_secs, None);
    }

    #[test]
    fn test_ban_file_root_default() {
        let yaml = "";
        let file: BanFile = serde_yaml::from_str(yaml).expect("解析失败");
        assert!(file.bans.is_empty());
    }

    #[test]
    fn test_ban_file_root_with_entries() {
        let yaml = r#"
bans:
  - target:
      type: ip
      value: "1.2.3.4"
    reason: "test1"
  - target:
      type: user
      value: "u1"
    reason: "test2"
"#;
        let file: BanFile = serde_yaml::from_str(yaml).expect("解析失败");
        assert_eq!(file.bans.len(), 2);
        assert_eq!(file.bans[0].reason, "test1");
        assert_eq!(file.bans[1].reason, "test2");
    }

    #[tokio::test]
    async fn test_load_once_success() {
        let yaml = r#"
bans:
  - target:
      type: ip
      value: "192.168.1.1"
    reason: "恶意请求"
    duration_secs: 3600
  - target:
      type: user
      value: "user123"
    reason: "违规"
"#;
        let file = write_temp_yaml(yaml);
        let manager = make_manager().await;
        let loader = BanFileLoader::new(file.path());

        let result = loader.load_once(&manager).await.expect("加载失败");
        assert_eq!(result.success_count, 2);
        assert_eq!(result.failure_count, 0);
        assert!(result.errors.is_empty());

        // 验证封禁已写入存储
        let ban = manager
            .read_ban(&BanTarget::Ip("192.168.1.1".to_string()))
            .await
            .expect("查询失败");
        assert!(ban.is_some());
    }

    #[tokio::test]
    async fn test_load_once_empty_file() {
        let file = write_temp_yaml("");
        let manager = make_manager().await;
        let loader = BanFileLoader::new(file.path());

        let result = loader.load_once(&manager).await.expect("加载失败");
        assert_eq!(result.success_count, 0);
        assert_eq!(result.failure_count, 0);
    }

    #[tokio::test]
    async fn test_load_once_nonexistent_file() {
        let manager = make_manager().await;
        let loader = BanFileLoader::new("/nonexistent/path/bans.yaml");

        let result = loader.load_once(&manager).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        // 文件不存在时 metadata 检查先失败
        assert!(
            err.contains("读取封禁文件元数据失败") || err.contains("读取封禁文件失败"),
            "错误信息: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_load_once_invalid_yaml() {
        let yaml = "this is not: valid: yaml: [";
        let file = write_temp_yaml(yaml);
        let manager = make_manager().await;
        let loader = BanFileLoader::new(file.path());

        let result = loader.load_once(&manager).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("解析封禁文件 YAML 失败"), "错误信息: {}", err);
    }

    #[tokio::test]
    async fn test_load_once_partial_failure() {
        // 第一条 IP 无效，第二条有效
        let yaml = r#"
bans:
  - target:
      type: ip
      value: "invalid_ip"
    reason: "无效 IP"
  - target:
      type: ip
      value: "10.0.0.1"
    reason: "有效 IP"
"#;
        let file = write_temp_yaml(yaml);
        let manager = make_manager().await;
        let loader = BanFileLoader::new(file.path());

        let result = loader.load_once(&manager).await.expect("加载失败");
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 1);
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].target_desc.contains("invalid_ip"));
    }

    #[tokio::test]
    async fn test_load_once_geo_target() {
        let yaml = r#"
bans:
  - target:
      type: geo
      value:
        country_code: "CN"
    reason: "地区封禁"
    duration_secs: 86400
"#;
        let file = write_temp_yaml(yaml);
        let manager = make_manager().await;
        let loader = BanFileLoader::new(file.path());

        let result = loader.load_once(&manager).await.expect("加载失败");
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);

        let ban = manager
            .read_ban(&BanTarget::Geo {
                country_code: "CN".to_string(),
            })
            .await
            .expect("查询失败");
        assert!(ban.is_some());
    }

    #[tokio::test]
    async fn test_load_once_invalid_geo() {
        let yaml = r#"
bans:
  - target:
      type: geo
      value:
        country_code: "china"
    reason: "无效国家代码"
"#;
        let file = write_temp_yaml(yaml);
        let manager = make_manager().await;
        let loader = BanFileLoader::new(file.path());

        let result = loader.load_once(&manager).await.expect("加载失败");
        assert_eq!(result.success_count, 0);
        assert_eq!(result.failure_count, 1);
    }

    #[tokio::test]
    async fn test_load_once_idempotent_recreate() {
        // 同一文件加载两次，第二次应仍成功（MemoryBanStorage 不追踪历史，ban_times 不递增）
        let yaml = r#"
bans:
  - target:
      type: ip
      value: "10.0.0.1"
    reason: "测试"
    duration_secs: 3600
"#;
        let file = write_temp_yaml(yaml);
        let manager = make_manager().await;
        let loader = BanFileLoader::new(file.path());

        let r1 = loader.load_once(&manager).await.expect("第一次加载失败");
        assert_eq!(r1.success_count, 1);

        let r2 = loader.load_once(&manager).await.expect("第二次加载失败");
        assert_eq!(r2.success_count, 1);

        // 验证封禁仍存在（MemoryBanStorage::get_history 返回 None，ban_times 保持 1）
        let ban = manager
            .read_ban(&BanTarget::Ip("10.0.0.1".to_string()))
            .await
            .expect("查询失败")
            .expect("封禁应存在");
        assert_eq!(ban.ban_times, 1);
    }

    #[test]
    fn test_loader_path() {
        let loader = BanFileLoader::new("/tmp/bans.yaml");
        assert_eq!(loader.path(), Path::new("/tmp/bans.yaml"));
    }

    #[test]
    fn test_load_result_default() {
        let r = LoadResult::default();
        assert_eq!(r.success_count, 0);
        assert_eq!(r.failure_count, 0);
        assert!(r.errors.is_empty());
    }

    #[cfg(feature = "config-watcher")]
    #[tokio::test]
    async fn test_stop_watching_without_start() {
        // 停止一个未启动的监听不应 panic
        let loader = BanFileLoader::new("/tmp/nonexistent.yaml");
        loader.stop_watching().await;
    }

    #[cfg(feature = "config-watcher")]
    #[tokio::test]
    async fn test_start_and_stop_watching() {
        let yaml = r#"
bans:
  - target:
      type: ip
      value: "10.0.0.1"
    reason: "热重载测试"
"#;
        let file = write_temp_yaml(yaml);
        let manager = make_manager().await;
        let loader = Arc::new(BanFileLoader::new(file.path()));

        // 初始加载
        let r = loader.load_once(&manager).await.expect("加载失败");
        assert_eq!(r.success_count, 1);

        // 启动监听
        loader
            .start_watching(manager.clone())
            .await
            .expect("启动监听失败");

        // 等待监听就绪
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 修改文件，追加一条封禁
        let new_yaml = r#"
bans:
  - target:
      type: ip
      value: "10.0.0.1"
    reason: "热重载测试"
  - target:
      type: ip
      value: "10.0.0.2"
    reason: "新增封禁"
"#;
        std::fs::write(file.path(), new_yaml).expect("写入失败");

        // 等待 notify 触发 + 重载完成
        tokio::time::sleep(Duration::from_secs(3)).await;

        // 验证新封禁已加载
        let ban = manager
            .read_ban(&BanTarget::Ip("10.0.0.2".to_string()))
            .await
            .expect("查询失败");
        assert!(ban.is_some(), "热重载后应能查到新封禁");

        // 停止监听
        loader.stop_watching().await;
    }
}
