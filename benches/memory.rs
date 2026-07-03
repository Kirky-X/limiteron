//! 内存基准测试
//!
//! 测试各种组件的内存占用和内存稳定性，包括：
//! - 不同键数量内存占用
//! - 不同数据结构内存对比
//! - 持续运行内存稳定性
//! - 内存泄漏检测
//
// 此 benchmark 文件测试 deprecated 的 SlidingWindowLimiter 以维护历史性能基线。
#![allow(deprecated)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode};
use dashmap::DashMap;
use limiteron::limiters::{
    FixedWindowLimiter, Limiter, ShardedSlidingWindowLimiter, SlidingWindowLimiter,
    TokenBucketLimiter,
};
use limiteron::matchers::{ConditionEvaluator, MatchCondition, RequestContext, Rule, RuleMatcher};
use oxcache::Cache;
use std::alloc::{GlobalAlloc, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

// ============================================================================
// 内存分配跟踪器
// ============================================================================

/// 内存分配跟踪器
///
/// 用于跟踪内存分配情况，帮助检测内存泄漏
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator::new();

/// 跟踪内存分配的分配器
struct TrackingAllocator {
    inner: System,
    allocated: AtomicUsize,
}

impl TrackingAllocator {
    const fn new() -> Self {
        Self {
            inner: System,
            allocated: AtomicUsize::new(0),
        }
    }

    /// 获取已分配的总内存
    fn get_allocated(&self) -> usize {
        self.allocated.load(Ordering::Relaxed)
    }

    /// 重置分配计数
    fn reset(&self) {
        self.allocated.store(0, Ordering::Relaxed);
    }
}

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        self.allocated.fetch_add(layout.size(), Ordering::Relaxed);
        self.inner.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        self.allocated.fetch_sub(layout.size(), Ordering::Relaxed);
        self.inner.dealloc(ptr, layout);
    }
}

/// 获取当前进程的内存使用量（近似值）
fn get_memory_usage() -> usize {
    ALLOCATOR.get_allocated()
}

// ============================================================================
// 内存占用测量
// ============================================================================

/// 基准测试：TokenBucket 内存占用
///
/// 测量不同键数量下的内存占用
fn bench_token_bucket_memory_usage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("token_bucket_memory_usage");
    group.sampling_mode(SamplingMode::Auto);

    // 测量不同数量限流器的内存占用
    for count in [100, 1_000, 10_000, 100_000].iter() {
        ALLOCATOR.reset();
        let before = get_memory_usage();

        let limiters: Vec<Arc<TokenBucketLimiter>> = (0..*count)
            .map(|_| Arc::new(TokenBucketLimiter::new(1000, 100)))
            .collect();

        let after = get_memory_usage();
        let used = after.saturating_sub(before);

        group.bench_with_input(BenchmarkId::new("instances", count), count, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    for limiter in &limiters {
                        let _ = black_box(limiter.allow(1).await);
                    }
                });
            });
        });

        println!(
            "TokenBucket: {} 个实例，内存占用: {} bytes ({:.2} MB), 平均每实例: {} bytes",
            count,
            used,
            used as f64 / 1024.0 / 1024.0,
            used / count
        );

        // 防止优化掉 limiters
        black_box(limiters);
    }

    group.finish();
}

/// 基准测试：SlidingWindow 内存占用
fn bench_sliding_window_memory_usage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("sliding_window_memory_usage");
    group.sampling_mode(SamplingMode::Auto);

    for count in [100, 1_000, 10_000, 100_000].iter() {
        ALLOCATOR.reset();
        let before = get_memory_usage();

        let limiters: Vec<Arc<SlidingWindowLimiter>> = (0..*count)
            .map(|_| Arc::new(SlidingWindowLimiter::new(Duration::from_secs(60), 1000)))
            .collect();

        let after = get_memory_usage();
        let used = after.saturating_sub(before);

        group.bench_with_input(BenchmarkId::new("instances", count), count, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    for limiter in &limiters {
                        let _ = black_box(limiter.allow(1).await);
                    }
                });
            });
        });

        println!(
            "SlidingWindow: {} 个实例，内存占用: {} bytes ({:.2} MB), 平均每实例: {} bytes",
            count,
            used,
            used as f64 / 1024.0 / 1024.0,
            used / count
        );

        black_box(limiters);
    }

    group.finish();
}

/// 基准测试：ShardedSlidingWindow 内存占用
fn bench_sharded_sliding_window_memory_usage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("sharded_sliding_window_memory_usage");
    group.sampling_mode(SamplingMode::Auto);

    for count in [100, 1_000, 10_000, 100_000].iter() {
        ALLOCATOR.reset();
        let before = get_memory_usage();

        let limiters: Vec<Arc<ShardedSlidingWindowLimiter>> = (0..*count)
            .map(|_| {
                Arc::new(ShardedSlidingWindowLimiter::new(
                    Duration::from_secs(60),
                    1000,
                ))
            })
            .collect();

        let after = get_memory_usage();
        let used = after.saturating_sub(before);

        group.bench_with_input(BenchmarkId::new("instances", count), count, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    for limiter in &limiters {
                        let _ = black_box(limiter.allow(1).await);
                    }
                });
            });
        });

        println!(
            "ShardedSlidingWindow: {} 个实例，内存占用: {} bytes ({:.2} MB), 平均每实例: {} bytes",
            count,
            used,
            used as f64 / 1024.0 / 1024.0,
            used / count
        );

        black_box(limiters);
    }

    group.finish();
}

/// 基准测试：FixedWindow 内存占用
fn bench_fixed_window_memory_usage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("fixed_window_memory_usage");
    group.sampling_mode(SamplingMode::Auto);

    for count in [100, 1_000, 10_000, 100_000].iter() {
        ALLOCATOR.reset();
        let before = get_memory_usage();

        let limiters: Vec<Arc<FixedWindowLimiter>> = (0..*count)
            .map(|_| Arc::new(FixedWindowLimiter::new(Duration::from_secs(60), 1000)))
            .collect();

        let after = get_memory_usage();
        let used = after.saturating_sub(before);

        group.bench_with_input(BenchmarkId::new("instances", count), count, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    for limiter in &limiters {
                        let _ = black_box(limiter.allow(1).await);
                    }
                });
            });
        });

        println!(
            "FixedWindow: {} 个实例，内存占用: {} bytes ({:.2} MB), 平均每实例: {} bytes",
            count,
            used,
            used as f64 / 1024.0 / 1024.0,
            used / count
        );

        black_box(limiters);
    }

    group.finish();
}

// ============================================================================
// 数据结构内存对比
// ============================================================================

/// 基准测试：DashMap 内存占用
fn bench_dashmap_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("dashmap_memory_usage");
    group.sampling_mode(SamplingMode::Auto);

    for count in [1_000, 10_000, 100_000, 1_000_000].iter() {
        ALLOCATOR.reset();
        let before = get_memory_usage();

        let map: DashMap<String, u64> = DashMap::new();
        for i in 0..*count {
            map.insert(format!("key_{}", i), i as u64);
        }

        let after = get_memory_usage();
        let used = after.saturating_sub(before);

        println!(
            "DashMap: {} 个键值对，内存占用: {} bytes ({:.2} MB), 平均每条目: {} bytes",
            count,
            used,
            used as f64 / 1024.0 / 1024.0,
            used / count
        );

        group.bench_with_input(BenchmarkId::new("entries", count), count, |b, _| {
            b.iter(|| {
                let _ = black_box(map.get(&format!("key_{}", count / 2)));
            });
        });

        black_box(map);
    }

    group.finish();
}

/// 基准测试：Cache 内存占用
fn bench_cache_memory_usage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("cache_memory_usage");
    group.sampling_mode(SamplingMode::Auto);

    for count in [1_000, 10_000, 100_000].iter() {
        ALLOCATOR.reset();
        let before = get_memory_usage();

        let cache: Arc<Cache<String, String>> = Arc::new(
            rt.block_on(
                Cache::builder()
                    .capacity(*count as u64 * 2)
                    .ttl(Duration::from_secs(60))
                    .build(),
            )
            .unwrap(),
        );

        rt.block_on(async {
            for i in 0..*count {
                let _ = cache
                    .set(&format!("key_{}", i), &format!("value_{}", i))
                    .await;
            }
        });

        let after = get_memory_usage();
        let used = after.saturating_sub(before);

        println!(
            "Cache: {} 个条目，内存占用: {} bytes ({:.2} MB), 平均每条目: {} bytes",
            count,
            used,
            used as f64 / 1024.0 / 1024.0,
            used / count
        );

        let cache_clone = cache.clone();
        group.bench_with_input(BenchmarkId::new("entries", count), count, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    let _ = black_box(cache_clone.get(&format!("key_{}", count / 2)).await);
                });
            });
        });

        black_box(cache);
    }

    group.finish();
}

/// 基准测试：规则匹配器内存占用
fn bench_rule_matcher_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("rule_matcher_memory_usage");
    group.sampling_mode(SamplingMode::Auto);

    for count in [10, 50, 100, 500, 1000].iter() {
        ALLOCATOR.reset();
        let before = get_memory_usage();

        let rules: Vec<Rule> = (0..*count)
            .map(|i| {
                let condition: Box<dyn ConditionEvaluator> =
                    Box::new(MatchCondition::User(vec![format!("user_{}", i)]));
                Rule {
                    id: format!("rule_{}", i),
                    name: format!("Test Rule {}", i),
                    priority: (100 - i as u16 % 100),
                    condition,
                    enabled: true,
                }
            })
            .collect();

        let matcher = RuleMatcher::with_dependencies(rules);

        let after = get_memory_usage();
        let used = after.saturating_sub(before);

        println!(
            "RuleMatcher: {} 条规则，内存占用: {} bytes ({:.2} KB), 平均每规则: {} bytes",
            count,
            used,
            used as f64 / 1024.0,
            used / count
        );

        let mut context = RequestContext::new();
        context.path = "/api/v5/users".to_string();
        context.method = "GET".to_string();

        group.bench_with_input(BenchmarkId::new("rules", count), count, |b, _| {
            b.iter(|| {
                let matched = matcher.match_all(&context);
                black_box(matched);
            });
        });

        black_box(matcher);
    }

    group.finish();
}

// ============================================================================
// 内存稳定性测试
// ============================================================================

/// 基准测试：持续运行内存稳定性
///
/// 测量在持续运行情况下内存是否稳定（不持续增长）
fn bench_memory_stability(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("memory_stability");
    group.sampling_mode(SamplingMode::Auto);
    group.measurement_time(Duration::from_secs(30));

    // TokenBucket 稳定性
    let limiter = Arc::new(TokenBucketLimiter::new(1_000_000, 100_000));
    ALLOCATOR.reset();
    let initial_memory = get_memory_usage();

    group.bench_function("token_bucket_stability", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..1000 {
                    let _ = black_box(limiter.allow(1).await);
                }
            });
        });
    });

    let final_memory = get_memory_usage();
    let memory_growth = final_memory.saturating_sub(initial_memory);
    println!(
        "TokenBucket 内存增长: {} bytes ({:.2} KB)",
        memory_growth,
        memory_growth as f64 / 1024.0
    );

    // SlidingWindow 稳定性
    let limiter = Arc::new(SlidingWindowLimiter::new(
        Duration::from_secs(60),
        1_000_000,
    ));
    ALLOCATOR.reset();
    let initial_memory = get_memory_usage();

    group.bench_function("sliding_window_stability", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..1000 {
                    let _ = black_box(limiter.allow(1).await);
                }
            });
        });
    });

    let final_memory = get_memory_usage();
    let memory_growth = final_memory.saturating_sub(initial_memory);
    println!(
        "SlidingWindow 内存增长: {} bytes ({:.2} KB)",
        memory_growth,
        memory_growth as f64 / 1024.0
    );

    // ShardedSlidingWindow 稳定性
    let limiter = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        1_000_000,
    ));
    ALLOCATOR.reset();
    let initial_memory = get_memory_usage();

    group.bench_function("sharded_stability", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..1000 {
                    let _ = black_box(limiter.allow(1).await);
                }
            });
        });
    });

    let final_memory = get_memory_usage();
    let memory_growth = final_memory.saturating_sub(initial_memory);
    println!(
        "ShardedSlidingWindow 内存增长: {} bytes ({:.2} KB)",
        memory_growth,
        memory_growth as f64 / 1024.0
    );

    group.finish();
}

/// 基准测试：高负载内存稳定性
///
/// 测量在高负载情况下的内存稳定性
fn bench_high_load_memory_stability(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("high_load_memory_stability");
    group.sampling_mode(SamplingMode::Auto);
    group.measurement_time(Duration::from_secs(30));

    // 创建大量请求模拟高负载
    let limiter = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        10_000_000,
    ));

    ALLOCATOR.reset();
    let initial_memory = get_memory_usage();

    // 预热
    rt.block_on(async {
        for _ in 0..100_000 {
            let _ = limiter.allow(1).await;
        }
    });

    let after_warmup = get_memory_usage();
    println!(
        "预热后内存: {} bytes ({:.2} MB)",
        after_warmup,
        after_warmup as f64 / 1024.0 / 1024.0
    );

    group.bench_function("high_load_stability", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..10_000 {
                    let _ = black_box(limiter.allow(1).await);
                }
            });
        });
    });

    let final_memory = get_memory_usage();
    let total_growth = final_memory.saturating_sub(initial_memory);
    println!(
        "高负载测试总内存增长: {} bytes ({:.2} MB)",
        total_growth,
        total_growth as f64 / 1024.0 / 1024.0
    );

    group.finish();
}

// ============================================================================
// 内存泄漏检测
// ============================================================================

/// 基准测试：内存泄漏检测
///
/// 通过多次迭代检测是否存在内存泄漏
fn bench_memory_leak_detection(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("memory_leak_detection");
    group.sampling_mode(SamplingMode::Auto);

    // 测试创建和销毁限流器
    ALLOCATOR.reset();
    let mut memory_samples = Vec::new();

    for iteration in 0..10 {
        let before = get_memory_usage();

        // 创建并使用限流器
        for _ in 0..1000 {
            let limiter = TokenBucketLimiter::new(1000, 100);
            rt.block_on(async {
                let _ = limiter.allow(1).await;
            });
        }

        let after = get_memory_usage();
        let used = after.saturating_sub(before);
        memory_samples.push(used);

        println!("迭代 {}: 内存使用 {} bytes", iteration, used);
    }

    // 分析内存趋势
    if memory_samples.len() >= 3 {
        let first_third = &memory_samples[..memory_samples.len() / 3];
        let last_third = &memory_samples[memory_samples.len() * 2 / 3..];

        let avg_first: usize = first_third.iter().sum::<usize>() / first_third.len();
        let avg_last: usize = last_third.iter().sum::<usize>() / last_third.len();

        let growth_rate = (avg_last as f64 - avg_first as f64) / avg_first as f64;

        println!(
            "内存增长趋势: 前1/3平均 {} bytes, 后1/3平均 {} bytes, 增长率 {:.2}%",
            avg_first,
            avg_last,
            growth_rate * 100.0
        );

        if growth_rate > 0.1 {
            println!("警告: 检测到可能的内存泄漏 (增长率 > 10%)");
        }
    }

    group.finish();
}

/// 基准测试：缓存内存泄漏检测
fn bench_cache_memory_leak_detection(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("cache_memory_leak_detection");
    group.sampling_mode(SamplingMode::Auto);

    let cache: Arc<Cache<String, String>> = Arc::new(
        rt.block_on(
            Cache::builder()
                .capacity(100_000)
                .ttl(Duration::from_secs(60))
                .build(),
        )
        .unwrap(),
    );

    ALLOCATOR.reset();
    let mut memory_samples = Vec::new();

    for iteration in 0..10 {
        let before = get_memory_usage();

        // 写入和读取缓存
        rt.block_on(async {
            for i in 0..10_000 {
                let _ = cache
                    .set(&format!("key_{}", i), &format!("value_{}", i))
                    .await;
                let _ = cache.get(&format!("key_{}", i)).await;
            }
        });

        let after = get_memory_usage();
        let used = after.saturating_sub(before);
        memory_samples.push(used);

        println!("缓存迭代 {}: 内存使用 {} bytes", iteration, used);
    }

    // 分析内存趋势
    if memory_samples.len() >= 3 {
        let first = memory_samples[0];
        let last = *memory_samples.last().unwrap();
        let growth = last.saturating_sub(first);

        println!(
            "缓存测试: 初始 {} bytes, 最终 {} bytes, 增长 {} bytes",
            first, last, growth
        );

        if growth > 1_000_000 {
            println!("警告: 缓存内存增长超过 1MB，可能存在内存泄漏");
        }
    }

    group.finish();
}

// ============================================================================
// 并发内存测试
// ============================================================================

/// 基准测试：并发内存使用
fn bench_concurrent_memory_usage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("concurrent_memory_usage");
    group.sampling_mode(SamplingMode::Auto);

    let limiter = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        10_000_000,
    ));

    for concurrency in [1, 4, 8, 16, 32].iter() {
        ALLOCATOR.reset();
        let before = get_memory_usage();

        group.bench_with_input(
            BenchmarkId::new("concurrency", concurrency),
            concurrency,
            |b, _| {
                let limiter = limiter.clone();
                b.iter(|| {
                    rt.block_on(async {
                        let mut handles = vec![];
                        for _ in 0..*concurrency {
                            let limiter = limiter.clone();
                            handles.push(async move {
                                for _ in 0..1000 {
                                    let _ = limiter.allow(1).await;
                                }
                            });
                        }
                        for handle in handles {
                            handle.await;
                        }
                    });
                });
            },
        );

        let after = get_memory_usage();
        let used = after.saturating_sub(before);

        println!(
            "并发 {} 线程: 内存使用 {} bytes ({:.2} MB)",
            concurrency,
            used,
            used as f64 / 1024.0 / 1024.0
        );
    }

    group.finish();
}

// ============================================================================
// 基准测试组配置
// ============================================================================

/// 配置 Criterion
fn configure_criterion() -> Criterion {
    Criterion::default()
        .confidence_level(0.95)
        .significance_level(0.05)
        .sample_size(50) // 减少样本数以加快内存测试
        .with_plots()
}

criterion_group! {
    name = memory_usage;
    config = configure_criterion();
    targets =
        bench_token_bucket_memory_usage,
        bench_sliding_window_memory_usage,
        bench_sharded_sliding_window_memory_usage,
        bench_fixed_window_memory_usage
}

criterion_group! {
    name = data_structure_memory;
    config = configure_criterion();
    targets =
        bench_dashmap_memory_usage,
        bench_cache_memory_usage,
        bench_rule_matcher_memory_usage
}

criterion_group! {
    name = memory_stability;
    config = configure_criterion();
    targets =
        bench_memory_stability,
        bench_high_load_memory_stability
}

criterion_group! {
    name = memory_leak;
    config = configure_criterion();
    targets =
        bench_memory_leak_detection,
        bench_cache_memory_leak_detection
}

criterion_group! {
    name = concurrent_memory;
    config = configure_criterion();
    targets =
        bench_concurrent_memory_usage
}

criterion_main!(
    memory_usage,
    data_structure_memory,
    memory_stability,
    memory_leak,
    concurrent_memory
);
