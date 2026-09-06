# 📂 Limiteron 示例集

本目录是 workspace 内的独立子 crate（包名 `limiteron-examples`，不发布到 crates.io），共包含 **21 个可运行示例**，覆盖限流、配额、熔断、封禁管理等核心场景。示例源码位于 `src/bin/` 目录，需要 **Rust 1.97.1+**（与 workspace 的 `rust-version` 保持一致）。

## 🚀 运行方式

示例是独立子 crate 的 `[[bin]]` 目标（不是主 crate 的 `--example` 目标），请在仓库根目录使用 `-p limiteron-examples` 指定包运行：

```bash
# 无 feature 依赖的示例
cargo run -p limiteron-examples --bin simple_rate_limit

# 需要 feature 的示例，先通过 --features 启用（可组合多个）
cargo run -p limiteron-examples --features "ban-manager,admin-api" --bin ban_http_api
```

也可以进入本目录后直接运行（等价写法）：

```bash
cd examples
cargo run --bin simple_rate_limit
```

> 💡 本 crate 定义了与主 crate 对应的 feature，并额外提供 `full` feature 一次性启用所有示例所需的全部 features：`cargo run -p limiteron-examples --features full --bin <示例名>`。

## 📋 示例清单

### 1. 限流基础

| 示例 | 说明 | 运行命令 |
|------|------|----------|
| `simple_rate_limit` | 最基本的限流器使用方式 | `cargo run -p limiteron-examples --bin simple_rate_limit` |
| `rate_limiters` | 五种限流算法：令牌桶、滑动窗口、固定窗口、并发限制器、GCRA | `cargo run -p limiteron-examples --bin rate_limiters` |
| `macro_usage` | `flow_control` 宏的使用方式与当前版本的限制 | `cargo run -p limiteron-examples --bin macro_usage` |

### 2. 核心 API

| 示例 | 说明 | 运行命令 |
|------|------|----------|
| `governor_demo` | Governor 主控制器的三种构造模式、请求检查、决策解析与统计信息 | `cargo run -p limiteron-examples --bin governor_demo` |
| `matchers_demo` | 标识符提取器、请求上下文与规则匹配器的完整使用流程 | `cargo run -p limiteron-examples --bin matchers_demo` |
| `decision_chain` | 责任链模式的决策链：组合多个限流器、按优先级执行、支持短路 | `cargo run -p limiteron-examples --bin decision_chain` |
| `custom_matchers` | 自定义匹配器 trait 的实现、注册表使用及内置 Header / TimeWindow 匹配器 | `cargo run -p limiteron-examples --bin custom_matchers` |
| `authorization_demo` | 授权提供者 trait 的实现与内置 `SimpleAuthorizationProvider` | `cargo run -p limiteron-examples --bin authorization_demo` |
| `graceful_shutdown` | 监听 Ctrl+C 信号并调用 `Governor::shutdown()` 优雅关闭（幂等） | `cargo run -p limiteron-examples --bin graceful_shutdown` |

### 3. 熔断 / 配额 / 封禁

| 示例 | 说明 | 运行命令 |
|------|------|----------|
| `circuit_breaker` | 熔断器模式：故障检测、熔断打开、半开恢复探测与超时自动恢复 | `cargo run -p limiteron-examples --features circuit-breaker --bin circuit_breaker` |
| `quota_control` | 配额消费跟踪、限额执行与用量百分比计算 | `cargo run -p limiteron-examples --features quota-control --bin quota_control` |
| `ban_manager` | 针对 IP / 用户 ID / MAC 目标的封禁创建、状态查询、更新与移除 | `cargo run -p limiteron-examples --features ban-manager --bin ban_manager` |
| `ban_file_loader` | 从 YAML 文件加载封禁规则到 `BanManager`，支持文件变更热重载 | `cargo run -p limiteron-examples --features "ban-manager,config-watcher" --bin ban_file_loader` |
| `ban_http_api` | 启动 `AdminServer` 管理 API 服务器，并通过 HTTP 调用封禁增删端点 | `cargo run -p limiteron-examples --features "ban-manager,admin-api" --bin ban_http_api` |

### 4. Feature-gated 扩展

| 示例 | 说明 | 运行命令 |
|------|------|----------|
| `validation_demo` | 统一验证模块：IP 地址、用户 ID、MAC 地址、API Key、封禁目标等验证 | `cargo run -p limiteron-examples --features validation --bin validation_demo` |
| `storage_factory` | 通过 `StorageFactory` 从 DSN 创建 Postgres / MySQL / SQLite 存储后端 | `cargo run -p limiteron-examples --features postgres --bin storage_factory` |
| `fallback_demo` | 降级策略管理器：策略配置、故障注入、降级执行与孤岛模式 | `cargo run -p limiteron-examples --features fallback --bin fallback_demo` |
| `audit_log_demo` | 审计日志：事件记录、配置、统计与签名验证 | `cargo run -p limiteron-examples --features audit-log --bin audit_log_demo` |
| `tower_middleware` | 将 Governor 流量控制集成到 Tower Service 处理链 | `cargo run -p limiteron-examples --features tower-middleware --bin tower_middleware` |
| `telemetry_demo` | Prometheus 指标采集与 OpenTelemetry 分布式追踪 | `cargo run -p limiteron-examples --features telemetry --bin telemetry_demo` |
| `device_geo_matching` | User-Agent 解析、设备类型识别、IP 地理位置查询与地理条件匹配 | `cargo run -p limiteron-examples --features "device-matching,geo-matching" --bin device_geo_matching` |

## 📚 更多文档

- [主 README](../README.md)
- [用户指南](../docs/USER_GUIDE.md)
- [API 参考](../docs/API_REFERENCE.md)
- [测试指南](../docs/TESTING.md)
