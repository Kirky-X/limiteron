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
pub async fn allow(&self, cost: u64) -> Result<bool, FlowGuardError>
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
<td><code>Result&lt;bool, FlowGuardError&gt;</code> - Ok(true) 表示允许，Ok(false) 表示被限流</td>
</tr>
<tr>
<td><b>错误</b></td>
<td>

- `FlowGuardError::LimitError` - 限流错误
- `FlowGuardError::ValidationError` - 成本验证错误

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

#### `BanManager::new()`

创建新的封禁管理器。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn new(
    storage: Arc<dyn BanStorage>,
    config: Option<BanManagerConfig>
) -> Result<Self, FlowGuardError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `storage: Arc<dyn BanStorage>` - 封禁存储后端
- `config: Option<BanManagerConfig>` - 可选配置

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;BanManager, FlowGuardError&gt;</code></td>
</tr>
</table>

**示例:**

```rust
use limiteron::ban_manager::{BanManager, BanManagerConfig};
use limiteron::storage::MockBanStorage;
use std::sync::Arc;

let storage = Arc::new(MockBanStorage::default());
let ban_manager = BanManager::new(storage, None).await?;
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
    duration_secs: Option<u64>,
    source: Option<BanSource>
) -> Result<BanDetail, FlowGuardError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `target: BanTarget` - 封禁目标（IP、用户ID等）
- `reason: String` - 封禁原因
- `duration_secs: Option<u64>` - 封禁时长（秒），None表示永久
- `source: Option<BanSource>` - 封禁来源

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;BanDetail, FlowGuardError&gt;</code> - 封禁详情</td>
</tr>
</table>

**示例:**

```rust
use limiteron::ban_manager::{BanTarget, BanSource};

let target = BanTarget::Ip("192.168.1.100".to_string());
let ban_detail = ban_manager.create_ban(
    target,
    "恶意请求".to_string(),
    Some(3600),
    Some(BanSource::Manual)
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
pub async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, FlowGuardError>
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
<td><code>Result&lt;Option&lt;BanRecord&gt;, FlowGuardError&gt;</code> - Some表示被封禁，None表示未封禁</td>
</tr>
</table>

**示例:**

```rust
use limiteron::ban_manager::BanTarget;

let user_target = BanTarget::UserId("user123".to_string());
if let Some(ban_record) = ban_manager.is_banned(&user_target).await? {
    println!("User is banned: {:?}", ban_record);
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
    limit: u64,
    window_secs: u64,
    // 内部字段
}
```

</td>
</tr>
</table>

---

#### `QuotaController::new()`

创建新的配额控制器。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn new(limit: u64, window_secs: u64) -> Self
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `limit: u64` - 配额限制
- `window_secs: u64` - 时间窗口（秒）

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Self</code> - 新的配额控制器</td>
</tr>
</table>

**示例:**

```rust
use limiteron::quota_controller::QuotaController;

let quota = QuotaController::new(10000, 60); // 10000 次/分钟
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

创建新的熔断器。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn new(failure_threshold: u32, timeout_secs: u64) -> Self
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `failure_threshold: u32` - 失败阈值
- `timeout_secs: u64` - 超时时长（秒）

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Self</code> - 新的熔断器</td>
</tr>
</table>

**示例:**

```rust
use limiteron::circuit_breaker::CircuitBreaker;

let breaker = CircuitBreaker::new(5, 30); // 5 次失败后熔断，30秒后恢复
```

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

#### `Governor::new()`

创建新的 Governor。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn new(
    config: FlowControlConfig,
    storage: Arc<dyn Storage>,
    ban_storage: Arc<dyn BanStorage>
) -> Result<Self, FlowGuardError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `config: FlowControlConfig` - 流量控制配置
- `storage: Arc<dyn Storage>` - 存储后端
- `ban_storage: Arc<dyn BanStorage>` - 封禁存储后端

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;Governor, FlowGuardError&gt;</code></td>
</tr>
</table>

**示例:**

```rust
use limiteron::{Governor, FlowControlConfig};
use limiteron::storage::{MemoryStorage, MockBanStorage};
use std::sync::Arc;

let storage = Arc::new(MemoryStorage::new());
let ban_storage = Arc::new(MockBanStorage::default());
let governor = Governor::new(FlowControlConfig::default(), storage, ban_storage).await?;
```

---

#### `Governor::check()`

检查请求是否允许通过。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn check(&self, context: &RequestContext) -> Result<Decision, FlowGuardError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `context: &RequestContext` - 请求上下文

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;Decision, FlowGuardError&gt;</code> - 决策结果</td>
</tr>
</table>

**示例:**

```rust
use limiteron::governor::RequestContext;

let context = RequestContext::builder()
    .identifier("user123")
    .path("/api/v1/users")
    .method("GET")
    .build();

let decision = governor.check(&context).await?;
if decision.is_allowed() {
    // 处理请求
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

- `header_names: Vec<String>` - HTTP 头名称列表
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

let extractor = IpExtractor::new(
    vec!["X-Forwarded-For".to_string(), "X-Real-IP".to_string()],
    true,
);
```

---

## 错误处理

<div align="center">

#### 🚨 错误类型和处理

</div>

### `FlowGuardError` 枚举

```rust
pub enum FlowGuardError {
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
    Err(FlowGuardError::LimitError(msg)) => {
        eprintln!("❌ 限流错误: {}", msg);
    }
    Err(FlowGuardError::BanError(msg)) => {
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
async fn process_request() -> Result<(), FlowGuardError> {
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
    std::result::Result<T, FlowGuardError>;
```

**配置类型**
```rust
pub struct FlowControlConfig {
    pub rate_limit: Option<String>,
    pub quota_limit: Option<String>,
    pub concurrency_limit: Option<u64>,
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
use limiteron::ban_manager::{BanManager, BanTarget, BanSource};
use limiteron::storage::MockBanStorage;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let storage = Arc::new(MockBanStorage::default());
    let ban_manager = BanManager::new(storage, None).await?;

    // 封禁 IP
    let ip_target = BanTarget::Ip("192.168.1.100".to_string());
    ban_manager.create_ban(
        ip_target,
        "恶意请求".to_string(),
        Some(3600),
        Some(BanSource::Manual)
    ).await?;

    // 检查是否被封禁
    if let Some(ban_record) = ban_manager.is_banned(&ip_target).await? {
        println!("❌ IP 已被封禁: {:?}", ban_record);
    }

    Ok(())
}
```

### 示例 3: 使用 Governor

```rust
use limiteron::{Governor, FlowControlConfig};
use limiteron::governor::RequestContext;
use limiteron::storage::{MemoryStorage, MockBanStorage};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let storage = Arc::new(MemoryStorage::new());
    let ban_storage = Arc::new(MockBanStorage::default());
    let governor = Governor::new(FlowControlConfig::default(), storage, ban_storage).await?;

    let context = RequestContext::builder()
        .identifier("user123")
        .path("/api/v1/users")
        .method("GET")
        .build();

    let decision = governor.check(&context).await?;
    if decision.is_allowed() {
        println!("✅ 请求允许");
        // 处理请求
    } else {
        println!("❌ 请求被拒绝");
    }

    Ok(())
}
```

### 示例 4: 使用宏

```rust
use limiteron::flow_control;

#[flow_control(rate = "100/s", quota = "10000/m", concurrency = 50)]
async fn api_handler(user_id: &str) -> Result<String, limiteron::error::FlowGuardError> {
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