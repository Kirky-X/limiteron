// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Rate Limiters Example
//!
//! Demonstrates various rate limiting algorithms:
//! - Token Bucket
//! - Sliding Window
//! - Fixed Window
//! - Concurrency Limiter
//! - GCRA (Generic Cell Rate Algorithm)
//!
//! Run: cargo run --bin rate_limiters

use limiteron::error::LimiteronError;
use limiteron::limiters::{
    ConcurrencyLimiter, FixedWindowLimiter, GcraLimiter, Limiter, ShardedSlidingWindowLimiter,
    TokenBucketLimiter,
};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), LimiteronError> {
    println!("=== Limiteron Rate Limiters Demo ===\n");

    demo_token_bucket().await?;
    demo_sliding_window().await?;
    demo_fixed_window().await?;
    demo_concurrency().await?;
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
