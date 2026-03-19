//! Ban Manager Example
//!
//! Demonstrates ban management functionality:
//! - Creating bans for different targets (IP, User ID, MAC)
//! - Checking ban status
//! - Updating and removing bans
//!
//! Run: cargo run --example ban_manager --features ban-manager

#[cfg(feature = "ban-manager")]
use ahash::AHashMap as HashMap;
#[cfg(feature = "ban-manager")]
use chrono::Utc;
#[cfg(feature = "ban-manager")]
use limiteron::ban_manager::{BanManager, BanManagerConfig, BanSource};
#[cfg(feature = "ban-manager")]
use limiteron::error::{FlowGuardError, StorageError};
#[cfg(feature = "ban-manager")]
use limiteron::storage_trait::{BanHistory, BanRecord, BanStorage, BanTarget};
#[cfg(feature = "ban-manager")]
use std::sync::Arc;
#[cfg(feature = "ban-manager")]
use std::time::Duration;
#[cfg(feature = "ban-manager")]
use tokio::sync::RwLock;

#[cfg(feature = "ban-manager")]
struct MemoryBanStorage {
    bans: RwLock<HashMap<BanTarget, BanRecord>>,
    history: RwLock<HashMap<BanTarget, BanHistory>>,
}

#[cfg(feature = "ban-manager")]
impl MemoryBanStorage {
    fn new() -> Self {
        Self {
            bans: RwLock::new(HashMap::new()),
            history: RwLock::new(HashMap::new()),
        }
    }
}

#[cfg(feature = "ban-manager")]
#[async_trait::async_trait]
impl BanStorage for MemoryBanStorage {
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

#[cfg(feature = "ban-manager")]
#[tokio::main]
async fn main() -> Result<(), FlowGuardError> {
    println!("=== Limiteron Ban Manager Demo ===\n");

    demo_ip_ban().await?;
    demo_user_ban().await?;
    demo_ban_update().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

#[cfg(feature = "ban-manager")]
async fn demo_ip_ban() -> Result<(), FlowGuardError> {
    println!("--- IP Ban Demo ---\n");

    let storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let ban_manager = BanManager::with_dependencies(storage, config).await?;

    let target = BanTarget::Ip("192.168.1.100".to_string());
    println!("Creating ban for IP: {}", "192.168.1.100");

    let detail = ban_manager
        .create_ban(
            target.clone(),
            "suspicious activity".to_string(),
            BanSource::Auto,
            serde_json::json!({"source": "rate_limiter", "attempts": 100}),
            Some(Duration::from_secs(3600)),
        )
        .await?;

    println!("  Ban created: {:?}", detail.target);
    println!("  Reason: {}", detail.reason);
    println!("  Source: {:?}", detail.source);
    println!("  Duration: {:?}\n", detail.duration);

    let is_banned = ban_manager.is_banned(&target).await?;
    println!("  Is banned: {}\n", is_banned.is_some());

    Ok(())
}

#[cfg(feature = "ban-manager")]
async fn demo_user_ban() -> Result<(), FlowGuardError> {
    println!("--- User Ban Demo ---\n");

    let storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let ban_manager = BanManager::with_dependencies(storage, config).await?;

    let target = BanTarget::UserId("user-12345".to_string());
    println!("Creating manual ban for user: {}", "user-12345");

    let detail = ban_manager
        .create_ban(
            target.clone(),
            "terms of service violation".to_string(),
            BanSource::Manual {
                operator: "admin@example.com".to_string(),
            },
            serde_json::json!({"case_id": "CASE-2024-001", "severity": "high"}),
            Some(Duration::from_secs(86400)),
        )
        .await?;

    println!("  Ban created for user: {:?}", detail.target);
    println!("  Operator: {:?}", detail.source);
    println!("  Metadata: {:?}\n", detail.metadata);

    let history = ban_manager.get_history(&target).await?;
    println!("  Ban history: {:?}\n", history);

    Ok(())
}

#[cfg(feature = "ban-manager")]
async fn demo_ban_update() -> Result<(), FlowGuardError> {
    println!("--- Ban Update Demo ---\n");

    let storage: Arc<dyn BanStorage> = Arc::new(MemoryBanStorage::new());
    let config = BanManagerConfig {
        enable_auto_unban: false,
        ..BanManagerConfig::default()
    };
    let ban_manager = BanManager::with_dependencies(storage, config).await?;

    let target = BanTarget::Ip("10.0.0.50".to_string());
    println!("Creating initial ban...");

    ban_manager
        .create_ban(
            target.clone(),
            "rate limit exceeded".to_string(),
            BanSource::Auto,
            serde_json::json!({"requests": 1000}),
            Some(Duration::from_secs(300)),
        )
        .await?;

    let initial = ban_manager.is_banned(&target).await?.unwrap();
    println!("  Initial reason: {}", initial.reason);

    println!("\nUpdating ban reason...");
    let updated = ban_manager
        .update_ban(&target, Some("repeated rate limit violations".to_string()), None, None)
        .await?;

    if let Some(record) = updated {
        println!("  Updated reason: {}", record.reason);
    }

    println!("\nRemoving ban...");
    ban_manager.delete_ban(&target, "admin".to_string()).await?;
    let is_banned = ban_manager.is_banned(&target).await?;
    println!("  Is banned after removal: {}\n", is_banned.is_some());

    Ok(())
}

#[cfg(not(feature = "ban-manager"))]
fn main() {
    eprintln!("This example requires the 'ban-manager' feature.");
    eprintln!("Run with: cargo run --example ban_manager --features ban-manager");
}
