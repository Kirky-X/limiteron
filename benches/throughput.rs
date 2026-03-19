//! 吞吐量基准测试
//!
//! 测试系统的吞吐量性能

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};
use limiteron::{
    config::{FlowControlConfig, LimiterConfig, Rule},
    governor::Governor,
    limiters::{Limiter, ShardedSlidingWindowLimiter, SlidingWindowLimiter, TokenBucketLimiter},
    matchers::RequestContext,
};

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

/// 基准测试：ShardedSlidingWindowLimiter吞吐量
fn bench_sharded_sliding_window_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        100000,
    ));

    let mut group = c.benchmark_group("sharded_sliding_window_throughput");

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

/// 基准测试：滑动窗口限流器对比（传统 vs 分片）
fn bench_sliding_window_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("sliding_window_comparison");

    // 传统滑动窗口
    let traditional = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(60), 100000));
    group.bench_function("traditional_sliding_window", |b| {
        let limiter = traditional.clone();
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..1000 {
                    let _ = black_box(limiter.allow(1).await);
                }
            });
        });
    });

    // 分片滑动窗口
    let sharded = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        100000,
    ));
    group.bench_function("sharded_sliding_window", |b| {
        let limiter = sharded.clone();
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..1000 {
                    let _ = black_box(limiter.allow(1).await);
                }
            });
        });
    });

    group.finish();
}

/// 基准测试：高并发下分片滑动窗口性能
fn bench_sharded_high_concurrency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        1000000,
    ));

    let mut group = c.benchmark_group("sharded_high_concurrency");

    for concurrency in [1, 10, 50, 100].iter() {
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
    bench_sharded_sliding_window_throughput,
    bench_sliding_window_comparison,
    bench_sharded_high_concurrency,
    // bench_governor_throughput, // 需要 PostgreSQL
    bench_concurrent_throughput,
    bench_mixed_operations_throughput
);

criterion_main!(benches);
