# 📘 Limiteron API 参考

本文档完整描述 Limiteron 的公开 API，包括限流器、封禁管理、配额控制、熔断器、Governor、匹配器、存储后端、Admin REST API、配置加载与错误类型。所有签名与 `src/` 源码一致（版本 0.3.0-rc.3）。使用方法与场景示例请见 [用户指南](USER_GUIDE.md)。

[🏠 首页](../README.md) • [📖 用户指南](USER_GUIDE.md) • [❓ 常见问题](FAQ.md)

---

## 📋 目录

<details open>
<summary>📑 目录</summary>

- [🧭 概述](#-概述)
- [🚦 限流器](#-限流器)
  - [Limiter trait](#limiter-trait)
  - [TokenBucketLimiter](#tokenbucketlimiter)
  - [GcraLimiter](#gcralimiter)
  - [其他限流算法](#其他限流算法)
  - [已弃用导出](#已弃用导出)
- [🚪 封禁管理](#-封禁管理)
  - [BanManager](#banmanager)
  - [BanTarget 与 BanSource](#bantarget-与-bansource)
  - [BanFileLoader](#banfileloader)
- [📊 配额控制](#-配额控制)
- [🔌 熔断器](#-熔断器)
- [🎛️ Governor](#️-governor)
- [🔍 匹配器](#-匹配器)
- [💾 存储后端](#-存储后端)
  - [MemoryStorage](#memorystorage)
  - [StorageFactory](#storagefactory)
- [🌐 Admin REST API](#-admin-rest-api)
  - [POST /api/v1/ban](#post-apiv1ban)
  - [DELETE /api/v1/ban/{target}](#delete-apiv1bantarget)
- [⚙️ 配置加载](#️-配置加载)
- [🚨 错误处理](#-错误处理)
- [📐 类型定义](#-类型定义)
- [💡 使用示例](#-使用示例)

</details>

---

## 🧭 概述

| 设计原则 | 说明 |
|---------|------|
| 简单 | 核心类型收敛在少数模块，`prelude` 一行导入常用类型 |
| 安全 | 类型安全，默认特性为空、零外部存储依赖 |
| 可组合 | `Limiter` trait 统一算法接口，决策链按优先级级联 |
| 文档完善 | 全部公开 API 带文档注释，docs.rs 在线可查 |

带特性门控的 API 在对应小节标注所需 feature。`default = []` 时仅核心限流可用。

---

## 🚦 限流器

限流器模块位于 `limiteron::limiters`，全部实现统一的 `Limiter` trait。

### Limiter trait

```rust
#[async_trait]
pub trait Limiter: Send + Sync {
    /// 消费指定成本，返回是否允许
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError>;

    /// 非消费预检：返回标准限流头数据，绝不修改限流器状态
    async fn peek(&self, cost: u64) -> Result<RateLimitSnapshot, LimiteronError>;

    /// 非消费查询剩余额度
    async fn remaining(&self) -> Result<RateLimitSnapshot, LimiteronError>;

    /// 检查是否允许（接受 key 参数，供宏生成代码使用）
    async fn check(&self, key: &str) -> Result<(), LimiteronError>;
}
```

`peek` / `remaining` 返回的 `RateLimitSnapshot` 对应 IETF `RateLimit-*` 头数据：

| 字段 | 类型 | 说明 |
|------|------|------|
| `limit` | `u64` | 窗口/桶容量上限（`RateLimit-Limit`） |
| `remaining` | `u64` | 当前剩余额度（`RateLimit-Remaining`） |
| `reset_secs` | `u64` | 距额度重置的秒数（`RateLimit-Reset`） |

---

### TokenBucketLimiter

令牌桶限流器。

#### `TokenBucketLimiter::new()`

```rust
pub fn new(capacity: u64, refill_rate: u64) -> Self
```

| 参数 | 类型 | 说明 |
|------|------|------|
| `capacity` | `u64` | 桶容量（最大令牌数） |
| `refill_rate` | `u64` | 每秒补充的令牌数 |

#### `TokenBucketLimiter::allow()`

```rust
pub async fn allow(&self, cost: u64) -> Result<bool, LimiteronError>
```

返回 `Ok(true)` 允许、`Ok(false)` 被限流；成本为 0 或超过 `MAX_COST` 时返回 `ConfigError`。

**示例：**

```rust
use limiteron::limiters::TokenBucketLimiter;

let limiter = TokenBucketLimiter::new(10, 1); // 10 个令牌，每秒补充 1 个

match limiter.allow(1).await {
    Ok(true) => println!("✅ 请求允许"),
    Ok(false) => println!("❌ 请求被限流"),
    Err(e) => println!("❌ 错误: {:?}", e),
}
```

---

### GcraLimiter

GCRA（Generic Cell Rate Algorithm）限流器，需要启用 `gcra` 特性。位于 `limiteron::limiters::GcraLimiter`。

#### `GcraLimiter::new()`

```rust
pub fn new(capacity: u64, refill_interval_us: u64) -> Self
```

| 参数 | 类型 | 说明 |
|------|------|------|
| `capacity` | `u64` | 桶容量（最大令牌数） |
| `refill_interval_us` | `u64` | 每个令牌的补充间隔（微秒） |

#### `GcraLimiter::with_rate()`

```rust
pub fn with_rate(capacity: u64, requests_per_second: u64) -> Self
```

| 参数 | 类型 | 说明 |
|------|------|------|
| `capacity` | `u64` | 桶容量（最大令牌数） |
| `requests_per_second` | `u64` | 每秒允许的请求数 |

#### `GcraLimiter::check()`

```rust
pub fn check(&self, cost: u64) -> GcraCheckResult
```

返回详细的同步检查结果：

| 字段 | 类型 | 说明 |
|------|------|------|
| `allowed` | `bool` | 是否允许 |
| `remaining` | `u64` | 剩余额度 |
| `retry_after_us` | `u64` | 被拒绝时建议等待的微秒数 |

**示例：**

```rust
use limiteron::limiters::GcraLimiter;

// 容量 10，每秒 100 个请求
let limiter = GcraLimiter::with_rate(10, 100);
let result = limiter.check(1);
if result.allowed {
    println!("✅ 允许，剩余: {}", result.remaining);
} else {
    println!("❌ 拒绝，需等待 {} 微秒", result.retry_after_us);
}
```

`GcraLimiter` 同样实现了 `Limiter` trait，可通过 `allow(cost).await` 异步消费。

---

### 其他限流算法

| 类型 | 构造 | 所需特性 | 说明 |
|------|------|---------|------|
| `FixedWindowLimiter` | `new(window_size: Duration, max_requests: u64)` | 无 | 固定窗口计数 |
| `ShardedSlidingWindowLimiter` | `new(window_size: Duration, max_requests: u64)` | 无 | 分片滑动窗口，高并发友好 |
| `ConcurrencyLimiter` | `new(max_concurrent: u64)` | 无 | 并发许可控制 |
| `HierarchicalTokenBucket` | `new(root_capacity: u64, root_refill_rate: u64)` | 无 | HTB 分层令牌桶，`allow(class_path: &[&str], cost)` 按分类路径消费 |
| `AdaptiveConcurrencyLimiter` | `new(config: AdaptiveConcurrencyConfig)` | `adaptive-limiting` | AIMD 自适应并发，按延迟/错误率反馈调窗 |
| `QuotaLimiter` | 见 `limiters::quota_limiter` | `quota-control` | 配额型限流器，供宏与 `LimiterManager` 使用 |

### 已弃用导出

> 自 **v0.2.1** 起，`SlidingWindowLimiter` 不再通过 `limiteron::limiters` 平铺导出。推荐使用 `limiteron::limiters::ShardedSlidingWindowLimiter` 替代，提供更好的并发性能。
>
> 仍可通过全路径 `limiteron::limiters::sliding_window::SlidingWindowLimiter` 访问（模块标注 `#[allow(deprecated)]`），但不推荐新代码使用。

---

## 🚪 封禁管理

需要启用 `ban-manager` 特性。模块位于 `limiteron::ban`。

### BanManager

封禁管理器，提供封禁 CRUD、指数退避时长计算与自动解封任务。

#### `BanManager::new()` / `BanManager::builder()`

```rust
pub async fn new() -> Result<Self, LimiteronError>       // 默认内存存储
pub fn builder() -> BanManagerBuilder                     // 链式配置后调用 build().await
```

#### `BanManager::with_dependencies()`

```rust
pub async fn with_dependencies(
    storage: Arc<dyn BanStorage>,
    config: BanManagerConfig,
) -> Result<Self, LimiteronError>
```

| 参数 | 类型 | 说明 |
|------|------|------|
| `storage` | `Arc<dyn BanStorage>` | 封禁存储后端 |
| `config` | `BanManagerConfig` | 封禁管理器配置（退避参数、自动解封开关等） |

#### `BanManager::create_ban()`

```rust
pub async fn create_ban(
    &self,
    target: BanTarget,
    reason: String,
    source: BanSource,
    metadata: serde_json::Value,
    duration: Option<StdDuration>,
) -> Result<BanDetail, LimiteronError>
```

| 参数 | 类型 | 说明 |
|------|------|------|
| `target` | `BanTarget` | 封禁目标 |
| `reason` | `String` | 封禁原因 |
| `source` | `BanSource` | `BanSource::Auto` 或 `BanSource::Manual { operator }` |
| `metadata` | `serde_json::Value` | 附加元数据 |
| `duration` | `Option<StdDuration>` | 封禁时长；`None` 表示使用指数退避算法自动计算 |

#### `BanManager::is_banned()`

```rust
pub async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, LimiteronError>
```

返回 `Some(BanRecord)` 表示被封禁。`BanRecord` 包含 `target`、`ban_times`、`duration`、`banned_at`、`expires_at`、`is_manual`、`reason` 等字段。

**示例：**

```rust
use limiteron::ban::BanTarget;
use std::time::Duration;

let target = BanTarget::Ip("192.168.1.100".to_string());

// 创建封禁
ban_manager.create_ban(
    target.clone(),
    "恶意请求".to_string(),
    limiteron::ban::BanSource::Manual { operator: "admin".to_string() },
    serde_json::json!({}),
    Some(Duration::from_secs(3600)),
).await?;

// 查询封禁
if let Some(record) = ban_manager.is_banned(&target).await? {
    println!("已被封禁: {}，到期: {}", record.reason, record.expires_at);
}
```

**其余方法一览：**

| 方法 | 说明 |
|------|------|
| `read_ban(&target)` | 读取封禁详情（`BanDetail`） |
| `update_ban(...)` | 更新封禁记录 |
| `delete_ban(&target, unbanned_by: String)` | 解封（返回是否成功） |
| `list_bans(filter: BanFilter)` | 分页/过滤查询封禁列表 |
| `calculate_ban_duration(ban_times)` | 按退避算法计算封禁时长 |
| `get_config()` / `update_config()` | 读取与更新运行时配置 |
| `stop_auto_unban_task()` | 停止自动解封后台任务 |

### BanTarget 与 BanSource

```rust
#[serde(tag = "type", content = "value")]
pub enum BanTarget {
    Ip(String),
    UserId(String),
    Mac(String),
    Geo { country_code: String }, // 大写 2 字母 ISO 3166-1 alpha-2
    Cidr(String),                 // IPv4/IPv6 网段，如 "10.0.0.0/8"
}

pub enum BanSource {
    Auto,
    Manual { operator: String },
}
```

serde 序列化格式：

| 变体 | type 字段 | value 格式 |
|------|----------|-----------|
| `Ip(String)` | `"ip"` | IP 字符串 |
| `UserId(String)` | `"user"` | 用户 ID |
| `Mac(String)` | `"mac"` | MAC 地址 |
| `Geo { country_code }` | `"geo"` | `{"country_code":"CN"}` |
| `Cidr(String)` | `"cidr"` | 网段字符串 |

> **查询语义**：查询目标为 `Ip` 且精确未命中时，按最长前缀匹配网段封禁记录（`BanTarget::contains_ip` / `prefix_len`）。

### BanFileLoader

从 YAML 文件批量加载封禁规则到 `BanManager`，可选支持文件变更热重载。需要 `ban-manager` 特性；热重载需要额外启用 `config-watcher` 特性。

```rust
pub struct BanFileLoader {
    path: PathBuf,
    // config-watcher 特性下另有监听任务句柄
}
```

**YAML 文件格式：**

```yaml
bans:
  - target:
      type: ip              # ip | user | mac | geo | cidr
      value: "192.168.1.1"  # geo 时为 {country_code: "CN"}
    reason: "恶意请求"
    duration_secs: 3600     # 可选，null/省略 = 使用退避算法
```

| 方法 | 签名 | 说明 |
|------|------|------|
| `new` | `pub fn new(path: impl Into<PathBuf>) -> Self` | 创建加载器 |
| `load_once` | `pub async fn load_once(&self, manager: &BanManager) -> Result<LoadResult, LimiteronError>` | 一次性加载；单条失败不中断整体，失败详情在 `LoadResult.errors`；文件读取/解析失败才返回 `Err` |
| `start_watching` | `pub async fn start_watching(&self, manager: BanManager) -> Result<(), LimiteronError>` | 启动热重载（500ms debounce 防止 DoS），需 `config-watcher` |
| `stop_watching` | `pub async fn stop_watching(&self)` | 停止监听；`Drop` 时自动调用 |

```rust
pub struct LoadResult {
    pub success_count: usize,
    pub failure_count: usize,
    pub errors: Vec<BanLoadError>, // 每项含 target_desc 与 error
}
```

> **安全**：内置 YAML 炸弹防护，文件大小上限 2MB，超限返回 `ConfigError`。

---

## 📊 配额控制

需要启用 `quota-control` 特性。模块位于 `limiteron::quota`。

### QuotaController

#### `QuotaController::builder()`

```rust
pub fn builder() -> QuotaControllerBuilder
```

`QuotaControllerBuilder` 提供以下方法：

| 方法 | 说明 |
|------|------|
| `with_storage(storage: Arc<dyn QuotaStorage>)` | 设置配额存储后端 |
| `with_config(config: QuotaConfig)` | 设置配额配置 |
| `build()` | 构建并返回 `Result<QuotaController, LimiteronError>` |

#### `QuotaController::with_dependencies()`

```rust
pub fn with_dependencies(storage: Arc<dyn QuotaStorage>, config: QuotaConfig) -> Self
```

> **注意**：不存在 `new(limit, window_size)` 参数化构造；`QuotaController::new()` 为零参数默认构造（默认内存存储与默认配置）。

#### `QuotaController::consume()`

```rust
pub async fn consume(
    &self,
    user_id: &str,
    resource: &str,
    cost: u64,
) -> Result<ConsumeResult, LimiteronError>
```

返回的 `ConsumeResult`：

| 字段 | 类型 | 说明 |
|------|------|------|
| `allowed` | `bool` | 是否允许继续消费 |
| `remaining` | `u64` | 剩余配额 |
| `alert_triggered` | `bool` | 是否触发告警（基于使用率阈值） |
| `usage_percent` | `f64` | 当前使用率百分比（0-100） |

> **注意**：`user_id` / `resource` 不得包含 `:`（存储 key 以 `{user_id}:{resource}` 拼接，含 `:` 会触发防碰撞 `ValidationError`）。

#### `QuotaConfig`

```rust
pub struct QuotaConfig {
    pub quota_type: QuotaType,          // 配额类型（默认 Count）
    pub limit: u64,                     // 配额上限
    pub window_size: u64,               // 窗口大小（秒）
    pub allow_overdraft: bool,          // 是否允许透支
    pub overdraft_limit_percent: u8,    // 透支上限（配额的百分比 0-100）
    pub alert_config: AlertConfig,      // 告警配置
}
```

**示例：**

```rust
use limiteron::quota::{QuotaConfig, QuotaController};

let config = QuotaConfig {
    limit: 10000,
    window_size: 60,
    ..Default::default()
};
let quota = QuotaController::builder().with_config(config).build()?;

let result = quota.consume("user123", "api_call", 1).await?;
println!("剩余 {}，使用率 {:.1}%", result.remaining, result.usage_percent);
```

---

## 🔌 熔断器

需要启用 `circuit-breaker` 特性。模块位于 `limiteron::circuit`。

### CircuitBreaker

```rust
pub fn new(config: CircuitBreakerConfig) -> Self
```

`CircuitBreaker` 同时实现 `Default`（默认配置）。

> **注意**：不存在 `new(failure_threshold, timeout_secs)` 参数化签名，也不存在 `with_config()` 方法。自定义配置请构造 `CircuitBreakerConfig` 后传入 `new()`，或使用 `CircuitBreaker::builder()`。

#### `CircuitBreakerConfig`

```rust
pub struct CircuitBreakerConfig {
    pub failure_threshold: u64,          // 失败阈值（默认 5）
    pub success_threshold: u64,          // 半开状态恢复所需成功数（默认 3）
    pub timeout: Duration,               // 打开状态等待时长（默认 30 秒）
    pub half_open_max_calls: u64,        // 半开状态最大探测调用数（默认 3）
    pub slow_call_duration_threshold: Duration, // 慢调用时长阈值（默认 500ms）
    pub slow_call_rate_threshold: f64,   // 慢调用率阈值（默认 0.5）
    pub error_classifier: Arc<dyn ErrorClassifier>, // 错误分类器
}
```

**常用方法：**

| 方法 | 说明 |
|------|------|
| `execute(op).await` | 执行操作，自动处理熔断逻辑 |
| `get_state().await` | 查询当前状态（`CircuitState`：Closed / Open / HalfOpen） |
| `config()` | 读取生效配置 |

**示例：**

```rust
use limiteron::circuit::{CircuitBreaker, CircuitBreakerConfig};

// 默认配置
let breaker = CircuitBreaker::new(CircuitBreakerConfig::default());
// 等价于 CircuitBreaker::default()

let state = breaker.get_state().await;
println!("当前状态: {:?}", state);
```

---

## 🎛️ Governor

主控制器，提供端到端的流量控制：标识符提取、规则匹配、决策链级联执行、负缓存与统计。模块位于 `limiteron::governor`。

### 构造方式

```rust
// 零参数构造：默认内存存储，开箱即用
pub async fn new() -> Result<Self, LimiteronError>

// 完整参数构造（按特性追加 metrics/tracer 参数）
pub async fn with_storage(
    config: FlowControlConfig,
    storage: Arc<dyn Storage>,
    ban_storage: Arc<dyn BanStorage>,
    #[cfg(feature = "monitoring")] metrics: Option<Arc<Metrics>>,
    #[cfg(feature = "telemetry")] tracer: Option<Arc<Tracer>>,
) -> Result<Self, LimiteronError>

// builder 模式（推荐）
pub fn builder() -> GovernorBuilder
```

`GovernorBuilder` 链式方法：

| 方法 | 说明 |
|------|------|
| `with_config(config: FlowControlConfig)` | 注入配置 |
| `with_storage(storage: Arc<dyn Storage>)` | 注入存储后端 |
| `with_ban_storage(ban_storage: Arc<dyn BanStorage>)` | 注入封禁存储 |
| `with_metrics(metrics: Arc<Metrics>)` | 注入指标收集器（`monitoring` 特性） |
| `with_tracer(tracer: Arc<Tracer>)` | 注入追踪器（`telemetry` 特性） |
| `with_l1_cache_enabled(enabled: bool)` | 开关 L1 负缓存 |
| `with_l1_cache_config(config: L1CacheConfig)` | 自定义 L1 负缓存（TTL 与容量） |
| `build().await` | 构建 Governor |

**示例：**

```rust
use limiteron::Governor;
use limiteron::adapters::StorageFactory;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    factory.initialize(None).await?;
    let storage = factory.create_storage().await?;
    let ban_storage = factory.create_ban_storage().await?;

    let governor = Governor::builder()
        .with_storage(storage)
        .with_ban_storage(ban_storage)
        .build()
        .await?;
    Ok(())
}
```

### `Governor::check()`

```rust
pub async fn check(&self, context: &RequestContext) -> Result<Decision, LimiteronError>
```

| 参数 | 类型 | 说明 |
|------|------|------|
| `context` | `&RequestContext` | 请求上下文（位于 `limiteron::matchers`） |

返回 `Decision`：`Allowed(RateLimitMetadata)` / `Rejected(RejectionMetadata)` / `Banned(BanInfo)`。

### 生命周期与健康检测

| 方法 | 签名 | 说明 |
|------|------|------|
| `shutdown` | `pub async fn shutdown(&self) -> Result<(), LimiteronError>` | 触发优雅关闭，停止后台任务（配额分配、封禁清理等） |
| `shutdown_token` | `pub fn shutdown_token(&self) -> &tokio_util::sync::CancellationToken` | 获取关闭令牌引用，供异步任务监听关闭信号 |
| `is_shutdown` | `pub fn is_shutdown(&self) -> bool` | 是否已关闭 |
| `health_check` | `pub async fn health_check(&self) -> Result<(), LimiteronError>` | 执行真实健康检测（存储、封禁存储等依赖） |
| `health_status` | `pub async fn health_status(&self) -> HealthStatus` | 读取最近一次健康检测的状态快照 |

```rust
pub struct HealthStatus {
    pub storage_healthy: bool,
    pub ban_storage_healthy: bool,
    pub cache_healthy: bool,        // L1 缓存
    pub background_tasks_alive: bool,
}
```

**示例：**

```rust
use tokio_util::sync::CancellationToken;

// shutdown_token() 返回引用，需 clone 后再 move 到异步任务
let token = governor.shutdown_token().clone();
tokio::spawn(async move {
    token.cancelled().await;
    println!("Governor 正在关闭");
});

// 健康检测
governor.health_check().await?;
let status = governor.health_status().await;
if !status.storage_healthy {
    println!("⚠️ 存储不可用");
}
```

---

## 🔍 匹配器

模块位于 `limiteron::matchers`，负责标识符提取与规则匹配。

### Identifier

```rust
pub enum Identifier {
    UserId(String),
    Ip(String),
    Mac(String),
    ApiKey(String),
    DeviceId(String),
}
```

### RequestContext

请求上下文，字段公开可直接构造，也可经链式方法构建：

```rust
pub struct RequestContext {
    pub user_id: Option<String>,
    pub ip: Option<String>,
    pub mac: Option<String>,
    pub device_id: Option<String>,
    pub api_key: Option<String>,
    pub headers: HashMap<String, String>,
    pub path: String,
    pub method: String,
    pub client_ip: Option<String>,
    pub query_params: HashMap<String, String>,
}

impl RequestContext {
    pub fn new() -> Self;
    pub fn with_header(mut self, key: &str, value: &str) -> Self;
    pub fn with_client_ip(mut self, ip: &str) -> Self;
    pub fn with_query_param(mut self, key: &str, value: &str) -> Self;
    pub fn with_path(mut self, path: &str) -> Self;
    pub fn with_method(mut self, method: &str) -> Self;
    pub fn get_header(&self, key: &str) -> Option<&String>;
}
```

### 提取器

内置标识符提取器统一实现 `IdentifierExtractor` trait：

| 提取器 | 构造 | 说明 |
|--------|------|------|
| `UserIdExtractor` | `new(header_name, query_param_name, default_user_id)` / `from_header(name)` / `builder()` | 从 HTTP 头或查询参数提取用户 ID |
| `IpExtractor` | `new(header_names: Vec<String>, validate: bool)` / `builder()` | 按优先级从 HTTP 头列表提取 IP，可校验格式并配置可信代理 |
| `MacExtractor` | `new(...)` / `builder()` | MAC 地址提取 |
| `ApiKeyExtractor` | `from_header(name)` 等 | API Key 提取 |
| `DeviceIdExtractor` | `new(...)` / `builder()` | 设备 ID 提取 |
| `CompositeExtractor` | `new(extractors, fallback_to_default)` / `builder()` | 组合多个提取器，依序尝试 |

**示例：**

```rust
use limiteron::matchers::IpExtractor;

// 使用 new 方法创建
let extractor = IpExtractor::new(
    vec!["X-Forwarded-For".to_string(), "X-Real-IP".to_string()],
    true,
);

// 或使用 builder 模式
let extractor = IpExtractor::builder()
    .header_name("X-Forwarded-For")
    .header_name("X-Real-IP")
    .validate(true)
    .build();
```

### 规则匹配

| 类型 | 说明 |
|------|------|
| `RuleMatcher` | 规则匹配引擎（`new(rules: Vec<Rule>)`） |
| `Matcher` | 内置匹配条件（User / Ip 等，见 `limiteron::config::Matcher`） |
| `CustomMatcher` / `CustomMatcherRegistry` | 自定义匹配器 trait 与注册表 |
| `HeaderMatcher` / `TimeWindowMatcher` | 内置 Header 与时间窗匹配器 |
| `GeoMatcher`（`geo-matching`） | 地理位置条件匹配 |
| `DeviceMatcher`（`device-matching`） | User-Agent 解析与设备识别 |

---

## 💾 存储后端

模块位于 `limiteron::storage` 与 `limiteron::adapters`。

### 核心 Trait

| Trait | 职责 |
|-------|------|
| `Storage` | 限流数据存储（令牌桶、计数器等 KV 操作） |
| `BanStorage` | 封禁记录存储 |
| `QuotaStorage` | 配额数据存储 |

三者均以 `Arc<dyn Trait>` 形式注入 Governor 与各组件。

### MemoryStorage

内存存储实现，同时实现 `Storage` / `BanStorage` / `QuotaStorage`。始终可用（无需特性门控），适用于单实例开发、测试与快速原型。

```rust
pub fn new() -> Self
pub fn create_storage() -> Arc<dyn Storage>   // 便捷构造（替代已移除的 StorageCreate trait）
```

> **注意**：v0.2.1 移除了 `RedisStorage` 与 `redis-storage` 特性，缓存统一经 oxcache 管理（启用 `cache-storage` 特性接入 Redis 缓存后端）。

**示例：**

```rust
use limiteron::storage::MemoryStorage;
use limiteron::Governor;

let storage = MemoryStorage::create_storage();
let governor = Governor::builder()
    .with_storage(storage)
    .build()
    .await?;
```

### StorageFactory

经 dbnexus 创建持久化存储后端的工厂，需要 `postgres` / `sqlite` / `mysql` 之一（三者互斥）。

```rust
pub struct StorageFactory { /* ... */ }

impl StorageFactory {
    pub fn from_dsn(dsn: impl Into<String>) -> Self;
    pub async fn initialize(&mut self, config: Option<StorageFactoryConfig>) -> Result<(), StorageError>;
    pub async fn create_storage(&self) -> Result<Arc<dyn Storage>, StorageError>;
    pub async fn create_ban_storage(&self) -> Result<Arc<dyn BanStorage>, StorageError>;
    pub async fn create_quota_storage(&self) -> Result<Arc<dyn QuotaStorage>, StorageError>;
}
```

**示例：**

```rust
use limiteron::adapters::StorageFactory;

let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
factory.initialize(None).await?;
let storage = factory.create_storage().await?;
```

---

## 🌐 Admin REST API

启用 `admin-api` 特性后，Limiteron 提供 REST 端点管理封禁、配额与状态。除探针端点外，全部要求 `Authorization: Bearer <api_key>` 头部认证（恒定时间比较防止时序攻击；多 key 部署可配置 `api_key_operators` 映射与 admin/viewer 角色矩阵）。

**端点总览：**

| 方法与路径 | 说明 | 认证 |
|-----------|------|------|
| `GET /healthz` / `GET /readyz` | K8s 探针端点 | 免认证 |
| `GET /metrics` | Prometheus 指标 | 免认证 |
| `GET /api/v1/status` | 运行状态 | Bearer |
| `GET /api/v1/status/circuit-breaker` | 熔断器状态 | Bearer |
| `GET /api/v1/introspect` | 规则/决策链/配额/封禁/熔断自省 JSON | Bearer |
| `POST /api/v1/ban` | 创建封禁（需 `ban-manager`） | Bearer |
| `DELETE /api/v1/ban/{target}` | 解除封禁（需 `ban-manager`） | Bearer |
| `PUT /api/v1/quota/{tenant_id}` | 更新租户配额 | Bearer |
| `POST /api/v1/config` | 原子热更新配置 | Bearer |
| `POST /api/v1/check/batch` | 批量决策检查 | Bearer |
| `POST /api/v1/tokens/prefetch` | 批量令牌预取（`BatchTokenPrefetcher`） | Bearer |

> 管理端点自带按路径、按客户端分桶的限流自保护（分桶内存上限 `RATE_BUCKET_MAX_ENTRIES=10000`）。

### POST /api/v1/ban

创建封禁记录。支持 `ip` / `user` / `mac` / `geo` / `cidr` 五种 target 类型。需要 `ban-manager` 特性。

**请求体：**

```rust
pub struct CreateBanRequest {
    pub target: BanTarget,           // serde: {"type":"...","value":...}
    pub reason: String,
    pub operator: Option<String>,    // 默认 "admin-api"
    pub duration_secs: Option<u64>,  // None = 退避算法自动计算
}
```

**响应状态码：**

| 状态码 | 含义 |
|--------|------|
| `201 Created` | 封禁创建成功，返回 `{id, ban_times, expires_at, is_manual}` |
| `400 Bad Request` | JSON 语法错误或 `ValidationError`（如无效 IP、小写国家码） |
| `401 Unauthorized` | 缺少或错误的 `Authorization` 头部 |
| `403 Forbidden` | `AuthorizationError`（授权拒绝） |
| `422 Unprocessable Entity` | JSON 合法但缺少必填字段（如 `reason`） |
| `503 Service Unavailable` | 未配置 ban_manager |
| `500 Internal Server Error` | 其他内部错误 |

**示例：**

```bash
# IP 封禁
curl -X POST http://localhost:8080/api/v1/ban \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{"target":{"type":"ip","value":"192.168.1.100"},"reason":"恶意请求"}'

# Geo 地区封禁
curl -X POST http://localhost:8080/api/v1/ban \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{"target":{"type":"geo","value":{"country_code":"CN"}},"reason":"地区封禁","duration_secs":3600}'
```

**成功响应：**

```json
{
  "success": true,
  "message": "OK",
  "data": {
    "id": "ban-uuid",
    "ban_times": 1,
    "expires_at": 1783290000,
    "is_manual": true
  }
}
```

### DELETE /api/v1/ban/{target}

解除封禁。通过 `?type=` query 参数显式指定目标类型，未提供时自动推断（合法 IP → `ip`，否则 → `user`）。需要 `ban-manager` 特性。

**Query 参数：**

| 参数 | 值 | 说明 |
|------|-----|------|
| `type` | `ip` / `user` / `mac` / `geo` / `cidr` | 显式指定目标类型，未提供时自动推断 |

**请求体（可选）：**

```rust
pub struct UnbanRequest {
    pub reason: Option<String>,
    pub operator: Option<String>,
}
```

**响应状态码：**

| 状态码 | 含义 |
|--------|------|
| `200 OK` | 解封成功 |
| `400 Bad Request` | 不支持的 `type` 值 |
| `401 Unauthorized` | 缺少或错误的 `Authorization` 头部 |
| `404 Not Found` | 目标未被封禁 |
| `503 Service Unavailable` | 未配置 ban_manager |
| `500 Internal Server Error` | 其他内部错误 |

**示例：**

```bash
# 解封 Geo 目标（必须显式指定 type=geo）
curl -X DELETE "http://localhost:8080/api/v1/ban/CN?type=geo" \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{"reason":"解封","operator":"admin-alice"}'

# 解封 IP（type 可省略，自动推断）
curl -X DELETE "http://localhost:8080/api/v1/ban/192.168.1.100" \
  -H "Authorization: Bearer your-api-key"

# 解封 MAC（必须显式指定 type=mac）
curl -X DELETE "http://localhost:8080/api/v1/ban/00:1a:2b:3c:4d:5e?type=mac" \
  -H "Authorization: Bearer your-api-key"
```

---

## ⚙️ 配置加载

模块位于 `limiteron::config`。

### `ConfigLoader::load_from_file()`

```rust
pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<FlowControlConfig, LimiteronError>
```

从 TOML 配置文件加载配置。

### `ConfigLoader::load_from_file_with_env()`

```rust
pub fn load_from_file_with_env<P: AsRef<Path>>(path: P) -> Result<FlowControlConfig, LimiteronError>
```

从 TOML 配置文件加载配置，并支持环境变量覆盖（前缀 `LIMITERON_`）。

| 环境变量 | 覆盖配置项 | 说明 |
|---------|-----------|------|
| `LIMITERON_GLOBAL_STORAGE` | `global.storage` | 存储类型：`memory` / `postgres` |
| `LIMITERON_GLOBAL_CACHE` | `global.cache` | 缓存类型：`memory` / `redis`（经 oxcache） |
| `LIMITERON_GLOBAL_METRICS` | `global.metrics` | 指标类型：`prometheus` / `none` |

**示例：**

```rust
use limiteron::ConfigLoader;

// 先设置环境变量覆盖
std::env::set_var("LIMITERON_GLOBAL_STORAGE", "postgres");

// 加载配置（环境变量覆盖配置文件中的同名项）
let config = ConfigLoader::load_from_file_with_env("config.toml")?;
```

### 程序化构建

`limiteron::config::ConfigBuilder` 提供程序化构建（`with_storage(StorageType)` / `with_cache(CacheBackend)` / `with_metrics(MetricsBackend)` / `with_trusted_proxies(TrustedProxyConfig)` / `with_rule(closure)` / `build()`）。注意 `build()` 要求至少一条规则，且每条规则必须包含至少一个匹配器与一个限流器，校验失败返回 `Err(String)`。规则构建器 `RuleBuilder` 提供 `id` / `name` / `priority` / `user_matcher` / `ip_matcher` / `token_bucket` / `fixed_window` / `sliding_window` / `concurrency_limit` 等方法。

---

## 🚨 错误处理

### `LimiteronError` 枚举

```rust
pub enum LimiteronError {
    ConfigError(String),
    StorageError(#[from] StorageError),
    LimitError(String),
    BanError(String),
    CircuitBreakerError(String),
    FallbackError(String),
    AuditLogError(String),
    AuthorizationError(String),
    IoError(#[from] std::io::Error),
    SerdeError(#[from] serde_json::Error),
    YamlError(#[from] serde_yaml_ng::Error),
    RateLimitExceeded(String),
    QuotaExceeded(String),
    ConcurrencyLimitExceeded(String),
    Throttled(String),           // 宏 throttle 排队超时
    ValidationError(String),
    LockError(String),
    TimeError(String),
    DependencyError(String),
    Other(String),
}
```

类型别名 `pub type LimiteronResult<T> = std::result::Result<T, LimiteronError>;`

### 错误处理模式

```rust
// 模式匹配：区分限流、封禁与其他错误
match limiter.allow(1).await {
    Ok(true) => println!("✅ 请求允许"),
    Ok(false) => println!("❌ 请求被限流"),
    Err(LimiteronError::LimitError(msg)) => {
        eprintln!("❌ 限流错误: {}", msg);
    }
    Err(LimiteronError::BanError(msg)) => {
        eprintln!("❌ 封禁错误: {}", msg);
    }
    Err(e) => {
        eprintln!("❌ 错误: {:?}", e);
    }
}
```

```rust
// ? 操作符：向上传播
async fn process_request() -> Result<(), LimiteronError> {
    let limiter = TokenBucketLimiter::new(10, 1);
    limiter.allow(1).await?;
    Ok(())
}
```

---

## 📐 类型定义

### Decision

决策结果，位于 `limiteron::error`：

```rust
pub enum Decision {
    Allowed(RateLimitMetadata),
    Rejected(RejectionMetadata),
    Banned(BanInfo),
}
```

| 携带类型 | 字段 | 说明 |
|---------|------|------|
| `RateLimitMetadata` | `limit` / `remaining` / `reset_at` / `retry_after: Option<u64>` / `policy` | 允许决策的限流元数据 |
| `RejectionMetadata` | `reason` / `retry_after` / `limit` / `reset_at` | 拒绝决策的详细信息 |
| `BanInfo` | `reason()` / `banned_until()` / `ban_times()` | 封禁信息（经访问器读取） |

### FlowControlConfig

```rust
pub struct FlowControlConfig {
    pub version: String,
    pub global: GlobalConfig,
    pub rules: Vec<Rule>,
}

pub struct GlobalConfig {
    pub storage: StorageType,                 // Memory / PostgreSQL / Redis
    pub cache: CacheBackend,                  // Memory / Redis / None
    pub metrics: MetricsBackend,              // Prometheus / Statsd / None
    pub trusted_proxies: TrustedProxyConfig,  // 可信代理配置
}

pub struct TrustedProxyConfig {
    pub enabled: bool,          // 是否启用可信代理模式
    pub proxies: Vec<String>,   // 可信代理 IP 列表（支持 CIDR）
    // 另有 X-Forwarded-For 最大跳数限制（默认 10）
}
```

### 其他常用类型

| 类型 | 说明 |
|------|------|
| `LimiteronResult<T>` | `Result<T, LimiteronError>` 别名 |
| `RateLimitSnapshot` | `peek` / `remaining` 返回的标准限流头数据 |
| `ConsumeResult` | 配额消费结果（allowed / remaining / alert_triggered / usage_percent） |
| `LoadResult` | BanFileLoader 加载结果 |
| `CircuitState` | 熔断器状态：Closed / Open / HalfOpen |
| `ChainStats` | 决策链统计（总检查数、节点允许/拒绝数） |
| `HealthStatus` | Governor 健康状态快照 |
| `Identifier` | 标识符枚举（UserId / Ip / Mac / ApiKey / DeviceId） |

---

## 💡 使用示例

### 示例 1：基础限流

```rust
use limiteron::limiters::{Limiter, TokenBucketLimiter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limiter = TokenBucketLimiter::new(10, 1);

    for i in 0..15 {
        match limiter.allow(1).await {
            Ok(true) => println!("请求 {} ✅", i),
            Ok(false) => println!("请求 {} ❌", i),
            Err(e) => println!("请求 {} 错误: {:?}", i, e),
        }
    }
    Ok(())
}
```

### 示例 2：封禁管理

```rust
use limiteron::adapters::StorageFactory;
use limiteron::ban::{BanManager, BanManagerConfig, BanSource, BanTarget};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    factory.initialize(None).await?;
    let ban_storage = factory.create_ban_storage().await?;
    let ban_manager = BanManager::with_dependencies(ban_storage, BanManagerConfig::default()).await?;

    let ip_target = BanTarget::Ip("192.168.1.100".to_string());
    ban_manager.create_ban(
        ip_target.clone(),
        "恶意请求".to_string(),
        BanSource::Manual { operator: "admin".to_string() },
        serde_json::json!({"severity": "high"}),
        Some(Duration::from_secs(3600)),
    ).await?;

    if let Some(record) = ban_manager.is_banned(&ip_target).await? {
        println!("❌ IP 已被封禁: {}", record.reason);
        println!("到期时间: {}", record.expires_at);
    }
    Ok(())
}
```

### 示例 3：Governor 决策

```rust
use limiteron::adapters::StorageFactory;
use limiteron::error::Decision;
use limiteron::matchers::RequestContext;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    factory.initialize(None).await?;
    let storage = factory.create_storage().await?;
    let ban_storage = factory.create_ban_storage().await?;

    let governor = limiteron::Governor::builder()
        .with_storage(storage)
        .with_ban_storage(ban_storage)
        .build()
        .await?;

    let context = RequestContext::new()
        .with_header("X-User-Id", "user123")
        .with_path("/api/v1/users")
        .with_method("GET");

    let decision = governor.check(&context).await?;
    match decision {
        Decision::Allowed(meta) => println!("✅ 请求允许，剩余 {}", meta.remaining),
        Decision::Rejected(meta) => println!("❌ 请求被拒绝: {}", meta.reason),
        Decision::Banned(info) => println!("❌ 请求被封禁: {}", info.reason()),
    }
    Ok(())
}
```

### 示例 4：声明式宏

```rust
use limiteron::flow_control;

#[flow_control(rate = "100/s", quota = "10000/m", concurrency = 50)]
async fn api_handler(user_id: &str) -> Result<String, limiteron::error::LimiteronError> {
    Ok(format!("处理用户 {} 的请求", user_id))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = api_handler("user123").await?;
    println!("{}", result);
    Ok(())
}
```

更多可运行示例见 [examples/](../examples/)（21 个示例覆盖全部核心能力）。

---

[🏠 返回首页](../README.md) • [📖 用户指南](USER_GUIDE.md) • [❓ 常见问题](FAQ.md)
