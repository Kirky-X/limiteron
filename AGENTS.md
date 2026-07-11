# Agents Guide

## Overview

limiteron 是 Rust 统一流量控制框架，提供限流（rate limiting）、配额管理（quota control）、熔断（circuit breaking）、封禁管理（ban management）等能力。版本 0.2.2，MIT 许可证，edition 2024，rust-version 1.85。Workspace 结构包含主 crate（`.`）、宏 crate（`macros`）和示例 crate（`examples`）。

## Project Structure

```
limiteron/
├── src/
│   ├── lib.rs              # 公共 API 重新导出（所有 pub 项集中于此）
│   ├── governor.rs         # 主控制器，端到端流量控制
│   ├── limiters/           # 限流算法
│   │   ├── token_bucket.rs #   令牌桶
│   │   ├── fixed_window.rs #   固定窗口
│   │   ├── sliding_window.rs #   滑动窗口
│   │   ├── sharded_sliding_window.rs #   分片滑动窗口
│   │   ├── gcra.rs         #   GCRA 算法（feature-gated）
│   │   ├── concurrency.rs  #   并发控制
│   │   ├── quota_limiter.rs #   配额限流器
│   │   ├── factory.rs      #   限流器工厂
│   │   ├── manager.rs      #   限流器生命周期管理
│   │   └── traits.rs       #   Limiter trait
│   ├── matchers/           # 标识符提取与规则匹配
│   │   ├── extractors.rs   #   IP/User/Device/APIKey/MAC 提取器
│   │   ├── engine.rs       #   匹配引擎
│   │   ├── composite.rs    #   组合匹配
│   │   ├── custom.rs       #   自定义匹配器
│   │   ├── geo.rs          #   地理位置匹配（geo-matching）
│   │   └── device.rs       #   设备信息匹配（device-matching）
│   ├── ban/                # 封禁管理（ban-manager）
│   │   ├── mod.rs          #   BanManager / BanTarget / BanDetail
│   │   ├── types.rs        #   封禁类型
│   │   └── file_loader.rs  #   YAML 批量加载 + 热重载
│   ├── quota/              # 配额控制（quota-control）
│   │   └── controller.rs   #   QuotaController
│   ├── circuit/            # 熔断器（circuit-breaker）
│   │   └── mod.rs          #   CircuitBreaker
│   ├── fallback.rs         # 降级策略（fallback）
│   ├── storage/            # 存储 trait 与内存实现
│   │   ├── mod.rs          #   Storage / BanStorage / QuotaStorage trait + MemoryStorage
│   │   └── parallel_checker.rs #   并行封禁检查（parallel-checker）
│   ├── adapters/           # DBNexus 存储适配器（postgres）
│   │   ├── dbnexus_storage.rs
│   │   ├── dbnexus_ban_storage.rs
│   │   ├── dbnexus_quota_storage.rs
│   │   └── storage_factory.rs
│   ├── cache/              # oxcache 统一缓存服务（cache-service）
│   │   ├── cache_service.rs #   CacheService trait
│   │   ├── memory_cache.rs  #   MemoryCache 实现
│   │   ├── storage.rs       #   CacheStorage（cache-storage）
│   │   ├── ban_storage.rs   #   CacheBanStorage（cache-storage）
│   │   └── quota_storage.rs #   CacheQuotaStorage（cache-storage）
│   ├── config/             # 配置类型与加载器
│   ├── decision_chain/     # 策略决策引擎
│   ├── events/             # 事件系统（event-system）
│   ├── middleware/         # Tower HTTP 中间件（tower-middleware）
│   ├── admin/              # 管理 REST API（admin-api）
│   ├── telemetry/          # 指标与追踪（telemetry / monitoring）
│   ├── logging/            # 审计日志与日志脱敏
│   ├── error/              # 错误类型与抽象
│   ├── rules/              # 规则构建器与统计
│   ├── tenant/             # 多租户（multi-tenant）
│   ├── i18n/               # ICU4X 国际化（i18n）
│   ├── integrations/       # 外部集成（kit）
│   ├── oxcache_lua.rs      # Lua 脚本（lua-script）
│   ├── clock.rs            # 时钟抽象
│   ├── l1_cache.rs         # L1 缓存
│   ├── authorization.rs    # 授权 provider
│   ├── validation.rs       # 输入验证
│   ├── constants.rs
│   └── webhook_validator.rs # Webhook（webhook）
├── macros/                  # limiteron-macros proc-macro crate
├── examples/                # 使用示例
├── benches/                 # 性能基准测试
├── tests/                   # 集成 / E2E 测试
├── Cargo.toml               # workspace + 主 crate 定义
└── deny.toml                # cargo-deny 配置
```

## Where to Look

| 任务 | 位置 | 说明 |
|------|------|------|
| 公共 API | `src/lib.rs` | 所有重新导出集中于此 |
| 主控制器 | `src/governor.rs` | Governor：端到端流量控制 |
| 核心限流 | `src/limiters/` | 令牌桶、固定窗口、滑动窗口、GCRA、并发 |
| 熔断器 | `src/circuit/` | 自动故障转移、状态恢复 |
| 降级 | `src/fallback.rs` | FallbackManager |
| 封禁 | `src/ban/` | BanManager、BanFileLoader、热重载 |
| 配额 | `src/quota/` | QuotaController |
| 网关 | `src/governor.rs` | Governor 主控制器 |
| 存储 trait | `src/storage/mod.rs` | Storage / BanStorage / QuotaStorage |
| DBNexus 适配器 | `src/adapters/` | PostgreSQL 持久化（postgres feature） |
| 缓存服务 | `src/cache/` | oxcache 统一缓存（cache-service / cache-storage） |
| 标识符匹配 | `src/matchers/` | IP / User / Device / APIKey / Geo / 自定义 |
| 策略决策 | `src/decision_chain/` | DecisionChain |
| Tower 中间件 | `src/middleware/` | RateLimitLayer（tower-middleware） |
| 管理 API | `src/admin/` | REST 端点（admin-api） |
| 指标/追踪 | `src/telemetry/` | Prometheus + OpenTelemetry |
| 审计/脱敏 | `src/logging/` | audit_log、log_redaction |
| 事件系统 | `src/events/` | EventEmitter / EventDispatcher |
| 多租户 | `src/tenant/` | Namespace / TenantResolver |
| 宏 | `src/macros.rs` + `macros/` | `#[flow_control]` |
| 配置 | `src/config/` | ConfigLoader / ConfigBuilder |

## Conventions

- **Edition 2024**，**rust 1.85+**，**MIT License**
- 依赖必须通过 feature 门控（`optional = true`）
- 100 字符最大行宽（`rustfmt.toml`），4 空格缩进
- 禁止通配符导入（`warn-on-all-wildcard-imports`）
- 使用 `ahash` + `DashMap`，禁止 `std::collections::HashMap/Set`
- 使用 `parking_lot` 替代 `std::sync` 原语
- 所有公开 API 必须有文档注释
- 不安全代码必须注明安全不变式
- **TDD 开发流程**：Red → Green → Commit → Analyze → Next（详见 CONTRIBUTING.md）
- 不修改外部依赖代码（oxcache / dbnexus / confers）— 出现问题上报而非自行修复

### 依赖注入架构

特性组件支持三种构造模式：

- `new()` — 开箱即用（内部创建默认依赖，使用 MemoryStorage）
- `builder()` — 部分依赖注入
- `with_dependencies()` — 完整依赖注入（生产环境）

所有依赖以 `Arc<dyn Trait>` 存储，trait 必须实现 `Send + Sync`。

## Commands

```bash
# 构建
cargo build --all-features
cargo build --features minimal      # 核心限流 + PostgreSQL
cargo build --features standard     # 核心 + 基础高级功能
cargo build --features full         # 全部功能

# 测试
cargo test --all-features --lib     # 单元测试
cargo test --test integration_tests -- --ignored  # 集成测试（需 Postgres/Redis）

# 代码质量
cargo fmt
cargo fmt --all -- --check
cargo clippy --all-features --all-targets -- -D warnings
cargo deny check                    # 安全审计

# 基准测试
cargo bench --release --bench throughput
cargo bench --release --bench latency

# 覆盖率
cargo tarpaulin --all-features --workspace --timeout 300
```

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

本项目由 GitNexus 索引为 **limiteron**。使用 GitNexus MCP 工具理解代码、评估影响、安全导航。

> 若 GitNexus 工具提示索引过期，先在终端运行 `npx gitnexus analyze`。

## Always Do

- **修改任何符号前必须运行影响分析**：`gitnexus_impact({target: "symbolName", direction: "upstream"})`，报告爆炸半径（直接调用者、受影响流程、风险等级）。
- **提交前必须运行 `gitnexus_detect_changes()`**，验证变更只影响预期符号与执行流。
- **影响分析返回 HIGH 或 CRITICAL 时必须警告用户**，再决定是否继续。
- 探索陌生代码时使用 `gitnexus_query({query: "concept"})` 查找执行流（按相关性排序的流程分组结果）。
- 需要某符号完整上下文（调用者、被调用者、参与的执行流）时使用 `gitnexus_context({name: "symbolName"})`。

## Never Do

- 永不未运行 `gitnexus_impact` 就修改函数 / 类 / 方法。
- 永不忽略 HIGH 或 CRITICAL 风险警告。
- 永不用查找替换重命名符号 — 使用 `gitnexus_rename`（理解调用图）。
- 永不未运行 `gitnexus_detect_changes()` 检查影响范围就提交。

## Resources

| Resource | 用途 |
|----------|------|
| `gitnexus://repo/limiteron/context` | 代码库概览、检查索引新鲜度 |
| `gitnexus://repo/limiteron/clusters` | 所有功能区域 |
| `gitnexus://repo/limiteron/processes` | 所有执行流 |
| `gitnexus://repo/limiteron/process/{name}` | 逐步执行追踪 |

<!-- gitnexus:end -->
