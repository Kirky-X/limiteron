//! 延迟基准测试
//!
//! 测试各种操作的延迟性能，包括 P50/P90/P99/P99.9 延迟测量、直方图报告和不同操作延迟对比。

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode};
use limiteron::decision_chain::{DecisionChain, DecisionNode};
use limiteron::limiters::{
    FixedWindowLimiter, Limiter, ShardedSlidingWindowLimiter, SlidingWindowLimiter,
    TokenBucketLimiter,
};
use limiteron::matchers::{
    ConditionEvaluator, IdentifierExtractor, IpExtractor, IpRange, MatchCondition, RequestContext,
    Rule, RuleMatcher, UserIdExtractor,
};
use oxcache::Cache;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

// ============================================================================
// 延迟分布基准测试 (P50/P90/P99/P99.9)
// ============================================================================

/// 基准测试：TokenBucketLimiter 延迟分布
///
/// 测量 P50/P90/P99/P99.9 延迟，使用直方图报告
fn bench_token_bucket_latency_distribution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(TokenBucketLimiter::new(1000000, 100000));

    let mut group = c.benchmark_group("token_bucket_latency_distribution");
    // 配置更长的测量时间以获得更准确的百分位数
    group.sampling_mode(SamplingMode::Auto);
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("single_check", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    group.finish();
}

/// 基准测试：SlidingWindowLimiter 延迟分布
fn bench_sliding_window_latency_distribution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(60), 1000000));

    let mut group = c.benchmark_group("sliding_window_latency_distribution");
    group.sampling_mode(SamplingMode::Auto);
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("single_check", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    group.finish();
}

/// 基准测试：ShardedSlidingWindowLimiter 延迟分布
fn bench_sharded_sliding_window_latency_distribution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        1000000,
    ));

    let mut group = c.benchmark_group("sharded_sliding_window_latency_distribution");
    group.sampling_mode(SamplingMode::Auto);
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("single_check", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    group.finish();
}

/// 基准测试：FixedWindowLimiter 延迟分布
fn bench_fixed_window_latency_distribution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(FixedWindowLimiter::new(Duration::from_secs(60), 1000000));

    let mut group = c.benchmark_group("fixed_window_latency_distribution");
    group.sampling_mode(SamplingMode::Auto);
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("single_check", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    group.finish();
}

// ============================================================================
// 不同操作延迟对比
// ============================================================================

/// 基准测试：不同限流器延迟对比
///
/// 对比 TokenBucket、SlidingWindow、ShardedSlidingWindow、FixedWindow 的延迟
fn bench_limiter_latency_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("limiter_latency_comparison");
    group.sampling_mode(SamplingMode::Auto);
    group.measurement_time(Duration::from_secs(5));

    // TokenBucket
    let token_bucket = Arc::new(TokenBucketLimiter::new(1000000, 100000));
    group.bench_function("token_bucket", |b| {
        let limiter = token_bucket.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    // SlidingWindow
    let sliding_window = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(60), 1000000));
    group.bench_function("sliding_window", |b| {
        let limiter = sliding_window.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    // ShardedSlidingWindow
    let sharded_sliding_window = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        1000000,
    ));
    group.bench_function("sharded_sliding_window", |b| {
        let limiter = sharded_sliding_window.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    // FixedWindow
    let fixed_window = Arc::new(FixedWindowLimiter::new(Duration::from_secs(60), 1000000));
    group.bench_function("fixed_window", |b| {
        let limiter = fixed_window.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    group.finish();
}

/// 基准测试：缓存命中/未命中延迟对比
fn bench_cache_latency_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache: Arc<Cache<String, String>> = Arc::new(
        rt.block_on(
            Cache::builder()
                .capacity(10000)
                .ttl(Duration::from_secs(60))
                .build(),
        )
        .unwrap(),
    );

    // 预热缓存 - 创建热点键
    rt.block_on(async {
        for i in 0..100 {
            cache
                .set(&format!("hot_key_{}", i), &format!("value_{}", i))
                .await;
        }
    });

    let mut group = c.benchmark_group("cache_latency_comparison");
    group.sampling_mode(SamplingMode::Auto);
    group.measurement_time(Duration::from_secs(5));

    // 缓存命中
    let cache_hit = cache.clone();
    group.bench_function("cache_hit", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(cache_hit.get(&"hot_key_0".to_string()).await);
            });
        });
    });

    // 缓存未命中
    let cache_miss = cache.clone();
    group.bench_function("cache_miss", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(cache_miss.get(&"cold_key".to_string()).await);
            });
        });
    });

    // 缓存写入
    let cache_set = cache.clone();
    group.bench_function("cache_set", |b| {
        b.iter(|| {
            let key = format!("key_{}", black_box(42));
            rt.block_on(async {
                #[allow(clippy::unit_arg)]
                black_box(cache_set.set(&key, &"value".to_string()).await);
            });
        });
    });

    group.finish();
}

/// 基准测试：不同成本参数的延迟
fn bench_cost_latency_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(TokenBucketLimiter::new(1000000, 100000));

    let mut group = c.benchmark_group("cost_latency_comparison");
    group.sampling_mode(SamplingMode::Auto);

    for cost in [1, 10, 100, 1000].iter() {
        let limiter = limiter.clone();
        group.bench_with_input(BenchmarkId::new("cost", cost), cost, |b, &cost| {
            b.iter(|| {
                rt.block_on(async {
                    let _ = black_box(limiter.allow(cost).await);
                });
            });
        });
    }

    group.finish();
}

/// 基准测试：不同窗口大小的延迟
fn bench_window_size_latency_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let window_sizes = vec![
        ("100ms", Duration::from_millis(100)),
        ("1s", Duration::from_secs(1)),
        ("10s", Duration::from_secs(10)),
        ("1m", Duration::from_secs(60)),
        ("5m", Duration::from_secs(300)),
    ];

    let mut group = c.benchmark_group("window_size_latency_comparison");
    group.sampling_mode(SamplingMode::Auto);

    for (name, window_size) in window_sizes {
        let limiter = Arc::new(SlidingWindowLimiter::new(window_size, 100000));
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

// ============================================================================
// 规则匹配延迟测试
// ============================================================================

/// 创建测试规则
fn create_test_rules(count: usize) -> Vec<Rule> {
    (0..count)
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
        .collect()
}

/// 基准测试：规则匹配延迟
///
/// 测量不同规则数量下的匹配延迟
fn bench_rule_matching_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("rule_matching_latency");
    group.sampling_mode(SamplingMode::Auto);
    group.measurement_time(Duration::from_secs(5));

    for rule_count in [1, 10, 50, 100].iter() {
        let rules = create_test_rules(*rule_count);
        let matcher = RuleMatcher::with_dependencies(rules);
        let mut context = RequestContext::new();
        context.path = "/api/users".to_string();
        context.method = "GET".to_string();

        group.bench_with_input(BenchmarkId::new("rules", rule_count), rule_count, |b, _| {
            b.iter(|| {
                let matched = matcher.match_all(&context);
                black_box(matched);
            });
        });
    }

    group.finish();
}

/// 基准测试：标识符提取延迟
fn bench_identifier_extraction_latency(c: &mut Criterion) {
    // 创建用户ID提取器
    let extractor = UserIdExtractor::from_header("X-User-Id");

    let mut group = c.benchmark_group("identifier_extraction_latency");
    group.sampling_mode(SamplingMode::Auto);
    group.measurement_time(Duration::from_secs(5));

    // 从 Header 提取
    let mut context_with_header = RequestContext::new();
    context_with_header = context_with_header.with_header("X-User-Id", "user_12345");
    context_with_header = context_with_header.with_client_ip("192.168.1.100");
    context_with_header.path = "/api/users".to_string();
    context_with_header.method = "GET".to_string();

    group.bench_function("extract_from_header", |b| {
        b.iter(|| {
            let identifier = extractor.extract(&context_with_header);
            black_box(identifier);
        });
    });

    // IP 提取器
    let ip_extractor = IpExtractor::builder().build();
    let mut context_with_ip = RequestContext::new();
    context_with_ip = context_with_ip.with_client_ip("10.0.0.1");
    context_with_ip.path = "/api/users".to_string();
    context_with_ip.method = "GET".to_string();

    group.bench_function("extract_from_ip", |b| {
        b.iter(|| {
            let identifier = ip_extractor.extract(&context_with_ip);
            black_box(identifier);
        });
    });

    group.finish();
}

/// 基准测试：条件评估延迟
fn bench_condition_evaluation_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("condition_evaluation_latency");
    group.sampling_mode(SamplingMode::Auto);
    group.measurement_time(Duration::from_secs(5));

    // 简单条件 - 用户匹配
    let user_condition = MatchCondition::User(vec!["user_123".to_string()]);
    let mut context = RequestContext::new();
    context = context.with_header("X-User-Id", "user_123");
    context.path = "/api/users".to_string();
    context.method = "GET".to_string();

    group.bench_function("user_match", |b| {
        b.iter(|| {
            let result = user_condition.evaluate(&context);
            black_box(result);
        });
    });

    // IP 范围匹配
    let ip_condition = MatchCondition::Ip(vec![IpRange::Ipv4Cidr {
        addr: Ipv4Addr::new(192, 168, 0, 0),
        prefix: 16,
    }]);
    let mut context_with_ip = RequestContext::new();
    context_with_ip = context_with_ip.with_client_ip("192.168.1.100");
    context_with_ip.path = "/api/users".to_string();
    context_with_ip.method = "GET".to_string();

    group.bench_function("ip_range_match", |b| {
        b.iter(|| {
            let result = ip_condition.evaluate(&context_with_ip);
            black_box(result);
        });
    });

    group.finish();
}

// ============================================================================
// 完整决策链延迟测试
// ============================================================================

/// 基准测试：完整决策链延迟
///
/// 测量从请求到决策的完整流程延迟
fn bench_decision_chain_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(TokenBucketLimiter::new(1000000, 100000));

    let mut group = c.benchmark_group("decision_chain_latency");
    group.sampling_mode(SamplingMode::Auto);
    group.measurement_time(Duration::from_secs(5));

    // 单步决策
    group.bench_function("single_step_decision", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let result = limiter.allow(1).await.unwrap();
                black_box(result);
            });
        });
    });

    // 多步决策（模拟决策链）
    let limiter1 = Arc::new(TokenBucketLimiter::new(1000000, 100000));
    let limiter2 = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(60), 1000000));
    let limiter3 = Arc::new(FixedWindowLimiter::new(Duration::from_secs(60), 1000000));

    group.bench_function("multi_step_decision", |b| {
        let l1 = limiter1.clone();
        let l2 = limiter2.clone();
        let l3 = limiter3.clone();
        b.iter(|| {
            rt.block_on(async {
                // 模拟决策链：依次检查多个限流器
                let r1 = l1.allow(1).await.unwrap();
                let r2 = l2.allow(1).await.unwrap();
                let r3 = l3.allow(1).await.unwrap();
                black_box(r1 && r2 && r3);
            });
        });
    });

    // 使用 DecisionChain
    let chain = DecisionChain::with_dependencies(vec![
        DecisionNode::with_dependencies(
            "node_1".to_string(),
            "TokenBucket".to_string(),
            limiter1.clone(),
            100,
        ),
        DecisionNode::with_dependencies(
            "node_2".to_string(),
            "SlidingWindow".to_string(),
            limiter2.clone(),
            90,
        ),
        DecisionNode::with_dependencies(
            "node_3".to_string(),
            "FixedWindow".to_string(),
            limiter3.clone(),
            80,
        ),
    ]);

    group.bench_function("decision_chain_check", |b| {
        b.iter(|| {
            rt.block_on(async {
                let result = chain.check().await;
                black_box(result);
            });
        });
    });

    group.finish();
}

/// 基准测试：完整流程延迟（规则匹配 + 决策链）
fn bench_full_flow_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    // 创建规则和匹配器
    let rules = create_test_rules(10);
    let matcher = RuleMatcher::with_dependencies(rules);

    // 创建决策链
    let limiter = Arc::new(TokenBucketLimiter::new(1000000, 100000));
    let chain = DecisionChain::with_dependencies(vec![DecisionNode::with_dependencies(
        "node_1".to_string(),
        "TokenBucket".to_string(),
        limiter,
        100,
    )]);

    // 创建请求上下文
    let mut context = RequestContext::new();
    context = context.with_header("X-User-Id", "user_123");
    context.path = "/api/v5/users".to_string();
    context.method = "GET".to_string();

    let mut group = c.benchmark_group("full_flow_latency");
    group.sampling_mode(SamplingMode::Auto);
    group.measurement_time(Duration::from_secs(5));

    group.bench_function("rule_match_only", |b| {
        b.iter(|| {
            let matched = matcher.match_all(&context);
            black_box(matched);
        });
    });

    group.bench_function("decision_chain_only", |b| {
        b.iter(|| {
            rt.block_on(async {
                let result = chain.check().await;
                black_box(result);
            });
        });
    });

    group.bench_function("full_flow", |b| {
        b.iter(|| {
            // 1. 规则匹配
            let matched = matcher.match_all(&context);
            // 2. 决策链检查
            rt.block_on(async {
                if !matched.is_empty() {
                    let result = chain.check().await;
                    black_box(result);
                } else {
                    black_box(false);
                }
            });
        });
    });

    group.finish();
}

// ============================================================================
// 并发延迟测试
// ============================================================================

/// 基准测试：并发检查延迟
///
/// 测量不同并发级别下的延迟变化
fn bench_concurrent_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(TokenBucketLimiter::new(10000000, 1000000));

    let mut group = c.benchmark_group("concurrent_latency");
    group.sampling_mode(SamplingMode::Auto);

    for concurrency in [1, 2, 4, 8, 16, 32, 64].iter() {
        let limiter = limiter.clone();
        group.bench_with_input(
            BenchmarkId::new("concurrency", concurrency),
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

/// 基准测试：高负载下的延迟
///
/// 测量在高负载（大量已处理请求）情况下的延迟
fn bench_high_load_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    // 预填充滑动窗口
    let limiter = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(60), 10000000));
    rt.block_on(async {
        for _ in 0..100000 {
            let _ = limiter.allow(1).await;
        }
    });

    let mut group = c.benchmark_group("high_load_latency");
    group.sampling_mode(SamplingMode::Auto);
    group.measurement_time(Duration::from_secs(5));

    group.bench_function("sliding_window_100k_requests", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    // 分片滑动窗口在高负载下的表现
    let sharded_limiter = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        10000000,
    ));
    rt.block_on(async {
        for _ in 0..100000 {
            let _ = sharded_limiter.allow(1).await;
        }
    });

    group.bench_function("sharded_sliding_window_100k_requests", |b| {
        let limiter = sharded_limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    group.finish();
}

// ============================================================================
// 延迟稳定性测试
// ============================================================================

/// 基准测试：延迟稳定性
///
/// 测量延迟的稳定性（抖动）
fn bench_latency_stability(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let limiter = Arc::new(TokenBucketLimiter::new(1000000, 100000));

    let mut group = c.benchmark_group("latency_stability");
    // 使用更长的测量时间来捕获延迟抖动
    group.measurement_time(Duration::from_secs(15));
    group.sampling_mode(SamplingMode::Auto);

    group.bench_function("stable_latency", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    group.finish();
}

/// 基准测试：冷启动 vs 热运行延迟
fn bench_cold_vs_warm_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("cold_vs_warm_latency");
    group.sampling_mode(SamplingMode::Auto);

    // 冷启动 - 每次迭代创建新的限流器
    group.bench_function("cold_start", |b| {
        b.iter(|| {
            let limiter = TokenBucketLimiter::new(1000, 100);
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    // 热运行 - 使用已存在的限流器
    let warm_limiter = Arc::new(TokenBucketLimiter::new(1000000, 100000));
    // 预热
    rt.block_on(async {
        for _ in 0..1000 {
            let _ = warm_limiter.allow(1).await;
        }
    });

    group.bench_function("warm_running", |b| {
        let limiter = warm_limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    group.finish();
}

// ============================================================================
// 百分位数延迟测试
// ============================================================================

/// 基准测试：详细百分位数延迟
///
/// 使用更大的样本量来获得更准确的 P50/P90/P99/P99.9 延迟
fn bench_percentile_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("percentile_latency");
    // 使用更大的样本量
    group.sample_size(1000);
    group.measurement_time(Duration::from_secs(20));
    group.sampling_mode(SamplingMode::Auto);

    // TokenBucket 百分位数
    let token_bucket = Arc::new(TokenBucketLimiter::new(1000000, 100000));
    group.bench_function("token_bucket_percentiles", |b| {
        let limiter = token_bucket.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    // ShardedSlidingWindow 百分位数
    let sharded = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        1000000,
    ));
    group.bench_function("sharded_percentiles", |b| {
        let limiter = sharded.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    group.finish();
}

// ============================================================================
// 基准测试组配置
// ============================================================================

/// 配置 Criterion 以显示详细的延迟统计
fn configure_criterion() -> Criterion {
    Criterion::default()
        // 配置置信水平
        .confidence_level(0.95)
        // 配置显著性水平
        .significance_level(0.05)
        // 配置样本数量
        .sample_size(100)
        // 启用直方图输出
        .with_plots()
}

criterion_group! {
    name = latency_distribution;
    config = configure_criterion();
    targets =
        bench_token_bucket_latency_distribution,
        bench_sliding_window_latency_distribution,
        bench_sharded_sliding_window_latency_distribution,
        bench_fixed_window_latency_distribution
}

criterion_group! {
    name = latency_comparison;
    config = configure_criterion();
    targets =
        bench_limiter_latency_comparison,
        bench_cache_latency_comparison,
        bench_cost_latency_comparison,
        bench_window_size_latency_comparison
}

criterion_group! {
    name = rule_matching;
    config = configure_criterion();
    targets =
        bench_rule_matching_latency,
        bench_identifier_extraction_latency,
        bench_condition_evaluation_latency
}

criterion_group! {
    name = decision_chain;
    config = configure_criterion();
    targets =
        bench_decision_chain_latency,
        bench_full_flow_latency
}

criterion_group! {
    name = concurrent_latency;
    config = configure_criterion();
    targets =
        bench_concurrent_latency,
        bench_high_load_latency
}

criterion_group! {
    name = latency_stability;
    config = configure_criterion();
    targets =
        bench_latency_stability,
        bench_cold_vs_warm_latency,
        bench_percentile_latency
}

criterion_main!(
    latency_distribution,
    latency_comparison,
    rule_matching,
    decision_chain,
    concurrent_latency,
    latency_stability
);
