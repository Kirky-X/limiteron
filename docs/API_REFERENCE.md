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

#### `TokenBucketLimiter::check()`

检查是否允许通过。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub fn check(&mut self, key: &str) -> Result<(), FlowGuardError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `key: &str` - 限流键（通常为用户ID或IP）

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;(), FlowGuardError&gt;</code> - Ok 表示允许，Err 表示被限流</td>
</tr>
<tr>
<td><b>错误</b></td>
<td>

- `FlowGuardError::RateLimitExceeded` - 超过速率限制

</td>
</tr>
</table>

**示例:**

```rust
let limiter = TokenBucketLimiter::new(10, 1);
let key = "user123";

match limiter.check(key).await {
    Ok(_) => println!("✅ 请求允许"),
    Err(_) => println!("❌ 请求被限流"),
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
pub async fn new() -> Result<Self, FlowGuardError>
```

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;BanManager, FlowGuardError&gt;</code></td>
</tr>
</table>

**示例:**

```rust
use limiteron::BanManager;

let ban_manager = BanManager::new().await?;
```

---

#### `BanManager::ban()`

封禁指定标识符。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn ban(&self, identifier: &str, reason: &str, duration_secs: u64) -> Result<(), FlowGuardError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `identifier: &str` - 要封禁的标识符（IP、用户ID等）
- `reason: &str` - 封禁原因
- `duration_secs: u64` - 封禁时长（秒）

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;(), FlowGuardError&gt;</code></td>
</tr>
</table>

**示例:**

```rust
ban_manager.ban("192.168.1.100", "恶意请求", 3600).await?;
```

---

#### `BanManager::is_banned()`

检查标识符是否被封禁。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn is_banned(&self, identifier: &str) -> Result<bool, FlowGuardError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `identifier: &str` - 要检查的标识符

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;bool, FlowGuardError&gt;</code> - true 表示被封禁</td>
</tr>
</table>

**示例:**

```rust
if ban_manager.is_banned("user123").await? {
    return Err(FlowGuardError::Banned("User is banned".into()));
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
pub async fn new(config: FlowControlConfig) -> Result<Self, FlowGuardError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `config: FlowControlConfig` - 流量控制配置

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

let governor = Governor::new(FlowControlConfig::default()).await?;
```

---

#### `Governor::check_request()`

检查请求是否允许通过。

<table>
<tr>
<td width="30%"><b>签名</b></td>
<td width="70%">

```rust
pub async fn check_request(&self, identifier: &str, path: &str) -> Result<Decision, FlowGuardError>
```

</td>
</tr>
<tr>
<td><b>参数</b></td>
<td>

- `identifier: &str` - 请求标识符
- `path: &str` - 请求路径

</td>
</tr>
<tr>
<td><b>返回</b></td>
<td><code>Result&lt;Decision, FlowGuardError&gt;</code> - 决策结果</td>
</tr>
</table>

**示例:**

```rust
let decision = governor.check_request("user123", "/api/v1/users").await?;
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
    RateLimitExceeded(String),
    QuotaExceeded(String),
    Banned(String),
    CircuitBreakerOpen(String),
    InvalidInput(String),
    StorageError(String),
    ConfigError(String),
}
```

### 错误处理模式

<table>
<tr>
<td width="50%">

**模式匹配**
```rust
match limiter.check(key).await {
    Ok(_) => {
        println!("✅ 请求允许");
    }
    Err(FlowGuardError::RateLimitExceeded(msg)) => {
        eprintln!("❌ 速率限制: {}", msg);
    }
    Err(FlowGuardError::Banned(msg)) => {
        eprintln!("❌ 已封禁: {}", msg);
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
    limiter.check(key).await?;
    
    // 处理请求
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
use limiteron::limiters::TokenBucketLimiter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut limiter = TokenBucketLimiter::new(10, 1);
    let key = "user123";

    for i in 0..15 {
        match limiter.check(key).await {
            Ok(_) => println!("请求 {} ✅", i),
            Err(_) => println!("请求 {} ❌", i),
        }
    }

    Ok(())
}
```

### 示例 2: 封禁管理

```rust
use limiteron::BanManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ban_manager = BanManager::new().await?;

    // 封禁 IP
    ban_manager.ban("192.168.1.100", "恶意请求", 3600).await?;

    // 检查是否被封禁
    if ban_manager.is_banned("192.168.1.100").await? {
        println!("❌ IP 已被封禁");
    }

    Ok(())
}
```

### 示例 3: 使用 Governor

```rust
use limiteron::{Governor, FlowControlConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let governor = Governor::new(FlowControlConfig::default()).await?;

    let decision = governor.check_request("user123", "/api/v1/users").await?;
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