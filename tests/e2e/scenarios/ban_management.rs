//! 封禁管理场景测试
//!
//! 测试用户被封禁后无法访问，以及封禁过期后可访问的完整流程

#[cfg(feature = "ban-manager")]
use ahash::AHashMap;
#[cfg(feature = "ban-manager")]
use chrono::Utc;
#[cfg(feature = "ban-manager")]
use limiteron::ban::{BanManager, BanManagerConfig, BanSource, BanTarget};
#[cfg(feature = "ban-manager")]
use limiteron::error::StorageError;
#[cfg(feature = "ban-manager")]
use limiteron::storage::{BanHistory, BanRecord, BanStorage};
#[cfg(feature = "ban-manager")]
use std::sync::Arc;
#[cfg(feature = "ban-manager")]
use std::time::Duration;

// ==================== Mock Storage ====================

#[cfg(feature = "ban-manager")]
#[derive(Clone)]
struct TestBanStorage {
    bans: Arc<tokio::sync::RwLock<AHashMap<BanTarget, BanRecord>>>,
    history: Arc<tokio::sync::RwLock<AHashMap<BanTarget, BanHistory>>>,
}

#[cfg(feature = "ban-manager")]
impl TestBanStorage {
    fn new() -> Self {
        Self {
            bans: Arc::new(tokio::sync::RwLock::new(AHashMap::new())),
            history: Arc::new(tokio::sync::RwLock::new(AHashMap::new())),
        }
    }
}

#[cfg(feature = "ban-manager")]
#[async_trait::async_trait]
impl BanStorage for TestBanStorage {
    async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
        let now = Utc::now();
        let mut bans = self.bans.write().await;
        if let Some(record) = bans.get(target) {
            if record.expires_at > now {
                return Ok(Some(record.clone()));
            }
            bans.remove(target);
        }
        Ok(None)
    }

    async fn save(&self, record: &BanRecord) -> Result<(), StorageError> {
        let mut bans = self.bans.write().await;
        bans.insert(record.target.clone(), record.clone());
        let mut history = self.history.write().await;
        history.insert(
            record.target.clone(),
            BanHistory {
                ban_times: record.ban_times,
                last_banned_at: record.banned_at,
            },
        );
        Ok(())
    }

    async fn get_history(&self, target: &BanTarget) -> Result<Option<BanHistory>, StorageError> {
        Ok(self.history.read().await.get(target).cloned())
    }

    async fn increment_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let mut history = self.history.write().await;
        let next = match history.get(target) {
            Some(value) => value.ban_times.saturating_add(1),
            None => 1,
        };
        history.insert(
            target.clone(),
            BanHistory {
                ban_times: next,
                last_banned_at: Utc::now(),
            },
        );
        Ok(next as u64)
    }

    async fn get_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let history = self.history.read().await;
        Ok(history.get(target).map(|v| v.ban_times as u64).unwrap_or(0))
    }

    async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError> {
        self.bans.write().await.remove(target);
        Ok(())
    }

    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        let now = Utc::now();
        let mut bans = self.bans.write().await;
        let before = bans.len();
        bans.retain(|_, record| record.expires_at > now);
        let removed = before.saturating_sub(bans.len());
        Ok(removed as u64)
    }

    async fn list_bans(
        &self,
        active_only: bool,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BanRecord>, StorageError> {
        let bans = self.bans.read().await;
        let now = Utc::now();
        let mut records: Vec<_> = bans.values().cloned().collect();

        if active_only {
            records.retain(|r| r.expires_at > now);
        }

        let start = offset as usize;
        let end = (offset.saturating_add(limit)) as usize;

        if start >= records.len() {
            return Ok(vec![]);
        }

        Ok(records.into_iter().skip(start).take(end - start).collect())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ==================== E2E Scenario Tests ====================

/// 场景 1: 用户被封禁后无法访问
///
/// 管理员手动封禁用户后，该用户的所有请求被拒绝。
#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn e2e_ban_user_cannot_access() {
    let storage: Arc<dyn BanStorage> = Arc::new(TestBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let ban_manager = BanManager::with_dependencies(storage, config)
        .await
        .expect("Failed to create ban manager");

    let target = BanTarget::UserId("banned_user_1".to_string());

    // 封禁用户
    ban_manager
        .create_ban(
            target.clone(),
            "违规操作".to_string(),
            BanSource::Manual {
                operator: "admin@example.com".to_string(),
            },
            serde_json::json!({"reason_code": "ABUSE", "severity": "high"}),
            Some(Duration::from_secs(3600)),
        )
        .await
        .expect("Failed to create ban");

    // 验证用户被封禁
    let is_banned = ban_manager
        .is_banned(&target)
        .await
        .expect("Failed to check ban status");
    assert!(is_banned.is_some(), "User should be banned");

    let ban_record = is_banned.unwrap();
    assert_eq!(ban_record.reason, "违规操作");
    assert!(matches!(ban_record.source, BanSource::Manual { .. }));
}

/// 场景 2: 封禁过期后用户可访问
///
/// 封禁到期后，用户可以正常访问。
#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn e2e_ban_expired_user_can_access() {
    let storage: Arc<dyn BanStorage> = Arc::new(TestBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let ban_manager = BanManager::with_dependencies(storage, config)
        .await
        .expect("Failed to create ban manager");

    let target = BanTarget::UserId("temp_banned_user".to_string());

    // 创建短期封禁 (1 秒)
    ban_manager
        .create_ban(
            target.clone(),
            "临时封禁".to_string(),
            BanSource::Auto,
            serde_json::json!({"auto_ban": true}),
            Some(Duration::from_secs(1)),
        )
        .await
        .expect("Failed to create ban");

    // 验证用户被封禁
    let is_banned = ban_manager
        .is_banned(&target)
        .await
        .expect("Failed to check ban status");
    assert!(is_banned.is_some(), "User should be banned initially");

    // 等待封禁过期
    tokio::time::sleep(Duration::from_millis(1100)).await;

    // 验证封禁已过期
    let is_banned = ban_manager
        .is_banned(&target)
        .await
        .expect("Failed to check ban status");
    assert!(
        is_banned.is_none(),
        "User should not be banned after expiration"
    );
}

/// 场景 3: IP 封禁测试
///
/// 封禁特定 IP 地址后，来自该 IP 的请求被拒绝。
#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn e2e_ban_ip_address() {
    let storage: Arc<dyn BanStorage> = Arc::new(TestBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let ban_manager = BanManager::with_dependencies(storage, config)
        .await
        .expect("Failed to create ban manager");

    let ip_target = BanTarget::Ip("192.168.100.50".to_string());

    // 封禁 IP
    ban_manager
        .create_ban(
            ip_target.clone(),
            "恶意攻击来源".to_string(),
            BanSource::Auto,
            serde_json::json!({"attack_type": "DDoS", "attempts": 1000}),
            Some(Duration::from_secs(86400)),
        )
        .await
        .expect("Failed to create IP ban");

    // 验证 IP 被封禁
    let is_banned = ban_manager
        .is_banned(&ip_target)
        .await
        .expect("Failed to check IP ban status");
    assert!(is_banned.is_some(), "IP should be banned");

    let ban_record = is_banned.unwrap();
    assert_eq!(ban_record.target, ip_target);
    assert_eq!(ban_record.reason, "恶意攻击来源");
}

/// 场景 4: 手动解封测试
///
/// 管理员手动解除封禁后，用户可以立即访问。
#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn e2e_ban_manual_unban() {
    let storage: Arc<dyn BanStorage> = Arc::new(TestBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let ban_manager = BanManager::with_dependencies(storage, config)
        .await
        .expect("Failed to create ban manager");

    let target = BanTarget::UserId("user_to_unban".to_string());

    // 封禁用户
    ban_manager
        .create_ban(
            target.clone(),
            "测试封禁".to_string(),
            BanSource::Manual {
                operator: "admin@example.com".to_string(),
            },
            serde_json::json!({}),
            Some(Duration::from_secs(3600)),
        )
        .await
        .expect("Failed to create ban");

    // 验证用户被封禁
    let is_banned = ban_manager
        .is_banned(&target)
        .await
        .expect("Failed to check ban status");
    assert!(is_banned.is_some(), "User should be banned");

    // 手动解封
    ban_manager
        .delete_ban(&target, "admin".to_string())
        .await
        .expect("Failed to delete ban");

    // 验证用户已解封
    let is_banned = ban_manager
        .is_banned(&target)
        .await
        .expect("Failed to check ban status");
    assert!(is_banned.is_none(), "User should be unbanned");
}

/// 场景 5: 封禁历史记录
///
/// 系统正确记录用户的封禁历史。
#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn e2e_ban_history_tracking() {
    let storage: Arc<dyn BanStorage> = Arc::new(TestBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let ban_manager = BanManager::with_dependencies(storage, config)
        .await
        .expect("Failed to create ban manager");

    let target = BanTarget::UserId("repeat_offender".to_string());

    // 第一次封禁
    ban_manager
        .create_ban(
            target.clone(),
            "第一次违规".to_string(),
            BanSource::Auto,
            serde_json::json!({"count": 1}),
            Some(Duration::from_secs(60)),
        )
        .await
        .expect("Failed to create first ban");

    // 获取历史
    let history = ban_manager
        .get_history(&target)
        .await
        .expect("Failed to get history");
    assert!(history.is_some(), "History should exist");
    assert_eq!(history.unwrap().ban_times, 1, "Ban times should be 1");

    // 等待封禁过期
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 第二次封禁（递增）
    let ban_times = ban_manager
        .increment_ban_times(&target)
        .await
        .expect("Failed to increment ban times");
    assert_eq!(ban_times, 2, "Ban times should be incremented to 2");
}

/// 场景 6: 封禁更新测试
///
/// 管理员可以更新封禁的原因和时长。
#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn e2e_ban_update() {
    let storage: Arc<dyn BanStorage> = Arc::new(TestBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let ban_manager = BanManager::with_dependencies(storage, config)
        .await
        .expect("Failed to create ban manager");

    let target = BanTarget::UserId("user_to_update".to_string());

    // 创建封禁
    ban_manager
        .create_ban(
            target.clone(),
            "初始原因".to_string(),
            BanSource::Auto,
            serde_json::json!({}),
            Some(Duration::from_secs(300)),
        )
        .await
        .expect("Failed to create ban");

    // 更新封禁原因
    let updated = ban_manager
        .update_ban(&target, Some("更新后的原因".to_string()), None, None)
        .await
        .expect("Failed to update ban");

    assert!(updated.is_some(), "Update should return the updated record");
    let record = updated.unwrap();
    assert_eq!(record.reason, "更新后的原因");
}

/// 场景 7: 批量封禁列表查询
///
/// 可以查询当前所有活跃的封禁记录。
#[tokio::test]
#[cfg(feature = "ban-manager")]
async fn e2e_ban_list_active_bans() {
    let storage: Arc<dyn BanStorage> = Arc::new(TestBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let ban_manager = BanManager::with_dependencies(storage, config)
        .await
        .expect("Failed to create ban manager");

    // 创建多个封禁
    for i in 0..5 {
        let target = BanTarget::UserId(format!("list_user_{}", i));
        ban_manager
            .create_ban(
                target,
                format!("封禁原因 {}", i),
                BanSource::Auto,
                serde_json::json!({"index": i}),
                Some(Duration::from_secs(3600)),
            )
            .await
            .expect("Failed to create ban");
    }

    // 查询活跃封禁列表
    let active_bans = ban_manager
        .list_bans(true, 0, 10)
        .await
        .expect("Failed to list bans");
    assert_eq!(active_bans.len(), 5, "Should have 5 active bans");

    // 分页查询
    let page1 = ban_manager
        .list_bans(true, 0, 2)
        .await
        .expect("Failed to list bans");
    assert_eq!(page1.len(), 2, "Page 1 should have 2 bans");

    let page2 = ban_manager
        .list_bans(true, 2, 2)
        .await
        .expect("Failed to list bans");
    assert_eq!(page2.len(), 2, "Page 2 should have 2 bans");
}
