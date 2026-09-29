// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Rate Limiters Example
//!
//! Demonstrates various rate limiting algorithms:
//! - Token Bucket
//! - Sliding Window
//! - Fixed Window
//! - Concurrency Limiter
//! - Leaky Bucket (water-level meter, dual of token bucket)
//! - Sliding Window Log (exact sliding window)
//! - GCRA (Generic Cell Rate Algorithm)
//!
//! Run: cargo run --bin rate_limiters

use limiteron::error::LimiteronError;
#[cfg(feature = "gcra")]
use limiteron::limiters::GcraLimiter;
use limiteron::limiters::{
    ConcurrencyLimiter, FixedWindowLimiter, LeakyBucketLimiter, Limiter,
    ShardedSlidingWindowLimiter, SlidingWindowLogLimiter, TokenBucketLimiter,
};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), LimiteronError> {
    println!("=== Limiteron Rate Limiters Demo ===\n");

    demo_token_bucket().await?;
    demo_sliding_window().await?;
    demo_fixed_window().await?;
    demo_concurrency().await?;
    demo_leaky_bucket().await?;
    demo_sliding_window_log().await?;
    #[cfg(feature = "gcra")]
    demo_gcra().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

async fn demo_token_bucket() -> Result<(), LimiteronError> {
    println!("--- Token Bucket Limiter ---");
    println!("Capacity: 3 tokens, Refill rate: 1 token/sec\n");

    let limiter = TokenBucketLimiter::new(3, 1);

    let results: Vec<_> = futures::future::join_all(vec![
        limiter.allow(1),
        limiter.allow(1),
        limiter.allow(1),
        limiter.allow(1),
    ])
    .await
    .into_iter()
    .map(|r| r.unwrap())
    .collect();

    println!(
        "  Requests 1-4: [{}, {}, {}, {}]",
        results[0], results[1], results[2], results[3]
    );
    println!("  (First 3 succeed, 4th fails - bucket empty)\n");

    println!("  Waiting 1.1 seconds for refill...");
    tokio::time::sleep(Duration::from_millis(1100)).await;

    let after_refill = limiter.allow(1).await?;
    println!("  After refill: allowed={}\n", after_refill);

    Ok(())
}

async fn demo_sliding_window() -> Result<(), LimiteronError> {
    println!("--- Sharded Sliding Window Limiter ---");
    println!("Window: 200ms, Max requests: 2\n");

    let limiter = ShardedSlidingWindowLimiter::new(Duration::from_millis(200), 2);

    let first = limiter.allow(1).await?;
    let second = limiter.allow(1).await?;
    let third = limiter.allow(1).await?;

    println!("  Request 1: allowed={}", first);
    println!("  Request 2: allowed={}", second);
    println!(
        "  Request 3: allowed={} (blocked - window limit reached)\n",
        third
    );

    println!("  Waiting 220ms for window to slide...");
    tokio::time::sleep(Duration::from_millis(220)).await;

    let after_window = limiter.allow(1).await?;
    println!("  After window: allowed={}\n", after_window);

    Ok(())
}

async fn demo_fixed_window() -> Result<(), LimiteronError> {
    println!("--- Fixed Window Limiter ---");
    println!("Window: 200ms, Max requests: 2\n");

    let limiter = FixedWindowLimiter::new(Duration::from_millis(200), 2);

    let first = limiter.allow(1).await?;
    let second = limiter.allow(1).await?;
    let third = limiter.allow(1).await?;

    println!("  Request 1: allowed={}", first);
    println!("  Request 2: allowed={}", second);
    println!(
        "  Request 3: allowed={} (blocked - window limit reached)\n",
        third
    );

    println!("  Waiting 220ms for new window...");
    tokio::time::sleep(Duration::from_millis(220)).await;

    let after_window = limiter.allow(1).await?;
    println!("  After window: allowed={}\n", after_window);

    Ok(())
}

async fn demo_concurrency() -> Result<(), LimiteronError> {
    println!("--- Concurrency Limiter ---");
    println!("Max concurrent: 2, Timeout: 50ms\n");

    let limiter = ConcurrencyLimiter::with_timeout(2, Duration::from_millis(50));

    let permit_one = limiter.acquire(1).await?;
    println!("  Acquired permit 1");

    let permit_two = limiter.acquire(1).await?;
    println!("  Acquired permit 2");

    let third_result = limiter.acquire(1).await;
    println!(
        "  Third acquire: {:?} (blocked - max concurrent reached)",
        third_result
    );

    drop(permit_one);
    drop(permit_two);
    println!("  Released both permits\n");

    Ok(())
}

async fn demo_leaky_bucket() -> Result<(), LimiteronError> {
    println!("--- Leaky Bucket Limiter ---");
    println!("Capacity: 3, Leak rate: 10 units/sec (water-level meter)\n");

    let limiter = LeakyBucketLimiter::new(3, 10)?;

    // 空桶注入 3 单位即满：第 4 笔被拒（判定与令牌桶对偶：水位 = 容量 − 令牌）
    let results: Vec<_> = futures::future::join_all(vec![
        limiter.allow(1),
        limiter.allow(1),
        limiter.allow(1),
        limiter.allow(1),
    ])
    .await
    .into_iter()
    .map(|r| r.unwrap())
    .collect();

    println!(
        "  Requests 1-4: [{}, {}, {}, {}]",
        results[0], results[1], results[2], results[3]
    );
    println!("  (First 3 fill the bucket, 4th rejected - bucket full)\n");

    println!("  Waiting 300ms for drain (10/s → ~3 units leaked)...");
    tokio::time::sleep(Duration::from_millis(300)).await;

    println!("  Water level after drain: {}", limiter.water_level().await);
    let after_drain = limiter.allow(1).await?;
    println!("  After drain: allowed={}\n", after_drain);

    Ok(())
}

async fn demo_sliding_window_log() -> Result<(), LimiteronError> {
    println!("--- Sliding Window Log Limiter ---");
    println!("Window: 200ms, Max requests: 2 (exact sliding window)\n");

    let limiter = SlidingWindowLogLimiter::new(2, Duration::from_millis(200))?;

    let first = limiter.allow(1).await?;
    let second = limiter.allow(1).await?;
    let third = limiter.allow(1).await?;

    println!("  Request 1: allowed={}", first);
    println!("  Request 2: allowed={}", second);
    println!("  Request 3: allowed={} (window full)", third);
    println!(
        "  Window count: {}, logged entries: {}",
        limiter.window_count().await,
        limiter.logged_entries().await
    );

    println!("  Waiting 220ms for full window slide...");
    tokio::time::sleep(Duration::from_millis(220)).await;

    let after_window = limiter.allow(2).await?;
    println!("  After slide (cost=2): allowed={}", after_window);
    println!("  (Exact counting: no boundary burst like fixed windows)\n");

    Ok(())
}

#[cfg(feature = "gcra")]
async fn demo_gcra() -> Result<(), LimiteronError> {
    println!("--- GCRA (Generic Cell Rate Algorithm) Limiter ---");
    println!("Capacity: 3 burst, Rate: 10 req/s (100ms interval)\n");

    // GcraLimiter::with_rate(capacity, requests_per_second)
    // capacity=3 burst, 10 req/s sustained → 100ms between tokens
    let limiter = GcraLimiter::with_rate(3, 10);

    // GCRA's check() returns a rich result (sync, not async)
    let results: Vec<_> = (0..5).map(|_| limiter.check(1)).collect();

    for (i, r) in results.iter().enumerate() {
        println!(
            "  Request {}: allowed={}, remaining={}, retry_after_us={}",
            i + 1,
            r.allowed,
            r.remaining,
            r.retry_after_us
        );
    }
    println!("  (First 3 succeed (burst), 4th-5th denied (rate-limited))\n");

    println!("  Waiting 150ms for next token...");
    tokio::time::sleep(Duration::from_millis(150)).await;

    let after_wait = limiter.check(1);
    println!(
        "  After wait: allowed={}, remaining={}, retry_after_us={}\n",
        after_wait.allowed, after_wait.remaining, after_wait.retry_after_us
    );

    Ok(())
}
