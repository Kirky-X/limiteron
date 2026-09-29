// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 吞吐量基准测试
//!
//! 测试系统的吞吐量性能，包括单线程吞吐量、并发吞吐量和吞吐量扩展曲线。
//
// 此 benchmark 文件测试 deprecated 的 SlidingWindowLimiter 以维护历史性能基线。
#![allow(deprecated)]

use criterion::{
    BatchSize, BenchmarkId, Criterion, SamplingMode, Throughput, black_box, criterion_group,
    criterion_main,
};
use limiteron::limiters::{
    FixedWindowLimiter, LeakyBucketLimiter, Limiter, ShardedSlidingWindowLimiter,
    SlidingWindowLimiter, SlidingWindowLogLimiter, TokenBucketLimiter,
};
use limiteron::oxcache::Cache;
use limiteron::tokio::runtime::Runtime;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

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

/// 基准测试：LeakyBucketLimiter 单线程吞吐量
///
/// 每测量迭代重建全新实例（未计时 setup）：空桶注入 ≤ 容量的 size 单位
/// 全程走放行路径，预算耗尽后转入拒绝稳态的共享实例口径不可用（容量
/// 会在采样中途打满，测到的将变成拒绝判定而非注入记账）。漏速 1M/s 下
/// 毫秒级迭代漏出可忽略，测得的是纯判定路径（锁 + 记账）吞吐。
fn bench_leaky_bucket_single_thread_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("leaky_bucket_single_thread_throughput");
    group.sampling_mode(SamplingMode::Auto);

    for size in [100, 1_000, 10_000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || Arc::new(LeakyBucketLimiter::new(10_000_000, 1_000_000).unwrap()),
                |limiter| {
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

/// 基准测试：SlidingWindowLogLimiter 单线程吞吐量
///
/// max_requests 受日志型上限 100K 约束（内存随条目线性增长）。
/// 每测量迭代重建全新实例（未计时 setup）：共享实例的预算会在数十个
/// 采样迭代内耗尽（size=10_000 时约 10 迭代即满），此后稳态为纯拒绝
/// 路径；重建后空窗口追加 size 条全程走放行路径（过期确认 + 追加记账）。
fn bench_sliding_window_log_single_thread_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("sliding_window_log_single_thread_throughput");
    group.sampling_mode(SamplingMode::Auto);

    for size in [100, 1_000, 10_000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || {
                    Arc::new(
                        SlidingWindowLogLimiter::new(100_000, Duration::from_secs(3600)).unwrap(),
                    )
                },
                |limiter| {
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

/// 基准测试：LeakyBucketLimiter 并发吞吐量（Mutex 串行点争用曲线）
fn bench_leaky_bucket_concurrent_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("leaky_bucket_concurrent_throughput");
    group.sampling_mode(SamplingMode::Auto);

    for concurrency in [1, 2, 4, 8, 16, 32].iter() {
        let requests_per_task = 1000;
        group.throughput(Throughput::Elements(
            (requests_per_task * concurrency) as u64,
        ));
        group.bench_with_input(
            BenchmarkId::new("threads", concurrency),
            concurrency,
            |b, &concurrency| {
                b.iter_batched(
                    // 每迭代全新实例：总注入 32K ≤ 容量 10M，全程放行路径
                    || Arc::new(LeakyBucketLimiter::new(10_000_000, 1_000_000).unwrap()),
                    |limiter| {
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

/// 基准测试：SlidingWindowLogLimiter 并发吞吐量（状态锁争用曲线）
fn bench_sliding_window_log_concurrent_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("sliding_window_log_concurrent_throughput");
    group.sampling_mode(SamplingMode::Auto);

    for concurrency in [1, 2, 4, 8, 16, 32].iter() {
        let requests_per_task = 1000;
        group.throughput(Throughput::Elements(
            (requests_per_task * concurrency) as u64,
        ));
        group.bench_with_input(
            BenchmarkId::new("threads", concurrency),
            concurrency,
            |b, &concurrency| {
                b.iter_batched(
                    // 每迭代全新实例：总注入 32K ≤ 上限 100K，全程放行路径
                    || {
                        Arc::new(
                            SlidingWindowLogLimiter::new(100_000, Duration::from_secs(3600))
                                .unwrap(),
                        )
                    },
                    |limiter| {
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

    // 口径说明：全体成员每测量迭代重建全新实例（iter_batched 未计时
    // setup），统一为「fresh 实例 × size 次放行」口径。共享实例 + 大预算
    // 的旧口径在 criterion Auto 采样下预算会中途耗尽（滑动日志 size=10_000
    // 时约 10 迭代即满），放行/拒绝两种测量路径混入同组使跨算法对比失真；
    // 重建成本（结构体初始化，ns 级）相对 size 次判定（µs~ms 级）可忽略。
    // 历史基线数据（如 README 令牌桶 12M ops/s）为旧口径测得，对比时注意。

    // TokenBucket
    group.bench_function("token_bucket", |b| {
        b.iter_batched(
            || Arc::new(TokenBucketLimiter::new(100_000_000, 10_000_000)),
            |limiter| {
                rt.block_on(async {
                    for _ in 0..size {
                        let _ = black_box(limiter.allow(1).await);
                    }
                });
            },
            BatchSize::PerIteration,
        );
    });

    // SlidingWindow
    group.bench_function("sliding_window", |b| {
        b.iter_batched(
            || {
                Arc::new(SlidingWindowLimiter::new(
                    Duration::from_secs(60),
                    100_000_000,
                ))
            },
            |limiter| {
                rt.block_on(async {
                    for _ in 0..size {
                        let _ = black_box(limiter.allow(1).await);
                    }
                });
            },
            BatchSize::PerIteration,
        );
    });

    // ShardedSlidingWindow
    group.bench_function("sharded_sliding_window", |b| {
        b.iter_batched(
            || {
                Arc::new(ShardedSlidingWindowLimiter::new(
                    Duration::from_secs(60),
                    100_000_000,
                ))
            },
            |limiter| {
                rt.block_on(async {
                    for _ in 0..size {
                        let _ = black_box(limiter.allow(1).await);
                    }
                });
            },
            BatchSize::PerIteration,
        );
    });

    // FixedWindow
    group.bench_function("fixed_window", |b| {
        b.iter_batched(
            || {
                Arc::new(FixedWindowLimiter::new(
                    Duration::from_secs(60),
                    100_000_000,
                ))
            },
            |limiter| {
                rt.block_on(async {
                    for _ in 0..size {
                        let _ = black_box(limiter.allow(1).await);
                    }
                });
            },
            BatchSize::PerIteration,
        );
    });

    // LeakyBucket（fresh 空桶注入 size ≤ 容量上限，全程放行记账路径）
    group.bench_function("leaky_bucket", |b| {
        b.iter_batched(
            || Arc::new(LeakyBucketLimiter::new(10_000_000, 1_000_000).unwrap()),
            |limiter| {
                rt.block_on(async {
                    for _ in 0..size {
                        let _ = black_box(limiter.allow(1).await);
                    }
                });
            },
            BatchSize::PerIteration,
        );
    });

    // SlidingWindowLog（fresh 空窗口追加 size ≤ 上限 100K，全程放行路径；
    // 容量口径受日志型上限约束与其余算法不同构，见 USER_GUIDE 内存换算说明）
    group.bench_function("sliding_window_log", |b| {
        b.iter_batched(
            || Arc::new(SlidingWindowLogLimiter::new(100_000, Duration::from_secs(3600)).unwrap()),
            |limiter| {
                rt.block_on(async {
                    for _ in 0..size {
                        let _ = black_box(limiter.allow(1).await);
                    }
                });
            },
            BatchSize::PerIteration,
        );
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
            let _ = cache
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
                    let _ = black_box(
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
        bench_fixed_window_single_thread_throughput,
        bench_leaky_bucket_single_thread_throughput,
        bench_sliding_window_log_single_thread_throughput
}

criterion_group! {
    name = concurrent_throughput;
    config = configure_criterion();
    targets =
        bench_token_bucket_concurrent_throughput,
        bench_sharded_sliding_window_concurrent_throughput,
        bench_sliding_window_concurrent_throughput,
        bench_leaky_bucket_concurrent_throughput,
        bench_sliding_window_log_concurrent_throughput
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
