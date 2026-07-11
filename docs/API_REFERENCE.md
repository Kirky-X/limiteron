<div align="center">

# 📘 API 参考

### 完整 API 文档

[🏠 首页](../README.md) • [📖 用户指南](USER_GUIDE.md) • [❓ 常见问题](FAQ.md)

---

</div>

## 📋 目录

- [概述](#概述)
- [核心 API](#核心-api)
  - [限流器](#限流器)
  - [封禁管理](#封禁管理)
  - [配额控制](#配额控制)
  - [熔断器](#熔断器)
  - [Governor](#governor)
- [匹配器](#匹配器)
- [存储后端](#存储后端)
  - [MemoryStorage](#memorystorage)
- [文件封禁加载](#文件封禁加载)
  - [BanFileLoader](#banfileloader)
- [Admin REST API](#admin-rest-api)
  - [POST /api/v1/ban](#post-apiv1ban)
  - [DELETE /api/v1/ban/{target}](#delete-apiv1bantarget)
- [配置加载](#配置加载)
  - [ConfigLoader](#configloaderload_from_file_with_env)
- [错误处理](#错误处理)
- [类型定义](#类型定义)
- [示例](#示例)

---

## 概述

<div align="center">

### 🎯 API 设计原则

</div>

<table>
<tr>
<td width="25%" align="center">
<img src="https://img.icons8.com/fluency/96/000000/easy.png" width="64"><br>
<b>简单</b><br>
直观易用
</td>
<td width="25%" align="center">
<img src="https://img.icons8.com/fluency/96/000000/security-checked.png" width="64"><br>
<b>安全</b><br>
类型安全，默认安全
</td>
<td width="25%" align="center">
<img src="https://img.icons8.com/fluency/96/000000/module.png" width="64"><br>
<b>可组合</b><br>
轻松构建复杂工作流
</td>
<td width="25%" align="center">
<img src="https://img.icons8.com/fluency/96/000000/documentation.png" width="64"><br>
<b>文档完善</b><br>
全面的文档
</td>
</tr>
</table>

---

## 核心 API

### 限流器

<div align="center">

#### 🚀 限流器接口

</div>

---

#### `TokenBucketLimiter`

令牌桶限流器。

<table>
<tr>
<td width="30%"><b>类型</b></td>
<td width="70%">

```rust
pub struct TokenBucketLimiter {
    capacity: u64,
    refill_rate: u64,
    // 内部字段
}
```

</td>
</tr>
</table>

---

#### `TokenBucketLimiter::new()`

创建新的令牌桶限流器。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn new(capacity: u64, refill_rate: u64) -> Self
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `capacity: u64` - 桶容量（最大令牌数）
- `refill_rate: u64` - 每秒补充的令牌数

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Self</code> - 新的限流器实例</td>
</tr>
</table>

**示例:**

```rust
use limiteron::limiters::TokenBucketLimiter;

let limiter = TokenBucketLimiter::new(10, 1); // 10 个令牌，每秒补充 1 个
```

---

#### `TokenBucketLimiter::allow()`

检查是否允许通过指定成本。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn allow(&self, cost: u64) -> Result<bool, LimiteronError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `cost: u64` - 请求成本（通常为1）

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;bool, LimiteronError&gt;</code> - Ok(true) 表示允许，Ok(false) 表示被限流</td>
</tr>
<tr>
<td><b>错误</b></td>
<td>

- `LimiteronError::LimitError` - 限流错误
- `LimiteronError::ValidationError` - 成本验证错误

</td>
</tr>
</table>

**示例:**

```rust
let limiter = TokenBucketLimiter::new(10, 1);

match limiter.allow(1).await {
    Ok(true) => println!("✅ 请求允许"),
    Ok(false) => println!("❌ 请求被限流"),
    Err(e) => println!("❌ 错误: {:?}", e),
}
```

---

#### `GcraLimiter`

GCRA（Generic Cell Rate Algorithm）限流器。需要启用 `gcra` feature。

<table>
<tr>
<td width="30%"><b>类型</b></td>
<td width="70%">

```rust
pub struct GcraLimiter {
    // 内部字段
}
```

</td>
</tr>
</table>

---

#### `GcraLimiter::new()`

按容量与补充间隔创建新的 GCRA 限流器。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn new(capacity: u64, refill_interval_us: u64) -> Self
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `capacity: u64` - 桶容量（最大令牌数）
- `refill_interval_us: u64` - 每个令牌的补充间隔（微秒）

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Self</code> - 新的 GCRA 限流器</td>
</tr>
</table>

**示例:**

```rust
use limiteron::GcraLimiter;

// 容量 10，每 1_000_000 微秒（1 秒）补充 1 个令牌
let limiter = GcraLimiter::new(10, 1_000_000);
```

---

#### `GcraLimiter::with_rate()`

按容量与每秒请求数创建新的 GCRA 限流器。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn with_rate(capacity: u64, requests_per_second: u64) -> Self
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `capacity: u64` - 桶容量（最大令牌数）
- `requests_per_second: u64` - 每秒允许的请求数

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Self</code> - 新的 GCRA 限流器</td>
</tr>
</table>

**示例:**

```rust
use limiteron::GcraLimiter;

// 容量 10，每秒 100 个请求
let limiter = GcraLimiter::with_rate(10, 100);
```

---

#### `GcraLimiter::check()`

检查是否允许通过指定成本，返回详细检查结果。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn check(&self, cost: u64) -> GcraCheckResult
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `cost: u64` - 请求成本

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>GcraCheckResult</code> - 检查结果，包含是否允许及等待时长等信息</td>
</tr>
</tr>
</table>

**示例:**

```rust
use limiteron::GcraLimiter;

let limiter = GcraLimiter::with_rate(10, 100);
let result = limiter.check(1);
if result.allowed {
    println!("✅ 允许，剩余: {}", result.remaining);
} else {
    println!("❌ 拒绝，需等待: {:?}", result.retry_after);
}
```

---

#### `GcraLimiter::allow()`

检查是否允许通过指定成本。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn allow(&self, cost: u64) -> Result<bool, LimiteronError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `cost: u64` - 请求成本（通常为 1）

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;bool, LimiteronError&gt;</code> - Ok(true) 表示允许，Ok(false) 表示被限流</td>
</tr>
</table>

**示例:**

```rust
use limiteron::GcraLimiter;

let limiter = GcraLimiter::with_rate(10, 100);
match limiter.allow(1).await {
    Ok(true) => println!("✅ 请求允许"),
    Ok(false) => println!("❌ 请求被限流"),
    Err(e) => println!("❌ 错误: {:?}", e),
}
```

---

#### ⚠️ 已弃用：`SlidingWindowLimiter`

> **v0.2.1 起**，`SlidingWindowLimiter` 不再通过 `limiteron::` 顶层导出。推荐使用 [`ShardedSlidingWindowLimiter`](#)（`limiteron::limiters::ShardedSlidingWindowLimiter`）替代，提供更好的并发性能。
>
> 仍可通过全路径 `limiteron::limiters::sliding_window::SlidingWindowLimiter` 访问（模块标注 `#[allow(deprecated)]`），但不推荐新代码使用。

---

### 封禁管理

<div align="center">

#### 🔐 封禁管理器

</div>

---

#### `BanManager`

封禁管理器，用于管理 IP 和用户封禁。

<table>
<tr>
<td width="30%"><b>类型</b></td>
<td width="70%">

```rust
pub struct BanManager {
    // 内部字段
}
```

</td>
</tr>
</table>

---

#### `BanManager::with_dependencies()`

使用依赖注入创建新的封禁管理器。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn with_dependencies(
    storage: Arc<dyn BanStorage>,
    config: BanManagerConfig
) -> Result<Self, LimiteronError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `storage: Arc<dyn BanStorage>` - 封禁存储后端
- `config: BanManagerConfig` - 封禁管理器配置

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;BanManager, LimiteronError&gt;</code></td>
</tr>
</table>

**示例:**

```rust
use limiteron::ban::{BanManager, BanManagerConfig};
use limiteron::storage::BanStorage;
use std::sync::Arc;

let storage: Arc<dyn BanStorage> = Arc::new(my_storage);
let ban_manager = BanManager::with_dependencies(storage, BanManagerConfig::default()).await?;
```

---

#### `BanManager::create_ban()`

创建封禁记录。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

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

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `target: BanTarget` - 封禁目标（`Ip` / `UserId` / `Mac` / `Geo { country_code }`）
- `reason: String` - 封禁原因
- `source: BanSource` - 封禁来源（`BanSource::Auto` 或 `BanSource::Manual { operator }`）
- `metadata: serde_json::Value` - 附加元数据
- `duration: Option<StdDuration>` - 封禁时长，None表示使用指数退避算法自动计算

> **`BanTarget` 变体**（v0.2.1 新增 `Geo`）：`Ip(String)` / `UserId(String)` / `Mac(String)` / `Geo { country_code: String }`。`country_code` 必须是大写 2 字母 ISO 3166-1 alpha-2 格式。

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;BanDetail, LimiteronError&gt;</code> - 封禁详情</td>
</tr>
</table>

**示例:**

```rust
use limiteron::ban::{BanTarget, BanSource};
use std::time::Duration;

// IP 封禁
let target = BanTarget::Ip("192.168.1.100".to_string());
let ban_detail = ban_manager.create_ban(
    target,
    "恶意请求".to_string(),
    BanSource::Manual { operator: "admin".to_string() },
    serde_json::json!({}),
    Some(Duration::from_secs(3600)),
).await?;

// Geo 地区封禁（v0.2.1+，country_code 必须大写 2 字母）
let geo_target = BanTarget::Geo { country_code: "CN".to_string() };
ban_manager.create_ban(
    geo_target,
    "地区封禁".to_string(),
    BanSource::Manual { operator: "admin".to_string() },
    serde_json::json!({}),
    None, // 使用退避算法自动计算时长
).await?;
```

---

#### `BanManager::is_banned()`

检查目标是否被封禁。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, LimiteronError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `target: &BanTarget` - 要检查的目标

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;Option&lt;BanRecord&gt;, LimiteronError&gt;</code> - Some表示被封禁，None表示未封禁</td>
</tr>
</table>

**示例:**

```rust
use limiteron::ban::BanTarget;

let user_target = BanTarget::UserId("user123".to_string());
if let Some(ban_record) = ban_manager.is_banned(&user_target).await? {
    println!("User is banned: {:?}", ban_record);
    println!("Reason: {}", ban_record.reason);
    println!("Expires at: {}", ban_record.expires_at);
    return Err("User is banned".into());
}
```

---

### 配额控制

<div align="center">

#### 📊 配额控制器

</div>

---

#### `QuotaController`

配额控制器，用于管理配额分配和消费。

<table>
<tr>
<td width="30%"><b>类型</b></td>
<td width="70%">

```rust
pub struct QuotaController {
    // 内部字段
}
```

</td>
</tr>
</table>

---

#### `QuotaController::builder()`

创建 QuotaControllerBuilder 用于链式配置。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn builder() -> QuotaControllerBuilder
```

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>QuotaControllerBuilder</code> - 用于链式配置的构建器</td>
</tr>
</table>

`QuotaControllerBuilder` 提供以下方法：

| 方法 | 说明 |
|------|------|
| `with_storage(storage: Arc<dyn QuotaStorage>)` | 设置配额存储后端 |
| `with_config(config: QuotaConfig)` | 设置配额配置 |
| `build()` | 构建并返回 `QuotaController` |

**示例:**

```rust
use limiteron::quota::{QuotaController, QuotaConfig};
use limiteron::storage::QuotaStorage;
use std::sync::Arc;

let config = QuotaConfig {
    limit: 10000,
    window_secs: 60,
    ..Default::default()
};

let quota = QuotaController::builder()
    .with_storage(storage)
    .with_config(config)
    .build();
```

---

#### `QuotaController::with_dependencies()`

使用完整依赖注入创建新的配额控制器。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn with_dependencies(
    storage: Arc<dyn QuotaStorage>,
    config: QuotaConfig,
) -> Self
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `storage: Arc<dyn QuotaStorage>` - 配额存储后端
- `config: QuotaConfig` - 配额配置

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Self</code> - 新的配额控制器</td>
</tr>
</table>

**示例:**

```rust
use limiteron::quota::{QuotaController, QuotaConfig};
use std::sync::Arc;

let config = QuotaConfig {
    limit: 10000,
    window_secs: 60,
    ..Default::default()
};
let quota = QuotaController::with_dependencies(storage, config);
```

> **注意**: 不存在 `new(limit, window_secs)` 方法，也不存在 `with_config()` 直接方法（`with_config()` 是 builder 的方法）。

---

#### `QuotaController::consume()`

消费配额。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn consume(
    &self,
    user_id: &str,
    resource: &str,
    cost: u64,
) -> Result<(), LimiteronError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `user_id: &str` - 用户标识
- `resource: &str` - 资源名称
- `cost: u64` - 消费成本

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;(), LimiteronError&gt;</code> - Ok(()) 表示消费成功，Err 表示超出配额或存储错误</td>
</tr>
</table>

**示例:**

```rust
quota.consume("user123", "api_call", 1).await?;
```

---

### 熔断器

<div align="center">

#### 🔌 熔断器

</div>

---

#### `CircuitBreaker`

熔断器，用于在系统故障时自动熔断。

<table>
<tr>
<td width="30%"><b>类型</b></td>
<td width="70%">

```rust
pub struct CircuitBreaker {
    failure_threshold: u32,
    timeout_secs: u64,
    // 内部字段
}
```

</td>
</tr>
</table>

---

#### `CircuitBreaker::new()`

创建新的熔断器。提供两种构造形式：无参数默认构造，或传入 `CircuitBreakerConfig` 进行自定义配置。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn new() -> Self
pub fn new(config: CircuitBreakerConfig) -> Self
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `config: CircuitBreakerConfig` - 熔断器配置（可选，无参数时使用默认配置）

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Self</code> - 新的熔断器</td>
</tr>
</table>

**示例:**

```rust
use limiteron::circuit::{CircuitBreaker, CircuitBreakerConfig};

// 使用默认配置
let breaker = CircuitBreaker::new();

// 或使用自定义配置
let breaker = CircuitBreaker::new(CircuitBreakerConfig::default());
```

> **注意**: 不存在 `new(failure_threshold, timeout_secs)` 签名，也不存在 `with_config()` 方法。如需自定义配置，请在 `new()` 中传入 `CircuitBreakerConfig`。

---

### Governor

<div align="center">

#### 🎛️ 主控制器

</div>

---

#### `Governor`

主控制器，提供端到端的流量控制。

<table>
<tr>
<td width="30%"><b>类型</b></td>
<td width="70%">

```rust
pub struct Governor {
    config: Arc<RwLock<FlowControlConfig>>,
    // 内部字段
}
```

</td>
</tr>
</table>

---

#### `Governor::builder()`

创建 GovernorBuilder 用于链式配置。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn builder() -> GovernorBuilder
```

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>GovernorBuilder</code> - 用于链式配置的构建器</td>
</tr>
</table>

**示例:**

```rust
use limiteron::Governor;
use limiteron::adapters::StorageFactory;
use std::sync::Arc;

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

---

#### `Governor::new()`

创建新的 Governor（推荐使用 `builder()` 方法）。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn new(
    config: FlowControlConfig,
    storage: Arc<dyn Storage>,
    ban_storage: Arc<dyn BanStorage>,
    #[cfg(feature = "monitoring")] metrics: Option<Arc<Metrics>>,
    #[cfg(feature = "telemetry")] tracer: Option<Arc<Tracer>>,
) -> Result<Self, LimiteronError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `config: FlowControlConfig` - 流量控制配置
- `storage: Arc<dyn Storage>` - 存储后端
- `ban_storage: Arc<dyn BanStorage>` - 封禁存储后端
- `metrics: Option<Arc<Metrics>>` - 指标收集器（需要 `monitoring` 特性）
- `tracer: Option<Arc<Tracer>>` - 追踪器（需要 `telemetry` 特性）

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;Governor, LimiteronError&gt;</code></td>
</tr>
</table>

**示例:**

```rust
use limiteron::{Governor, FlowControlConfig};
use limiteron::adapters::StorageFactory;
use limiteron::storage::{Storage, BanStorage};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    factory.initialize(None).await?;
    let storage = factory.create_storage().await?;
    let ban_storage = factory.create_ban_storage().await?;

    let governor = Governor::new(
        FlowControlConfig::default(),
        storage,
        ban_storage,
        None,  // metrics
        None,  // tracer
    ).await?;
    Ok(())
}
```

---

#### `Governor::check()`

检查请求是否允许通过。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn check(&self, context: &RequestContext) -> Result<Decision, LimiteronError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `context: &RequestContext` - 请求上下文（位于 `limiteron::matchers` 模块）

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;Decision, LimiteronError&gt;</code> - 决策结果</td>
</tr>
</table>

**示例:**

```rust
use limiteron::matchers::RequestContext;

let context = RequestContext::builder()
    .identifier("user123")
    .path("/api/v1/users")
    .method("GET")
    .build();

let decision = governor.check(&context).await?;
match decision {
    Decision::Allowed(_) => println!("请求允许"),
    Decision::Rejected(reason) => println!("请求拒绝: {}", reason),
    Decision::Banned(info) => println!("请求被封禁: {}", info.reason()),
}
```

---

#### `Governor::shutdown()`

触发优雅关闭，停止 Governor 的所有后台任务（如配额分配、封禁清理等）。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn shutdown(&self) -> Result<(), LimiteronError>
```

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;(), LimiteronError&gt;</code> - 关闭结果</td>
</tr>
</table>

**示例:**

```rust
// 优雅关闭 Governor
governor.shutdown().await?;
```

---

#### `Governor::shutdown_token()`

获取关闭令牌的引用，可用于在异步任务中监听关闭信号。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn shutdown_token(&self) -> &tokio_util::sync::CancellationToken
```

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>&tokio_util::sync::CancellationToken</code> - 关闭令牌的引用</td>
</tr>
</table>

**示例:**

```rust
use tokio_util::sync::CancellationToken;

// shutdown_token() 返回引用，需 clone 后再 move 到异步任务
let token = governor.shutdown_token().clone();

tokio::spawn(async move {
    token.cancelled().await;
    println!("Governor 正在关闭...");
});
```

---

#### `Governor::is_shutdown()`

检查 Governor 是否已关闭。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn is_shutdown(&self) -> bool
```

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>bool</code> - true 表示已关闭</td>
</tr>
</table>

**示例:**

```rust
if governor.is_shutdown() {
    println!("Governor 已关闭");
}
```

---

#### `Governor::health_check()`

执行真实的健康检测，检查存储、封禁存储等关键依赖的可用性。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn health_check(&self) -> Result<(), LimiteronError>
```

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;(), LimiteronError&gt;</code> - Ok(()) 表示所有依赖健康，Err 表示检测失败</td>
</tr>
</table>

**示例:**

```rust
// 执行健康检测，失败时返回错误
governor.health_check().await?;
println!("✅ 所有依赖健康");

// 如需获取详细状态字段，使用 health_status()
let status = governor.health_status().await;
if !status.storage_healthy {
    println!("⚠️ 存储不可用");
}
```

---

#### `Governor::health_status()`

获取最近一次健康检测的状态（不触发新的检测）。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn health_status(&self) -> HealthStatus
```

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>HealthStatus</code> - 最近一次的健康状态</td>
</tr>
</table>

---

#### `HealthStatus`

健康状态结构体。

```rust
pub struct HealthStatus {
    pub storage_healthy: bool,
    pub ban_storage_healthy: bool,
    pub cache_healthy: bool,
    pub background_tasks_alive: bool,
}
```

---

## 匹配器

<div align="center">

#### 🔍 标识符提取器

</div>

---

#### `Identifier`

标识符类型。

<table>
<tr>
<td width="30%"><b>定义</b></td>
<td width="70%">

```rust
pub enum Identifier {
    UserId(String),
    Ip(String),
    Mac(String),
    ApiKey(String),
    DeviceId(String),
}
```

</td>
</tr>
</table>

---

#### `IpExtractor`

IP 地址提取器。

<table>
<tr>
<td width="30%"><b>类型</b></td>
<td width="70%">

```rust
pub struct IpExtractor {
    header_names: Vec<String>,
    validate: bool,
}
```

</td>
</tr>
</table>

---

#### `IpExtractor::new()`

创建新的 IP 提取器。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn new(header_names: Vec<String>, validate: bool) -> Self
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `header_names: Vec<String>` - HTTP 头名称列表（按优先级顺序）
- `validate: bool` - 是否验证 IP 格式

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Self</code> - 新的 IP 提取器</td>
</tr>
</table>

**示例:**

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

---

## 存储后端

<div align="center">

#### 💾 存储后端实现

</div>

---

#### `MemoryStorage`

内存存储后端，实现 `Storage`/`BanStorage`/`QuotaStorage` trait，适用于单实例开发、测试和快速原型。始终可用（无需 feature flag）。

<table>
<tr>
<td width="30%"><b>类型</b></td>
<td width="70%">

```rust
pub struct MemoryStorage {
    // 内部字段（HashMap + RwLock）
}
```

</td>
</tr>
</table>

> **注意**: v0.2.1 移除了 `RedisStorage` 与 `redis-storage` feature。所有缓存通过 oxcache 统一管理（启用 `cache-storage` feature 可接入 Redis 缓存后端）。`StorageCreate`/`BanStorageCreate` trait 也已移除，改用 `MemoryStorage::create_storage()` 固有方法。

---

#### `MemoryStorage::new()`

创建新的 MemoryStorage 实例。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn new() -> Self
```

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Self</code> - 新的内存存储实例</td>
</tr>
</table>

---

#### `MemoryStorage::create_storage()`

创建 `Arc<dyn Storage>` 的便捷方法（替代已移除的 `StorageCreate` trait）。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn create_storage() -> Arc<dyn Storage>
```

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Arc&lt;dyn Storage&gt;</code> - 装箱好的存储 trait 对象</td>
</tr>
</table>

**示例:**

```rust
use limiteron::storage::MemoryStorage;
use limiteron::Governor;

// 便捷构造
let storage = MemoryStorage::create_storage();
let governor = Governor::builder()
    .with_storage(storage)
    .build()
    .await?;
```

---

#### Trait 实现

`MemoryStorage` 实现以下 trait，可作为 Governor 和各组件的存储后端：

| Trait | 说明 |
|-------|------|
| `Storage` | 限流数据存储（令牌桶、计数器等） |
| `BanStorage` | 封禁记录存储 |
| `QuotaStorage` | 配额数据存储 |

---

## 文件封禁加载

<div align="center">

#### 📄 BanFileLoader

</div>

---

#### `BanFileLoader`

从 YAML 文件批量加载封禁规则到 `BanManager`，可选支持文件变更热重载。需要启用 `ban-manager` feature；热重载需要额外启用 `config-watcher` feature。

<table>
<tr>
<td width="30%"><b>类型</b></td>
<td width="70%">

```rust
pub struct BanFileLoader {
    path: PathBuf,
    #[cfg(feature = "config-watcher")]
    watch_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}
```

</td>
</tr>
</table>

**YAML 文件格式：**

```yaml
bans:
  - target:
      type: ip              # ip | user | mac | geo
      value: "192.168.1.1"  # geo 时为 {country_code: "CN"}
    reason: "恶意请求"
    duration_secs: 3600     # 可选，null/省略 = 使用退避算法
```

---

#### `BanFileLoader::new()`

创建新的文件加载器。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn new(path: impl Into<PathBuf>) -> Self
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `path: impl Into<PathBuf>` - YAML 文件路径

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Self</code> - 新的加载器实例</td>
</tr>
</table>

---

#### `BanFileLoader::load_once()`

一次性加载文件中的所有封禁规则到 BanManager。单条加载失败不会中断整体加载，失败详情记录在 `LoadResult.errors` 中。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn load_once(&self, manager: &BanManager) -> Result<LoadResult, LimiteronError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `manager: &BanManager` - 目标封禁管理器

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td>
<code>Result&lt;LoadResult, LimiteronError&gt;</code><br>
<code>Ok</code> = 加载完成（可能含部分失败）；<code>Err</code> = 文件读取或 YAML 解析失败
</td>
</tr>
<tr>
<td><b>安全</b></td>
<td>内置 YAML 炸弹防护：文件大小上限 2MB，超限返回 <code>ConfigError</code></td>
</tr>
</table>

**`LoadResult` 结构：**

```rust
pub struct LoadResult {
    pub success_count: usize,
    pub failure_count: usize,
    pub errors: Vec<BanLoadError>,
}
```

**示例:**

```rust
use limiteron::ban::{BanFileLoader, BanManager};

let ban_manager = BanManager::new().await?;
let loader = BanFileLoader::new("config/bans.yaml");
let result = loader.load_once(&ban_manager).await?;
println!("成功 {} 条，失败 {} 条", result.success_count, result.failure_count);
```

---

#### `BanFileLoader::start_watching()`

启动文件变更热重载，文件修改后自动重新加载（500ms debounce 防止 DoS）。需要 `config-watcher` feature。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn start_watching(&self, manager: BanManager) -> Result<(), LimiteronError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `manager: BanManager` - 目标封禁管理器（clone 传入，热重载时调用）

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;(), LimiteronError&gt;</code> - 启动结果</td>
</tr>
<tr>
<td><b>特性</b></td>
<td><code>config-watcher</code></td>
</tr>
</table>

---

#### `BanFileLoader::stop_watching()`

停止文件监听。`BanFileLoader` 的 `Drop` impl 也会自动调用此方法，防止任务泄漏。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn stop_watching(&self)
```

</td>
</tr>
</table>

**完整示例:**

```rust
use limiteron::ban::{BanFileLoader, BanManager};

let ban_manager = BanManager::new().await?;
let loader = BanFileLoader::new("config/bans.yaml");

// 首次加载
let result = loader.load_once(&ban_manager).await?;

// 启动热重载（需要 config-watcher feature）
loader.start_watching(ban_manager.clone()).await?;

// loader drop 时自动停止监听
```

---

## Admin REST API

<div align="center">

#### 🌐 HTTP 管理端点

</div>

启用 `admin-api` feature 后，Limiteron 提供 REST 端点管理封禁、配额和状态。所有端点要求 `Authorization: Bearer <api_key>` 头部认证（使用恒定时间比较防止时序攻击）。

---

#### `POST /api/v1/ban`

创建封禁记录。支持 `ip`/`user`/`mac`/`geo` 四种 target 类型。需要 `ban-manager` feature。

**请求体：**

```rust
pub struct CreateBanRequest {
    pub target: BanTarget,           // serde: {"type":"...","value":...}
    pub reason: String,
    pub operator: Option<String>,    // 默认 "admin-api"
    pub duration_secs: Option<u64>,  // None = 退避算法自动计算
}
```

**BanTarget serde 格式：**

| 类型 | type 字段 | value 格式 |
|------|----------|-----------|
| `Ip(String)` | `"ip"` | IP 字符串 |
| `UserId(String)` | `"user"` | 用户 ID |
| `Mac(String)` | `"mac"` | MAC 地址 |
| `Geo { country_code }` | `"geo"` | `{"country_code":"CN"}`（大写 2 字母 ISO 3166-1 alpha-2） |

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

**示例:**

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

---

#### `DELETE /api/v1/ban/{target}`

解除封禁。通过 `?type=` query 参数显式指定目标类型，未提供时按 IP 优先自动推断（合法 IP → `Ip`，否则 → `UserId`）。需要 `ban-manager` feature。

**路径参数：**

- `target` - 封禁目标标识（IP/用户 ID/MAC/国家代码）

**Query 参数：**

| 参数 | 值 | 说明 |
|------|-----|------|
| `type` | `ip` / `user` / `mac` / `geo` | 显式指定目标类型。未提供时自动推断（IP 优先，回退 UserId） |

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

**示例:**

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

## 配置加载

<div align="center">

#### ⚙️ ConfigLoader

</div>

---

#### `ConfigLoader::load_from_file()`

从 TOML 配置文件加载配置。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn load_from_file(path: &str) -> Result<FlowControlConfig, LimiteronError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `path: &str` - 配置文件路径

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;FlowControlConfig, LimiteronError&gt;</code></td>
</tr>
</table>

**示例:**

```rust
use limiteron::ConfigLoader;

let config = ConfigLoader::load_from_file("config.toml")?;
```

---

#### `ConfigLoader::load_from_file_with_env()`

从 TOML 配置文件加载配置，并支持环境变量覆盖。环境变量前缀为 `LIMITERON_`，支持覆盖全局配置项。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn load_from_file_with_env(path: &str) -> Result<FlowControlConfig, LimiteronError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `path: &str` - 配置文件路径

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;FlowControlConfig, LimiteronError&gt;</code></td>
</tr>
</table>

**支持的环境变量：**

| 环境变量 | 覆盖配置项 | 说明 |
|---------|-----------|------|
| `LIMITERON_GLOBAL_STORAGE` | `global.storage` | 存储类型：`memory` / `postgres` |
| `LIMITERON_GLOBAL_CACHE` | `global.cache` | 缓存类型：`memory` / `redis`（通过 oxcache） |
| `LIMITERON_GLOBAL_METRICS` | `global.metrics` | 指标类型：`prometheus` / `none` |

**示例:**

```rust
use limiteron::ConfigLoader;

// 先设置环境变量覆盖
std::env::set_var("LIMITERON_GLOBAL_STORAGE", "postgres");

// 加载配置（环境变量会覆盖配置文件中的值）
let config = ConfigLoader::load_from_file_with_env("config.toml")?;
// config.global.storage 现在为 "postgres"
```

---

## 错误处理

<div align="center">

#### 🚨 错误类型和处理

</div>

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
    IoError(#[from] std::io::Error),
    SerdeError(#[from] serde_json::Error),
    YamlError(#[from] serde_yaml::Error),
    RateLimitExceeded(String),
    QuotaExceeded(String),
    ConcurrencyLimitExceeded(String),
    ValidationError(String),
    LockError(String),
    Other(String),
}
```

### 错误处理模式

<table>
<tr>
<td width="50%">

**模式匹配**
```rust
match limiter.allow(1).await {
    Ok(true) => {
        println!("✅ 请求允许");
    }
    Ok(false) => {
        println!("❌ 请求被限流");
    }
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

</td>
<td width="50%">

**? 操作符**
```rust
async fn process_request() -> Result<(), LimiteronError> {
    let limiter = TokenBucketLimiter::new(10, 1);
    limiter.allow(1).await?;

    Ok(())
}
```

</td>
</tr>
</table>

---

## 类型定义

### 常用类型

<table>
<tr>
<td width="50%">

**决策类型**
```rust
pub enum Decision {
    Allowed,
    Denied(String),
}
```

**标识符类型**
```rust
pub enum Identifier {
    UserId(String),
    Ip(String),
    Mac(String),
    ApiKey(String),
    DeviceId(String),
}
```

</td>
<td width="50%">

**结果类型**
```rust
pub type Result<T> =
    std::result::Result<T, LimiteronError>;
```

**配置类型**
```rust
/// 流量控制配置
pub struct FlowControlConfig {
    pub version: String,
    pub global: GlobalConfig,
    pub rules: Vec<Rule>,
}

/// 全局配置
pub struct GlobalConfig {
    pub storage: StorageType,        // 存储后端类型
    pub cache: CacheBackend,        // 缓存后端类型
    pub metrics: MetricsBackend,     // 指标后端类型
    pub trusted_proxies: TrustedProxyConfig,  // 可信代理配置
}

/// 可信代理配置（用于安全提取客户端 IP）
pub struct TrustedProxyConfig {
    pub enabled: bool,              // 是否启用可信代理模式
    pub proxies: Vec<String>,        // 可信代理 IP 列表（支持 CIDR）
}
```

</td>
</tr>
</table>

---

## 示例

<div align="center">

### 💡 常见使用模式

</div>

### 示例 1: 基础限流

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

### 示例 2: 封禁管理

```rust
use limiteron::ban::{BanManager, BanManagerConfig, BanTarget, BanSource};
use limiteron::adapters::StorageFactory;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    factory.initialize(None).await?;
    let ban_storage = factory.create_ban_storage().await?;
    let ban_manager = BanManager::with_dependencies(ban_storage, BanManagerConfig::default()).await?;

    // 封禁 IP
    let ip_target = BanTarget::Ip("192.168.1.100".to_string());
    ban_manager.create_ban(
        ip_target.clone(),
        "恶意请求".to_string(),
        BanSource::Manual { operator: "admin".to_string() },
        serde_json::json!({"severity": "high"}),
        Some(Duration::from_secs(3600)),
    ).await?;

    // 检查是否被封禁
    if let Some(ban_detail) = ban_manager.is_banned(&ip_target).await? {
        println!("❌ IP 已被封禁: {}", ban_detail.reason);
        println!("到期时间: {}", ban_detail.expires_at);
    }

    Ok(())
}
```

### 示例 3: 使用 Governor

```rust
use limiteron::{Governor, FlowControlConfig, Decision};
use limiteron::matchers::RequestContext;
use limiteron::adapters::StorageFactory;
use std::sync::Arc;

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

    let context = RequestContext::builder()
        .identifier("user123")
        .path("/api/v1/users")
        .method("GET")
        .build();

    let decision = governor.check(&context).await?;
    match decision {
        Decision::Allowed(_) => {
            println!("✅ 请求允许");
            // 处理请求
        }
        Decision::Rejected(reason) => {
            println!("❌ 请求被拒绝: {}", reason);
        }
        Decision::Banned(info) => {
            println!("❌ 请求被封禁: {}", info.reason());
        }
    }

    Ok(())
}
```

### 示例 4: 使用宏

```rust
use limiteron::flow_control;

#[flow_control(rate = "100/s", quota = "10000/m", concurrency = 50)]
async fn api_handler(user_id: &str) -> Result<String, limiteron::error::LimiteronError> {
    // API 业务逻辑
    Ok(format!("处理用户 {} 的请求", user_id))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = api_handler("user123").await?;
    println!("{}", result);
    Ok(())
}
```

---

<div align="center">

**[📖 用户指南](USER_GUIDE.md)** • **[❓ 常见问题](FAQ.md)** • **[🏠 首页](../README.md)**

由文档团队制作

[⬆ 返回顶部](#-api-参考)

</div>
