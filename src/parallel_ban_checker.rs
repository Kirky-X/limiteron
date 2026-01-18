//! 并行封禁检查器
//!
//! 专门负责高效的并行封禁检查，支持多种目标类型的并发验证。

#[cfg(feature = "ban-manager")]
use crate::ban_manager::BanManager;
use crate::error::{BanInfo, FlowGuardError};
#[cfg(feature = "ban-manager")]
use crate::matchers::RequestContext;
#[cfg(not(feature = "ban-manager"))]
use crate::storage::BanStorage;
use crate::storage::BanTarget;
#[cfg(feature = "ban-manager")]
use futures::future::join_all;
use std::sync::Arc;
#[cfg(feature = "ban-manager")]
use tracing::{debug, instrument};

#[cfg(feature = "ban-manager")]
/// 并行封禁检查器
///
/// 提供高性能的多目标并行封禁检查功能。
pub struct ParallelBanChecker {
    ban_manager: Arc<BanManager>,
}

#[cfg(feature = "ban-manager")]
impl ParallelBanChecker {
    /// 创建新的并行封禁检查器
    pub fn new(ban_manager: Arc<BanManager>) -> Self {
        Self { ban_manager }
    }

    /// 并行检查多个封禁目标
    #[instrument(skip(self))]
    pub async fn check_targets_parallel(
        &self,
        targets: &[BanTarget],
        _context: Option<&RequestContext>,
    ) -> Result<Option<BanInfo>, FlowGuardError> {
        let start = std::time::Instant::now();

        debug!("开始并行封禁检查，目标数量: {}", targets.len());

        // 并行检查所有目标
        let check_futures: Vec<_> = targets
            .iter()
            .map(|target| {
                let ban_manager = self.ban_manager.clone();
                let target_clone = target.clone();
                let target_for_check = target_clone.clone();
                async move {
                    (
                        target_clone,
                        ban_manager.check_ban_priority(&[target_for_check]).await,
                    )
                }
            })
            .collect();

        // 等待所有检查完成
        let results = join_all(check_futures).await;

        // 查找第一个封禁结果
        for (target, ban_result) in results {
            if let Ok(Some(detail)) = ban_result {
                if detail.expires_at > chrono::Utc::now() {
                    let duration = start.elapsed();
                    debug!(
                        "发现活跃封禁: 目标={:?}, 原因={}, 耗时={:?}",
                        target, detail.reason, duration
                    );

                    return Ok(Some(BanInfo {
                        reason: detail.reason.clone(),
                        banned_until: detail.expires_at,
                        ban_times: detail.ban_times,
                    }));
                }
            }
        }

        debug!("并行封禁检查完成，总耗时: {:?}", start.elapsed());
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

#[cfg(not(feature = "ban-manager"))]
/// 并行封禁检查器（无操作存根）
///
/// 当 ban-manager 特性未启用时提供空实现。
pub struct ParallelBanChecker;

#[cfg(not(feature = "ban-manager"))]
impl ParallelBanChecker {
    /// 创建新的并行封禁检查器（空操作）
    pub fn new(_ban_storage: Arc<dyn BanStorage>) -> Self {
        Self
    }

    /// 并行检查多个封禁目标（空操作）
    pub async fn check_targets_parallel(
        &self,
        _targets: &[BanTarget],
        _context: Option<&()>,
    ) -> Result<Option<BanInfo>, FlowGuardError> {
        Ok(None)
    }

    /// 快速检查单个封禁目标（空操作）
    pub async fn check_single_target(
        &self,
        _target: &BanTarget,
    ) -> Result<Option<BanInfo>, FlowGuardError> {
        Ok(None)
    }

    /// 检查用户ID是否被封禁（空操作）
    pub async fn check_user_banned(
        &self,
        _user_id: &str,
    ) -> Result<Option<BanInfo>, FlowGuardError> {
        Ok(None)
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_types)]
mod tests {
    use super::*;
    #[cfg(feature = "ban-manager")]
    use crate::ban_manager::BanManager;
    #[cfg(feature = "ban-manager")]
    use crate::error::StorageError;
    #[cfg(feature = "ban-manager")]
    use crate::storage::{BanHistory, BanRecord, BanStorage};
    #[cfg(feature = "ban-manager")]
    use async_trait::async_trait;
    #[cfg(feature = "ban-manager")]
    #[allow(clippy::disallowed_types)]
    use std::collections::HashMap;
    #[cfg(feature = "ban-manager")]
    use tokio::sync::Mutex;

    #[cfg(feature = "ban-manager")]
    struct TestBanStorage {
        bans: Mutex<HashMap<BanTarget, BanRecord>>,
    }

    #[cfg(feature = "ban-manager")]
    impl TestBanStorage {
        fn new() -> Self {
            Self {
                bans: Mutex::new(HashMap::new()),
            }
        }
    }

    #[cfg(feature = "ban-manager")]
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
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[tokio::test]
    #[cfg(feature = "ban-manager")]
    async fn test_parallel_ban_checker() {
        let ban_storage = Arc::new(TestBanStorage::new());
        let ban_manager = Arc::new(BanManager::new(ban_storage.clone(), None).await.unwrap());

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

    #[tokio::test]
    #[cfg(not(feature = "ban-manager"))]
    async fn test_parallel_ban_checker_stub() {
        let checker = ParallelBanChecker;
        let result = checker.check_user_banned("test_user").await.unwrap();
        assert!(result.is_none());
    }
}
