# Limiteron 测试指南

本文档说明如何运行 Limiteron 项目的测试，包括按 feature 运行测试的详细说明。

## 目录

- [快速开始](#快速开始)
- [按 Feature 运行测试](#按-feature-运行测试)
- [集成测试](#集成测试)
- [单元测试](#单元测试)
- [测试覆盖率](#测试覆盖率)

## 快速开始

### 运行所有测试

```bash
# 运行所有测试（包括所有 features）
cargo test --all-features

# 运行库的单元测试
cargo test --lib

# 运行所有集成测试
cargo test --test '*_tests'
```

## 按 Feature 运行测试

Limiteron 使用 feature flags 来模块化功能。以下是各 feature 的测试运行方式：

### 核心 Features

#### 1. 基础功能 (default)

```bash
# 运行默认功能测试
cargo test
```

#### 2. Ban Manager (封禁管理)

```bash
# 运行 ban-manager feature 测试
cargo test --features ban-manager

# 运行 ban-manager 集成测试
cargo test --test unified_tests --features ban-manager test_list_bans

# 运行特定测试
cargo test --features ban-manager test_ban
```

#### 3. Quota Control (配额控制)

```bash
# 运行 quota-control feature 测试
cargo test --features quota-control

# 运行配额集成测试
cargo test --test unified_tests --features quota-control test_quota

# 运行特定配额测试
cargo test --features quota-control test_consume
```

#### 4. Circuit Breaker (熔断器)

```bash
# 运行 circuit-breaker feature 测试
cargo test --features circuit-breaker

# 运行熔断器集成测试
cargo test --test unified_tests --features circuit-breaker test_circuit_breaker

# 运行特定熔断器测试
cargo test --features circuit-breaker test_circuit
```

### 组合 Features

```bash
# 运行多个 features 的测试
cargo test --features "ban-manager,quota-control,circuit-breaker"

# 运行所有可选 features
cargo test --all-features

# 运行标准功能集
cargo test --features standard
```

### 可选 Features 列表

| Feature | 描述 | 测试命令 |
|---------|------|----------|
| `ban-manager` | 封禁管理功能 | `cargo test --features ban-manager` |
| `quota-control` | 配额控制功能 | `cargo test --features quota-control` |
| `circuit-breaker` | 熔断器功能 | `cargo test --features circuit-breaker` |
| `monitoring` | 监控指标功能 | `cargo test --features monitoring` |
| `telemetry` | 遥测功能 | `cargo test --features telemetry` |
| `postgres` | PostgreSQL 存储 | `cargo test --features postgres` |
| `redis` | Redis 缓存 | `cargo test --features redis` |
| `parallel-checker` | 并行封禁检查 | `cargo test --features parallel-checker` |
| `audit-log` | 审计日志 | `cargo test --features audit-log` |
| `fallback` | 降级策略 | `cargo test --features fallback` |
| `validation` | 配置验证 | `cargo test --features validation` |
| `full` | 所有功能 | `cargo test --features full` |

## 集成测试

### Unified Tests (推荐)

`unified_tests` 是新的统一测试入口，包含所有模块的集成测试：

```bash
# 运行所有统一集成测试
cargo test --test unified_tests --features "ban-manager,quota-control,circuit-breaker"

# 运行特定模块的集成测试
cargo test --test unified_tests --features ban-manager test_list_bans_pagination
cargo test --test unified_tests --features quota-control test_quota_persists_state
cargo test --test unified_tests --features circuit-breaker test_circuit_breaker_recovers_after_timeout
```

### 传统测试入口

```bash
# Common 测试
cargo test --test common_tests

# 集成测试
cargo test --test integration_tests

# E2E 测试
cargo test --test e2e_tests
```

## 单元测试

### 运行库内单元测试

```bash
# 运行所有库内单元测试
cargo test --lib

# 运行特定模块的单元测试
cargo test --lib test_token_bucket
cargo test --lib test_sliding_window
cargo test --lib test_fixed_window
cargo test --lib test_concurrency_limiter
```

### 按模块运行测试

```bash
# Ban Manager 测试
cargo test --lib limiteron::ban_manager

# Quota Controller 测试
cargo test --lib limiteron::quota_controller

# Circuit Breaker 测试
cargo test --lib limiteron::circuit_breaker

# Governor 测试
cargo test --lib limiteron::governor
```

## 测试覆盖率

### 使用 cargo-tarpaulin (任务 5.1)

```bash
# 安装 cargo-tarpaulin
cargo install cargo-tarpaulin

# 生成覆盖率报告（所有 features）
cargo tarpaulin --all-features --out Html

# 生成特定 feature 的覆盖率报告
cargo tarpaulin --features ban-manager --out Html
cargo tarpaulin --features quota-control --out Html
cargo tarpaulin --features circuit-breaker --out Html

# 生成终端输出
cargo tarpaulin --all-features --out Stdout

# 设置最低覆盖率阈值
cargo tarpaulin --all-features --threshold 70
```

### 覆盖率目标

- 核心模块：> 70% 覆盖率
- 关键路径：> 80% 覆盖率
- 工具函数：> 60% 覆盖率

## 测试最佳实践

### 1. 运行测试前的准备

```bash
# 确保代码格式正确
cargo fmt --check

# 运行 clippy 检查
cargo clippy --all-features

# 运行编译检查
cargo check --all-features
```

### 2. 并行运行测试

```bash
# 使用多线程加速测试
cargo test --all-features -- --test-threads=4

# 显示测试输出
cargo test --all-features -- --show-output
```

### 3. 调试失败的测试

```bash
# 运行单个测试并显示输出
cargo test --features ban-manager test_list_bans_pagination -- --nocapture

# 显示测试的打印输出
cargo test --features quota-control test_quota_persists_state -- --exact --nocapture

# 运行测试并启用日志
RUST_LOG=debug cargo test --features circuit-breaker -- --nocapture
```

### 4. 只运行更改的测试

```bash
# 只运行未通过的测试
cargo test --all-features -- --ignored

# 运行特定包的测试
cargo test -p limiteron --all-features
```

## CI/CD 集成

### GitHub Actions 示例

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        feature:
          - ban-manager
          - quota-control
          - circuit-breaker
          - "ban-manager,quota-control,circuit-breaker"
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run tests
        run: cargo test --features ${{ matrix.feature }}
```

## 故障排查

### 常见问题

#### 1. Feature 冲突

```bash
# 错误：feature 冲突
# 解决：使用 --no-default-flags
cargo test --no-default-features --features "ban-manager,quota-control"
```

#### 2. 测试超时

```bash
# 增加测试超时时间
cargo test --test unified_tests -- --test-threads=1
```

#### 3. 内存不足

```bash
# 减少并行测试线程
cargo test --all-features -- --test-threads=1
```

## 测试覆盖率

### 覆盖率报告

项目当前测试状态: **1209 个测试全部通过 ✅**

| 测试类型 | 测试数量 | 状态 |
|---------|---------|------|
| 单元测试 | 523 | ✅ 通过 |
| 集成测试 (unified_tests) | 192 | ✅ 通过 |
| 集成测试 (integration_tests) | 247 | ✅ 通过 |
| 安全测试 | 82 | ✅ 通过 |
| E2E 测试 | 165 | ✅ 通过 |

详细报告请查看: [COVERAGE_REPORT.md](./COVERAGE_REPORT.md)

### 生成覆盖率报告

```bash
# 安装 tarpaulin
cargo install cargo-tarpaulin

# 生成覆盖率报告
cargo tarpaulin --out Html --out Json --features minimal

# 查看 HTML 报告
open tarpaulin-report.html
```

### 覆盖率目标

| 阶段 | 目标覆盖率 | 状态 |
|------|-----------|------|
| P0 | 所有测试通过 | ✅ 已达成 (1209 tests) |
| P1 | 代码覆盖率 60% | 🔄 进行中 |
| P2 | 代码覆盖率 75% | 📋 计划中 |

## 更多信息

- [API 参考文档](API_REFERENCE.md)
- [用户指南](USER_GUIDE.md)
- [架构分析](ARCHITECTURE_ANALYSIS.md)
- [常见问题](FAQ.md)
