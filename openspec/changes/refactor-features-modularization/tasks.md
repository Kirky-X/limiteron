# Implementation Tasks

## Phase 1: Cargo.toml Refactoring (Week 1)

### 1.1 特性矩阵定义
- [ ] 1.1.1 定义核心特性（`core`）
  - 包含：limiters, decision_chain, storage trait, matchers, error, config
  - 基础依赖：tokio, async-trait, serde, dashmap, parking_lot, lru
- [ ] 1.1.2 定义存储特性（互斥选择）
  - `memory`（默认）
  - `postgres`：依赖 sqlx
  - `redis`：依赖 redis-rs
- [ ] 1.1.3 定义高级功能特性
  - `ban-manager`：封禁管理
  - `quota-control`：配额控制
  - `circuit-breaker`：熔断器
  - `fallback`：降级策略
- [ ] 1.1.4 定义匹配特性
  - `geo-matching`：地理位置匹配（maxminddb）
  - `device-matching`：设备指纹识别（woothee）
- [ ] 1.1.5 定义可观测性特性
  - `telemetry`：OpenTelemetry + Jaeger
  - `monitoring`：Prometheus metrics
  - `audit-log`：审计日志（依赖 telemetry）
- [ ] 1.1.6 定义工具特性
  - `macros`：#[flow_control] 宏
  - `config-watcher`：配置热重载（notify）
  - `webhook`：Webhook 通知（reqwest）

### 1.2 依赖条件化
- [ ] 1.2.1 将所有外部依赖改为 `optional = true`
- [ ] 1.2.2 为每个特性定义对应的依赖启用条件
- [ ] 1.2.3 定义 feature 组合（default-features, full-features, minimal-features）

### 1.3 Workspace Dependencies
- [ ] 1.3.1 将 shared dependencies 移至 `[workspace.dependencies]`
- [ ] 1.3.2 更新所有使用 workspace dependencies 的模块

## Phase 2: Storage Layer Modularization (Week 2)

### 2.1 条件编译存储实现
- [ ] 2.1.1 重构 `postgres_storage.rs`，添加 `#[cfg(feature = "postgres")]`
- [ ] 2.1.2 重构 `redis_storage.rs`，添加 `#[cfg(feature = "redis")]`
- [ ] 2.1.3 提取内存存储为独立模块 `memory_storage.rs`
- [ ] 2.1.4 为禁用的特性提供存根实现（no-op）

### 2.2 Storage Trait 更新
- [ ] 2.2.1 确保 `storage.rs` 中的 trait 定义始终可用
- [ ] 2.2.2 为每个存储实现添加 feature-gated 条件编译

### 2.3 lib.rs 条件导出
- [ ] 2.3.1 条件导出 `PostgresStorage` 和 `PostgresStorageConfig`
- [ ] 2.3.2 条件导出 `RedisStorage` 和 `RedisConfig`
- [ ] 2.3.3 条件导出 `MemoryStorage`（默认）

## Phase 3: Optional Module Feature-gating (Week 3)

### 3.1 地理位置匹配（geo-matching）
- [ ] 3.1.1 在 `geo_matcher.rs` 添加 `#[cfg(feature = "geo-matching")]`
- [ ] 3.1.2 为禁用状态提供 `NoOpGeoMatcher` 存根
- [ ] 3.1.3 条件导出 GeoMatcher 相关类型

### 3.2 设备指纹识别（device-matching）
- [ ] 3.2.1 在 `device_matcher.rs` 添加 `#[cfg(feature = "device-matching")]`
- [ ] 3.2.2 为禁用状态提供 `NoOpDeviceMatcher` 存根
- [ ] 3.2.3 条件导出 DeviceMatcher 相关类型

### 3.3 熔断器（circuit-breaker）
- [ ] 3.3.1 在 `circuit_breaker.rs` 添加 `#[cfg(feature = "circuit-breaker")]`
- [ ] 3.3.2 提供轻量级 `NoOpCircuitBreaker` 存根
- [ ] 3.3.3 更新 DecisionChain 以支持可选熔断器

### 3.4 封禁管理（ban-manager）
- [ ] 3.4.1 在 `ban_manager.rs` 添加 `#[cfg(feature = "ban-manager")]`
- [ ] 3.4.2 为禁用状态提供 `NoOpBanManager` 存根
- [ ] 3.4.3 更新 Governor 以支持可选封禁检查

### 3.5 配额控制（quota-control）
- [ ] 3.5.1 在 `quota_controller.rs` 添加 `#[cfg(feature = "quota-control")]`
- [ ] 3.5.2 为禁用状态提供 `NoOpQuotaController` 存根
- [ ] 3.5.3 更新 DecisionChain 以支持可选配额检查

## Phase 4: Observability Modularization (Week 4)

### 4.1 Telemetry（telemetry）
- [ ] 4.1.1 在 `telemetry.rs` 添加 `#[cfg(feature = "telemetry")]`
- [ ] 4.1.2 为禁用状态提供 no-op telemetry 实现
- [ ] 4.1.3 条件导出 TelemetryConfig, Tracer, Metrics

### 4.2 Monitoring（monitoring）
- [ ] 4.2.1 提取 Prometheus 相关代码到独立模块
- [ ] 4.2.2 添加 `#[cfg(feature = "monitoring")]`
- [ ] 4.2.3 为禁用状态提供 no-op metrics 实现

### 4.3 Audit Log（audit-log）
- [ ] 4.3.1 在 `audit_log.rs` 添加 `#[cfg(feature = "audit-log")]`，依赖 telemetry
- [ ] 4.3.2 为禁用状态提供 no-op audit 实现
- [ ] 4.3.3 条件导出 AuditLogger, AuditEvent

## Phase 5: Tooling and Macros (Week 5)

### 5.1 宏系统（macros）
- [ ] 5.1.1 在 `macros.rs` 添加 `#[cfg(feature = "macros")]`
- [ ] 5.1.2 在 macros 子项目 Cargo.toml 添加特性支持
- [ ] 5.1.3 条件导出 `#[flow_control]` 宏

### 5.2 Config Watcher（config-watcher）
- [ ] 5.2.1 在 `config_watcher.rs` 添加 `#[cfg(feature = "config-watcher")]`
- [ ] 5.2.2 条件导出 ConfigWatcher 相关类型

### 5.3 Webhook（webhook）
- [ ] 5.3.1 添加 Webhook 通知模块（新建）
- [ ] 5.3.2 添加 `#[cfg(feature = "webhook")]`
- [ ] 5.3.3 条件导出 WebhookNotifier 相关类型

## Phase 6: Testing and Validation (Week 6)

### 6.1 单元测试
- [ ] 6.1.1 为每个特性组合编写单元测试
- [ ] 6.1.2 测试禁用特性的 no-op 实现正确性
- [ ] 6.1.3 测试特性之间的交互

### 6.2 集成测试
- [ ] 6.2.1 创建 minimal-features 集成测试
- [ ] 6.2.2 创建 full-features 集成测试
- [ ] 6.2.3 创建 custom-features 集成测试

### 6.3 CI Matrix Build
- [ ] 6.3.1 配置 GitHub Actions matrix build，测试所有特性组合
- [ ] 6.3.2 验证每个组合编译通过
- [ ] 6.3.3 验证每个组合测试通过

## Phase 7: Documentation (Week 7)

### 7.1 README 更新
- [ ] 7.1.1 添加特性矩阵表格
- [ ] 7.1.2 提供最小化、标准、完整三种配置示例
- [ ] 7.1.3 更新快速开始指南

### 7.2 MIGRATION.md
- [ ] 7.2.1 创建迁移指南文档
- [ ] 7.2.2 提供升级路径和代码示例
- [ ] 7.2.3 列出所有 breaking changes

### 7.3 Examples 更新
- [ ] 7.3.1 为每个特性组合创建示例代码
- [ ] 7.3.2 添加 `examples/minimal_usage.rs`
- [ ] 7.3.3 添加 `examples/custom_features.rs`

## Phase 8: Release Preparation (Week 8)

### 8.1 Pre-release Checks
- [ ] 8.1.1 运行完整测试套件（all-features）
- [ ] 8.1.2 验证二进制体积减少目标
- [ ] 8.1.3 验证编译时间减少目标
- [ ] 8.1.4 更新 CHANGELOG.md

### 8.2 Legacy Compatibility
- [ ] 8.2.1 添加 `legacy-compat` 特性（可选）
- [ ] 8.2.2 测试旧代码是否无需修改即可编译
- [ ] 8.2.3 文档化 legacy-compat 的弃用计划

### 8.3 Release
- [ ] 8.3.1 发布 v1.0.0-rc.1（候选版本）
- [ ] 8.3.2 收集用户反馈
- [ ] 8.3.3 修复发现的问题
- [ ] 8.3.4 发布 v1.0.0 正式版
