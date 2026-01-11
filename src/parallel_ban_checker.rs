//! 并行封禁检查器
//!
//! 专门负责高效的并行封禁检查，支持多种目标类型的并发验证。

use crate::ban_manager::{BanManager, BanTarget};
use crate::error::{BanInfo as BanInfoAlias, Decision, FlowGuardError};
use crate::matchers::RequestContext;
use futures::future::join_all;
use std::sync::Arc;
use tracing::{debug, instrument};

/// 并行封禁检查器
///
/// 提供高性能的多目标并行封禁检查功能。
pub struct ParallelBanChecker {
    ban_manager: Arc<BanManager>,
}

impl ParallelBanChecker {
    /// 创建新的并行封禁检查器
    pub fn new(ban_manager: Arc<BanManager>) -> Self {
        Self { ban_manager }
    }

    /// 并行检查多个封禁目标
    ///
    /// # 参数
    /// - `targets`: 封禁目标列表
    /// - `context`: 请求上下文（可选，用于日志）
    ///
    /// # 返回
    /// - `Ok(Option<BanInfo>)`: 如果发现封禁，返回封禁信息
    /// - `Ok(None)`: 如果没有封禁
    /// - `Err(_)`: 检查过程中的错误
    #[instrument(skip(self))]
    pub async fn check_targets_parallel(
        &self,
        targets: &[BanTarget],
        context: Option<&RequestContext>,
    ) -> Result<Option<BanInfo>, FlowGuardError> {
        let start = std::time::Instant::now();

        debug!("开始并行封禁检查，目标数量: {}", targets.len());

        // 并行检查所有目标
        let check_futures: Vec<_> = targets
            .iter()
            .map(|target| {
                let ban_manager = self.ban_manager.clone();
                let target = target.clone();
                async move { ban_manager.check_ban_priority(&[target]).await }
            })
            .collect();

        // 等待所有检查完成
        let results = join_all(check_futures).await;

        // 查找第一个封禁结果
        for ban_detail in results {
            if let Some(detail) = ban_detail {
                if detail.banned_at > chrono::Utc::now() {
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
        self.check_targets_parallel(&[target.clone()], None).await
    }

    /// 检查用户ID是否被封禁
    pub async fn check_user_banned(
        &self,
        user_id: &str,
    ) -> Result<Option<BanInfo>, FlowGuardError> {
        let target = BanTarget::UserId(user_id.to_string());
        self.check_single_target(&target).await
    }

    /// 检查IP是否被封禁
    pub async fn check_ip_banned(&self, ip: &str) -> Result<Option<BanInfo>, FlowGuardError> {
        let target = BanTarget::Ip(ip.to_string());
        self.check_single_target(&target).await
    }

    /// 检查MAC是否被封禁
    pub async fn check_mac_banned(&self, mac: &str) -> Result<Option<BanInfo>, FlowGuardError> {
        let target = BanTarget::Mac(mac.to_string());
        self.check_single_target(&target).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ban_manager::MockBanStorage;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_parallel_ban_checker() {
        let ban_storage = Arc::new(MockBanStorage::new());
        let ban_manager = Arc::new(BanManager::new(ban_storage).await.unwrap());
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
    async fn test_parallel_performance() {
        let ban_storage = Arc::new(MockBanStorage::new());
        let ban_manager = Arc::new(BanManager::new(ban_storage).await.unwrap());
        let checker = ParallelBanChecker::new(ban_manager);

        let start = std::time::Instant::now();

        // 性能测试：大量并发检查
        let mut handles = Vec::new();
        for i in 0..100 {
            let checker = checker.clone();
            let handle =
                tokio::spawn(
                    async move { checker.check_user_banned(&format!("user_{}", i)).await },
                );
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        let duration = start.elapsed();
        println!("100次并行检查耗时: {:?}", duration);

        // 性能应该明显优于串行检查
        assert!(duration.as_millis() < 1000); // 应该在1秒内完成
    }
}
