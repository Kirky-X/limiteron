//! 延迟基准测试
//!
//! 测试各种操作的延迟性能

use criterion::{black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use limiteron::{
    l2_cache::L2Cache,
    limiters::{FixedWindowLimiter, Limiter, SlidingWindowLimiter, TokenBucketLimiter},
};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

/// 基准测试：TokenBucketLimiter延迟
fn bench_token_bucket_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(TokenBucketLimiter::new(1000, 100));

    c.bench_function("token_bucket_check", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });
}

/// 基准测试：SlidingWindowLimiter延迟
fn bench_sliding_window_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(1), 1000));

    c.bench_function("sliding_window_check", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });
}

/// 基准测试：FixedWindowLimiter延迟
fn bench_fixed_window_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(FixedWindowLimiter::new(Duration::from_secs(1), 1000));

    c.bench_function("fixed_window_check", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });
}

/// 基准测试：L2缓存命中延迟
fn bench_l2_cache_hit_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));

    // 预热缓存
    rt.block_on(async {
        cache
            .set("hot_key", "hot_value", Some(Duration::from_secs(60)))
            .await;
    });

    let cache = cache.clone();
    c.bench_function("l2_cache_hit", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(cache.get("hot_key").await);
            });
        });
    });
}

/// 基准测试：L2缓存未命中延迟
fn bench_l2_cache_miss_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));

    let cache = cache.clone();
    c.bench_function("l2_cache_miss", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(cache.get("cold_key").await);
            });
        });
    });
}

/// 基准测试：L2缓存写入延迟
fn bench_l2_cache_set_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = Arc::new(L2Cache::new(10000, Duration::from_secs(60)));

    let cache = cache.clone();
    c.bench_function("l2_cache_set", |b| {
        b.iter(|| {
            let key = format!("key_{}", black_box(42));
            rt.block_on(async {
                let _ = black_box(
                    cache
                        .set(&key, "value", Some(Duration::from_secs(60)))
                        .await,
                );
            });
        });
    });
}

/// 基准测试：不同窗口大小的延迟
fn bench_window_size_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let window_sizes = vec![
        ("100ms", Duration::from_millis(100)),
        ("1s", Duration::from_secs(1)),
        ("10s", Duration::from_secs(10)),
        ("1m", Duration::from_secs(60)),
    ];

    let mut group = c.benchmark_group("window_size_latency");

    for (name, window_size) in window_sizes {
        let limiter = Arc::new(SlidingWindowLimiter::new(window_size, 1000));
        group.bench_with_input(BenchmarkId::from_parameter(name), &limiter, |b, limiter| {
            let limiter = limiter.clone();
            b.iter(|| {
                rt.block_on(async {
                    let _ = black_box(limiter.allow(1).await);
                });
            });
        });
    }

    group.finish();
}

/// 基准测试：并发检查延迟
fn bench_concurrent_check_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(TokenBucketLimiter::new(10000, 1000));

    let mut group = c.benchmark_group("concurrent_latency");

    for concurrency in [1, 10, 100, 1000].iter() {
        let limiter = limiter.clone();
        group.bench_with_input(
            BenchmarkId::from_parameter(concurrency),
            concurrency,
            |b, &concurrency| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut handles = vec![];
                        for _ in 0..concurrency {
                            let limiter = limiter.clone();
                            handles.push(async move {
                                let _ = limiter.allow(1).await;
                            });
                        }
                        for handle in handles {
                            let _ = handle.await;
                        }
                    });
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_token_bucket_latency,
    bench_sliding_window_latency,
    bench_fixed_window_latency,
    bench_l2_cache_hit_latency,
    bench_l2_cache_miss_latency,
    bench_l2_cache_set_latency,
    bench_window_size_latency,
    bench_concurrent_check_latency
);

criterion_main!(benches);
