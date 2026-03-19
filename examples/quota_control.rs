//! Quota Control Example
//!
//! Demonstrates quota management functionality:
//! - Quota consumption tracking
//! - Limit enforcement
//! - Usage percentage calculation
//!
//! Run: cargo run --example quota_control --features quota-control

#[cfg(feature = "quota-control")]
use ahash::AHashMap as HashMap;
#[cfg(feature = "quota-control")]
use chrono::{DateTime, Duration as ChronoDuration, Utc};
#[cfg(feature = "quota-control")]
use limiteron::error::{ConsumeResult, FlowGuardError, StorageError};
#[cfg(feature = "quota-control")]
use limiteron::quota_controller::{
    AlertChannel, AlertConfig, QuotaConfig, QuotaController, QuotaType,
};
#[cfg(feature = "quota-control")]
use limiteron::storage_trait::{QuotaInfo, QuotaStorage};
#[cfg(feature = "quota-control")]
use std::sync::Arc;
#[cfg(feature = "quota-control")]
use std::time::Duration;
#[cfg(feature = "quota-control")]
use tokio::sync::RwLock;

#[cfg(feature = "quota-control")]
struct MemoryQuotaStorage {
    quotas: RwLock<HashMap<String, QuotaInfo>>,
}

#[cfg(feature = "quota-control")]
impl MemoryQuotaStorage {
    fn new() -> Self {
        Self {
            quotas: RwLock::new(HashMap::new()),
        }
    }

    fn now_window_end(now: DateTime<Utc>, window: Duration) -> DateTime<Utc> {
        now + ChronoDuration::seconds(window.as_secs() as i64)
    }
}

#[cfg(feature = "quota-control")]
#[async_trait::async_trait]
impl QuotaStorage for MemoryQuotaStorage {
    async fn get_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError> {
        let key = format!("{}:{}", user_id, resource);
        Ok(self.quotas.read().await.get(&key).cloned())
    }

    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
        limit: u64,
        window: Duration,
    ) -> Result<ConsumeResult, StorageError> {
        let key = format!("{}:{}", user_id, resource);
        let now = Utc::now();
        let mut quotas = self.quotas.write().await;
        let entry = quotas.entry(key).or_insert_with(|| QuotaInfo {
            consumed: 0,
            limit,
            window_start: now,
            window_end: Self::now_window_end(now, window),
        });

        if now > entry.window_end {
            entry.consumed = 0;
            entry.limit = limit;
            entry.window_start = now;
            entry.window_end = Self::now_window_end(now, window);
        }

        let next_consumed = entry.consumed.saturating_add(cost);
        let allowed = next_consumed <= limit;
        if allowed {
            entry.consumed = next_consumed;
        }

        let remaining = limit.saturating_sub(entry.consumed);
        let usage_percent = if limit == 0 {
            0.0
        } else {
            (entry.consumed as f64 / limit as f64) * 100.0
        };

        Ok(ConsumeResult {
            allowed,
            remaining,
            alert_triggered: false,
            usage_percent,
        })
    }

    async fn reset(
        &self,
        user_id: &str,
        resource: &str,
        limit: u64,
        window: Duration,
    ) -> Result<(), StorageError> {
        let key = format!("{}:{}", user_id, resource);
        let now = Utc::now();
        let mut quotas = self.quotas.write().await;
        quotas.insert(
            key,
            QuotaInfo {
                consumed: 0,
                limit,
                window_start: now,
                window_end: Self::now_window_end(now, window),
            },
        );
        Ok(())
    }
}

#[cfg(feature = "quota-control")]
#[tokio::main]
async fn main() -> Result<(), FlowGuardError> {
    println!("=== Limiteron Quota Control Demo ===\n");

    demo_basic_quota().await?;
    demo_limit_enforcement().await?;
    demo_usage_tracking().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

#[cfg(feature = "quota-control")]
async fn demo_basic_quota() -> Result<(), FlowGuardError> {
    println!("--- Basic Quota Operations ---");
    println!("Config: limit=10, window=60s\n");

    let storage: Arc<dyn QuotaStorage> = Arc::new(MemoryQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 10,
        window_size: 60,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            thresholds: vec![80, 100],
            channels: vec![AlertChannel::Log],
            dedup_window: 60,
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);

    let result = controller.consume("user-1", "api", 3).await?;
    println!(
        "Consume 3: allowed={}, remaining={}",
        result.allowed, result.remaining
    );

    let result = controller.consume("user-1", "api", 2).await?;
    println!(
        "Consume 2: allowed={}, remaining={}",
        result.allowed, result.remaining
    );

    let result = controller.consume("user-1", "api", 5).await?;
    println!(
        "Consume 5: allowed={}, remaining={}\n",
        result.allowed, result.remaining
    );

    Ok(())
}

#[cfg(feature = "quota-control")]
async fn demo_limit_enforcement() -> Result<(), FlowGuardError> {
    println!("--- Limit Enforcement Demo ---\n");

    let storage: Arc<dyn QuotaStorage> = Arc::new(MemoryQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 5,
        window_size: 60,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: false,
            thresholds: vec![80, 100],
            channels: vec![],
            dedup_window: 60,
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);

    println!("Consuming quota in increments:");
    for i in 1..=6 {
        let result = controller.consume("user-2", "api", 1).await?;
        println!(
            "  Request {}: allowed={}, remaining={}, usage={:.1}%",
            i, result.allowed, result.remaining, result.usage_percent
        );
    }

    println!("\nQuota exhausted - further requests will be denied\n");

    Ok(())
}

#[cfg(feature = "quota-control")]
async fn demo_usage_tracking() -> Result<(), FlowGuardError> {
    println!("--- Usage Tracking Demo ---\n");

    let storage: Arc<dyn QuotaStorage> = Arc::new(MemoryQuotaStorage::new());
    let config = QuotaConfig {
        quota_type: QuotaType::Count,
        limit: 100,
        window_size: 60,
        allow_overdraft: false,
        overdraft_limit_percent: 0,
        alert_config: AlertConfig {
            enabled: true,
            thresholds: vec![50, 80, 100],
            channels: vec![AlertChannel::Log],
            dedup_window: 60,
        },
    };
    let controller = QuotaController::with_dependencies(storage, config);

    println!("Consuming 25 units (25% usage):");
    let result = controller.consume("user-3", "api", 25).await?;
    println!(
        "  allowed={}, usage={:.1}%\n",
        result.allowed, result.usage_percent
    );

    println!("Consuming 30 more units (55% total):");
    let result = controller.consume("user-3", "api", 30).await?;
    println!(
        "  allowed={}, usage={:.1}%\n",
        result.allowed, result.usage_percent
    );

    println!("Consuming 25 more units (80% total - alert threshold):");
    let result = controller.consume("user-3", "api", 25).await?;
    println!(
        "  allowed={}, usage={:.1}%, alert_triggered={}\n",
        result.allowed, result.usage_percent, result.alert_triggered
    );

    Ok(())
}

#[cfg(not(feature = "quota-control"))]
fn main() {
    eprintln!("This example requires the 'quota-control' feature.");
    eprintln!("Run with: cargo run --example quota_control --features quota-control");
}
