//! 性能测试
//!
//! 测试并行执行的优化效果

use limiteron::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 并行执行性能测试 ===");

    let config = create_test_config();
    let storage = Arc::new(limiteron::storage::MemoryStorage::new());
    let ban_storage = Arc::new(limiteron::storage::MemoryStorage::new());
    let metrics = Arc::new(limiteron::telemetry::Metrics::new());
    let tracer = Arc::new(limiteron::telemetry::Tracer::new(false));

    let governor =
        Arc::new(Governor::new(config, storage, ban_storage, Some(metrics), Some(tracer)).await?);

    test_parallel_performance(&governor).await?;

    Ok(())
}

fn create_test_config() -> FlowControlConfig {
    use limiteron::config::{ActionConfig, GlobalConfig, LimiterConfig, Matcher, Rule};

    FlowControlConfig {
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
                capacity: 1000,
                refill_rate: 100,
            }],
            action: ActionConfig {
                on_exceed: "allow".to_string(),
                ban: None,
            },
        }],
    }
}

async fn test_parallel_performance(
    governor: &Arc<Governor>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- 并行检查性能测试 ---");

    let iterations = 1000;

    let start_serial = Instant::now();
    for _ in 0..iterations {
        let _ = governor
            .check_resource_parallel(&format!("user_{}", 1))
            .await?;
    }
    let serial_duration = start_serial.elapsed();

    let start_concurrent = Instant::now();
    let mut handles = Vec::new();

    for i in 0..iterations {
        let governor = governor.clone();
        let user_id = format!("user_{}", i % 10 + 1);
        let handle = tokio::spawn(async move { governor.check_resource_parallel(&user_id).await });
        handles.push(handle);
    }

    for handle in handles {
        handle.await??;
    }
    let concurrent_duration = start_concurrent.elapsed();

    let serial_ops_per_sec = iterations as f64 / serial_duration.as_secs_f64();
    let concurrent_ops_per_sec = iterations as f64 / concurrent_duration.as_secs_f64();
    let speedup = concurrent_ops_per_sec / serial_ops_per_sec;

    println!(
        "串行执行: {:.2} ops/sec ({} 次请求耗时 {:?})",
        serial_ops_per_sec, iterations, serial_duration
    );
    println!(
        "并发执行: {:.2} ops/sec ({} 次请求耗时 {:?})",
        concurrent_ops_per_sec, iterations, concurrent_duration
    );
    println!("性能提升: {:.2}x", speedup);

    println!("\n--- 延迟分布测试 ---");
    let test_requests = 100;
    let mut latencies = Vec::new();

    for i in 0..test_requests {
        let start = Instant::now();
        let _ = governor
            .check_resource_parallel(&format!("user_{}", i))
            .await?;
        latencies.push(start.elapsed());
    }

    latencies.sort();

    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() as f64 * 0.95) as usize];
    let p99 = latencies[(latencies.len() as f64 * 0.99) as usize];

    println!("延迟分布 ({} 次请求):", test_requests);
    println!("  P50: {:?}", p50);
    println!("  P95: {:?}", p95);
    println!("  P99: {:?}", p99);
    println!(
        "  平均: {:?}",
        latencies.iter().sum::<Duration>() / latencies.len() as u32
    );

    Ok(())
}
