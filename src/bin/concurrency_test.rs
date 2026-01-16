//! 并发数据一致性测试
//!
//! 验证高并发场景下数据的一致性

use limiteron::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 并发数据一致性测试 ===");

    let config = create_test_config();
    let storage = Arc::new(limiteron::storage::MemoryStorage::new());
    let ban_storage = Arc::new(limiteron::storage::MemoryStorage::new());

    #[cfg(feature = "monitoring")]
    let metrics = Arc::new(limiteron::telemetry::Metrics::new());
    #[cfg(feature = "telemetry")]
    let tracer = Arc::new(limiteron::telemetry::Tracer::new(false));

    let governor = Arc::new(
        Governor::new(
            config,
            storage,
            ban_storage,
            #[cfg(feature = "monitoring")]
            Some(metrics),
            #[cfg(feature = "telemetry")]
            Some(tracer),
        )
        .await?,
    );

    test_concurrent_same_user(&governor).await?;
    test_concurrent_different_users(&governor).await?;

    Ok(())
}

fn create_test_config() -> limiteron::config::FlowControlConfig {
    use limiteron::config::{ActionConfig, GlobalConfig, LimiterConfig, Matcher, Rule};

    limiteron::config::FlowControlConfig {
        version: "1.0".to_string(),
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
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 100,
                refill_rate: 10,
            }],
            action: ActionConfig {
                on_exceed: "allow".to_string(),
                ban: None,
            },
        }],
    }
}

async fn test_concurrent_same_user(
    governor: &Arc<Governor>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- 同一用户并发测试 ---");

    let user_id = "concurrent_user";
    let request_count = 50;
    let success_count = Arc::new(AtomicU64::new(0));
    let reject_count = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();

    for _ in 0..request_count {
        let governor = governor.clone();
        let user_id = user_id.to_string();
        let success_count = success_count.clone();
        let reject_count = reject_count.clone();

        let handle = tokio::spawn(async move {
            match governor.check_resource_parallel(&user_id).await {
                Ok(Decision::Allowed(_)) => {
                    success_count.fetch_add(1, Ordering::Relaxed);
                }
                Ok(Decision::Banned(_)) | Ok(Decision::Rejected(_)) => {
                    reject_count.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    reject_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await?;
    }

    let success = success_count.load(Ordering::Relaxed);
    let rejects = reject_count.load(Ordering::Relaxed);

    println!("总请求数: {}", request_count);
    println!("成功请求: {}", success);
    println!("拒绝请求: {}", rejects);
    println!(
        "成功率: {:.2}%",
        (success as f64 / request_count as f64) * 100.0
    );

    Ok(())
}

async fn test_concurrent_different_users(
    governor: &Arc<Governor>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- 不同用户并发测试 ---");

    let user_count = 20;
    let requests_per_user = 5;
    let total_requests = user_count * requests_per_user;
    let result_count = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();

    for user_index in 0..user_count {
        let governor = governor.clone();
        let user_id = format!("user_{}", user_index);
        let result_count = result_count.clone();

        let handle = tokio::spawn(async move {
            for _ in 0..requests_per_user {
                match governor.check_resource_parallel(&user_id).await {
                    Ok(_) => {
                        result_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        result_count.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await?;
    }

    let completed = result_count.load(Ordering::Relaxed);

    println!("总请求数: {}", total_requests);
    println!("完成请求数: {}", completed);
    println!(
        "完成率: {:.2}%",
        (completed as f64 / total_requests as f64) * 100.0
    );

    Ok(())
}
