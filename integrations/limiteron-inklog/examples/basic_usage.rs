//! 按目标分桶限流的 sink 装配示例。
//!
//! 运行：`cargo run -p limiteron-inklog --example basic_usage`

use std::sync::Arc;

use inklog::{InklogError, LogRecord, LogSink, RateLimitedSink, SinkRateLimit};
use limiteron_inklog::TargetBucketRateLimit;

/// 无操作 sink：仅演示装配形态。
struct NoopSink;

#[async_trait::async_trait]
impl LogSink for NoopSink {
    async fn write(&self, _record: &LogRecord) -> Result<(), InklogError> {
        Ok(())
    }

    async fn flush(&self) -> Result<(), InklogError> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), InklogError> {
        Ok(())
    }
}

fn main() {
    // 每秒 100 条、突发 100 条的每-target 预算；ERROR 记录直通，
    // sink 写失败自动归还预算
    let limiter: Arc<dyn SinkRateLimit> = Arc::new(TargetBucketRateLimit::new(100, 100));
    let _sink = RateLimitedSink::new(Arc::new(NoopSink), limiter);
    println!("rate-limited sink assembled (adapter: limiteron-inklog)");
}
