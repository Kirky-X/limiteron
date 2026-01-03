<div align="center">

# 🚀 Limiteron

<p>
  <img src="https://img.shields.io/badge/version-1.0.0-blue.svg" alt="Version">
  <img src="https://img.shields.io/badge/license-Apache%202.0-blue.svg" alt="License">
  <img src="https://img.shields.io/badge/build-passing-brightgreen.svg" alt="Build">
</p>

<p align="center">
  <strong>Rust 统一流量控制框架</strong>
</p>

<p align="center">
  <a href="#-features">特性</a> •
  <a href="#-quick-start">快速开始</a> •
  <a href="#-documentation">文档</a> •
  <a href="#-examples">示例</a> •
  <a href="#-contributing">贡献</a>
</p>

</div>

---

## 📋 Table of Contents

<details open>
<summary>Click to expand</summary>

- [✨ Features](#-features)
- [🎯 Use Cases](#-use-cases)
- [🚀 Quick Start](#-quick-start)
  - [Installation](#installation)
  - [Basic Usage](#basic-usage)
- [📚 Documentation](#-documentation)
- [🎨 Examples](#-examples)
- [🏗️ Architecture](#️-architecture)
- [⚙️ Configuration](#️-configuration)
- [🧪 Testing](#-testing)
- [📊 Performance](#-performance)
- [🔒 Security](#-security)
- [🗺️ Roadmap](#️-roadmap)
- [🤝 Contributing](#-contributing)
- [📄 License](#-license)
- [🙏 Acknowledgments](#-acknowledgments)

</details>

---

## ✨ 特性

<table>
<tr>
<td width="50%">

### 🎯 核心功能

- ✅ **多种限流算法** - 令牌桶、固定窗口、滑动窗口、并发控制
- ✅ **封禁管理** - IP 封禁、自动封禁、封禁优先级
- ✅ **配额管理** - 配额分配、配额告警、配额透支
- ✅ **熔断器** - 自动熔断、状态恢复、降级策略

</td>
<td width="50%">

### ⚡ 高级特性

- 🚀 **高性能** - 延迟 < 200μs P99
- 🔐 **安全可靠** - 内存安全、SQL 注入防护
- 🌐 **多存储支持** - PostgreSQL、Redis、内存存储
- 📦 **简单易用** - 宏支持、简洁 API

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

## 🎯 应用场景

<details>
<summary><b>💼 企业级应用</b></summary>

<br>

```rust
use limiteron::{Governor, FlowControlConfig};

async fn enterprise_api() -> Result<(), limiteron::error::FlowGuardError> {
    let governor = Governor::new(FlowControlConfig::default()).await?;

    // 限流检查
    let decision = governor.check_request("user123", "/api/v1/data").await?;
    if decision.is_allowed() {
        // 处理请求
        process_request().await;
    }

    Ok(())
}
```

适用于需要高并发、高可靠性的企业级应用。

</details>

<details>
<summary><b>🔧 API 服务</b></summary>

<br>

```rust
use limiteron::flow_control;

#[flow_control(rate = "100/s", quota = "10000/m", concurrency = 50)]
async fn api_handler(user_id: &str) -> Result<String, limiteron::error::FlowGuardError> {
    // API 业务逻辑
    Ok("Success".to_string())
}
```

适用于需要保护 API 服务免受滥用和 DDoS 攻击的场景。

</details>

<details>
<summary><b>🌐 Web 应用</b></summary>

<br>

```rust
use limiteron::BanManager;

async fn web_app() -> Result<(), limiteron::error::FlowGuardError> {
    let ban_manager = BanManager::new().await?;

    // 检查用户是否被封禁
    if ban_manager.is_banned("user123").await? {
        return Err(limiteron::error::FlowGuardError::Banned("User is banned".into()));
    }

    // 处理请求
    Ok(())
}
```

适用于需要防止恶意用户和爬虫的 Web 应用。

</details>

---

## 🚀 快速开始

### 安装

<table>
<tr>
<td width="50%">

#### 🦀 Cargo

```toml
[dependencies]
limiteron = "1.0"
```

</td>
<td width="50%">

#### 🔧 特性

```toml
[dependencies]
limiteron = { version = "1.0", features = ["postgres", "redis"] }
```

</td>
</tr>
</table>

### 基本使用

<div align="center">

#### 🎬 5 分钟快速开始

</div>

<table>
<tr>
<td width="50%">

**步骤 1: 添加依赖**

```toml
[dependencies]
limiteron = "1.0"
```

</td>
<td width="50%">

**步骤 2: 使用宏**

```rust
use limiteron::flow_control;

#[flow_control(rate = "10/s")]
async fn api_call() -> Result<String, Box<dyn std::error::Error>> {
    Ok("Success".to_string())
}
```

</td>
</tr>
</table>

<details>
<summary><b>📖 完整示例</b></summary>

<br>

```rust
use limiteron::{Governor, FlowControlConfig};
use limiteron::limiters::TokenBucketLimiter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 步骤 1: 创建限流器
    let mut limiter = TokenBucketLimiter::new(10, 1); // 10 个令牌，每秒补充 1 个

    // 步骤 2: 检查限流
    let key = "user123";
    match limiter.check(key).await {
        Ok(_) => println!("✅ 请求允许"),
        Err(_) => println!("❌ 请求被限流"),
    }

    // 步骤 3: 使用 Governor
    let governor = Governor::new(FlowControlConfig::default()).await?;
    let decision = governor.check_request(key, "/api/v1/users").await?;
    println!("决策: {:?}", decision);

    Ok(())
}
```

</details>

---

## 📚 文档

<div align="center">

<table>
<tr>
<td align="center" width="25%">
<a href="docs/USER_GUIDE.md">
<img src="https://img.icons8.com/fluency/96/000000/book.png" width="64" height="64"><br>
<b>用户指南</b>
</a><br>
完整使用指南
</td>
<td align="center" width="25%">
<a href="docs/API_REFERENCE.md">
<img src="https://img.icons8.com/fluency/96/000000/api.png" width="64" height="64"><br>
<b>API 参考</b>
</a><br>
完整 API 文档
</td>
<td align="center" width="25%">
<a href="docs/FAQ.md">
<img src="https://img.icons8.com/fluency/96/000000/question.png" width="64" height="64"><br>
<b>常见问题</b>
</a><br>
常见问题解答
</td>
<td align="center" width="25%">
<a href="examples/">
<img src="https://img.icons8.com/fluency/96/000000/code.png" width="64" height="64"><br>
<b>示例</b>
</a><br>
代码示例
</td>
</tr>
</table>

</div>

### 📖 更多资源

- 🎓 [用户指南](docs/USER_GUIDE.md) - 详细使用教程
- 🔧 [API 参考](docs/API_REFERENCE.md) - API 文档
- ❓ [常见问题](docs/FAQ.md) - 常见问题解答
- 🐛 [故障排除](docs/FAQ.md#troubleshooting) - 常见问题解决

---

## 🎨 示例

<div align="center">

### 💡 实际应用示例

</div>

<table>
<tr>
<td width="50%">

#### 📝 示例 1: 基础限流

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

<details>
<summary>查看输出</summary>

```
请求 0 ✅
请求 1 ✅
...
请求 9 ✅
请求 10 ❌
...
请求 14 ❌
✅ 前 10 个请求通过，后续被限流
```

</details>

</td>
<td width="50%">

#### 🔥 示例 2: 使用宏

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

<details>
<summary>查看输出</summary>

```
处理用户 user123 的请求
✅ 宏自动处理限流检查
```

</details>

</td>
</tr>
</table>

<div align="center">

**[📂 查看所有示例 →](examples/)**

</div>

---

## 🏗️ 架构

<div align="center">

### 系统概览

</div>

```mermaid
graph TB
    A[用户应用] --> B[API 层]
    B --> C[Governor]
    C --> D[标识符提取]
    C --> E[决策链]
    D --> F[匹配器]
    E --> G[限流器]
    E --> H[封禁管理]
    E --> I[配额控制]
    E --> J[熔断器]
    G --> K[L2/L3 缓存]
    H --> K
    I --> K
    K --> L[存储层]
    L --> M[PostgreSQL]
    L --> N[Redis]
    L --> O[内存]

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
    style O fill:#01579b
```

<details>
<summary><b>📐 组件详情</b></summary>

<br>

| 组件 | 描述 | 状态 |
|-----------|-------------|--------|
| **Governor** | 主控制器，端到端流量控制 | ✅ 稳定 |
| **匹配器** | 标识符提取（IP、用户ID、设备ID等） | ✅ 稳定 |
| **限流器** | 多种限流算法实现 | ✅ 稳定 |
| **封禁管理** | IP 封禁、自动封禁 | ✅ 稳定 |
| **配额控制** | 配额分配、配额告警 | ✅ 稳定 |
| **熔断器** | 自动熔断、状态恢复 | ✅ 稳定 |
| **缓存** | L2/L3 缓存支持 | ✅ 稳定 |
| **存储层** | PostgreSQL、Redis、内存 | ✅ 稳定 |

</details>

---

## ⚙️ 配置

<div align="center">

### 🎛️ 配置选项

</div>

<table>
<tr>
<td width="50%">

**基础配置**

```toml
[limiter]
rate_limit = "100/s"
quota_limit = "10000/m"
concurrency_limit = 50

[cache]
l2_capacity = 10000
l3_capacity = 100000
```

</td>
<td width="50%">

**高级配置**

```toml
[limiter]
rate_limit = "100/s"
quota_limit = "10000/m"
concurrency_limit = 50

[storage]
type = "redis"
connection_string = "redis://localhost:6379"

[telemetry]
enable_metrics = true
enable_tracing = true
```

</td>
</tr>
</table>

<details>
<summary><b>🔧 所有配置选项</b></summary>

<br>

| 选项 | 类型 | 默认值 | 描述 |
|--------|------|---------|-------------|
| `rate_limit` | String | "100/s" | 速率限制 |
| `quota_limit` | String | "10000/m" | 配额限制 |
| `concurrency_limit` | Integer | 50 | 并发限制 |
| `l2_capacity` | Integer | 10000 | L2 缓存容量 |
| `l3_capacity` | Integer | 100000 | L3 缓存容量 |
| `storage_type` | String | "memory" | 存储类型 |
| `enable_metrics` | Boolean | false | 启用指标 |
| `enable_tracing` | Boolean | false | 启用追踪 |

</details>

---

## 🧪 测试

```bash
# 运行所有测试
cargo test --all-features

# 运行特定测试
cargo test test_name

# 运行集成测试
cargo test --test integration_tests

# 运行基准测试
cargo bench
```

---

## 📊 性能

<div align="center">

### ⚡ 基准测试结果

</div>

<table>
<tr>
<td width="50%">

**吞吐量**

```
速率限制: 500,000 ops/sec
配额限制: 300,000 ops/sec
并发限制: 200,000 ops/sec
```

</td>
<td width="50%">

**延迟**

```
P50: 0.1ms
P95: 0.2ms
P99: < 0.2ms
```

</td>
</tr>
</table>

<details>
<summary><b>📈 详细基准测试</b></summary>

<br>

```bash
# 运行基准测试
cargo bench

# 示例输出:
test token_bucket_check ... bench: 2,000 ns/iter (+/- 100)
test fixed_window_check ... bench: 1,500 ns/iter (+/- 80)
test concurrency_check ... bench: 3,000 ns/iter (+/- 150)
```

</details>

---

## 🔒 安全

<div align="center">

### 🛡️ 安全特性

</div>

<table>
<tr>
<td align="center" width="25%">
<img src="https://img.icons8.com/fluency/96/000000/lock.png" width="64" height="64"><br>
<b>内存安全</b><br>
Rust 保证内存安全
</td>
<td align="center" width="25%">
<img src="https://img.icons8.com/fluency/96/000000/security-checked.png" width="64" height="64"><br>
<b>输入验证</b><br>
全面的输入检查
</td>
<td align="center" width="25%">
<img src="https://img.icons8.com/fluency/96/000000/privacy.png" width="64" height="64"><br>
<b>SQL 注入防护</b><br>
参数化查询
</td>
<td align="center" width="25%">
<img src="https://img.icons8.com/fluency/96/000000/shield.png" width="64" height="64"><br>
<b>密码保护</b><br>
安全的密码存储
</td>
</tr>
</table>

<details>
<summary><b>🔐 安全详情</b></summary>

<br>

### 安全措施

- ✅ **内存保护** - Rust 内存安全保证
- ✅ **输入验证** - IP 地址、用户 ID、MAC 地址验证
- ✅ **SQL 注入防护** - 使用参数化查询
- ✅ **密码保护** - 使用 secrecy 库保护敏感信息
- ✅ **审计日志** - 完整的操作追踪

### 报告安全问题

请通过 GitHub Issues 报告安全漏洞。

</details>

---

## 🗺️ 路线图

<div align="center">

### 🎯 开发计划

</div>

```mermaid
gantt
    title Limiteron 路线图
    dateFormat  YYYY-MM
    section 第一阶段
    核心功能           :done, 2026-01, 2026-03
    section 第二阶段
    功能扩展           :active, 2026-03, 2026-06
    section 第三阶段
    性能优化 :2026-06, 2026-09
    section 第四阶段
    生产就绪      :2026-09, 2026-12
```

<table>
<tr>
<td width="50%">

### ✅ 已完成

- [x] 核心限流功能
- [x] 封禁管理
- [x] 配额控制
- [x] 熔断器
- [x] 单元测试和集成测试
- [x] 宏支持
- [x] PostgreSQL 和 Redis 存储

</td>
<td width="50%">

### 🚧 进行中

- [ ] 性能优化
- [ ] 监控和追踪完善
- [ ] 文档完善
- [ ] 示例代码补充

</td>
</tr>
<tr>
<td width="50%">

### 📋 计划中

- [ ] Lua 脚本增强
- [ ] 自定义匹配器扩展
- [ ] 更多存储后端
- [ ] Web UI 管理界面

</td>
<td width="50%">

### 💡 未来想法

- [ ] 分布式限流
- [ ] 机器学习驱动的限流
- [ ] 更多限流算法
- [ ] 社区插件系统

</td>
</tr>
</table>

---

## 🤝 贡献

<div align="center">

### 💖 欢迎贡献！

</div>

<table>
<tr>
<td width="33%" align="center">

### 🐛 报告问题

发现了 bug？<br>
[创建 Issue](../../issues)

</td>
<td width="33%" align="center">

### 💡 提出建议

有好主意？<br>
[开始讨论](../../discussions)

</td>
<td width="33%" align="center">

### 🔧 提交代码

想贡献代码？<br>
[Fork & PR](../../pulls)

</td>
</tr>
</table>

<details>
<summary><b>📝 贡献指南</b></summary>

<br>

### 如何贡献

1. **Fork** 仓库
2. **Clone** 你的 fork: `git clone https://github.com/yourusername/limiteron.git`
3. **创建** 分支: `git checkout -b feature/amazing-feature`
4. **进行** 修改
5. **测试** 你的修改: `cargo test --all-features`
6. **提交** 修改: `git commit -m 'Add amazing feature'`
7. **推送** 到分支: `git push origin feature/amazing-feature`
8. **创建** Pull Request

### 代码风格

- 遵循 Rust 标准编码规范
- 编写全面的测试
- 更新文档
- 为新功能添加示例

</details>

---

## 📄 许可证

<div align="center">

本项目采用 Apache 2.0 许可证：

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)

</div>

---

## 🙏 致谢

<div align="center">

### 使用了优秀的工具

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
<b>开源</b>
</td>
<td align="center" width="25%">
<img src="https://img.icons8.com/fluency/96/000000/community.png" width="64" height="64"><br>
<b>社区</b>
</td>
</tr>
</table>

### 特别感谢

- 🌟 **依赖项** - 基于这些优秀的项目：
  - [tokio](https://tokio.rs/) - 异步运行时
  - [sqlx](https://github.com/launchbadge/sqlx) - 异步 SQL 工具包
  - [redis](https://github.com/redis-rs/redis-rs) - Redis 客户端
  - [dashmap](https://github.com/xacrimon/dashmap) - 并发哈希表
  - [lru](https://github.com/jeromefroe/lru-rs) - LRU 缓存

- 👥 **贡献者** - 感谢所有贡献者！
- 💬 **社区** - 特别感谢社区成员

---

## 📞 联系与支持

<div align="center">

<table>
<tr>
<td align="center" width="33%">
<a href="../../issues">
<img src="https://img.icons8.com/fluency/96/000000/bug.png" width="48" height="48"><br>
<b>Issues</b>
</a><br>
报告问题和错误
</td>
<td align="center" width="33%">
<a href="../../discussions">
<img src="https://img.icons8.com/fluency/96/000000/chat.png" width="48" height="48"><br>
<b>Discussions</b>
</a><br>
提问和分享想法
</td>
<td align="center" width="33%">
<a href="https://github.com/kirkyx/limiteron">
<img src="https://img.icons8.com/fluency/96/000000/github.png" width="48" height="48"><br>
<b>GitHub</b>
</a><br>
查看源代码
</td>
</tr>
</table>

### 保持联系

[![GitHub](https://img.shields.io/badge/GitHub-View%20Repo-100000?style=for-the-badge&logo=github&logoColor=white)](https://github.com/kirkyx/limiteron)

</div>

---

## ⭐ Star History

<div align="center">

[![Star History Chart](https://api.star-history.com/svg?repos=kirkyx/limiteron&type=Date)](https://star-history.com/#kirkyx/limiteron&Date)

</div>

---

<div align="center">

### 💝 支持本项目

如果你觉得这个项目有用，请考虑给它一个 ⭐️！

**由 Kirky.X 用 ❤️ 构建**

[⬆ 返回顶部](#-limiteron)

---

<sub>© 2026 Kirky.X. 保留所有权利。</sub>

</div>