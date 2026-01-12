//! Common模块测试入口

mod common;

#[cfg(test)]
mod tests {
    use super::common::*;
    use limiteron::storage::{BanStorage, QuotaStorage};

    #[tokio::test]
    async fn test_mock_quota_storage() {
        let storage = MockQuotaStorage::new();

        // 消费配额
        let result = storage
            .consume("user1", "resource1", 100, 1000, std::time::Duration::from_secs(60))
            .await
            .unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 900);

        // 再次消费
        let result = storage
            .consume("user1", "resource1", 500, 1000, std::time::Duration::from_secs(60))
            .await
            .unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 400);

        // 超过限制
        let result = storage
            .consume("user1", "resource1", 500, 1000, std::time::Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_mock_ban_storage() {
        use limiteron::storage::{BanRecord, BanTarget};
        use std::time::Duration;

        let storage = MockBanStorage::new();

        // 添加封禁
        let ban = BanRecord {
            target: BanTarget::Ip("192.168.1.1".to_string()),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "Test".to_string(),
        };

        storage.save(&ban).await.unwrap();

        // 查询封禁
        let result = storage
            .is_banned(&BanTarget::Ip("192.168.1.1".to_string()))
            .await
            .unwrap();
        assert!(result.is_some());

        // 移除封禁
        storage
            .remove_ban(&BanTarget::Ip("192.168.1.1".to_string()))
            .await
            .unwrap();

        // 再次查询
        let result = storage
            .is_banned(&BanTarget::Ip("192.168.1.1".to_string()))
            .await
            .unwrap();
        assert!(result.is_none());
    }
}
