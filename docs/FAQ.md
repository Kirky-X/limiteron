# ❓ Limiteron 常见问题

本文档按主题汇总 Limiteron 的常见问题与解答，覆盖通用问题、安装设置、使用功能、性能、安全与故障排除。系统性教程请见 [用户指南](USER_GUIDE.md)，API 细节请见 [API 参考](API_REFERENCE.md)。

[🏠 首页](../README.md) • [📖 用户指南](USER_GUIDE.md) • [📘 API 参考](API_REFERENCE.md)

---

## 📋 目录

<details open>
<summary>📑 目录</summary>

- [🤔 一般问题](#-一般问题)
- [📦 安装和设置](#-安装和设置)
- [💡 使用和功能](#-使用和功能)
- [⚡ 性能](#-性能)
- [🔒 安全](#-安全)
- [🔧 故障排除](#-故障排除)
- [🤝 贡献](#-贡献)
- [📄 许可证](#-许可证)

</details>

---

## 🤔 一般问题

<details>
<summary><b>❓ 什么是 Limiteron?</b></summary>

<br>

**Limiteron** 是一个 Rust 统一流量控制框架，提供：

- 多种限流算法（令牌桶、滑动/分片滑动/固定窗口、并发控制、GCRA、HTB 分层令牌桶、AIMD 自适应）
- 封禁管理（IP / 用户 / MAC / Geo / CIDR 封禁、自动封禁、封禁优先级、YAML 批量加载）
- 配额管理（周期配额、配额告警、配额透支）
- 熔断与降级（自动熔断、状态恢复、降级策略）

它为需要保护 API 服务免受滥用和突发流量冲击的开发者设计。

**了解更多:** [用户指南](USER_GUIDE.md)

</details>

<details>
<summary><b>❓ Limiteron 的核心能力有哪些?</b></summary>

<br>

| 能力 | 说明 |
|------|------|
| 多维限流 | 令牌桶、滑动/分片滑动/固定窗口、并发控制、GCRA、HTB、AIMD 自适应 |
| 纵深管控 | 封禁、配额、熔断、降级沿一条决策链协同执行 |
| 可插拔底座 | 内存存储开箱即用，经 dbnexus 与 oxcache 接入持久化与分布式缓存 |
| 生产可观测 | Prometheus 指标、OTLP 追踪导出、HMAC 链式审计日志、K8s 探针端点 |
| 声明式接入 | `#[flow_control]` 过程宏、Tower 中间件、Admin REST API、`limiteron-cli` |

**关键优势：**

- 🚀 高性能（令牌桶吞吐 12M+ ops/s，P99 延迟 < 1µs，见[性能](#-性能)）
- 💡 简洁的 API 设计与声明式宏支持
- 📖 完善的文档与 21 个可运行示例
- 🌟 全面的功能（限流、封禁、配额、熔断、降级）

</details>

<details>
<summary><b>❓ 这个项目可以用于生产环境吗?</b></summary>

<br>

**可以。** 当前 0.3.0-rc.x 发布线已在真实项目中验证核心能力：

- ✅ 核心限流与存储抽象稳定，默认构建零外部存储依赖
- ✅ 3298 个测试的分层测试体系与 llvm-cov ≥80% 行覆盖门禁（见[测试指南](TESTING.md)）
- ✅ CI 覆盖 fmt / clippy / 三平台构建 / cargo-deny / cargo-audit / CodeQL
- ✅ 性能数据可经仓库自带 criterion 基准复现（`cargo bench --features full`）

> **注意:** 升级版本前请查看[更新日志](CHANGELOG.md)，破坏性变更均有标注。

</details>

<details>
<summary><b>❓ 支持哪些平台?</b></summary>

<br>

| 平台 | 架构 | 状态 | 说明 |
|------|------|------|------|
| Linux | x86_64 / ARM64 | ✅ 支持 | CI 三平台矩阵覆盖 |
| macOS | x86_64 / ARM64 | ✅ 支持 | Apple Silicon 覆盖 |
| Windows | x86_64 | ✅ 支持 | CI windows runner 覆盖 |

</details>

<details>
<summary><b>❓ 支持哪些编程语言?</b></summary>

<br>

**Rust** 原生支持，完整 API 访问，docs.rs 在线文档见 [docs.rs/limiteron](https://docs.rs/limiteron)。其他语言经 FFI 接入属于远期设想，当前没有官方绑定。

</details>

---

## 📦 安装和设置

<details>
<summary><b>❓ 如何安装 Limiteron?</b></summary>

<br>

**对于 Rust 项目:**

```toml
[dependencies]
limiteron = { version = "0.3.0-rc.3", features = ["macros"] }
```

或使用 cargo:

```bash
cargo add limiteron --features macros
```

**从源码安装:**

```bash
git clone https://github.com/Kirky-X/limiteron
cd limiteron
cargo build --release
```

**验证安装:**

```rust
use limiteron::limiters::TokenBucketLimiter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limiter = TokenBucketLimiter::new(10, 1);
    println!("✅ 安装成功！");
    Ok(())
}
```

**另请参阅:** [用户指南](USER_GUIDE.md#安装)

</details>

<details>
<summary><b>❓ 系统要求是什么?</b></summary>

<br>

| 组件 | 要求 | 推荐 |
|------|------|------|
| Rust 版本 | 1.97.1+（[rust-toolchain.toml](../rust-toolchain.toml) 锁定） | 最新稳定版 |
| 内存 | 512 MB | 2 GB+ |
| 磁盘空间 | 50 MB | 100 MB |
| CPU | 1 核心 | 4+ 核心 |

**可选:**

- PostgreSQL / MySQL / SQLite（经 dbnexus 持久化存储）
- Redis（经 oxcache 缓存与分布式限流）
- Docker（容器化部署）

</details>

<details>
<summary><b>❓ 遇到编译错误怎么办?</b></summary>

<br>

**常见解决方案:**

1. **更新 Rust 工具链:**
   ```bash
   rustup update stable
   ```

2. **清理构建产物:**
   ```bash
   cargo clean
   cargo build
   ```

3. **检查 Rust 版本:**
   ```bash
   rustc --version
   # 应该是 1.97.1 或更高
   ```

4. **检查 feature 组合：** `postgres` / `sqlite` / `mysql` 三种存储驱动互斥，`--all-features` 会触发 dbnexus 编译错误，请使用显式特性组合。

**还有问题?**
- 📝 查看 [故障排除](#-故障排除)
- 🐛 [创建 issue](https://github.com/Kirky-X/limiteron/issues) 并附上错误详情

</details>

<details>
<summary><b>❓ 可以在 Docker 中使用吗?</b></summary>

<br>

**可以！** 这是一个示例 Dockerfile:

```dockerfile
FROM rust:1.97-slim AS builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/limiteron /usr/local/bin/

CMD ["limiteron"]
```

**Docker Compose:**

```yaml
services:
  app:
    build: .
    ports:
      - "8080:8080"
    environment:
      - RUST_LOG=info
```

**使用 docker-compose 启动服务:**

```bash
docker-compose up -d
```

</details>

---

## 💡 使用和功能

<details>
<summary><b>❓ 如何开始基础使用?</b></summary>

<br>

**5 分钟快速开始:**

```rust
use limiteron::limiters::{Limiter, TokenBucketLimiter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 创建限流器
    let limiter = TokenBucketLimiter::new(10, 1);

    // 2. 检查限流
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

**下一步:**
- 📖 [用户指南](USER_GUIDE.md)
- 💻 [更多示例](../examples/)

</details>

<details>
<summary><b>❓ 支持哪些限流算法?</b></summary>

<br>

| 算法 | 类型 | 说明 |
|------|------|------|
| 令牌桶 | `TokenBucketLimiter` | 固定速率补充令牌，允许突发 |
| 分片滑动窗口 | `ShardedSlidingWindowLimiter` | 高并发下的精确滑动窗口 |
| 滑动窗口 | `SlidingWindowLimiter`（已弃用导出） | 旧版滑动窗口 |
| 固定窗口 | `FixedWindowLimiter` | 固定时间窗口内限制请求数 |
| 并发控制 | `ConcurrencyLimiter` | 限制同时处理的请求数 |
| GCRA | `GcraLimiter`（`gcra` 特性） | 信元速率算法，平滑限流 |
| HTB | `HierarchicalTokenBucket` | 分层令牌桶，父子带宽借用 |
| AIMD 自适应 | `AdaptiveConcurrencyLimiter`（`adaptive-limiting` 特性） | 按延迟/错误率反馈调窗 |

**另请参阅:** [算法详情](API_REFERENCE.md#-限流器)

</details>

<details>
<summary><b>❓ 可以同时使用多个限流器吗?</b></summary>

<br>

**可以！** 使用决策链组合多个限流器:

```rust
use limiteron::Governor;
use limiteron::adapters::StorageFactory;
use limiteron::matchers::RequestContext;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建存储
    let mut factory = StorageFactory::from_dsn("postgresql://localhost/limiteron");
    factory.initialize(None).await?;
    let storage = factory.create_storage().await?;
    let ban_storage = factory.create_ban_storage().await?;

    // 创建 Governor
    let governor = Governor::builder()
        .with_storage(storage)
        .with_ban_storage(ban_storage)
        .build()
        .await?;

    // 决策链会按优先级依次检查所有限流器
    let context = RequestContext::new()
        .with_header("X-User-Id", "user123")
        .with_path("/api/v1/users")
        .with_method("GET");

    let decision = governor.check(&context).await?;
    match decision {
        limiteron::error::Decision::Allowed(_) => { /* 处理请求 */ }
        _ => { /* 拒绝或封禁处理 */ }
    }

    Ok(())
}
```

**好处:**
- 🔒 多层保护
- 🎯 更精细的控制
- 📊 更好的安全性

</details>

<details>
<summary><b>❓ 如何正确处理错误?</b></summary>

<br>

**推荐模式:**

```rust
use limiteron::error::LimiteronError;
use limiteron::limiters::TokenBucketLimiter;

async fn process_request(limiter: &TokenBucketLimiter) -> Result<(), LimiteronError> {
    match limiter.allow(1).await {
        Ok(true) => {
            println!("✅ 成功");
            Ok(())
        }
        Ok(false) => {
            println!("⚠️ 速率限制");
            Ok(())
        }
        Err(LimiteronError::RateLimitExceeded(msg)) => {
            println!("⚠️ 速率限制: {}", msg);
            Ok(())
        }
        Err(LimiteronError::BanError(msg)) => {
            eprintln!("❌ 已封禁: {}", msg);
            Err(LimiteronError::BanError(msg))
        }
        Err(e) => {
            eprintln!("❌ 错误: {:?}", e);
            Err(e)
        }
    }
}
```

**错误类型:**
- [错误参考](API_REFERENCE.md#-错误处理)

</details>

<details>
<summary><b>❓ 支持 async/await 吗?</b></summary>

<br>

**完全支持。** 全部核心 API 均为异步（基于 tokio），限流器同时实现 `Send + Sync` 可跨任务共享。

**示例:**

```rust
use limiteron::limiters::{Limiter, TokenBucketLimiter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limiter = TokenBucketLimiter::new(10, 1);

    match limiter.allow(1).await {
        Ok(true) => println!("✅ 请求允许"),
        Ok(false) => println!("❌ 请求被限流"),
        Err(e) => println!("❌ 错误: {:?}", e),
    }

    Ok(())
}
```

**使用宏:**

```rust
use limiteron::flow_control;

#[flow_control(rate = "10/s")]
async fn api_handler() -> Result<String, limiteron::error::LimiteronError> {
    Ok("Success".to_string())
}
```

</details>

---

## ⚡ 性能

<details>
<summary><b>❓ 性能如何?</b></summary>

<br>

**基准测试结果**（2026-01-19 实测，详见 [README 性能](../README.md#-性能)）：

| 操作 | 吞吐量 | 延迟 (P50) | 延迟 (P99) |
|------|--------|-----------|-----------|
| 令牌桶检查 | 12M+ ops/s | < 100ns | < 1µs |
| 固定窗口检查 | 20M+ ops/s | < 100ns | < 500ns |
| 并发检查 | 12M+ ops/s | < 100ns | < 1µs |

**自己运行基准测试:**

```bash
cargo bench --features full
```

</details>

<details>
<summary><b>❓ 如何提高性能?</b></summary>

<br>

**优化技巧:**

1. **启用 Release 模式:**
   ```bash
   cargo build --release
   ```

2. **利用 L1 负缓存:**
   ```rust
   // L1 负缓存默认启用，且为负缓存语义——仅缓存"拒绝/封禁"决策，
   // "允许"决策永不入缓存，因此任何一次请求都会重新执行限流/封禁检查。
   let governor = Governor::builder()
       .with_l1_cache_enabled(true)
       .build()
       .await?;
   ```

3. **使用全局共享实例:**
   ```rust
   use limiteron::limiters::TokenBucketLimiter;
   use std::sync::LazyLock;

   static LIMITER: LazyLock<TokenBucketLimiter> =
       LazyLock::new(|| TokenBucketLimiter::new(10, 1));
   ```

4. **使用宏:**
   ```rust
   #[flow_control(rate = "100/s")]
   async fn api_handler() -> Result<String, limiteron::error::LimiteronError> {
       Ok("Success".to_string())
   }
   ```

</details>

<details>
<summary><b>❓ 内存使用如何?</b></summary>

<br>

**内存特征（定性）：**

| 组件 | 内存特征 | 说明 |
|------|---------|------|
| 核心限流器 | 单实例 KB 级 | 原子计数器状态 |
| L1 负缓存 | 受容量上限约束 | 默认 1000 条，可经 `L1CacheConfig` 调整 |
| MemoryStorage | 随活跃 key 数增长 | 限流器管理器带 LRU 淘汰（上限 100,000 条）防刷爆 |
| 持久化后端 | 由连接池配置决定 | dbnexus 连接池参数可控 |

**减少内存使用:**

```rust
// 通过限制 L1 负缓存容量来减少内存使用
let governor = Governor::builder()
    .with_l1_cache_config(L1CacheConfig::new(Duration::from_secs(60), 1000))
    .build()
    .await?;
```

**内存安全:**
- ✅ Rust 内存安全保证
- ✅ 限流器管理器 LRU 淘汰防无界增长
- ✅ 后台任务 `Drop` 防泄漏

</details>

---

## 🔒 安全

<details>
<summary><b>❓ 这个库安全吗?</b></summary>

<br>

**安全设计贯穿始终。** 全部安全机制均可在源码与[安全文档](SECURITY.md)中对应：

| 防线 | 机制 |
|------|------|
| 输入防线 | 标识符 key 消毒（ASCII 白名单 + 128 字符截断）、IP / 用户 ID / MAC 格式校验 |
| 算法边界 | 令牌桶饱和运算封顶、配额窗口时钟回退防护 |
| Admin 自保护 | 管理端点自身限流、分桶内存上限、多 key 令牌认证与 RBAC |
| 数据保护 | secrecy 保护敏感数据、日志脱敏、审计事件 HMAC-SHA256 链式签名 |
| 传输防线 | 可信代理 X-Forwarded-For 提取、Webhook 外发签名与防重放、SSRF URL 校验 |
| 供应链 | rustls-webpki 最低版本锁（CVE-2025-48369）、cargo-deny / cargo-audit / CodeQL |

**更多详情:** [安全文档](SECURITY.md)

</details>

<details>
<summary><b>❓ 如何报告安全漏洞?</b></summary>

<br>

**请负责任地报告安全问题:**

1. **不要**创建公开 issue 披露漏洞细节
2. **优先使用 GitHub 私密漏洞报告**：仓库页 → Security → Report a vulnerability；若不可用，可在 Issues 中报告但**不要**包含可直接利用的细节、PoC 载荷或影响线上环境的信息
3. **包括:** 受影响版本、feature 组合、复现步骤、影响评估

**响应时间线**（与 [安全文档](SECURITY.md) 一致）:

- 📧 确认: 48 小时内
- 🔍 初步评估: 7 天内
- 🔧 修复: 先在私有分支进行，随下一个版本发布
- 📢 披露: 协调披露，修复发布后在更新日志"安全"分类记录

</details>

<details>
<summary><b>❓ 数据存储如何?</b></summary>

<br>

**数据存储选项:**

| 方法 | 适用场景 | 说明 |
|------|---------|------|
| 内存 | 开发、测试 | 开箱即用，重启即失 |
| SQLite（经 dbnexus） | 单机部署 | 嵌入式持久化，无需外部服务 |
| PostgreSQL / MySQL（经 dbnexus） | 生产部署 | 参数化查询防注入，建议启用 TLS |
| Redis（经 oxcache / lua-script） | 分布式缓存与限流 | Lua 脚本原子操作 |

**最佳实践:**

```rust
// 1. 使用环境变量管理连接串
let dsn = std::env::var("DATABASE_URL")?;

// 2. 设置适当的权限
// 确保数据库访问权限最小化
```

</details>

<details>
<summary><b>❓ 有已知漏洞吗?</b></summary>

<br>

**当前状态:** ✅ **无已知未修复漏洞**

**我们如何维护安全:**

1. **依赖扫描:**
   ```bash
   cargo audit
   cargo deny check
   ```

2. **定期更新:** CI 与 pre-push 钩子持续运行安全审计；对已知漏洞依赖做最低版本锁定（如 rustls-webpki，CVE-2025-48369）

3. **测试:** 安全测试套件（`security_tests`、`admin_security_tests`）与静态分析（CodeQL）

**保持知情:**
- 🔔 关注此仓库
- 📰 查看 [Security Advisories](https://github.com/Kirky-X/limiteron/security/advisories)

</details>

---

## 🔧 故障排除

<details>
<summary><b>❓ 限流不生效</b></summary>

<br>

**问题:**

```text
所有请求都通过了限流
```

**原因:** 每次请求都创建了新的 limiter 实例。

**解决方案:**

```rust
use limiteron::limiters::{Limiter, TokenBucketLimiter};
use std::sync::Arc;

// 使用 Arc 共享 limiter 实例
let limiter = Arc::new(TokenBucketLimiter::new(10, 1));

// 在多个请求中使用同一个实例
for i in 0..100 {
    let limiter_clone = Arc::clone(&limiter);
    tokio::spawn(async move {
        match limiter_clone.allow(1).await {
            Ok(true) => println!("请求 {} 允许", i),
            Ok(false) => println!("请求 {} 被限流", i),
            Err(e) => println!("请求 {} 错误: {:?}", i, e),
        }
    });
}
```

</details>

<details>
<summary><b>❓ 性能比预期慢</b></summary>

<br>

**检查清单:**

- [ ] 是否在 release 模式运行?
  ```bash
  cargo run --release
  ```

- [ ] 是否启用了 L1 负缓存?
  ```rust
  let governor = Governor::builder()
      .with_l1_cache_enabled(true)
      .build()
      .await?;
  ```

- [ ] 是否使用 Arc / LazyLock 共享实例?
  ```rust
  let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
  ```

**更多帮助:** [README 性能](../README.md#-性能)

</details>

<details>
<summary><b>❓ 内存使用过高</b></summary>

<br>

**解决方案:**

```rust
// 调低 L1 负缓存容量
let governor = Governor::builder()
    .with_l1_cache_config(L1CacheConfig::new(Duration::from_secs(60), 1000))
    .build()
    .await?;
```

限流器管理器自带 LRU 淘汰（上限 100,000 条），恶意 key 不会无限累积。

</details>

**更多问题?** 查看 [用户指南故障排除](USER_GUIDE.md#-故障排除)

---

## 🤝 贡献

<details>
<summary><b>❓ 如何贡献?</b></summary>

<br>

**贡献方式:**

| 代码贡献 | 非代码贡献 |
|---------|-----------|
| 🐛 修复 bug | 📖 编写教程 |
| ✨ 添加功能 | 🌍 翻译文档 |
| 📝 改进文档 | 💬 回答问题 |
| ✅ 编写测试 | 🎨 优化示例 |

**开始:**

1. 🍴 Fork 仓库
2. 🌱 创建分支
3. ✏️ 进行修改
4. ✅ 添加测试
5. 📤 提交 PR

详细流程见[贡献指南](CONTRIBUTING.md)。

</details>

<details>
<summary><b>❓ 发现了 bug，怎么办?</b></summary>

<br>

**报告前:**

1. ✅ 查看 [现有 issues](https://github.com/Kirky-X/limiteron/issues)
2. ✅ 尝试最新版本
3. ✅ 查看 [故障排除指南](USER_GUIDE.md#-故障排除)

**创建好的 bug 报告:**

```markdown
### 描述
bug 的清晰描述

### 复现步骤
1. 步骤一
2. 步骤二
3. 看到错误

### 预期行为
应该发生什么

### 实际行为
实际发生了什么

### 环境
- OS: Ubuntu 22.04
- Rust version: 1.97.1
- limiteron version: 0.3.0-rc.3

### 其他上下文
任何其他相关信息
```

**提交:** [创建 Issue](https://github.com/Kirky-X/limiteron/issues/new)

</details>

<details>
<summary><b>❓ 在哪里可以获得帮助?</b></summary>

<br>

| 渠道 | 用途 |
|------|------|
| 🐛 [GitHub Issues](https://github.com/Kirky-X/limiteron/issues) | Bug 报告和功能请求 |
| 💬 [GitHub Discussions](https://github.com/Kirky-X/limiteron/discussions) | 问答和想法 |
| 📦 [GitHub 仓库](https://github.com/Kirky-X/limiteron) | 查看源代码 |

</details>

---

## 📄 许可证

<details>
<summary><b>❓ 使用什么许可证?</b></summary>

<br>

**本项目基于 MIT 许可证发布**（自 v0.2.3 起，此前为 Apache-2.0，详见 [更新日志](CHANGELOG.md)）。

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](../LICENSE)

**权限:**
- ✅ 商业使用
- ✅ 修改
- ✅ 分发
- ✅ 私有使用

**唯一条件:** 保留许可证与版权声明（Copyright (c) 2026 Kirky.X）。

</details>

<details>
<summary><b>❓ 可以在商业项目中使用吗?</b></summary>

<br>

**可以！** MIT 许可证允许商业使用。

**你需要做的:**
1. ✅ 包含许可证文本
2. ✅ 包含版权声明
3. ✅ 声明任何修改

**你不需要做的:**
- ❌ 共享你的源代码
- ❌ 开源你的项目
- ❌ 支付版权费

</details>

---

### 🎯 还有其他问题?

| 渠道 | 入口 |
|------|------|
| 🐛 创建 Issue | [Issues](https://github.com/Kirky-X/limiteron/issues) |
| 💬 开始讨论 | [Discussions](https://github.com/Kirky-X/limiteron/discussions) |
| 🏠 GitHub 仓库 | [Kirky-X/limiteron](https://github.com/Kirky-X/limiteron) |

---

[📖 用户指南](USER_GUIDE.md) • [📘 API 参考](API_REFERENCE.md) • [🏠 返回首页](../README.md)
