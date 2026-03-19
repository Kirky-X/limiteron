//! Rate Limiters Example
//!
//! Demonstrates various rate limiting algorithms:
//! - Token Bucket
//! - Sliding Window
//! - Fixed Window
//! - Concurrency Limiter
//!
//! Run: cargo run --bin rate_limiters

use limiteron::error::FlowGuardError;
use limiteron::limiters::{
    ConcurrencyLimiter, FixedWindowLimiter, Limiter, SlidingWindowLimiter, TokenBucketLimiter,
};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), FlowGuardError> {
    println!("=== Limiteron Rate Limiters Demo ===\n");

    demo_token_bucket().await?;
    demo_sliding_window().await?;
    demo_fixed_window().await?;
    demo_concurrency().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

async fn demo_token_bucket() -> Result<(), FlowGuardError> {
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

async fn demo_sliding_window() -> Result<(), FlowGuardError> {
    println!("--- Sliding Window Limiter ---");
    println!("Window: 200ms, Max requests: 2\n");

    let limiter = SlidingWindowLimiter::new(Duration::from_millis(200), 2);

    let first = limiter.allow(1).await?;
    let second = limiter.allow(1).await?;
    let third = limiter.allow(1).await?;

    println!("  Request 1: allowed={}", first);
    println!("  Request 2: allowed={}", second);
    println!("  Request 3: allowed={} (blocked - window limit reached)\n", third);

    println!("  Waiting 220ms for window to slide...");
    tokio::time::sleep(Duration::from_millis(220)).await;

    let after_window = limiter.allow(1).await?;
    println!("  After window: allowed={}\n", after_window);

    Ok(())
}

async fn demo_fixed_window() -> Result<(), FlowGuardError> {
    println!("--- Fixed Window Limiter ---");
    println!("Window: 200ms, Max requests: 2\n");

    let limiter = FixedWindowLimiter::new(Duration::from_millis(200), 2);

    let first = limiter.allow(1).await?;
    let second = limiter.allow(1).await?;
    let third = limiter.allow(1).await?;

    println!("  Request 1: allowed={}", first);
    println!("  Request 2: allowed={}", second);
    println!("  Request 3: allowed={} (blocked - window limit reached)\n", third);

    println!("  Waiting 220ms for new window...");
    tokio::time::sleep(Duration::from_millis(220)).await;

    let after_window = limiter.allow(1).await?;
    println!("  After window: allowed={}\n", after_window);

    Ok(())
}

async fn demo_concurrency() -> Result<(), FlowGuardError> {
    println!("--- Concurrency Limiter ---");
    println!("Max concurrent: 2, Timeout: 50ms\n");

    let limiter = ConcurrencyLimiter::with_timeout(2, Duration::from_millis(50));

    let permit_one = limiter.acquire(1).await?;
    println!("  Acquired permit 1");

    let permit_two = limiter.acquire(1).await?;
    println!("  Acquired permit 2");

    let third_result = limiter.acquire(1).await;
    println!("  Third acquire: {:?} (blocked - max concurrent reached)", third_result);

    drop(permit_one);
    drop(permit_two);
    println!("  Released both permits\n");

    Ok(())
}
