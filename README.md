<div align="center">

<p>
  <img src="docs/image/limiteron.png" alt="Limiteron Logo" width="200">
</p>

<p>
  <img src="https://img.shields.io/badge/version-0.1.0-blue.svg" alt="Version">
  <img src="https://img.shields.io/badge/rust-1.75%2B-orange.svg" alt="Rust Version">
  <img src="https://img.shields.io/badge/license-Apache%202.0-blue.svg" alt="License">
  <img src="https://github.com/Kirky-X/limiteron/workflows/CI/badge.svg" alt="Build">
  <img src="https://codecov.io/gh/Kirky-X/limiteron/branch/main/graph/badge.svg" alt="Coverage">
  <img src="https://img.shields.io/github/stars/Kirky-X/limiteron?style=social" alt="GitHub Stars">
  <img src="https://img.shields.io/github/forks/Kirky-X/limiteron?style=social" alt="GitHub Forks">
  <img src="https://img.shields.io/github/issues/Kirky-X/limiteron" alt="GitHub Issues">
  <img src="https://img.shields.io/github/license/Kirky-X/limiteron" alt="License">
</p>

<p align="center">
  <strong>Rust Unified Flow Control Framework</strong>
</p>

<p align="center">
  <a href="#-features">Features</a> •
  <a href="#-quick-start">Quick Start</a> •
  <a href="#-documentation">Documentation</a> •
  <a href="#-examples">Examples</a> •
  <a href="#-contributing">Contributing</a>
</p>

</div>

---

## 📋 Table of Contents

<details open>
<summary>Click to expand</summary>

- [✨ Features](#✨-features)
- [🎯 Use Cases](#🎯-use-cases)
- [🚀 Quick Start](#🚀-quick-start)
  - [Installation](#installation)
  - [Basic Usage](#basic-usage)
- [📚 Documentation](#📚-documentation)
- [🎨 Examples](#🎨-examples)
- [🏗️ Architecture](#🏗️-architecture)
- [⚙️ Configuration](#⚙️-configuration)
- [🧪 Testing](#🧪-testing)
- [📊 Performance](#📊-performance)
- [🔒 Security](#🔒-security)
- [🗺️ Roadmap](#🗺️-roadmap)
- [🤝 Contributing](#🤝-contributing)
- [📄 License](#📄-license)
- [🙏 Acknowledgments](#🙏-acknowledgments)

</details>

---

## <span id="✨-features">✨ Features</span>

<table>
<tr>
<td width="50%">

### 🎯 Core Features

- ✅ **Multiple Rate Limiting Algorithms** - Token bucket, fixed window, sliding window, concurrency control
- ✅ **Ban Management** - IP ban, automatic ban, ban priority
- ✅ **Quota Control** - Quota allocation, quota alerts, quota overdraw
- ✅ **Circuit Breaker** - Automatic failover, state recovery, fallback strategy

</td>
<td width="50%">

### ⚡ Advanced Features

- 🚀 **High Performance** - Latency < 200μs P99
- 🔐 **Secure and Reliable** - Memory safety, SQL injection protection
- 🌐 **Multi-Storage Support** - PostgreSQL via DBNexus, in-memory storage
- 📦 **Easy to Use** - Macro support, clean API

</td>
</tr>
</table>

<div align="center">

### 🎨 特性亮点

</div>

```mermaid
graph LR
    A[请求] --> B[标识符提取]
    B --> C[限流检查]
    B --> D[封禁检查]
    B --> E[配额检查]
    C --> F[决策链]
    D --> F
    E --> F
    F --> G[允许/拒绝]

    style A fill:#e1f5ff
    style B fill:#b3e5fc
    style C fill:#81d4fa
    style D fill:#81d4fa
    style E fill:#81d4fa
    style F fill:#4fc3f7
    style G fill:#29b6f6
```

---

## 🎯 Use Cases

<details>
<summary><b>💼 Enterprise Applications</b></summary>

<br>

```rust
use limiteron::limiters::{Limiter, TokenBucketLimiter};

async fn enterprise_api() -> Result<(), Box<dyn std::error::Error>> {
    let limiter = TokenBucketLimiter::new(100, 10); // 100 tokens, refill 10 per second

    // Rate limiting check
    match limiter.allow(1).await {
        Ok(true) => {
            // Process request
            process_request().await;
        }
        Ok(false) => {
            eprintln!("Rate limit exceeded");
        }
        Err(e) => {
            eprintln!("Error: {:?}", e);
        }
    }

    Ok(())
}

async fn process_request() {
    println!("Processing request...");
}
```

Suitable for enterprise applications requiring high concurrency and reliability.

</details>

<details>
<summary><b>🔧 API Services</b></summary>

<br>

```rust
use limiteron::flow_control;

#[flow_control(rate = "100/s", quota = "10000/m", concurrency = 50)]
async fn api_handler(user_id: &str) -> Result<String, limiteron::error::FlowGuardError> {
    // API business logic
    Ok("Success".to_string())
}
```

Suitable for protecting API services from abuse and DDoS attacks.

</details>

<details>
<summary><b>🌐 Web Applications</b></summary>

<br>

```rust
use limiteron::ban_manager::{BanManager, BanTarget};
use limiteron::adapters::StorageFactory;
use std::sync::Arc;

async fn web_app() -> Result<(), Box<dyn std::error::Error>> {
    // Create storage using DBNexus factory
    let factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    let ban_storage = factory.create_ban_storage().await?;
    let ban_manager = BanManager::new(ban_storage, None).await?;

    // Check if user is banned
    let user_target = BanTarget::UserId("user123".to_string());
    if let Some(ban_record) = ban_manager.is_banned(&user_target).await? {
        println!("User is banned: {:?}", ban_record);
        return Err("User is banned".into());
    }

    // Process request
    println!("Processing request for user123");
    Ok(())
}
```

Suitable for web applications that need to prevent malicious users and crawlers.

</details>

---

## <span id="🚀-quick-start">🚀 Quick Start</span>

### Installation

<table>
<tr>
<td width="50%">

#### 🦀 Cargo

```toml
[dependencies]
limiteron = { version = "0.1", features = ["macros"] }
```

</td>
<td width="50%">

#### 🔧 Features

```toml
[dependencies]
limiteron = { version = "0.1", features = ["postgres", "macros"] }
```

</td>
</tr>
</table>

### Feature Flags

<div align="center">

#### 🎛️ 可选特性配置

</div>

Limiteron 使用 feature flags 来控制功能启用，默认只启用内存存储：

<table>
<tr>
<td width="50%">

**预定义组合**
```toml
# 最小化：仅核心限流
limiteron = { version = "0.1", features = ["minimal"] }

# 标准：核心 + 基础高级功能
limiteron = { version = "0.1", features = ["standard"] }

# 完整：所有功能
limiteron = { version = "0.1", features = ["full"] }
```

</td>
<td width="50%">

**单独特性**
```toml
# 存储后端
limiteron = { version = "0.1", features = ["postgres"] }

# 高级功能
limiteron = { version = "0.1", features = ["ban-manager", "quota-control", "circuit-breaker"] }

# 宏支持
limiteron = { version = "0.1", features = ["macros"] }
```

</td>
</tr>
</table>

<details>
<summary><b>📋 完整特性列表</b></summary>

<br>

| 特性 | 描述 | 默认 |
|------|------|------|
| `postgres` | PostgreSQL 存储（DBNexus） | ❌ |
| `ban-manager` | 封禁管理 | ❌ |
| `quota-control` | 配额控制 | ❌ |
| `circuit-breaker` | 熔断器 | ❌ |
| `cache-service` | 统一缓存服务（支持 DI） | ❌ |
| `macros` | 宏支持 | ❌ |
| `telemetry` | 遥测和追踪 | ❌ |
| `monitoring` | Prometheus 指标 | ❌ |

</details>

### Basic Usage

<div align="center">

#### 🎬 5-Minute Quick Start

</div>

<table>
<tr>
<td width="50%">

**Step 1: Add Dependency**

```toml
[dependencies]
limiteron = { version = "0.1", features = ["macros"] }
```

</td>
<td width="50%">

**Step 2: Use Macro**

```rust
use limiteron::flow_control;

#[flow_control(rate = "10/s")]
async fn api_call() -> Result<String, limiteron::error::FlowGuardError> {
    Ok("Success".to_string())
}
```

</td>
</tr>
</table>

<details>
<summary><b>📖 Complete Example</b></summary>

<br>

```rust
use limiteron::limiters::{Limiter, TokenBucketLimiter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Create limiter
    let limiter = TokenBucketLimiter::new(10, 1); // 10 tokens, refill 1 per second

    // Step 2: Check rate limit
    match limiter.allow(1).await {
        Ok(true) => println!("✅ Request allowed"),
        Ok(false) => println!("❌ Request rate limited"),
        Err(e) => println!("❌ Error: {:?}", e),
    }

    // Step 3: Use with cost
    match limiter.allow(2).await {
        Ok(true) => println!("✅ Request with cost 2 allowed"),
        Ok(false) => println!("❌ Request with cost 2 rate limited"),
        Err(e) => println!("❌ Error: {:?}", e),
    }

    Ok(())
}
```

</details>

---

## <span id="📚-documentation">📚 Documentation</span>

<div align="center">

<table>
<tr>
<td align="center" width="25%">
<a href="docs/USER_GUIDE.md">
<img src="https://img.icons8.com/fluency/96/000000/book.png" width="64" height="64"><br>
<b>User Guide</b>
</a><br>
Complete usage guide
</td>
<td align="center" width="25%">
<a href="docs/API_REFERENCE.md">
<img src="https://img.icons8.com/fluency/96/000000/api.png" width="64" height="64"><br>
<b>API Reference</b>
</a><br>
Complete API documentation
 </td>
<td align="center" width="25%">
<a href="docs/FAQ.md">
<img src="https://img.icons8.com/fluency/96/000000/question.png" width="64" height="64"><br>
<b>FAQ</b>
</a><br>
Frequently asked questions
</td>
<td align="center" width="25%">
<a href="examples/">
<img src="https://img.icons8.com/fluency/96/000000/code.png" width="64" height="64"><br>
<b>Examples</b>
</a><br>
Code examples
</td>
</tr>
</table>

</div>

### 📖 Additional Resources

- 🎓 [User Guide](docs/USER_GUIDE.md) - Detailed tutorial
- 🔧 [API Reference](docs/API_REFERENCE.md) - API documentation
- ❓ [FAQ](docs/FAQ.md) - Frequently asked questions
- 🐛 [Troubleshooting](docs/FAQ.md#troubleshooting) - Common issues and solutions

---

## <span id="🎨-examples">🎨 Examples</span>

<div align="center">

### 💡 Practical Examples

</div>

<table>
<tr>
<td width="50%">

#### 📝 Example 1: Basic Rate Limiting

```rust
use limiteron::limiters::{Limiter, TokenBucketLimiter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limiter = TokenBucketLimiter::new(10, 1);

    for i in 0..15 {
        match limiter.allow(1).await {
            Ok(true) => println!("Request {} ✅", i),
            Ok(false) => println!("Request {} ❌", i),
            Err(e) => println!("Request {} Error: {:?}", i, e),
        }
    }

    Ok(())
}
```

<details>
<summary>View Output</summary>

```
Request 0 ✅
Request 1 ✅
...
Request 9 ✅
Request 10 ❌
...
Request 14 ❌
✅ First 10 requests allowed, remaining rate limited
```

</details>

</td>
<td width="50%">

#### 🔥 Example 2: Using Macro

```rust
use limiteron::flow_control;

#[flow_control(rate = "100/s", quota = "10000/m", concurrency = 50)]
async fn api_handler(user_id: &str) -> Result<String, limiteron::error::FlowGuardError> {
    // API business logic
    Ok(format!("Processing request for user {}", user_id))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = api_handler("user123").await?;
    println!("{}", result);
    Ok(())
}
```

<details>
<summary>View Output</summary>

```
Processing request for user123
✅ Macro automatically handles rate limiting
```

</details>

</td>
</tr>
</table>

<div align="center">

**[📂 View All Examples →](examples/)**

</div>

---

## <span id="🏗️-architecture">🏗️ Architecture</span>

<div align="center">

### System Overview

</div>

```mermaid
graph TB
    A[User App] --> B[API Layer]
    B --> C[Governor]
    C --> D[Identifier Extraction]
    C --> E[Decision Chain]
    D --> F[Matchers]
    E --> G[Limiters]
    E --> H[Ban Management]
    E --> I[Quota Control]
    E --> J[Circuit Breaker]
    G --> K[L2/L3 Cache]
    H --> K
    I --> K
    K --> L[Storage Layer]
    L --> M[PostgreSQL via DBNexus]
    L --> N[In-Memory]

    style A fill:#e1f5ff
    style B fill:#b3e5fc
    style C fill:#81d4fa
    style D fill:#4fc3f7
    style E fill:#4fc3f7
    style F fill:#29b6f6
    style G fill:#29b6f6
    style H fill:#29b6f6
    style I fill:#29b6f6
    style J fill:#29b6f6
    style K fill:#0288d1
    style L fill:#0277bd
    style M fill:#01579b
    style N fill:#01579b
```

<details>
<summary><b>📐 Component Details</b></summary>

<br>

| Component | Description | Status |
|-----------|-------------|--------|
| **Governor** | Main controller, end-to-end flow control | ✅ Stable |
| **Matchers** | Identifier extraction (IP, User ID, Device ID, etc.) | ✅ Stable |
| **Limiters** | Multiple rate limiting algorithms | ✅ Stable |
| **Ban Management** | IP ban, automatic ban | ✅ Stable |
| **Quota Control** | Quota allocation, quota alerts | ✅ Stable |
| **Circuit Breaker** | Automatic failover, state recovery | ✅ Stable |
| **Cache** | L2/L3 cache support | ✅ Stable |
| **Storage Layer** | PostgreSQL via DBNexus | ✅ Stable |

</details>

---

## <span id="⚙️-configuration">⚙️ Configuration</span>

<div align="center">

### 🎛️ Configuration Options

</div>

Limiteron使用TOML格式的配置文件（`config.toml`），支持环境变量覆盖。

<table>
<tr>
<td width="50%">

**TOML Configuration (config.toml)**

```toml
version = "1.0"

[global]
storage = "memory"
cache = "memory"
metrics = "prometheus"

[[rules]]
id = "api_rate_limit"
name = "API Rate Limit"
priority = 100

[rules.matchers]
type = "User"
user_ids = ["*"]

[[rules.limiters]]
type = "TokenBucket"
capacity = 1000
refill_rate = 100

[rules.action]
on_exceed = "reject"
```

</td>
<td width="50%">

**Environment Variable Override**

```bash
# Override global storage
export LIMITERON_GLOBAL_STORAGE=redis

# Override limiter capacity
export LIMITERON_RULES_0_LIMITERS_0_CAPACITY=2000
```

**Load Configuration**

```rust
use limiteron::ConfigLoader;

let config = ConfigLoader::load_from_file("config.toml")?;
```

</td>
</tr>
</table>

<details>
<summary><b>🔧 All Configuration Options</b></summary>

<br>

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `version` | String | "0.1.0" | Configuration version |
| `global.storage` | String | "postgres" | Storage type: postgres (via DBNexus) |
| `global.cache` | String | "memory" | Cache type: memory/redis |
| `global.metrics` | String | "prometheus" | Metrics type |
| `rules[].id` | String | - | Rule identifier |
| `rules[].name` | String | - | Rule name |
| `rules[].priority` | u16 | 100 | Rule priority |
| `rules[].limiters[].capacity` | u64 | - | Limiter capacity |
| `rules[].limiters[].refill_rate` | u64 | - | Limiter refill rate |

</details>

**ConfigBuilder (Programmatic)**

```rust
use limiteron::ConfigBuilder;

let config = ConfigBuilder::new()
    .with_storage("memory")
    .with_rule(|rule| {
        rule.id("default")
            .token_bucket(1000, 100)
    })
    .build()?;
```

---

## <span id="🧪-testing">🧪 Testing</span>

**当前测试覆盖率: 53.56%**

```bash
# Run all tests
cargo test --all-features

# Run specific test
cargo test test_name

# Run integration tests
cargo test --test integration_tests

# Run benchmarks
cargo bench

# Generate coverage report
cargo tarpaulin --out Html
```

详细测试文档: [TESTING.md](./docs/TESTING.md)
覆盖率报告: [COVERAGE_REPORT.md](./docs/COVERAGE_REPORT.md)

---

## <span id="📊-performance">📊 Performance</span>

<div align="center">

### ⚡ Benchmark Results

</div>

> **Note:** The following data represents actual benchmark results from comprehensive testing (2026-01-19).

<table>
<tr>
<td width="50%">

**Throughput**

| Limiter Type | Actual | Target | Achievement |
|-------------|--------|--------|-------------|
| TokenBucket | **12M+ ops/s** | 500K ops/s | ✅ 24x |
| FixedWindow | **20M+ ops/s** | 300K ops/s | ✅ 66x |
| ConcurrencyLimiter | **12M+ ops/s** | 200K ops/s | ✅ 60x |

</td>
<td width="50%">

**Latency**

| Percentile | TokenBucket | FixedWindow |
|-----------|-------------|-------------|
| P50 | < 100ns | < 100ns |
| P95 | < 200ns | < 150ns |
| P99 | < 1µs | < 500ns |

</td>
</tr>
</table>

#### Concurrency Test Results

| Test Item | Result | Status |
|-----------|--------|--------|
| Data Consistency | 100% | ✅ Pass |
| High Concurrency Stability | 50/100 concurrent | ✅ Pass |
| Rate Limit Correctness | 1000/1000 | ✅ Pass |

<details>
<summary><b>📈 Detailed Benchmarks</b></summary>

<br>

```bash
# Run performance tests
cd temp/comprehensive_test
./target/release/functional_test    # Functional tests
./target/release/performance_test   # Performance tests
./target/release/concurrency_test   # Concurrency tests
```

**Sample output:**
```
功能测试: 7/7 Pass (100%)
TokenBucket: 12,088,759 ops/s
FixedWindow: 19,920,188 ops/s
ConcurrencyLimiter: 11,891,237 ops/s
并发测试: 100% 数据一致性
```

</details>

---

## <span id="🔒-security">🔒 Security</span>

<div align="center">

### 🛡️ Security Features

</div>

<table>
<tr>
<td align="center" width="25%">
<img src="https://img.icons8.com/fluency/96/000000/lock.png" width="64" height="64"><br>
<b>Memory Safety</b><br>
Rust guarantees memory safety
</td>
<td align="center" width="25%">
<img src="https://img.icons8.com/fluency/96/000000/security-checked.png" width="64" height="64"><br>
<b>Input Validation</b><br>
Comprehensive input checking
</td>
<td align="center" width="25%">
<img src="https://img.icons8.com/fluency/96/000000/privacy.png" width="64" height="64"><br>
<b>SQL Injection Protection</b><br>
 Parameterized queries
</td>
<td align="center" width="25%">
<img src="https://img.icons8.com/fluency/96/000000/shield.png" width="64" height="64"><br>
<b>Password Protection</b><br>
Secure password storage
</td>
</tr>
</table>

<details>
<summary><b>🔐 Security Details</b></summary>

<br>

### Security Measures

- ✅ **Memory Protection** - Rust memory safety guarantees
- ✅ **Input Validation** - IP address, User ID, MAC address validation
- ✅ **SQL Injection Protection** - Using parameterized queries
- ✅ **Password Protection** - Using secrecy library for sensitive data
- ✅ **Audit Logging** - Complete operation tracking

### Reporting Security Issues

Please report security vulnerabilities through GitHub Issues.

</details>

---

## <span id="🗺️-roadmap">🗺️ Roadmap</span>

<div align="center">

### 🎯 Development Plan

</div>

```mermaid
gantt
    title Limiteron Roadmap
    dateFormat  YYYY-MM
    section Phase 1
    Core Features           :done, 2026-01, 2026-03
    section Phase 2
    Feature Extensions      :active, 2026-03, 2026-06
    section Phase 3
    Performance Optimization :2026-06, 2026-09
    section Phase 4
    Production Ready        :2026-09, 2026-12
```

<table>
<tr>
<td width="50%">

### ✅ Completed

- [x] Core rate limiting
- [x] Ban management
- [x] Quota control
- [x] Circuit breaker
- [x] Unit and integration tests
- [x] Macro support
- [x] PostgreSQL storage via DBNexus

</td>
<td width="50%">

### 🚧 In Progress

- [ ] Performance optimization
- [ ] Monitoring and tracing improvements
- [ ] Documentation completion
- [ ] Example code additions

</td>
</tr>
<tr>
<td width="50%">

### 📋 Planned

- [ ] MySQL storage support via DBNexus
- [ ] SQLite storage support via DBNexus
- [ ] Additional storage backends
- [ ] Web UI management interface

</td>
<td width="50%">

### 💡 Future Ideas

- [ ] Distributed rate limiting
- [ ] Machine learning-driven rate limiting
- [ ] Additional rate limiting algorithms
- [ ] Community plugin system

</td>
</tr>
</table>

---

## <span id="🤝-contributing">🤝 Contributing</span>

<div align="center">

### 💖 Welcome Contributions!

</div>

<table>
<tr>
<td width="33%" align="center">

### 🐛 Report Issues

Found a bug?<br>
[Create Issue](../../issues)

</td>
<td width="33%" align="center">

### 💡 Feature Requests

Have a suggestion?<br>
[Start Discussion](../../discussions)

</td>
<td width="33%" align="center">

### 🔧 Submit Code

Want to contribute?<br>
[Fork & PR](../../pulls)

</td>
</tr>
</table>

<details>
<summary><b>📝 Contribution Guide</b></summary>

<br>

### How to Contribute

1. **Fork** the repository
2. **Clone** your fork: `git clone https://github.com/yourusername/limiteron.git`
3. **Create** a branch: `git checkout -b feature/amazing-feature`
4. **Make** your changes
5. **Test** your changes: `cargo test --all-features`
6. **Commit** your changes: `git commit -m 'Add amazing feature'`
7. **Push** to branch: `git push origin feature/amazing-feature`
8. **Create** a Pull Request

### Code Style

- Follow Rust standard coding conventions
- Write comprehensive tests
- Update documentation
- Add examples for new features

</details>

---

## <span id="📄-license">📄 License</span>

<div align="center">

This project is licensed under Apache 2.0:

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)

</div>

---

## <span id="🙏-acknowledgments">🙏 Acknowledgments</span>

<div align="center">

### Built with Excellent Tools

</div>

<table>
<tr>
<td align="center" width="25%">
<a href="https://www.rust-lang.org/">
<img src="https://www.rust-lang.org/static/images/rust-logo-blk.svg" width="64" height="64"><br>
<b>Rust</b>
</a>
</td>
<td align="center" width="25%">
<a href="https://github.com/">
<img src="https://github.githubassets.com/images/modules/logos_page/GitHub-Mark.png" width="64" height="64"><br>
<b>GitHub</b>
</a>
</td>
<td align="center" width="25%">
<img src="https://img.icons8.com/fluency/96/000000/code.png" width="64" height="64"><br>
<b>Open Source</b>
</td>
<td align="center" width="25%">
<img src="https://img.icons8.com/fluency/96/000000/community.png" width="64" height="64"><br>
<b>Community</b>
</td>
</tr>
</table>

### Special Thanks

- 🌟 **Dependencies** - Built on these excellent projects:
  - [tokio](https://tokio.rs/) - Async runtime
  - [dbnexus](https://github.com/) - Database abstraction layer
  - [dashmap](https://github.com/xacrimon/dashmap) - Concurrent HashMap
  - [lru](https://github.com/jeromefroe/lru-rs) - LRU cache

- 👥 **Contributors** - Thanks to all contributors!
- 💬 **Community** - Special thanks to community members

---

## 📞 Contact & Support

<div align="center">

<table>
<tr>
<td align="center" width="33%">
<a href="../../issues">
<img src="https://img.icons8.com/fluency/96/000000/bug.png" width="48" height="48"><br>
<b>Issues</b>
</a><br>
Report bugs and errors
</td>
<td align="center" width="33%">
<a href="../../discussions">
<img src="https://img.icons8.com/fluency/96/000000/chat.png" width="48" height="48"><br>
<b>Discussions</b>
</a><br>
Ask questions and share ideas
</td>
<td align="center" width="33%">
<a href="https://github.com/Kirky-X/limiteron">
<img src="https://img.icons8.com/fluency/96/000000/github.png" width="48" height="48"><br>
<b>GitHub</b>
</a><br>
View source code
</td>
</tr>
</table>

### Stay Connected

[![GitHub](https://img.shields.io/badge/GitHub-View%20Repo-100000?style=for-the-badge&logo=github&logoColor=white)](https://github.com/Kirky-X/limiteron)

</div>

---

## ⭐ Star History

<div align="center">

[![Star History Chart](https://api.star-history.com/svg?repos=Kirky-X/limiteron&type=Date)](https://star-history.com/#Kirky-X/limiteron&Date)

</div>

---

<div align="center">

### 💝 Support This Project

If you find this project useful, please consider giving it a ⭐️!

**Built with ❤️ by Kirky.X**

[⬆ Back to Top](#readme)

---

<sub>© 2026 Kirky.X. All rights reserved.</sub>

</div>