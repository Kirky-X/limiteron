# Limiteron 项目上下文

## 项目概述

Limiteron 是一个用 Rust 编写的统一流量控制框架，提供限流、配额管理、封禁管理和熔断器功能。该项目为高并发、高可靠性的企业级应用提供流量治理能力。

### 核心技术栈

- **语言**: Rust 2021 edition
- **异步运行时**: Tokio 1.35
- **存储后端**: PostgreSQL (sqlx 0.7)、Redis (0.24)、内存存储
- **并发数据结构**: DashMap (替代标准库的 HashMap/HashSet)
- **监控**: Prometheus、OpenTelemetry、Jaeger
- **地理定位**: MaxMind GeoIP (maxminddb 0.24)

### 主要功能模块

- **限流器**: 令牌桶、滑动窗口、固定窗口、并发控制
- **配额控制**: 配额分配、配额告警、配额透支
- **封禁管理**: IP 封禁、自动封禁、封禁优先级
- **熔断器**: 自动熔断、状态恢复、降级策略
- **标识符匹配**: IP、用户 ID、设备 ID、API Key、地理位置
- **缓存**: L2/L3 缓存支持
- **监控追踪**: Prometheus 指标、OpenTelemetry 追踪、审计日志

## 项目结构

```
limiteron/
├── src/                    # 主源代码目录
│   ├── lib.rs             # 库入口，重新导出所有公共 API
│   ├── governor.rs        # 主控制器
│   ├── limiters.rs        # 限流器实现
│   ├── ban_manager.rs     # 封禁管理器
│   ├── quota_controller.rs # 配额控制器
│   ├── circuit_breaker.rs # 熔断器
│   ├── storage.rs         # 存储接口定义
│   ├── postgres_storage.rs # PostgreSQL 存储
│   ├── redis_storage.rs   # Redis 存储
│   ├── cache/             # 缓存模块
│   │   ├── mod.rs         # 缓存模块入口
│   │   ├── l2.rs          # L2 缓存
│   │   ├── l3.rs          # L3 缓存
│   │   └── smart.rs       # 智能缓存策略
│   ├── matchers.rs        # 标识符匹配器
│   ├── geo_matcher.rs     # 地理位置匹配器
│   ├── device_matcher.rs  # 设备匹配器
│   ├── decision_chain.rs  # 决策链
│   ├── config.rs          # 配置管理
│   ├── config_watcher.rs  # 配置监视器
│   ├── telemetry.rs       # 监控和追踪
│   ├── audit_log.rs       # 审计日志
│   ├── fallback.rs        # 降级策略
│   ├── custom_limiter.rs  # 自定义限流器
│   ├── custom_matcher.rs  # 自定义匹配器
│   ├── lua_scripts.rs     # Lua 脚本管理
│   └── error.rs           # 错误类型
├── macros/                # 过程宏子项目
│   └── src/lib.rs        # 宏定义（flow_control 宏）
├── examples/              # 示例代码
│   ├── simple_rate_limit.rs
│   ├── quota_management.rs
│   ├── ban_management.rs
│   ├── macro_usage.rs
│   ├── redis_usage.rs
│   └── geo_device_matching.rs
├── tests/                 # 测试文件
│   ├── common_tests.rs
│   ├── integration_tests.rs
│   ├── e2e_tests.rs
│   ├── benches/           # 基准测试
│   ├── integration/       # 集成测试
│   └── e2e/              # 端到端测试
├── scripts/               # 脚本文件
│   ├── run-all-tests.sh   # 完整测试脚本
│   ├── run-integration-tests.sh
│   ├── pre-commit-check.sh
│   ├── init-db.sql       # 数据库初始化脚本
│   └── prometheus.yml    # Prometheus 配置
├── docs/                  # 文档
│   ├── USER_GUIDE.md
│   ├── API_REFERENCE.md
│   └── FAQ.md
├── Cargo.toml             # 主项目配置
├── rustfmt.toml           # 代码格式化配置
├── .clippy.toml           # Clippy 配置
└── docker-compose.yml     # Docker 开发环境
```

## 构建和运行

### 基本命令

```bash
# 检查编译（不生成二进制文件）
cargo check --all-features

# 构建项目
cargo build --all-features

# 构建发布版本
cargo build --release --all-features

# 运行示例
cargo run --example simple_rate_limit --all-features

# 运行所有测试
cargo test --all-features --workspace

# 运行特定测试
cargo test test_name

# 运行集成测试（需要 Docker 环境）
cargo test --test integration_tests -- --ignored --test-threads=1

# 运行基准测试
cargo bench

# 代码格式化
cargo fmt --all

# 代码格式化检查
cargo fmt --all -- --check

# Clippy 检查
cargo clippy --all-targets --all-features --workspace -- -D warnings

# 安全审计
cargo deny check
```

### 使用测试脚本

```bash
# 运行所有测试（编译检查 + 单元测试 + 集成测试）
./scripts/run-all-tests.sh

# 只运行单元测试
./scripts/run-all-tests.sh --unit

# 只运行集成测试（需要 Docker）
./scripts/run-all-tests.sh --integration

# 只运行编译检查
./scripts/run-all-tests.sh --check
```

### Docker 开发环境

```bash
# 启动开发环境（PostgreSQL、Redis、Prometheus、Jaeger）
docker-compose up -d

# 查看服务状态
docker-compose ps

# 查看日志
docker-compose logs -f

# 停止服务
docker-compose down

# 停止并删除数据卷
docker-compose down -v
```

### 环境变量

集成测试需要设置以下环境变量：

```bash
export REDIS_URL="redis://localhost:6379"
export REDIS_PASSWORD="limiteron123"
export POSTGRES_URL="postgresql://limiteron:limiteron123@localhost:5432/limiteron"
```

## 开发约定

### 代码风格

- **格式化**: 使用 `rustfmt`，配置在 `rustfmt.toml`
  - 最大行宽: 100 字符
  - 缩进: 4 空格
  - 换行符: Unix 风格
  - 自动排序导入和模块

- **代码检查**: 使用 `clippy`，配置在 `.clippy.toml`
  - 所有警告都被视为错误 (`-D warnings`)
  - 禁止使用标准库的 `HashMap`、`HashSet`、`BTreeMap`、`BTreeSet`
  - 必须使用 `DashMap` 和 `DashSet` 替代
  - 函数参数最多 7 个
  - 类型复杂度阈值: 250

### 测试规范

- **单元测试**: 与代码放在同一文件中，使用 `#[cfg(test)]` 模块
- **集成测试**: 放在 `tests/` 目录下
- **基准测试**: 放在 `tests/benches/` 目录下
- **测试命名**: 使用描述性名称，如 `test_token_bucket_rate_limiting`
- **异步测试**: 使用 `tokio::test` 宏

### 依赖管理

- **工作区**: 使用 Cargo workspace，包含主项目和 macros 子项目
- **版本控制**: 在 `workspace.dependencies` 中统一管理依赖版本
- **特性标志**:
  - `default`: 包含 `postgres` 和 `redis`
  - `postgres`: PostgreSQL 存储支持
  - `redis`: Redis 存储支持
  - `telemetry`: 监控和追踪支持
  - `webhook`: Webhook 通知支持

### 文档规范

- **公共 API**: 必须包含文档注释 (`///`)
- **模块文档**: 使用 `//!` 注释
- **示例**: 在文档中提供使用示例
- **语言**: 文档使用中文编写

### 提交规范

- **CI/CD**: 使用 GitHub Actions
  - 推送到 `main` 或 `develop` 分支时触发
  - Pull Request 时触发
  - 运行测试、格式化检查、Clippy 检查、安全审计、代码覆盖率

### 错误处理

- **错误类型**: 使用 `thiserror` 定义错误类型
- **错误传播**: 使用 `anyhow::Result` 在应用代码中
- **错误信息**: 提供清晰、有用的错误信息

### 并发安全

- **数据结构**: 使用 `DashMap` 和 `DashSet` 替代标准库集合
- **锁**: 使用 `parking_lot` 提供的锁（性能更好）
- **异步**: 使用 `tokio` 运行时和异步原语

## 关键特性

### 宏支持

使用 `#[flow_control]` 宏简化限流配置：

```rust
use limiteron::flow_control;

#[flow_control(rate = "100/s", quota = "10000/m", concurrency = 50)]
async fn api_handler(user_id: &str) -> Result<String, limiteron::error::FlowGuardError> {
    // API 业务逻辑
    Ok(format!("处理用户 {} 的请求", user_id))
}
```

### 决策链

决策链允许组合多个检查：

```rust
use limiteron::decision_chain::DecisionChainBuilder;

let chain = DecisionChainBuilder::new()
    .add_rate_limiter(rate_limiter)
    .add_quota_checker(quota_checker)
    .add_ban_checker(ban_checker)
    .build();
```

### 存储抽象

支持多种存储后端，通过统一的接口访问：

```rust
// 内存存储
let storage = Arc::new(MemoryStorage::new());

// PostgreSQL 存储
let storage = Arc::new(PostgresStorage::new(config).await?);

// Redis 存储
let storage = Arc::new(RedisStorage::new(config).await?);
```

## 性能指标

- **吞吐量**:
  - 速率限制: 500,000 ops/sec
  - 配额限制: 300,000 ops/sec
  - 并发限制: 200,000 ops/sec

- **延迟**:
  - P50: 0.1ms
  - P95: 0.2ms
  - P99: < 0.2ms

## 安全特性

- **内存安全**: Rust 保证
- **输入验证**: IP 地址、用户 ID、MAC 地址验证
- **SQL 注入防护**: 使用参数化查询
- **密码保护**: 使用 `secrecy` 库保护敏感信息
- **审计日志**: 完整的操作追踪

## 常见任务

### 添加新的限流器

1. 在 `src/custom_limiter.rs` 中实现 `CustomLimiter` trait
2. 在 `src/limiters.rs` 中添加限流器逻辑
3. 在 `src/lib.rs` 中重新导出
4. 编写单元测试和集成测试

### 添加新的存储后端

1. 在 `src/` 中创建新的存储文件（如 `my_storage.rs`）
2. 实现 `Storage` trait
3. 在 `Cargo.toml` 中添加依赖和特性标志
4. 在 `src/lib.rs` 中重新导出
5. 编写集成测试

### 添加新的匹配器

1. 在 `src/custom_matcher.rs` 中实现 `CustomMatcher` trait
2. 在 `src/matchers.rs` 中添加匹配器逻辑
3. 在 `src/lib.rs` 中重新导出
4. 编写单元测试

### 运行特定组件测试

```bash
# 测试限流器
cargo test limiters --lib

# 测试封禁管理
cargo test ban_manager --lib

# 测试配额控制
cargo test quota --lib

# 测试存储
cargo test storage --lib
```

## 故障排除

### 编译错误

如果遇到编译错误，确保：

1. 使用正确的 Rust 版本（2021 edition）
2. 启用了必要的特性标志（`--all-features`）
3. 依赖项已更新（`cargo update`）

### 测试失败

如果测试失败：

1. 确保 Docker 容器正在运行（集成测试）
2. 检查环境变量是否正确设置
3. 查看容器日志：`docker-compose logs`

### 性能问题

如果遇到性能问题：

1. 检查缓存配置
2. 运行基准测试：`cargo bench`
3. 启用监控和追踪
4. 查看 Prometheus 指标和 Jaeger 追踪

## 相关资源

- **GitHub**: https://github.com/Kirky-X/limiteron
- **文档**: `docs/` 目录
- **示例**: `examples/` 目录
- **问题报告**: GitHub Issues
- **讨论**: GitHub Discussions