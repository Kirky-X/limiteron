//! 吞吐量基准测试
//!
//! 测试系统的吞吐量性能，包括单线程吞吐量、并发吞吐量和吞吐量扩展曲线。

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, SamplingMode,
    Throughput,
};
use limiteron::limiters::{
    FixedWindowLimiter, Limiter, ShardedSlidingWindowLimiter, SlidingWindowLimiter,
    TokenBucketLimiter,
};
use oxcache::Cache;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

// ============================================================================
// 单线程吞吐量测试
// ============================================================================

/// 基准测试：TokenBucketLimiter 单线程吞吐量
fn bench_token_bucket_single_thread_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(TokenBucketLimiter::new(10_000_000, 1_000_000));

    let mut group = c.benchmark_group("token_bucket_single_thread_throughput");
    group.sampling_mode(SamplingMode::Auto);

    for size in [100, 1_000, 10_000].iter() {
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

/// 基准测试：SlidingWindowLimiter 单线程吞吐量
fn bench_sliding_window_single_thread_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(SlidingWindowLimiter::new(
        Duration::from_secs(60),
        10_000_000,
    ));

    let mut group = c.benchmark_group("sliding_window_single_thread_throughput");
    group.sampling_mode(SamplingMode::Auto);

    for size in [100, 1_000, 10_000].iter() {
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

/// 基准测试：ShardedSlidingWindowLimiter 单线程吞吐量
fn bench_sharded_sliding_window_single_thread_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        10_000_000,
    ));

    let mut group = c.benchmark_group("sharded_sliding_window_single_thread_throughput");
    group.sampling_mode(SamplingMode::Auto);

    for size in [100, 1_000, 10_000].iter() {
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

/// 基准测试：FixedWindowLimiter 单线程吞吐量
fn bench_fixed_window_single_thread_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(FixedWindowLimiter::new(Duration::from_secs(60), 10_000_000));

    let mut group = c.benchmark_group("fixed_window_single_thread_throughput");
    group.sampling_mode(SamplingMode::Auto);

    for size in [100, 1_000, 10_000].iter() {
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

// ============================================================================
// 并发吞吐量测试
// ============================================================================

/// 基准测试：TokenBucketLimiter 并发吞吐量
fn bench_token_bucket_concurrent_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(TokenBucketLimiter::new(100_000_000, 10_000_000));

    let mut group = c.benchmark_group("token_bucket_concurrent_throughput");
    group.sampling_mode(SamplingMode::Auto);

    for concurrency in [1, 2, 4, 8, 16, 32].iter() {
        let requests_per_task = 1000;
        group.throughput(Throughput::Elements(
            (requests_per_task * concurrency) as u64,
        ));
        let limiter = limiter.clone();
        group.bench_with_input(
            BenchmarkId::new("threads", concurrency),
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
                                    for _ in 0..requests_per_task {
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

/// 基准测试：ShardedSlidingWindowLimiter 并发吞吐量
fn bench_sharded_sliding_window_concurrent_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        100_000_000,
    ));

    let mut group = c.benchmark_group("sharded_sliding_window_concurrent_throughput");
    group.sampling_mode(SamplingMode::Auto);

    for concurrency in [1, 2, 4, 8, 16, 32].iter() {
        let requests_per_task = 1000;
        group.throughput(Throughput::Elements(
            (requests_per_task * concurrency) as u64,
        ));
        let limiter = limiter.clone();
        group.bench_with_input(
            BenchmarkId::new("threads", concurrency),
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
                                    for _ in 0..requests_per_task {
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

/// 基准测试：SlidingWindowLimiter 并发吞吐量
fn bench_sliding_window_concurrent_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(SlidingWindowLimiter::new(
        Duration::from_secs(60),
        100_000_000,
    ));

    let mut group = c.benchmark_group("sliding_window_concurrent_throughput");
    group.sampling_mode(SamplingMode::Auto);

    for concurrency in [1, 2, 4, 8, 16].iter() {
        let requests_per_task = 1000;
        group.throughput(Throughput::Elements(
            (requests_per_task * concurrency) as u64,
        ));
        let limiter = limiter.clone();
        group.bench_with_input(
            BenchmarkId::new("threads", concurrency),
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
                                    for _ in 0..requests_per_task {
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

// ============================================================================
// 吞吐量扩展曲线测试
// ============================================================================

/// 基准测试：吞吐量扩展曲线
///
/// 测量随着并发级别增加，吞吐量的变化曲线
fn bench_throughput_scaling_curve(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("throughput_scaling_curve");
    group.sampling_mode(SamplingMode::Auto);

    // TokenBucket 扩展曲线
    let token_bucket = Arc::new(TokenBucketLimiter::new(1_000_000_000, 100_000_000));
    for concurrency in [1, 2, 4, 8, 16, 32, 64, 128].iter() {
        let requests_per_task = 500;
        group.throughput(Throughput::Elements(
            (requests_per_task * concurrency) as u64,
        ));
        let limiter = token_bucket.clone();
        group.bench_with_input(
            BenchmarkId::new("token_bucket", concurrency),
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
                                    for _ in 0..requests_per_task {
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

    // ShardedSlidingWindow 扩展曲线
    let sharded = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        1_000_000_000,
    ));
    for concurrency in [1, 2, 4, 8, 16, 32, 64, 128].iter() {
        let requests_per_task = 500;
        group.throughput(Throughput::Elements(
            (requests_per_task * concurrency) as u64,
        ));
        let limiter = sharded.clone();
        group.bench_with_input(
            BenchmarkId::new("sharded", concurrency),
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
                                    for _ in 0..requests_per_task {
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

/// 基准测试：限流器吞吐量对比
///
/// 对比不同限流器在相同条件下的吞吐量
fn bench_limiter_throughput_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("limiter_throughput_comparison");
    group.sampling_mode(SamplingMode::Auto);

    let size = 10_000;
    group.throughput(Throughput::Elements(size));

    // TokenBucket
    let token_bucket = Arc::new(TokenBucketLimiter::new(100_000_000, 10_000_000));
    group.bench_function("token_bucket", |b| {
        let limiter = token_bucket.clone();
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..size {
                    let _ = black_box(limiter.allow(1).await);
                }
            });
        });
    });

    // SlidingWindow
    let sliding_window = Arc::new(SlidingWindowLimiter::new(
        Duration::from_secs(60),
        100_000_000,
    ));
    group.bench_function("sliding_window", |b| {
        let limiter = sliding_window.clone();
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..size {
                    let _ = black_box(limiter.allow(1).await);
                }
            });
        });
    });

    // ShardedSlidingWindow
    let sharded = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        100_000_000,
    ));
    group.bench_function("sharded_sliding_window", |b| {
        let limiter = sharded.clone();
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..size {
                    let _ = black_box(limiter.allow(1).await);
                }
            });
        });
    });

    // FixedWindow
    let fixed_window = Arc::new(FixedWindowLimiter::new(
        Duration::from_secs(60),
        100_000_000,
    ));
    group.bench_function("fixed_window", |b| {
        let limiter = fixed_window.clone();
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..size {
                    let _ = black_box(limiter.allow(1).await);
                }
            });
        });
    });

    group.finish();
}

// ============================================================================
// 缓存吞吐量测试
// ============================================================================

/// 基准测试：缓存吞吐量
fn bench_cache_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache: Arc<Cache<String, String>> = Arc::new(
        rt.block_on(
            Cache::builder()
                .capacity(100_000)
                .ttl(Duration::from_secs(60))
                .build(),
        )
        .unwrap(),
    );

    // 预填充缓存
    rt.block_on(async {
        for i in 0..10_000 {
            cache
                .set(&format!("key_{}", i), &format!("value_{}", i))
                .await;
        }
    });

    let mut group = c.benchmark_group("cache_throughput");
    group.sampling_mode(SamplingMode::Auto);

    // 缓存读取吞吐量
    for size in [1_000, 10_000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        let cache_read = cache.clone();
        group.bench_with_input(BenchmarkId::new("read", size), size, |b, _| {
            let counter = Arc::new(AtomicU64::new(0));
            b.iter(|| {
                let c = counter.fetch_add(1, Ordering::Relaxed) % 10_000;
                rt.block_on(async {
                    let _ = black_box(cache_read.get(&format!("key_{}", c)).await);
                });
            });
        });
    }

    // 缓存写入吞吐量
    for size in [1_000, 10_000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        let cache_write = cache.clone();
        group.bench_with_input(BenchmarkId::new("write", size), size, |b, _| {
            let counter = Arc::new(AtomicU64::new(0));
            b.iter(|| {
                let c = counter.fetch_add(1, Ordering::Relaxed);
                rt.block_on(async {
                    #[allow(clippy::unit_arg)]
                    black_box(
                        cache_write
                            .set(&format!("new_key_{}", c), &format!("value_{}", c))
                            .await,
                    );
                });
            });
        });
    }

    group.finish();
}

// ============================================================================
// 混合操作吞吐量测试
// ============================================================================

/// 基准测试：混合操作吞吐量
///
/// 模拟真实场景中不同操作的混合
fn bench_mixed_operations_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(TokenBucketLimiter::new(100_000_000, 10_000_000));

    let mut group = c.benchmark_group("mixed_operations_throughput");
    group.sampling_mode(SamplingMode::Auto);

    // 不同成本比例
    for (name, cost_distribution) in [
        ("uniform_cost_1", vec![1, 1, 1, 1, 1]),
        ("varied_costs", vec![1, 5, 10, 50, 100]),
        ("high_cost", vec![10, 50, 100, 500, 1000]),
    ] {
        let limiter = limiter.clone();
        let distribution = Arc::new(cost_distribution);
        group.bench_with_input(BenchmarkId::from_parameter(name), &limiter, |b, limiter| {
            let limiter = limiter.clone();
            let dist = distribution.clone();
            let idx = Arc::new(AtomicU64::new(0));
            b.iter(|| {
                rt.block_on(async {
                    let i = idx.fetch_add(1, Ordering::Relaxed) as usize % 5;
                    let cost = dist[i];
                    let _ = black_box(limiter.allow(cost).await);
                });
            });
        });
    }

    group.finish();
}

/// 基准测试：高负载吞吐量
///
/// 测量在高负载情况下的吞吐量
fn bench_high_load_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("high_load_throughput");
    group.sampling_mode(SamplingMode::Auto);

    // 预填充滑动窗口
    let sliding_window = Arc::new(SlidingWindowLimiter::new(
        Duration::from_secs(60),
        100_000_000,
    ));
    rt.block_on(async {
        for _ in 0..500_000 {
            let _ = sliding_window.allow(1).await;
        }
    });

    group.throughput(Throughput::Elements(10_000));
    group.bench_function("sliding_window_500k_loaded", |b| {
        let limiter = sliding_window.clone();
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..10_000 {
                    let _ = black_box(limiter.allow(1).await);
                }
            });
        });
    });

    // 分片滑动窗口
    let sharded = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        100_000_000,
    ));
    rt.block_on(async {
        for _ in 0..500_000 {
            let _ = sharded.allow(1).await;
        }
    });

    group.bench_function("sharded_500k_loaded", |b| {
        let limiter = sharded.clone();
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..10_000 {
                    let _ = black_box(limiter.allow(1).await);
                }
            });
        });
    });

    group.finish();
}

// ============================================================================
// 基准测试组配置
// ============================================================================

/// 配置 Criterion 以显示详细的吞吐量统计
fn configure_criterion() -> Criterion {
    Criterion::default()
        .confidence_level(0.95)
        .significance_level(0.05)
        .sample_size(100)
        .with_plots()
}

criterion_group! {
    name = single_thread_throughput;
    config = configure_criterion();
    targets =
        bench_token_bucket_single_thread_throughput,
        bench_sliding_window_single_thread_throughput,
        bench_sharded_sliding_window_single_thread_throughput,
        bench_fixed_window_single_thread_throughput
}

criterion_group! {
    name = concurrent_throughput;
    config = configure_criterion();
    targets =
        bench_token_bucket_concurrent_throughput,
        bench_sharded_sliding_window_concurrent_throughput,
        bench_sliding_window_concurrent_throughput
}

criterion_group! {
    name = scaling_curve;
    config = configure_criterion();
    targets =
        bench_throughput_scaling_curve,
        bench_limiter_throughput_comparison
}

criterion_group! {
    name = specialized_throughput;
    config = configure_criterion();
    targets =
        bench_cache_throughput,
        bench_mixed_operations_throughput,
        bench_high_load_throughput
}

criterion_main!(
    single_thread_throughput,
    concurrent_throughput,
    scaling_curve,
    specialized_throughput
);
