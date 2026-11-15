[English](README.md) | 中文

<div align="center">

<img src="docs/assets/limiteron.png" alt="Limiteron Logo" width="240">

[![Rust 2024](https://img.shields.io/badge/Rust-2024-edded?logo=rust&style=for-the-badge)](https://www.rust-lang.org/) [![Rustc 1.97.1+](https://img.shields.io/badge/Rustc-1.97.1+-orange.svg?style=for-the-badge)](https://www.rust-lang.org/) [![crates.io](https://img.shields.io/crates/v/limiteron?style=for-the-badge)](https://crates.io/crates/limiteron) [![docs.rs](https://img.shields.io/docsrs/limiteron?style=for-the-badge)](https://docs.rs/limiteron) [![downloads](https://img.shields.io/crates/d/limiteron?style=for-the-badge)](https://crates.io/crates/limiteron) [![CI](https://img.shields.io/github/actions/workflow/status/Kirky-X/limiteron/ci.yml?branch=main&label=CI&style=for-the-badge)](https://github.com/Kirky-X/limiteron/actions/workflows/ci.yml) [![coverage](https://img.shields.io/codecov/c/github/Kirky-X/limiteron?style=for-the-badge)](https://codecov.io/gh/Kirky-X/limiteron) [![license](https://img.shields.io/crates/l/limiteron?style=for-the-badge)](LICENSE)

**中文** | [English](README.md)

**Rust 统一流量控制框架**

[✨ 功能特性](#-功能特性) • [🚀 快速开始](#-快速开始) • [📚 文档](#-文档) • [💻 示例](#-示例) • [🤝 参与贡献](#-参与贡献)

</div>

---

<div align="center" style="padding: 32px; margin: 24px 0">

### 🚦 统一流量控制

一个 Governor 入口，多维限流与纵深管控协同决策：

<table style="width:100%; border-collapse: collapse">
<tr><td align="center" width="25%" style="padding: 12px">🚦<br><b>多维限流</b><br><span style="color:#64748B">令牌桶、滑动/固定窗口、漏桶、滑动窗口日志、并发控制、GCRA、HTB 分层令牌桶</span></td><td align="center" width="25%" style="padding: 12px">🛡️<br><b>纵深管控</b><br><span style="color:#64748B">封禁、配额、熔断、降级沿一条决策链协同执行</span></td><td align="center" width="25%" style="padding: 12px">🔌<br><b>可插拔底座</b><br><span style="color:#64748B">内存存储开箱即用，经 dbnexus 与 oxcache 接入持久化与分布式缓存</span></td><td align="center" width="25%" style="padding: 12px">📈<br><b>生产可观测</b><br><span style="color:#64748B">Prometheus 指标、OTLP 追踪导出、HMAC 链式审计日志</span></td></tr>
</table>

</div>

---

## 📋 目录

<details open>
<summary>📑 目录</summary>

- [✨ 功能特性](#-功能特性)
- [🚀 快速开始](#-快速开始)
- [🎨 特性标志](#-特性标志)
- [📚 文档](#-文档)
- [💻 示例](#-示例)
- [🏗️ 架构](#️-架构)
- [🧪 测试](#-测试)
- [📊 性能](#-性能)
- [🔒 安全](#-安全)
- [🗺️ 开发路线图](#️-开发路线图)
- [🤝 参与贡献](#-参与贡献)
- [📋 更新日志](#-更新日志)
- [📄 许可证](#-许可证)
- [🙏 致谢](#-致谢)
- [📞 联系与支持](#-联系与支持)
- [⭐ Star 历史](#-star-历史)

</details>

---

## ✨ 功能特性

<table style="width:100%; border-collapse: collapse">
<tr>
<td width="50%" style="vertical-align:top; padding: 12px">🎯 <b>限流算法矩阵</b><br><span style="color:#64748B">令牌桶（Token Bucket）、滑动窗口、分片滑动窗口、滑动窗口日志（精确滑动窗口）、固定窗口、漏桶（Leaky Bucket 水位记账，判定与令牌桶对偶）、并发控制、GCRA、HTB 分层令牌桶（<code>src/limiters/</code>）</span></td>
<td width="50%" style="vertical-align:top; padding: 12px">🚫 <b>封禁管理</b><br><span style="color:#64748B">IP / 用户 / MAC / Geo 目标封禁、CIDR 网段封禁、优先级体系、YAML 批量加载与热重载、跨实例同步（<code>ban-sync</code>）</span></td>
</tr>
<tr>
<td width="50%" style="vertical-align:top; padding: 12px">⚖️ <b>配额控制</b><br><span style="color:#64748B">周期性配额分配、配额预警、配额透支（<code>src/quota/</code>）</span></td>
<td width="50%" style="vertical-align:top; padding: 12px">🧯 <b>熔断与降级</b><br><span style="color:#64748B">自动故障转移、状态恢复、降级策略（<code>src/circuit/</code>、<code>src/fallback.rs</code>）</span></td>
</tr>
<tr>
<td width="50%" style="vertical-align:top; padding: 12px">🚀 <b>高性能</b><br><span style="color:#64748B">令牌桶吞吐 12M+ ops/s、P99 延迟 &lt; 1µs（见[性能](#-性能)）</span></td>
<td width="50%" style="vertical-align:top; padding: 12px">🧩 <b>声明式接入</b><br><span style="color:#64748B"><code>#[flow_control]</code> 过程宏、Tower 中间件、Admin REST API、<code>limiteron-cli</code></span></td>
</tr>
<tr>
<td width="50%" style="vertical-align:top; padding: 12px">🏢 <b>多租户</b><br><span style="color:#64748B">tenant+key 复合决策键，缓存/封禁/配额按租户隔离</span></td>
<td width="50%" style="vertical-align:top; padding: 12px">📈 <b>可观测性</b><br><span style="color:#64748B">Prometheus 指标、OTLP 追踪导出、HMAC-SHA256 链式审计日志、K8s 探针端点</span></td>
</tr>
<tr>
<td width="50%" style="vertical-align:top; padding: 12px">🔐 <b>安全内建</b><br><span style="color:#64748B">标识符 key 消毒、日志脱敏、Webhook 签名防重放、Admin RBAC</span></td>
<td width="50%" style="vertical-align:top; padding: 12px">🧠 <b>决策链级联</b><br><span style="color:#64748B">DecisionChain 按优先级级联执行多条规则，支持短路（<code>src/decision_chain/</code>）</span></td>
</tr>
</table>

完整能力清单与 API 明细见 [API 参考](docs/cn/API_REFERENCE.md)。

---

## 🚀 快速开始

### 📦 安装

<details>
<summary>📄 代码 · bash</summary>

```bash
cargo add limiteron
```

</details>

环境要求：Rust 1.97.1+（见 [rust-toolchain.toml](rust-toolchain.toml)）。默认特性为空（`default = []`），核心限流零外部存储依赖；需要持久化时启用存储特性：

<details>
<summary>📄 代码 · toml</summary>

```toml
[dependencies]
limiteron = { version = "0.3", features = ["macros"] }
```

</details>

### 💡 最小可运行示例

以下示例来自 [`examples/src/bin/simple_rate_limit.rs`](examples/src/bin/simple_rate_limit.rs)，演示最基本的令牌桶限流：

<details>
<summary>📄 代码 · rust</summary>

```rust
use limiteron::limiters::{Limiter, TokenBucketLimiter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 容量 10，每秒补充 1 个令牌
    let limiter = TokenBucketLimiter::new(10, 1);

    for i in 0..15 {
        match limiter.allow(1).await {
            Ok(true) => println!("请求 {} 允许", i),
            Ok(false) => println!("请求 {} 被限流", i),
            Err(e) => println!("请求 {} 错误: {:?}", i, e),
        }
    }
    Ok(())
}
```

</details>

运行方式：

<details>
<summary>📄 代码 · bash</summary>

```bash
cargo run -p limiteron-examples --bin simple_rate_limit
```

</details>

声明式宏接入见[用户指南 · 常见模式](docs/cn/USER_GUIDE.md#-常见模式)；核心概念与完整 API 见 [API 参考](docs/cn/API_REFERENCE.md)。

---

## 🎨 特性标志

Limiteron 默认不启用任何可选功能（`default = []`），按需组合。特性预设（minimal / standard / full）、全部特性明细与文件型 SQLite 部署提示见[用户指南 · 特性标志](docs/cn/USER_GUIDE.md#-特性标志)；特性依赖关系见[架构文档](docs/cn/ARCHITECTURE.md#-特性依赖总览)。

---

## 📚 文档

<details>
<summary>📋 表格</summary>

| 文档 | 说明 |
|------|------|
| [📖 用户指南](docs/cn/USER_GUIDE.md) | 从安装、核心概念到进阶用法与故障排除的完整教程 |
| [📘 API 参考](docs/cn/API_REFERENCE.md) | 全部公开 API 的详细说明 |
| [🏗️ 架构文档](docs/cn/ARCHITECTURE.md) | 设计理念、模块划分与扩展机制 |
| [❓ FAQ](docs/cn/FAQ.md) | 常见问题解答与故障排除 |
| [🧪 测试指南](docs/cn/TESTING.md) | 测试分类、运行命令与覆盖率说明 |
| [📈 基准测试规程](docs/cn/BENCHMARKS.md) | `cargo bench` 口径、基线刷新时机与 criterion 对比用法 |
| [🧬 测试场景固化](docs/cn/TEST_SCENARIOS.md) | 测试金字塔基线与 E2E 场景定义 |
| [🔒 安全文档](docs/cn/SECURITY.md) | 安全设计、版本支持策略与漏洞报告流程 |
| [📋 更新日志](docs/CHANGELOG.md) | 每个版本的变更记录 |
| [🤝 贡献指南](docs/cn/CONTRIBUTING.md) | 如何参与项目开发 |
| [📦 docs.rs 在线文档](https://docs.rs/limiteron) | 自动生成的最新 API 文档 |
| [📦 crates.io](https://crates.io/crates/limiteron) | 发布页面 |

</details>

---

## 💻 示例

[`examples/`](examples/) 是 workspace 内的独立子 crate（`limiteron-examples`），含 21 个可运行示例；完整示例清单与运行命令见 [examples/README.md](examples/README.md)。

---

## 🏗️ 架构

Limiteron 采用分层架构：接入层（Tower 中间件、Admin API）将流量交给 **Governor** 主控制器，经 matchers 提取标识符并匹配规则后，沿规则各自的 decision_chain 决策链级联执行 limiters 限流算法；封禁、配额、熔断、降级作为领域组件参与决策；状态经 storage 抽象落地（默认内存，生产可切换 dbnexus 的 PostgreSQL / SQLite / MySQL 持久化）。

整体架构图、核心决策时序、模块职责表、存储后端清单与扩展机制见[架构文档](docs/cn/ARCHITECTURE.md)。

---

### 🎯 核心决策流程

一次 `Governor::check(context)` 的完整决策路径（提炼自 `src/governor.rs`）：标识符提取与规则匹配（只计算一次）→ L1 负缓存查询（fail-closed，仅缓存拒绝/封禁决策）→ 按优先级级联执行各规则决策链（任一规则拒绝即拒绝，全部允许才放行）→ 发射限流事件并返回 `Allowed` / `Rejected` / `Banned`。

完整决策时序图与逐条关键语义（含 `parallel-checker` 下封禁检查先于缓存读取的语义）见[架构文档](docs/cn/ARCHITECTURE.md#-核心决策流程)。

---

### 🔗 生态与集成

Limiteron 与同工作区的兄弟 crate 深度协作，均通过 feature 显式启用、默认不引入：

| 集成 | 特性 | 说明 |
|------|------|------|
| [dbnexus](https://github.com/Kirky-X/dbnexus) | `postgres` / `sqlite` / `mysql` | 数据库抽象层，提供持久化存储适配器与指标传递 |
| [oxcache](https://github.com/Kirky-X/oxcache) | `cache-service` / `cache-redis` / `lua-script` / `ban-sync` | 统一缓存服务、Redis 缓存后端、Lua 执行、Pub/Sub 封禁广播 |
| [trait-kit](https://github.com/Kirky-X/trait-kit) | `kit` | `LimiteronModule` 模块化接入 + 健康/生命周期端口 |
| [confers](https://github.com/Kirky-X/confers) | `config-confers` / `config-confers-reload` | 配置源加载与热重载（验证失败自动回滚） |

inklog、ICU4X、sdforge 防护层与 dbnexus 限流端口适配等集成设计说明见[架构文档 · 生态与集成](docs/cn/ARCHITECTURE.md#-生态与集成)。

---

## 🧪 测试

测试策略矩阵（单元 / 集成与端到端 / 属性 / 文档 / 基准五层）与运行命令（与 [CI](.github/workflows/ci.yml) 一致，含覆盖率门禁）见[测试指南](docs/cn/TESTING.md)，分层基线与 E2E 场景定义见[测试场景固化](docs/cn/TEST_SCENARIOS.md)。

测试规模统计（按 `#[test]` / `#[tokio::test]` 属性 grep 口径）见[测试指南 · 测试规模统计](docs/cn/TESTING.md#-测试规模统计)。

---

## 📊 性能

历史实测数据（v0.1.0 时期口径，仅供量级对照）与基准设施明细已归档至[基准规程 · 历史数据对照](docs/cn/BENCHMARKS.md#历史数据对照)；当前可复现基线以该文件[运行](docs/cn/BENCHMARKS.md#运行)章节的命令与同机复测为准。

---

## 🔒 安全

**漏洞报告**：请勿通过公开 issue 报告安全漏洞，请使用 GitHub [Security Advisories](https://github.com/Kirky-X/limiteron/security/policy) 私密披露通道（"Report a vulnerability"）。维护者承诺 48 小时内确认、7 天内给出初步评估，采用协调披露。完整流程见 [SECURITY.md](docs/cn/SECURITY.md)。

安全设计要点（输入防线、算法边界、Admin 自保护、只读 Web UI 警示、数据保护、传输防线与供应链）详见[安全文档](docs/cn/SECURITY.md)。

---

## 🗺️ 开发路线图

<table>
<tr>
<td width="12%" align="center"><b>✅ 已完成</b></td>
<td>核心限流算法、封禁管理、配额控制、熔断器、<code>#[flow_control]</code> 宏、单元与集成测试体系、PostgreSQL / SQLite 存储（经 dbnexus）、Governor 优雅关闭与健康检查、ConfigLoader 环境变量覆盖（v0.2.0）；Tower 中间件完善、事件系统增强、RedisStorage 移除并统一经 oxcache 缓存（v0.2.1）</td>
</tr>
<tr>
<td width="12%" align="center"><b>✅ 已交付（随 0.3.0-rc.4 发布）</b></td>
<td>多租户贯穿 Governor、CIDR 网段封禁、Admin RBAC、OTLP 追踪导出、MySQL 存储、分布式限流器、<code>limiteron-cli</code> 等新能力（详见<a href="docs/CHANGELOG.md">更新日志</a>）</td>
</tr>
<tr>
<td width="12%" align="center"><b>✅ 已交付</b></td>
<td>性能优化与监控追踪增强——criterion 四组基准修复并统一 fresh 实例放行口径（<code>90fb0cb</code>）、HeaderMatcher ASCII 零分配快路径削减热点（12 项基准 -10.0%~-33.4%，<code>4cc7c6f</code>）、per-rule/per-limiter 维度指标与 <code>governor_check</code> span（<code>5422c6d</code>）；OTLP 追踪导出已随 0.3.0-rc.4 交付</td>
</tr>
<tr>
<td width="12%" align="center"><b>📋 计划中</b></td>
<td>更多存储后端</td>
</tr>
<tr>
<td width="12%" align="center"><b>💡 未来想法</b></td>
<td>「机器学习驱动的限流」已经裁决具体化为统计启发式（`adaptive-limiting` AIMD 并发窗口与 `adaptive-threshold` 滑动窗口动态配额，均为确定性阈值逻辑，非神经网络——见 `src/limiters/adaptive_threshold.rs` 模块文档）；「社区插件系统」已落地为编译期注册制（`plugins`，动态 .so 明确不做，见 `src/plugins/`）</td>
</tr>
</table>

---

## 🤝 参与贡献

欢迎任何形式的贡献！开发环境、TDD 工作流、代码规范与 PR 流程详见[贡献指南](docs/cn/CONTRIBUTING.md)。

<table>
<tr>
<td width="33%" align="center">

### 🐛 报告问题

发现了 Bug？<br>
[创建 Issue](https://github.com/Kirky-X/limiteron/issues)

</td>
<td width="33%" align="center">

### 💡 功能建议

有好想法？<br>
[发起讨论](https://github.com/Kirky-X/limiteron/discussions)

</td>
<td width="33%" align="center">

### 🔧 提交代码

想参与开发？<br>
[Fork & PR](https://github.com/Kirky-X/limiteron/pulls)

</td>
</tr>
</table>

<details>
<summary><b>🔧 开发环境基线</b></summary>

<br>

Rust 1.97.1（[rust-toolchain.toml](rust-toolchain.toml) 统一锁定）；lefthook / pre-commit 钩子覆盖 fmt、clippy、供应链检查、私钥扫描与覆盖率门禁。环境搭建、钩子明细与提交规范全文见[贡献指南](docs/cn/CONTRIBUTING.md)。

</details>

---

## 📋 更新日志

完整变更记录见 [CHANGELOG.md](docs/CHANGELOG.md)。

---

## 📄 许可证

本项目基于 [MIT](LICENSE) 许可证发布。

Copyright (c) 2026 Kirky.X🌠

---

## 🙏 致谢

- 🌟 **依赖项目** — 本项目构建在这些优秀的项目之上：
  - [tokio](https://tokio.rs/) — 异步运行时
  - [dbnexus](https://github.com/Kirky-X/dbnexus) — 数据库抽象层
  - [oxcache](https://github.com/Kirky-X/oxcache) — 统一缓存服务
  - [trait-kit](https://github.com/Kirky-X/trait-kit) — 模块标准接口与掩码引擎（trait-kit 仓的 masking 特性，本仓经 log-redaction 消费）
  - [inklog](https://github.com/Kirky-X/inklog) — 结构化日志（按 target 分桶 sink 限流见 `integrations/limiteron-inklog` 适配器）
  - [dashmap](https://github.com/xacrimon/dashmap) — 并发 HashMap
- 👥 **贡献者** — 感谢所有贡献者！
- 💬 **社区** — 特别感谢社区成员的支持

---

## 📞 联系与支持

- 🐛 [Issues](https://github.com/Kirky-X/limiteron/issues) — 报告 Bug 与问题
- 💬 [Discussions](https://github.com/Kirky-X/limiteron/discussions) — 提问与交流想法
- 📦 [GitHub 仓库](https://github.com/Kirky-X/limiteron) — 查看源代码
- 🤝 维护者：Kirky.X

---

## ⭐ Star 历史

[![Star History Chart](https://api.star-history.com/svg?repos=Kirky-X/limiteron&type=Date)](https://star-history.com/#Kirky-X/limiteron&Date)

### 💝 支持本项目

如果您觉得这个项目有用，请考虑给它一个 ⭐️！

**Built with ❤️ by Kirky.X**
