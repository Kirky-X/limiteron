//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 并行封禁检查器
//!
//! 专门负责高效的并行封禁检查，支持多种目标类型的并发验证。
//! 使用 `FuturesUnordered` 实现真正的并行提前退出机制。
//! 需要同时启用 `ban-manager` 和 `parallel-checker` feature。

use super::BanTarget;
use crate::error::{BanInfo, FlowGuardError};
use crate::matchers::RequestContext;
use futures::stream::{FuturesUnordered, StreamExt};
use log::debug;
use std::sync::Arc;

use crate::ban::BanManager;

/// 并行封禁检查器
///
/// 提供高性能的多目标并行封禁检查功能。
/// 使用提前退出机制，一旦发现封禁立即返回，无需等待所有检查完成。
pub struct ParallelBanChecker {
    ban_manager: Arc<BanManager>,
}

impl ParallelBanChecker {
    /// 创建新的并行封禁检查器
    pub fn new(ban_manager: Arc<BanManager>) -> Self {
        Self { ban_manager }
    }

    /// 并行检查多个封禁目标（提前退出）
    ///
    /// 使用 `FuturesUnordered` 实现真正的并行提前退出机制：
    /// - 所有检查任务并行执行
    /// - 一旦发现第一个活跃封禁，立即返回，取消其他任务
    /// - 无需等待所有检查完成，提高性能
    pub async fn check_targets_parallel(
        &self,
        targets: &[BanTarget],
        _context: Option<&RequestContext>,
    ) -> Result<Option<BanInfo>, FlowGuardError> {
        let start = std::time::Instant::now();

        debug!("开始并行封禁检查，目标数量: {}", targets.len());

        if targets.is_empty() {
            return Ok(None);
        }

        // 创建并行检查 futures 集合
        let mut check_futures = FuturesUnordered::new();

        for target in targets {
            let ban_manager = self.ban_manager.clone();
            let target_clone = target.clone();
            check_futures.push(async move {
                match ban_manager
                    .check_ban_priority(&[target_clone.clone()])
                    .await
                {
                    Ok(Some(detail)) if detail.expires_at > chrono::Utc::now() => {
                        debug!(
                            "发现活跃封禁: 目标={:?}, 原因={}",
                            target_clone, detail.reason
                        );
                        Ok(Some(BanInfo::new(
                            detail.reason.clone(),
                            detail.expires_at,
                            detail.ban_times,
                        )))
                    }
                    Ok(_) => Ok(None),
                    Err(e) => Err(e),
                }
            });
        }

        // 按完成顺序处理结果，实现提前退出
        while let Some(result) = check_futures.next().await {
            match result {
                Ok(Some(ban_info)) => {
                    debug!("并行封禁检查完成（提前退出），耗时: {:?}", start.elapsed());
                    return Ok(Some(ban_info));
                }
                Ok(None) => continue,
                Err(e) => {
                    debug!("封禁检查出错: {}", e);
                    continue;
                }
            }
        }

        debug!("并行封禁检查完成，无封禁，耗时: {:?}", start.elapsed());
        Ok(None)
    }

    /// 快速检查单个封禁目标
    pub async fn check_single_target(
        &self,
        target: &BanTarget,
    ) -> Result<Option<BanInfo>, FlowGuardError> {
        self.check_targets_parallel(std::slice::from_ref(target), None)
            .await
    }

    /// 检查用户ID是否被封禁
    pub async fn check_user_banned(
        &self,
        user_id: &str,
    ) -> Result<Option<BanInfo>, FlowGuardError> {
        let target = BanTarget::UserId(user_id.to_string());
        self.check_single_target(&target).await
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_types)]
mod tests {
    use super::*;
    use crate::ban::BanManager;
    use crate::error::StorageError;
    use crate::storage::{BanHistory, BanRecord, BanStorage};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use tokio::sync::Mutex;

    struct TestBanStorage {
        bans: Mutex<HashMap<BanTarget, BanRecord>>,
    }

    impl TestBanStorage {
        fn new() -> Self {
            Self {
                bans: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl BanStorage for TestBanStorage {
        async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
            let bans = self.bans.lock().await;
            Ok(bans.get(target).cloned())
        }

        async fn save(&self, record: &BanRecord) -> Result<(), StorageError> {
            let mut bans = self.bans.lock().await;
            bans.insert(record.target.clone(), record.clone());
            Ok(())
        }

        async fn get_history(
            &self,
            _target: &BanTarget,
        ) -> Result<Option<BanHistory>, StorageError> {
            Ok(None)
        }
        async fn increment_ban_times(&self, _target: &BanTarget) -> Result<u64, StorageError> {
            Ok(0)
        }
        async fn get_ban_times(&self, _target: &BanTarget) -> Result<u64, StorageError> {
            Ok(0)
        }
        async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError> {
            let mut bans = self.bans.lock().await;
            bans.remove(target);
            Ok(())
        }
        async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
            Ok(0)
        }
        async fn list_bans(
            &self,
            _active_only: bool,
            _offset: u64,
            _limit: u64,
        ) -> Result<Vec<BanRecord>, StorageError> {
            Ok(Vec::new())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[tokio::test]
    async fn test_parallel_ban_checker() {
        let ban_storage = Arc::new(TestBanStorage::new());
        let ban_manager = Arc::new(
            BanManager::builder()
                .with_storage(ban_storage.clone())
                .build()
                .await
                .unwrap(),
        );

        // Setup ban
        let banned_user = BanTarget::UserId("banned_user".to_string());
        let record = BanRecord {
            target: banned_user.clone(),
            ban_times: 1,
            duration: std::time::Duration::from_secs(3600),
            banned_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(3600),
            is_manual: true,
            reason: "Test ban".to_string(),
        };
        ban_storage.save(&record).await.unwrap();

        let checker = ParallelBanChecker::new(ban_manager);

        // 测试多个目标的并行检查
        let targets = vec![
            BanTarget::UserId("test_user".to_string()),
            BanTarget::Ip("192.168.1.1".to_string()),
            BanTarget::Mac("AA:BB:CC:DD:EE:FF".to_string()),
        ];

        let result = checker
            .check_targets_parallel(&targets, None)
            .await
            .unwrap();
        assert!(result.is_none());

        // 测试单个目标检查
        let user_result = checker.check_user_banned("banned_user").await.unwrap();
        assert!(user_result.is_some());
    }
}
