# limiteron-inklog

inklog sink 限流适配器：按 target 分桶实现 inklog 自有 [`SinkRateLimit`] 端口。

不发布 crates.io（与 `limiteron-dbnexus` 同款 workspace 集成 crate）。

## 行为

- **按 target 分桶**：每个 `record.target` 独立令牌预算（容量 `capacity`，每秒补充 `refill_per_sec`）；补充语义与 limiteron `TokenBucketLimiter::new(rate, rate)` 等价（纳秒时间积分携带，亚令牌积分滞留，封顶丢弃）。
- **ERROR 直通**：`ERROR` 级记录不限流（错误日志不因预算耗尽丢失）。
- **写失败归还**：sink 写失败（`report(_, Failed)`）归还 1 个预算令牌。
- **桶数上限**：默认 4096（`with_max_buckets` 覆盖），超限新 target 落入共享 fallback 桶，防无界 target 集合的内存放大。

## 装配

```rust
use std::sync::Arc;
use inklog::SinkRateLimit;
use limiteron-inklog::TargetBucketRateLimit;

let limiter: Arc<dyn SinkRateLimit> = Arc::new(TargetBucketRateLimit::new(100, 100));
// let sink = inklog::RateLimitedSink::new(inner_sink, limiter);
```

## 依赖策略

- `limiteron`：父仓 path+version 双钉（仅取 `Clock` 抽象与 `MockClock` 测试时钟）。
- `inklog`：crates.io registry（`0.3.0-rc.11` 起的 `SinkRateLimit` 端口）；发布顺序 trait-kit → oxcache/inklog → limiteron 保证清单可解析。
