//! 吞吐量基准测试
//!
//! 测试系统的吞吐量性能

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};
use limiteron::{
    config::{FlowControlConfig, LimiterConfig, Rule},
    governor::Governor,
    limiters::{Limiter, SlidingWindowLimiter, TokenBucketLimiter},
    matchers::RequestContext,
    storage::MemoryStorage,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

/// 基准测试：TokenBucketLimiter吞吐量
fn bench_token_bucket_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(TokenBucketLimiter::new(100000, 10000));

    let mut group = c.benchmark_group("token_bucket_throughput");

    for size in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        let limiter = limiter.clone();
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || (),
                |_| {
                    rt.block_on(async {
                        for _ in 0..size {
                            let _ = black_box(limiter.allow(1).await);
                        }
                    });
                },
                BatchSize::PerIteration,
            );
        });
    }

    group.finish();
}

/// 基准测试：SlidingWindowLimiter吞吐量
fn bench_sliding_window_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(1), 100000));

    let mut group = c.benchmark_group("sliding_window_throughput");

    for size in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        let limiter = limiter.clone();
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || (),
                |_| {
                    rt.block_on(async {
                        for _ in 0..size {
                            let _ = black_box(limiter.allow(1).await);
                        }
                    });
                },
                BatchSize::PerIteration,
            );
        });
    }

    group.finish();
}

/// 基准测试：Governor吞吐量
fn bench_governor_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: Default::default(),
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 100000,
                refill_rate: 10000,
            }],
            action: Default::default(),
        }],
    };

    let storage = Arc::new(MemoryStorage::new());
    let ban_storage = Arc::new(MemoryStorage::new());

    let gov = Arc::new(rt.block_on(async {
        Governor::new(config, storage, ban_storage, None, None)
            .await
            .unwrap()
    }));

    let ctx = RequestContext {
        user_id: Some("test_user".to_string()),
        ip: Some("192.168.1.1".to_string()),
        mac: None,
        device_id: None,
        api_key: None,
        headers: ahash::AHashMap::new(),
        path: "/test".to_string(),
        method: "GET".to_string(),
        client_ip: Some("192.168.1.1".to_string()),
        query_params: ahash::AHashMap::new(),
    };

    let mut group = c.benchmark_group("governor_throughput");

    for size in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        let gov = gov.clone();
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || (),
                |_| {
                    rt.block_on(async {
                        for _ in 0..size {
                            let _ = black_box(gov.check(&ctx).await);
                        }
                    });
                },
                BatchSize::PerIteration,
            );
        });
    }

    group.finish();
}

/// 基准测试：并发吞吐量
fn bench_concurrent_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(TokenBucketLimiter::new(100000, 10000));

    let mut group = c.benchmark_group("concurrent_throughput");

    for concurrency in [1, 10, 100].iter() {
        let size = 1000;
        group.throughput(Throughput::Elements((size * concurrency) as u64));
        let limiter = limiter.clone();
        group.bench_with_input(
            BenchmarkId::from_parameter(concurrency),
            concurrency,
            |b, &concurrency| {
                b.iter_batched(
                    || (),
                    |_| {
                        rt.block_on(async {
                            let mut handles = vec![];
                            for _ in 0..concurrency {
                                let limiter = limiter.clone();
                                handles.push(async move {
                                    for _ in 0..size {
                                        let _ = black_box(limiter.allow(1).await);
                                    }
                                });
                            }
                            for handle in handles {
                                let _ = handle.await;
                            }
                        });
                    },
                    BatchSize::PerIteration,
                );
            },
        );
    }

    group.finish();
}

/// 基准测试：混合操作吞吐量
fn bench_mixed_operations_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(TokenBucketLimiter::new(100000, 10000));

    let mut group = c.benchmark_group("mixed_operations_throughput");

    for (name, ratio) in [
        ("10%_cost_10", 10),
        ("50%_cost_10", 50),
        ("90%_cost_10", 90),
    ] {
        let limiter = limiter.clone();
        group.bench_with_input(BenchmarkId::from_parameter(name), &ratio, |b, ratio| {
            b.iter_batched(
                || (),
                |_| {
                    rt.block_on(async {
                        for i in 0..1000 {
                            let cost = if i % 100 < *ratio { 10 } else { 1 };
                            let _ = black_box(limiter.allow(cost).await);
                        }
                    });
                },
                BatchSize::PerIteration,
            );
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_token_bucket_throughput,
    bench_sliding_window_throughput,
    bench_governor_throughput,
    bench_concurrent_throughput,
    bench_mixed_operations_throughput
);

criterion_main!(benches);
