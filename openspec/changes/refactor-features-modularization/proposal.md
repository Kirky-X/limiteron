# Change: Refactor Features for Modularization and Pluggability

## Why

Limiteron 当前采用 monolithic 设计，所有功能模块在编译时强制启用，导致：
1. **二进制体积膨胀**：完整编译包含 80+ crate，约 5-8 MB，不适合边缘计算和嵌入式环境
2. **编译时间过长**：依赖关系复杂，增量编译时间长
3. **资源浪费**：用户只需部分功能却编译全部代码（如不需要地理位置匹配但仍需加载 MaxMindDB）
4. **扩展性受限**：难以支持自定义实现（存储、匹配器、限流器）

通过特性化改造，实现按需编译和运行时插件加载，降低部署复杂度，提升灵活性。

## What Changes

- [ ] **重构 Cargo.toml 特性系统**：从简单的 `postgres/redis/telemetry` 扩展到细粒度特性矩阵（存储、匹配、可观测性、高级功能）
- [ ] **条件编译存储层**：PostgreSQL 和 Redis 存储完全可选，默认仅内存存储
- [ ] **特性化可选模块**：
  - 地理位置匹配（`geo-matching` feature）
  - 设备指纹识别（`device-matching` feature）
  - 熔断器（`circuit-breaker` feature）
  - 封禁管理（`ban-manager` feature）
  - 配额控制（`quota-control` feature）
- [ ] **条件化可观测性**：Telemetry、AuditLog、Monitoring 可独立启用
- [ ] **宏系统可选化**：`#[flow_control]` 宏改为可选，减少编译时依赖
- [ ] **运行时配置支持**：通过 YAML 配置文件控制模块行为，结合编译时特性

### Breaking Changes

- **BREAKING**: Cargo.toml 的 `default` 特性从 `["postgres", "redis"]` 改为 `["memory"]`
- **BREAKING**: 地理位置匹配和设备匹配需要显式启用特性
- **BREAKING**: Telemetry 默认不再启用，需手动添加 `telemetry` 特性

### Migration Guide

**升级路径**：
```toml
# 旧版本（v0.1.0）
limiteron = { version = "0.1.0" }

# 新版本（v1.0.0）- 完整功能（与旧版等效）
limiteron = {
    version = "1.0.0",
    features = [
        "postgres",
        "redis",
        "ban-manager",
        "quota-control",
        "circuit-breaker",
        "geo-matching",
        "device-matching",
        "telemetry",
        "monitoring",
        "macros"
    ]
}

# 新版本（v1.0.0）- 最小化功能
limiteron = {
    version = "1.0.0",
    default-features = false,
    features = ["memory"]
}
```

## Impact

### Affected Specs

- **core**: 核心限流和决策链（始终保持启用）
- **storage**: 存储接口和后端（memory/postgres/redis）
- **matching**: 标识符匹配器（geo/device/custom）
- **observability**: 监控和追踪（telemetry/metrics/audit）
- **advanced**: 高级功能（ban/quota/circuit-breaker）

### Affected Code

- **Cargo.toml**: 完整重写特性定义和依赖条件
- **src/lib.rs**: 条件导出公共 API
- **src/postgres_storage.rs**: 条件编译实现
- **src/redis_storage.rs**: 条件编译实现
- **src/geo_matcher.rs**: 条件编译实现
- **src/device_matcher.rs**: 条件编译实现
- **src/telemetry.rs**: 条件编译实现
- **src/audit_log.rs**: 条件编译实现
- **src/circuit_breaker.rs**: 条件编译实现
- **src/ban_manager.rs**: 条件编译实现
- **src/quota_controller.rs**: 条件编译实现
- **src/macros.rs**: 条件编译实现

### Performance Impact

- **二进制体积**：最小配置减少 60-70%（从 5-8 MB 降至 1-2 MB）
- **编译时间**：最小配置减少约 40%
- **运行时内存**：无需的功能模块不加载（如 geo-matching 节省 ~10-20 MB）

### Documentation Impact

- 更新 README.md，添加特性矩阵和使用示例
- 创建 MIGRATION.md，提供升级指南
- 更新 examples/，按特性分组示例代码

### Testing Impact

- 需要为每个特性组合创建集成测试
- CI 流程需要测试所有特性组合（使用 cargo hack 或 matrix build）

## Success Criteria

1. **功能正确性**：所有特性组合都能正确编译和运行
2. **二进制体积**：最小配置相比完整配置减少 ≥50%
3. **依赖数量**：最小配置依赖 ≤30 个 crate
4. **向后兼容**：提供 `legacy-compat` 特性，旧代码无需修改即可编译
5. **文档完整**：README 包含特性矩阵和配置示例
6. **测试覆盖**：CI 覆盖所有特性组合
