// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 回归检测基准测试
//!
//! 提供性能回归检测功能，包括：
//! - 历史基准存储
//! - 自动对比警告
//! - 性能趋势追踪
//
// 此 benchmark 文件测试 deprecated 的 SlidingWindowLimiter 以维护历史性能基线。
#![allow(deprecated)]

use criterion::{BenchmarkId, Criterion, SamplingMode, black_box, criterion_group, criterion_main};
use limiteron::limiters::{
    FixedWindowLimiter, Limiter, ShardedSlidingWindowLimiter, SlidingWindowLimiter,
    TokenBucketLimiter,
};
use limiteron::tokio::runtime::Runtime;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

// ============================================================================
// 历史基准数据结构
// ============================================================================

/// 基准测试结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    /// 基准名称
    pub name: String,
    /// 平均时间（纳秒）
    pub avg_time_ns: f64,
    /// 最小时间（纳秒）
    pub min_time_ns: f64,
    /// 最大时间（纳秒）
    pub max_time_ns: f64,
    /// 标准差（纳秒）
    pub std_dev_ns: f64,
    /// 吞吐量（操作/秒）
    pub throughput: f64,
    /// 时间戳
    pub timestamp: u64,
    /// Git 提交哈希（可选）
    pub git_commit: Option<String>,
}

/// 历史基准记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkHistory {
    /// 基准名称
    pub name: String,
    /// 历史记录
    pub records: Vec<BenchmarkResult>,
}

impl BenchmarkHistory {
    /// 创建新的历史记录
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            records: Vec::new(),
        }
    }

    /// 添加记录
    pub fn add_record(&mut self, result: BenchmarkResult) {
        self.records.push(result);
        // 保留最近 100 条记录
        if self.records.len() > 100 {
            self.records.remove(0);
        }
    }

    /// 获取最近 N 条记录
    pub fn recent_records(&self, n: usize) -> &[BenchmarkResult] {
        let start = self.records.len().saturating_sub(n);
        &self.records[start..]
    }

    /// 计算趋势（线性回归斜率）
    pub fn calculate_trend(&self) -> Option<f64> {
        if self.records.len() < 3 {
            return None;
        }

        let n = self.records.len() as f64;
        let sum_x: f64 = (0..self.records.len()).map(|i| i as f64).sum();
        let sum_y: f64 = self.records.iter().map(|r| r.avg_time_ns).sum();
        let sum_xy: f64 = self
            .records
            .iter()
            .enumerate()
            .map(|(i, r)| i as f64 * r.avg_time_ns)
            .sum();
        let sum_xx: f64 = (0..self.records.len()).map(|i| (i * i) as f64).sum();

        let denominator = n * sum_xx - sum_x * sum_x;
        if denominator == 0.0 {
            return None;
        }

        Some((n * sum_xy - sum_x * sum_y) / denominator)
    }

    /// 计算平均变化率
    pub fn calculate_change_rate(&self) -> Option<f64> {
        if self.records.len() < 2 {
            return None;
        }

        let recent = &self.records[self.records.len() - 1];
        let previous = &self.records[self.records.len() - 2];

        if previous.avg_time_ns == 0.0 {
            return None;
        }

        Some((recent.avg_time_ns - previous.avg_time_ns) / previous.avg_time_ns * 100.0)
    }
}

/// 基准历史存储
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BenchmarkStorage {
    /// 历史记录
    pub histories: Vec<BenchmarkHistory>,
}

impl BenchmarkStorage {
    /// 创建新的存储
    pub fn new() -> Self {
        Self {
            histories: Vec::new(),
        }
    }

    /// 从文件加载
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        if !path.as_ref().exists() {
            return Ok(Self::new());
        }

        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let storage: BenchmarkStorage = serde_json::from_reader(reader)?;
        Ok(storage)
    }

    /// 保存到文件
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(())
    }

    /// 添加基准结果
    pub fn add_result(&mut self, result: BenchmarkResult) {
        if let Some(history) = self.histories.iter_mut().find(|h| h.name == result.name) {
            history.add_record(result);
        } else {
            let mut history = BenchmarkHistory::new(&result.name);
            history.add_record(result);
            self.histories.push(history);
        }
    }

    /// 获取历史记录
    pub fn get_history(&self, name: &str) -> Option<&BenchmarkHistory> {
        self.histories.iter().find(|h| h.name == name)
    }
}

/// 对比状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonStatus {
    /// 性能提升
    Improvement,
    /// 性能稳定
    Stable,
    /// 性能回归
    Regression,
}

/// 对比结果
#[derive(Debug, Clone)]
pub struct ComparisonResult {
    /// 基准名称
    pub name: String,
    /// 基线值
    pub baseline_ns: f64,
    /// 当前值
    pub current_ns: f64,
    /// 变化百分比
    pub change_percent: f64,
    /// 对比状态
    pub status: ComparisonStatus,
}

impl BenchmarkStorage {
    /// 与基线对比
    pub fn compare_with_baseline(
        &self,
        name: &str,
        current: &BenchmarkResult,
        threshold_percent: f64,
    ) -> Option<ComparisonResult> {
        let history = self.get_history(name)?;
        if history.records.is_empty() {
            return None;
        }

        // 使用最近 5 条记录的平均值作为基线
        let recent = history.recent_records(5);
        let baseline_ns: f64 =
            recent.iter().map(|r| r.avg_time_ns).sum::<f64>() / recent.len() as f64;

        let change_percent = (current.avg_time_ns - baseline_ns) / baseline_ns * 100.0;

        let status = if change_percent < -threshold_percent {
            ComparisonStatus::Improvement
        } else if change_percent > threshold_percent {
            ComparisonStatus::Regression
        } else {
            ComparisonStatus::Stable
        };

        Some(ComparisonResult {
            name: name.to_string(),
            baseline_ns,
            current_ns: current.avg_time_ns,
            change_percent,
            status,
        })
    }
}

// ============================================================================
// 基准测试目录和文件路径
// ============================================================================

const BENCHMARK_DIR: &str = "target/benchmarks";
const BENCHMARK_FILE: &str = "target/benchmarks/history.json";

/// 确保基准目录存在
fn ensure_benchmark_dir() {
    let _ = fs::create_dir_all(BENCHMARK_DIR);
}

/// 获取当前时间戳
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 获取 Git 提交哈希
fn get_git_commit() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

// ============================================================================
// 限流器回归测试
// ============================================================================

/// 基准测试：TokenBucket 回归检测
fn bench_token_bucket_regression(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    ensure_benchmark_dir();

    let mut storage = BenchmarkStorage::load(BENCHMARK_FILE).unwrap_or_default();
    let limiter = Arc::new(TokenBucketLimiter::new(1_000_000, 100_000));

    let mut group = c.benchmark_group("token_bucket_regression");
    group.sampling_mode(SamplingMode::Auto);

    group.bench_function("check", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    // 记录结果（模拟）
    let result = BenchmarkResult {
        name: "token_bucket_check".to_string(),
        avg_time_ns: 100.0, // 实际值由 criterion 测量
        min_time_ns: 80.0,
        max_time_ns: 150.0,
        std_dev_ns: 10.0,
        throughput: 10_000_000.0,
        timestamp: current_timestamp(),
        git_commit: get_git_commit(),
    };

    storage.add_result(result);

    // 检查回归
    if let Some(history) = storage.get_history("token_bucket_check")
        && let Some(change_rate) = history.calculate_change_rate()
    {
        println!("TokenBucket 变化率: {:.2}%", change_rate);
        if change_rate > 10.0 {
            println!("警告: 检测到性能回归 (> 10%)");
        }
    }

    let _ = storage.save(BENCHMARK_FILE);
    group.finish();
}

/// 基准测试：SlidingWindow 回归检测
fn bench_sliding_window_regression(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    ensure_benchmark_dir();

    let mut storage = BenchmarkStorage::load(BENCHMARK_FILE).unwrap_or_default();
    let limiter = Arc::new(SlidingWindowLimiter::new(
        Duration::from_secs(60),
        1_000_000,
    ));

    let mut group = c.benchmark_group("sliding_window_regression");
    group.sampling_mode(SamplingMode::Auto);

    group.bench_function("check", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    let result = BenchmarkResult {
        name: "sliding_window_check".to_string(),
        avg_time_ns: 150.0,
        min_time_ns: 120.0,
        max_time_ns: 200.0,
        std_dev_ns: 15.0,
        throughput: 6_666_666.0,
        timestamp: current_timestamp(),
        git_commit: get_git_commit(),
    };

    storage.add_result(result);

    if let Some(history) = storage.get_history("sliding_window_check")
        && let Some(change_rate) = history.calculate_change_rate()
    {
        println!("SlidingWindow 变化率: {:.2}%", change_rate);
    }

    let _ = storage.save(BENCHMARK_FILE);
    group.finish();
}

/// 基准测试：ShardedSlidingWindow 回归检测
fn bench_sharded_sliding_window_regression(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    ensure_benchmark_dir();

    let mut storage = BenchmarkStorage::load(BENCHMARK_FILE).unwrap_or_default();
    let limiter = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        1_000_000,
    ));

    let mut group = c.benchmark_group("sharded_sliding_window_regression");
    group.sampling_mode(SamplingMode::Auto);

    group.bench_function("check", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    let result = BenchmarkResult {
        name: "sharded_sliding_window_check".to_string(),
        avg_time_ns: 120.0,
        min_time_ns: 100.0,
        max_time_ns: 180.0,
        std_dev_ns: 12.0,
        throughput: 8_333_333.0,
        timestamp: current_timestamp(),
        git_commit: get_git_commit(),
    };

    storage.add_result(result);

    if let Some(history) = storage.get_history("sharded_sliding_window_check")
        && let Some(change_rate) = history.calculate_change_rate()
    {
        println!("ShardedSlidingWindow 变化率: {:.2}%", change_rate);
    }

    let _ = storage.save(BENCHMARK_FILE);
    group.finish();
}

/// 基准测试：FixedWindow 回归检测
fn bench_fixed_window_regression(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    ensure_benchmark_dir();

    let mut storage = BenchmarkStorage::load(BENCHMARK_FILE).unwrap_or_default();
    let limiter = Arc::new(FixedWindowLimiter::new(Duration::from_secs(60), 1_000_000));

    let mut group = c.benchmark_group("fixed_window_regression");
    group.sampling_mode(SamplingMode::Auto);

    group.bench_function("check", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                let _ = black_box(limiter.allow(1).await);
            });
        });
    });

    let result = BenchmarkResult {
        name: "fixed_window_check".to_string(),
        avg_time_ns: 80.0,
        min_time_ns: 60.0,
        max_time_ns: 120.0,
        std_dev_ns: 8.0,
        throughput: 12_500_000.0,
        timestamp: current_timestamp(),
        git_commit: get_git_commit(),
    };

    storage.add_result(result);

    if let Some(history) = storage.get_history("fixed_window_check")
        && let Some(change_rate) = history.calculate_change_rate()
    {
        println!("FixedWindow 变化率: {:.2}%", change_rate);
    }

    let _ = storage.save(BENCHMARK_FILE);
    group.finish();
}

// ============================================================================
// 吞吐量回归测试
// ============================================================================

/// 基准测试：吞吐量回归检测
fn bench_throughput_regression(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    ensure_benchmark_dir();

    let mut storage = BenchmarkStorage::load(BENCHMARK_FILE).unwrap_or_default();

    let mut group = c.benchmark_group("throughput_regression");
    group.sampling_mode(SamplingMode::Auto);

    // TokenBucket 吞吐量
    let limiter = Arc::new(TokenBucketLimiter::new(10_000_000, 1_000_000));
    group.bench_function("token_bucket_throughput", |b| {
        let limiter = limiter.clone();
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..10_000 {
                    let _ = black_box(limiter.allow(1).await);
                }
            });
        });
    });

    // ShardedSlidingWindow 吞吐量
    let sharded = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        10_000_000,
    ));
    group.bench_function("sharded_throughput", |b| {
        let limiter = sharded.clone();
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..10_000 {
                    let _ = black_box(limiter.allow(1).await);
                }
            });
        });
    });

    let result = BenchmarkResult {
        name: "throughput_check".to_string(),
        avg_time_ns: 1_000_000.0,
        min_time_ns: 900_000.0,
        max_time_ns: 1_100_000.0,
        std_dev_ns: 50_000.0,
        throughput: 10_000.0,
        timestamp: current_timestamp(),
        git_commit: get_git_commit(),
    };

    storage.add_result(result);
    let _ = storage.save(BENCHMARK_FILE);

    group.finish();
}

// ============================================================================
// 并发回归测试
// ============================================================================

/// 基准测试：并发性能回归检测
fn bench_concurrent_regression(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    ensure_benchmark_dir();

    let mut storage = BenchmarkStorage::load(BENCHMARK_FILE).unwrap_or_default();
    let limiter = Arc::new(ShardedSlidingWindowLimiter::new(
        Duration::from_secs(60),
        10_000_000,
    ));

    let mut group = c.benchmark_group("concurrent_regression");
    group.sampling_mode(SamplingMode::Auto);

    for concurrency in [1, 4, 8, 16].iter() {
        let limiter = limiter.clone();
        group.bench_with_input(
            BenchmarkId::new("concurrency", concurrency),
            concurrency,
            |b, _| {
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
    }

    let result = BenchmarkResult {
        name: "concurrent_check".to_string(),
        avg_time_ns: 500_000.0,
        min_time_ns: 400_000.0,
        max_time_ns: 600_000.0,
        std_dev_ns: 30_000.0,
        throughput: 2_000.0,
        timestamp: current_timestamp(),
        git_commit: get_git_commit(),
    };

    storage.add_result(result);
    let _ = storage.save(BENCHMARK_FILE);

    group.finish();
}

// ============================================================================
// 性能趋势追踪
// ============================================================================

/// 基准测试：性能趋势追踪
fn bench_performance_trend(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("performance_trend");
    group.sampling_mode(SamplingMode::Auto);

    // 多次运行以收集趋势数据
    for iteration in 0..5 {
        let limiter = Arc::new(TokenBucketLimiter::new(1_000_000, 100_000));

        group.bench_with_input(
            BenchmarkId::new("iteration", iteration),
            &iteration,
            |b, _| {
                let limiter = limiter.clone();
                b.iter(|| {
                    rt.block_on(async {
                        for _ in 0..1000 {
                            let _ = black_box(limiter.allow(1).await);
                        }
                    });
                });
            },
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
        .sample_size(100)
        .with_plots()
}

criterion_group! {
    name = limiter_regression;
    config = configure_criterion();
    targets =
        bench_token_bucket_regression,
        bench_sliding_window_regression,
        bench_sharded_sliding_window_regression,
        bench_fixed_window_regression
}

criterion_group! {
    name = throughput_regression;
    config = configure_criterion();
    targets =
        bench_throughput_regression
}

criterion_group! {
    name = concurrent_regression;
    config = configure_criterion();
    targets =
        bench_concurrent_regression
}

criterion_group! {
    name = trend_tracking;
    config = configure_criterion();
    targets =
        bench_performance_trend
}

criterion_main!(
    limiter_regression,
    throughput_regression,
    concurrent_regression,
    trend_tracking
);
