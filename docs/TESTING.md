# 🧪 Limiteron 测试指南

本文档说明如何运行 Limiteron 项目的测试，包括按 feature 运行测试的详细说明。测试金字塔基线与 E2E 场景定义另见 [测试场景固化](TEST_SCENARIOS.md)，历史覆盖率数据见 [覆盖率报告](COVERAGE_REPORT.md)。

## 📋 目录

<details open>
<summary>📑 目录</summary>

- [🚀 快速开始](#-快速开始)
- [🎨 按 Feature 运行测试](#-按-feature-运行测试)
- [🧱 测试布局](#-测试布局)
- [📊 测试规模基线](#-测试规模基线)
- [📈 测试覆盖率](#-测试覆盖率)
- [✅ 测试最佳实践](#-测试最佳实践)
- [🤖 CI 集成](#-ci-集成)
- [🔧 故障排查](#-故障排查)
- [📚 更多信息](#-更多信息)

</details>

## 🚀 快速开始

### 常用测试命令

```bash
# CI 全量测试口径（与 .github/workflows/ci.yml 一致）
cargo test --workspace --no-default-features --features full

# 库单元测试（full 特性）
cargo test --features full --lib

# 统一集成测试（按 feature 显式启用）
cargo test --test unified_tests --features "ban-manager,quota-control,circuit-breaker"

# 基准测试
cargo bench --features full
```

> 📌 `postgres` / `sqlite` / `mysql` 存储驱动互斥，`--all-features` 会触发 dbnexus 编译错误，请始终使用显式特性组合。

## 🎨 按 Feature 运行测试

Limiteron 使用 feature flags 来模块化功能。以下是各 feature 的测试运行方式：

### 核心 Features

```bash
# 默认特性（default = []，核心限流）
cargo test --no-default-features

# 封禁管理
cargo test --features ban-manager
cargo test --test unified_tests --features ban-manager test_list_bans

# 配额控制
cargo test --features quota-control
cargo test --test unified_tests --features quota-control test_quota

# 熔断器
cargo test --features circuit-breaker
cargo test --test unified_tests --features circuit-breaker test_circuit_breaker
```

### 组合 Features

```bash
# 组合多个 features
cargo test --features "ban-manager,quota-control,circuit-breaker"

# 完整功能集（full 含 postgres，不含 sqlite/mysql，可安全编译）
cargo test --features full

# 标准功能集（sqlite 预设）
cargo test --features standard
```

### 常用 Features 列表

| Feature | 描述 | 测试命令 |
|---------|------|----------|
| `ban-manager` | 封禁管理功能 | `cargo test --features ban-manager` |
| `quota-control` | 配额控制功能 | `cargo test --features quota-control` |
| `circuit-breaker` | 熔断器功能 | `cargo test --features circuit-breaker` |
| `monitoring` | Prometheus 指标 | `cargo test --features monitoring` |
| `telemetry` | 追踪遥测 | `cargo test --features telemetry` |
| `postgres` | PostgreSQL 存储 | `cargo test --features postgres` |
| `sqlite` | SQLite 存储 | `cargo test --features sqlite` |
| `cache-storage` | Redis 缓存后端（经 oxcache） | `cargo test --features cache-storage` |
| `distributed` | 分布式限流 | `cargo test --features distributed` |
| `gcra` | GCRA 限流算法 | `cargo test --features gcra` |
| `parallel-checker` | 并行封禁检查 | `cargo test --features parallel-checker` |
| `audit-log` | 审计日志 | `cargo test --features audit-log` |
| `fallback` | 降级策略 | `cargo test --features fallback` |
| `validation` | 输入校验 | `cargo test --features validation` |
| `full` | 完整功能预设 | `cargo test --features full` |

## 🧱 测试布局

测试按层级组织（详细的层级基线与场景定义见 [测试场景固化](TEST_SCENARIOS.md)）：

| 层级 | 承载位置 | 说明 |
|------|---------|------|
| 单元测试 | `src/**` 内联 `#[cfg(test)]` | governor、limiters、ban、quota、circuit、fallback、matchers 等模块自测 |
| 集成测试 | `tests/` 顶层目标与子目录模块 | `unified_tests`、`integration_tests`、`e2e_tests`、`common_tests`、`security_tests`、`admin_security_tests`、`chaos_tests`、`e2e_advanced` 等 |
| 属性测试 | `tests/property_tests/` | proptest：并发、固定窗口、滑动窗口、令牌桶 |
| 文档测试 | 文档注释代码块 | 随 `cargo test` 编译执行 |
| 基准测试 | `benches/` | criterion：throughput / latency / memory / regression |

```bash
# 运行传统测试入口
cargo test --test common_tests
cargo test --test integration_tests
cargo test --test e2e_tests

# 运行库内特定模块的单元测试
cargo test --lib test_token_bucket
cargo test --lib test_sliding_window
cargo test --lib test_fixed_window
cargo test --lib test_concurrency_limiter
```

## 📊 测试规模基线

以 CI 口径（`--workspace --no-default-features --features full`）的全量运行结果为基线：**38 个测试目标 / 3298 passed / 0 failed / 42 ignored**（ignored 为 manual / 容器依赖门控与文档测试门控）。分层基线与 E2E 场景明细见 [测试场景固化](TEST_SCENARIOS.md)。

> 📌 测试数量随版本演进，以最近一次 CI 全量运行输出为准。

## 📈 测试覆盖率

### 覆盖率门禁

CI 与 pre-push 钩子均启用 **cargo-llvm-cov 行覆盖率 ≥ 80%** 门禁（以 [CI 配置](../.github/workflows/ci.yml) 为准）：

```bash
# 与 CI 完全一致的口径
cargo llvm-cov --workspace --no-default-features --features full --lib --fail-under-lines 80

# 生成 HTML 报告
cargo llvm-cov --workspace --no-default-features --features full --lib --html
# 报告输出到 target/llvm-cov/html/
```

> 历史上曾使用 cargo-tarpaulin 生成覆盖率（v0.1.0 时期基线，见 [覆盖率报告](COVERAGE_REPORT.md)）。使用 tarpaulin 时同样需要注意 `postgres` 与 `sqlite` 互斥，不可使用 `--all-features`。

## ✅ 测试最佳实践

### 1. 运行测试前的准备

```bash
# 确保代码格式正确
cargo fmt --all -- --check

# 运行 clippy 检查（与 CI 同口径）
cargo clippy --all-targets --no-default-features --features full -- -D warnings

# 运行编译检查
cargo check --no-default-features --features full
```

### 2. 并行与输出控制

```bash
# 使用多线程加速测试
cargo test --features full -- --test-threads=4

# 显示测试输出
cargo test --features full -- --show-output
```

### 3. 调试失败的测试

```bash
# 运行单个测试并显示输出
cargo test --features ban-manager test_list_bans_pagination -- --nocapture

# 显示测试的打印输出
cargo test --test unified_tests --features quota-control test_quota_persists_state -- --exact --nocapture

# 运行测试并启用日志
RUST_LOG=debug cargo test --features circuit-breaker -- --nocapture
```

### 4. 控制运行范围

```bash
# 运行特定包的测试
cargo test -p limiteron --features full

# 按名称过滤
cargo test --features full -- quota
```

## 🤖 CI 集成

CI 质量门禁定义在 [ci.yml](../.github/workflows/ci.yml)，包含以下任务：

| 任务 | 命令口径 |
|------|---------|
| Format | `cargo fmt --all -- --check` |
| Clippy | `cargo clippy --workspace --all-targets --no-default-features --features full -- -D warnings` |
| Check | `cargo check --workspace --no-default-features --features full` 与 `--no-default-features` 双口径 |
| Test | `cargo test --workspace --no-default-features --features full` |
| Build | ubuntu / macos / windows 三平台矩阵，full 与 no-default 双口径 |
| Documentation | `cargo doc --workspace --no-deps --no-default-features --features full` |
| Security | `cargo deny check` 与 `cargo audit` |
| Coverage | `cargo llvm-cov ... --fail-under-lines 80` |

## 🔧 故障排查

### 1. Feature 冲突

```bash
# postgres/sqlite/mysql 互斥导致编译失败时，显式指定单驱动组合
cargo test --no-default-features --features "ban-manager,quota-control,sqlite"
```

### 2. 测试超时

```bash
# 串行运行排查时序问题
cargo test --test unified_tests -- --test-threads=1
```

### 3. 内存不足

```bash
# 减少并行测试线程
cargo test --features full -- --test-threads=1
```

## 📚 更多信息

- [测试场景固化](TEST_SCENARIOS.md)：测试金字塔基线与 E2E 场景定义
- [覆盖率报告](COVERAGE_REPORT.md)：历史覆盖率基线与数据口径说明
- [API 参考文档](API_REFERENCE.md)
- [用户指南](USER_GUIDE.md)
- [常见问题](FAQ.md)
