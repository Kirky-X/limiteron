<div align="center">

# ❓ 常见问题 (FAQ)

### 常见问题快速解答

[🏠 首页](../README.md) • [📖 用户指南](USER_GUIDE.md) • [📚 API 参考](API_REFERENCE.md)

---

</div>

## 📋 目录

- [一般问题](#一般问题)
- [安装和设置](#安装和设置)
- [使用和功能](#使用和功能)
- [性能](#性能)
- [安全](#安全)
- [故障排除](#故障排除)
- [贡献](#贡献)
- [许可证](#许可证)

---

## 一般问题

<div align="center">

### 🤔 关于项目

</div>

<details>
<summary><b>❓ 什么是 Limiteron?</b></summary>

<br>

**Limiteron** 是一个 Rust 统一流量控制框架，提供：

- ✅ 多种限流算法（令牌桶、固定窗口、滑动窗口、并发控制）
- ✅ 封禁管理（IP 封禁、自动封禁、封禁优先级）
- ✅ 配额管理（配额分配、配额告警、配额透支）
- ✅ 熔断器（自动熔断、状态恢复、降级策略）

它为需要保护 API 服务免受滥用和 DDoS 攻击的开发者设计。

**了解更多:** [用户指南](USER_GUIDE.md)

</details>

<details>
<summary><b>❓ 为什么选择 Limiteron 而不是其他方案?</b></summary>

<br>

<table>
<tr>
<th>特性</th>
<th>Limiteron</th>
<th>governor</th>
<th>bucket4j</th>
</tr>
<tr>
<td>性能</td>
<td>⚡⚡⚡</td>
<td>⚡⚡</td>
<td>⚡</td>
</tr>
<tr>
<td>易用性</td>
<td>✅ 简单</td>
<td>✅ 简单</td>
<td>⚠️ 复杂</td>
</tr>
<tr>
<td>文档</td>
<td>📚 完善</td>
<td>📚 良好</td>
<td>📄 基础</td>
</tr>
<tr>
<td>功能</td>
<td>🌟 全面</td>
<td>🌟 限流</td>
<td>🌟 限流</td>
</tr>
</table>

**关键优势:**
- 🚀 更好的性能（延迟 < 200μs P99）
- 💡 更简单的 API 设计（宏支持）
- 📖 完善的文档和示例
- 🌟 全面的功能（限流、封禁、配额、熔断）

</details>

<details>
<summary><b>❓ 这个项目可以用于生产环境吗?</b></summary>

<br>

**当前状态:** ✅ **是的，可以用于生产环境！**

<table>
<tr>
<td width="50%">

**已就绪:**
- ✅ 核心功能稳定
- ✅ 全面的测试
- ✅ 安全审计
- ✅ 性能优化
- ✅ 文档完善

</td>
<td width="50%">

**成熟度指标:**
- 👥 活跃的开发
- 📝 完整的文档
- 🔄 定期更新
- ✨ 多种限流算法

</td>
</tr>
</table>

> **注意:** 升级版本前请查看提交历史。

</details>

<details>
<summary><b>❓ 支持哪些平台?</b></summary>

<br>

<table>
<tr>
<th>平台</th>
<th>架构</th>
<th>状态</th>
<th>说明</th>
</tr>
<tr>
<td rowspan="2"><b>Linux</b></td>
<td>x86_64</td>
<td>✅ 完全支持</td>
<td>主要平台</td>
</tr>
<tr>
<td>ARM64</td>
<td>✅ 完全支持</td>
<td>在 ARM 服务器上测试</td>
</tr>
<tr>
<td rowspan="2"><b>macOS</b></td>
<td>x86_64</td>
<td>✅ 完全支持</td>
<td>Intel Mac</td>
</tr>
<tr>
<td>ARM64</td>
<td>✅ 完全支持</td>
<td>Apple Silicon (M1/M2)</td>
</tr>
<tr>
<td><b>Windows</b></td>
<td>x86_64</td>
<td>✅ 完全支持</td>
<td>Windows 10+</td>
</tr>
</table>

</details>

<details>
<summary><b>❓ 支持哪些编程语言?</b></summary>

<br>

<table>
<tr>
<td width="50%" align="center">

**🦀 Rust**

✅ **原生支持**

完整的 API 访问

</td>
<td width="50%" align="center">

**🌐 其他语言**

📋 **计划中**

通过 FFI

</td>
</tr>
</table>

**文档:**
- [Rust API](https://docs.rs/limiteron)

</details>

---

## 安装和设置

<div align="center">

### 🚀 快速开始

</div>

<details>
<summary><b>❓ 如何安装 Limiteron?</b></summary>

<br>

**对于 Rust 项目:**

```toml
[dependencies]
limiteron = "1.0"
```

或使用 cargo:

```bash
cargo add limiteron
```

**从源码安装:**

```bash
git clone https://github.com/kirkyx/limiteron
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

**最低要求:**

<table>
<tr>
<th>组件</th>
<th>要求</th>
<th>推荐</th>
</tr>
<tr>
<td>Rust 版本</td>
<td>1.75+</td>
<td>最新稳定版</td>
</tr>
<tr>
<td>内存</td>
<td>512 MB</td>
<td>2 GB+</td>
</tr>
<tr>
<td>磁盘空间</td>
<td>50 MB</td>
<td>100 MB</td>
</tr>
<tr>
<td>CPU</td>
<td>1 核心</td>
<td>4+ 核心</td>
</tr>
</table>

**可选:**
- 🔧 PostgreSQL（用于持久化存储）
- 🔧 Redis（用于缓存和分布式限流）
- 🐳 Docker（用于容器化部署）

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
   # 应该是 1.75.0 或更高
   ```

4. **验证依赖:**
   ```bash
   cargo tree
   ```

**还有问题?**
- 📝 查看 [故障排除](#故障排除)
- 🐛 [创建 issue](../../issues) 并附上错误详情

</details>

<details>
<summary><b>❓ 可以在 Docker 中使用吗?</b></summary>

<br>

**可以！** 这是一个示例 Dockerfile:

```dockerfile
FROM rust:1.75-slim as builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/limiteron /usr/local/bin/

CMD ["limiteron"]
```

**Docker Compose:**

```yaml
version: '3.8'
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

## 使用和功能

<div align="center">

### 💡 使用 API

</div>

<details>
<summary><b>❓ 如何开始基础使用?</b></summary>

<br>

**5 分钟快速开始:**

```rust
use limiteron::limiters::TokenBucketLimiter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 创建限流器
    let mut limiter = TokenBucketLimiter::new(10, 1);
    let key = "user123";

    // 2. 检查限流
    for i in 0..15 {
        match limiter.check(key).await {
            Ok(_) => println!("请求 {} ✅", i),
            Err(_) => println!("请求 {} ❌", i),
        }
    }

    println!("✅ 成功！");
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

<div align="center">

### 🔐 支持的限流算法

</div>

**令牌桶:**
- ✅ 令牌桶限流器 (TokenBucketLimiter)
- ✅ 固定速率补充令牌

**固定窗口:**
- ✅ 固定窗口限流器 (FixedWindowLimiter)
- ✅ 在固定时间窗口内限制请求数

**滑动窗口:**
- ✅ 滑动窗口限流器 (SlidingWindowLimiter)
- ✅ 使用滑动时间窗口提供更精确的限流

**并发控制:**
- ✅ 并发限流器 (ConcurrencyLimiter)
- ✅ 限制同时处理的请求数

**另请参阅:** [算法详情](API_REFERENCE.md#限流器)

</details>

<details>
<summary><b>❓ 可以同时使用多个限流器吗?</b></summary>

<br>

**可以！** 使用决策链组合多个限流器:

```rust
use limiteron::{Governor, FlowControlConfig};
use limiteron::limiters::{TokenBucketLimiter, FixedWindowLimiter};

let governor = Governor::new(FlowControlConfig::default()).await?;

// 决策链会依次检查所有限流器
let decision = governor.check_request("user123", "/api/v1/users").await?;
if decision.is_allowed() {
    // 处理请求
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
use limiteron::error::FlowGuardError;

async fn process_request() -> Result<(), FlowGuardError> {
    match limiter.check(key).await {
        Ok(_) => {
            println!("✅ 成功");
            Ok(())
        }
        Err(FlowGuardError::RateLimitExceeded(msg)) => {
            println!("⚠️ 速率限制: {}", msg);
            Ok(())
        }
        Err(FlowGuardError::Banned(msg)) => {
            eprintln!("❌ 已封禁: {}", msg);
            Err(FlowGuardError::Banned(msg))
        }
        Err(e) => {
            eprintln!("❌ 错误: {:?}", e);
            Err(e)
        }
    }
}
```

**错误类型:**
- [错误参考](API_REFERENCE.md#错误处理)

</details>

<details>
<summary><b>❓ 支持 async/await 吗?</b></summary>

<br>

**当前状态:** ✅ **完全支持**

**示例:**

```rust
use limiteron::limiters::TokenBucketLimiter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limiter = TokenBucketLimiter::new(10, 1);
    
    // 在异步上下文中使用
    match limiter.check("user123").await {
        Ok(_) => println!("✅ 请求允许"),
        Err(_) => println!("❌ 请求被限流"),
    }
    
    Ok(())
}
```

**使用宏:**

```rust
use limiteron::flow_control;

#[flow_control(rate = "10/s")]
async fn api_handler() -> Result<String, FlowGuardError> {
    Ok("Success".to_string())
}
```

</details>

---

## 性能

<div align="center">

### ⚡ 速度和优化

</div>

<details>
<summary><b>❓ 性能如何?</b></summary>

<br>

**基准测试结果:**

<table>
<tr>
<th>操作</th>
<th>吞吐量</th>
<th>延迟 (P50)</th>
<th>延迟 (P99)</th>
</tr>
<tr>
<td>令牌桶检查</td>
<td>500,000 ops/s</td>
<td>0.1 ms</td>
<td>< 0.2 ms</td>
</tr>
<tr>
<td>固定窗口检查</td>
<td>300,000 ops/s</td>
<td>0.15 ms</td>
<td>< 0.2 ms</td>
</tr>
<tr>
<td>并发检查</td>
<td>200,000 ops/s</td>
<td>0.2 ms</td>
<td>< 0.2 ms</td>
</tr>
</table>

**自己运行基准测试:**

```bash
cargo bench
```

**另请参阅:** [性能指南](../README.md#性能)

</details>

<details>
<summary><b>❓ 如何提高性能?</b></summary>

<br>

**优化技巧:**

1. **启用 Release 模式:**
   ```bash
   cargo build --release
   ```

2. **使用缓存:**
   ```rust
   use limiteron::l2_cache::L2Cache;
   
   let cache = L2Cache::new(10000, 3600)?;
   ```

3. **使用全局实例:**
   ```rust
   lazy_static! {
       static ref LIMITER: TokenBucketLimiter = TokenBucketLimiter::new(10, 1);
   }
   ```

4. **使用宏:**
   ```rust
   #[flow_control(rate = "100/s")]
   async fn api_handler() -> Result<String, FlowGuardError> {
       Ok("Success".to_string())
   }
   ```

</details>

<details>
<summary><b>❓ 内存使用如何?</b></summary>

<br>

**典型内存使用:**

<table>
<tr>
<th>场景</th>
<th>内存使用</th>
<th>说明</th>
</tr>
<tr>
<td>基础初始化</td>
<td>~10 MB</td>
<td>最低开销</td>
</tr>
<tr>
<td>使用 L2 缓存 (10,000 条目)</td>
<td>~50 MB</td>
<td>每条目 ~4KB</td>
</tr>
<tr>
<td>使用 L3 缓存 (100,000 条目)</td>
<td>~200 MB</td>
<td>可配置</td>
</tr>
<tr>
<td>高吞吐模式</td>
<td>~100 MB</td>
<td>额外缓冲区</td>
</tr>
</table>

**减少内存使用:**

```rust
let cache = L2Cache::new(1000, 3600)?;
```

**内存安全:**
- ✅ Rust 内存安全保证
- ✅ 无内存泄漏
- ✅ 自动清理

</details>

---

## 安全

<div align="center">

### 🔒 安全特性

</div>

<details>
<summary><b>❓ 这个库安全吗?</b></summary>

<br>

**是的！** 安全是我们的首要任务。

**安全特性:**

<table>
<tr>
<td width="50%">

**实现**
- ✅ 内存安全 (Rust)
- ✅ 输入验证
- ✅ SQL 注入防护
- ✅ 密码保护

</td>
<td width="50%">

**保护**
- ✅ 缓冲区溢出保护
- ✅ 侧信道抵抗
- ✅ 内存安全
- ✅ 参数化查询

</td>
</tr>
</table>

**更多详情:** [安全指南](../README.md#安全)

</details>

<details>
<summary><b>❓ 如何报告安全漏洞?</b></summary>

<br>

**请负责任地报告安全问题:**

1. **不要**创建公开的 GitHub issues
2. **通过 GitHub Issues 报告**，并标记为 security
3. **包括:**
   - 漏洞描述
   - 复现步骤
   - 潜在影响
   - 建议的修复（如果有）

**响应时间线:**
- 📧 初始响应: 24 小时
- 🔍 评估: 72 小时
- 🔧 修复（如果有效）: 7-30 天
- 📢 公开披露: 修复发布后

</details>

<details>
<summary><b>❓ 数据存储如何?</b></summary>

<br>

**数据存储选项:**

<table>
<tr>
<th>方法</th>
<th>安全性</th>
<th>使用场景</th>
</tr>
<tr>
<td><b>内存</b></td>
<td>🔒 良好</td>
<td>开发、测试</td>
</tr>
<tr>
<td><b>PostgreSQL</b></td>
<td>🔒🔒 更好</td>
<td>单服务器部署</td>
</tr>
<tr>
<td><b>Redis</b></td>
<td>🔒🔒 更好</td>
<td>分布式部署</td>
</tr>
</table>

**最佳实践:**

```rust
// 1. 使用环境变量
let redis_url = env::var("REDIS_URL")?;

// 2. 设置适当的权限
// 确保数据库访问权限最小化
```

</details>

<details>
<summary><b>❓ 有已知漏洞吗?</b></summary>

<br>

**当前状态:** ✅ **无已知漏洞**

**我们如何维护安全:**

1. **依赖扫描:**
   ```bash
   cargo audit
   ```

2. **定期更新:**
   - 每周依赖更新
   - 48 小时内安全补丁

3. **测试:**
   - 模糊测试
   - 静态分析
   - 安全代码审查

**保持知情:**
- 🔔 关注此仓库
- 📰 查看 [GitHub Security Advisories](../../security/advisories)

</details>

---

## 故障排除

<div align="center">

### 🔧 常见问题

</div>

<details>
<summary><b>❓ 限流不生效</b></summary>

<br>

**问题:**
```
所有请求都通过了限流
```

**原因:** 每次请求都创建了新的 limiter 实例。

**解决方案:**

```rust
// 使用全局共享的 limiter 实例
lazy_static! {
    static ref LIMITER: TokenBucketLimiter = TokenBucketLimiter::new(10, 1);
}

// 或使用宏
#[flow_control(rate = "10/s")]
async fn handler() -> Result<(), FlowGuardError> {
    Ok(())
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

- [ ] 是否使用了缓存?
  ```rust
  let cache = L2Cache::new(10000, 3600)?;
  ```

- [ ] 是否使用全局实例?
  ```rust
  lazy_static! {
      static ref LIMITER: TokenBucketLimiter = TokenBucketLimiter::new(10, 1);
  }
  ```

**更多帮助:** [性能指南](../README.md#性能)

</details>

<details>
<summary><b>❓ 内存使用过高</b></summary>

<br>

**解决方案:**

```rust
// 减少缓存大小
let cache = L2Cache::new(1000, 3600)?;
```

</details>

**更多问题?** 查看 [用户指南](USER_GUIDE.md#故障排除)

---

## 贡献

<div align="center">

### 🤝 加入社区

</div>

<details>
<summary><b>❓ 如何贡献?</b></summary>

<br>

**贡献方式:**

<table>
<tr>
<td width="50%">

**代码贡献**
- 🐛 修复 bug
- ✨ 添加功能
- 📝 改进文档
- ✅ 编写测试

</td>
<td width="50%">

**非代码贡献**
- 📖 编写教程
- 🎨 设计资源
- 🌍 翻译文档
- 💬 回答问题

</td>
</tr>
</table>

**开始:**

1. 🍴 Fork 仓库
2. 🌱 创建分支
3. ✏️ 进行修改
4. ✅ 添加测试
5. 📤 提交 PR

</details>

<details>
<summary><b>❓ 发现了 bug，怎么办?</b></summary>

<br>

**报告前:**

1. ✅ 查看 [现有 issues](../../issues)
2. ✅ 尝试最新版本
3. ✅ 查看 [故障排除指南](USER_GUIDE.md#故障排除)

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
- Rust version: 1.75.0
- Project version: 1.0.0

### 其他上下文
任何其他相关信息
```

**提交:** [创建 Issue](../../issues/new)

</details>

<details>
<summary><b>❓ 在哪里可以获得帮助?</b></summary>

<br>

<div align="center">

### 💬 支持渠道

</div>

<table>
<tr>
<td width="50%" align="center">

**🐛 Issues**

[GitHub Issues](../../issues)

Bug 报告和功能请求

</td>
<td width="50%" align="center">

**💬 Discussions**

[GitHub Discussions](../../discussions)

问答和想法

</td>
</tr>
</table>

**响应时间:**
- 🐛 严重 bug: 24 小时
- 🔧 功能请求: 1 周
- 💬 问题: 2-3 天

</details>

---

## 许可证

<div align="center">

### 📄 许可证信息

</div>

<details>
<summary><b>❓ 使用什么许可证?</b></summary>

<br>

**双重许可证:**

<table>
<tr>
<td width="50%" align="center">

**MIT 许可证**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](../LICENSE-MIT)

**权限:**
- ✅ 商业使用
- ✅ 修改
- ✅ 分发
- ✅ 私有使用

</td>
<td width="50%" align="center">

**Apache 2.0 许可证**

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](../LICENSE-APACHE)

**权限:**
- ✅ 商业使用
- ✅ 修改
- ✅ 分发
- ✅ 专利授权

</td>
</tr>
</table>

**你可以选择任一许可证使用。**

</details>

<details>
<summary><b>❓ 可以在商业项目中使用吗?</b></summary>

<br>

**可以！** MIT 和 Apache 2.0 许可证都允许商业使用。

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

<div align="center">

### 🎯 还有其他问题?

<table>
<tr>
<td width="33%" align="center">
<a href="../../issues">
<img src="https://img.icons8.com/fluency/96/000000/bug.png" width="48"><br>
<b>创建 Issue</b>
</a>
</td>
<td width="33%" align="center">
<a href="../../discussions">
<img src="https://img.icons8.com/fluency/96/000000/chat.png" width="48"><br>
<b>开始讨论</b>
</a>
</td>
<td width="33%" align="center">
<a href="https://github.com/kirkyx/limiteron">
<img src="https://img.icons8.com/fluency/96/000000/github.png" width="48"><br>
<b>GitHub</b>
</a>
</td>
</tr>
</table>

---

**[📖 用户指南](USER_GUIDE.md)** • **[📚 API 参考](API_REFERENCE.md)** • **[🏠 首页](../README.md)**

由文档团队制作

[⬆ 返回顶部](#-常见问题-faq)

</div>