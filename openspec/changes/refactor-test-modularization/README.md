# 测试套件模块化重构提案

## 概述

本提案旨在将 `tests/` 目录重构为按功能模块组织的层级结构，提高测试的可维护性和可发现性。

## 变更 ID
`refactor-test-modularization`

## 目标

1. **模块化组织**: 按功能组件组织测试（storage, limiters, ban_manager, quota, circuit_breaker, matchers, governor, cache）
2. **清晰分离**: 每个模块包含单元测试（`unit/`）和集成测试（`integration.rs`）
3. **保持兼容**: 保留现有的测试命令和 E2E 测试结构
4. **提高可维护性**: 使测试更容易定位、理解和扩展

## 新的目录结构

```
tests/
├── modules/                    # 功能模块
│   ├── storage/               # 存储模块
│   │   ├── mod.rs
│   │   ├── integration.rs     # 集成测试
│   │   └── unit/              # 单元测试
│   │       ├── mod.rs
│   │       ├── memory.rs
│   │       ├── postgres.rs
│   │       └── redis.rs
│   ├── limiters/              # 限流器模块
│   │   ├── mod.rs
│   │   ├── integration.rs
│   │   └── unit/
│   │       ├── mod.rs
│   │       ├── token_bucket.rs
│   │       ├── sliding_window.rs
│   │       ├── fixed_window.rs
│   │       └── concurrency.rs
│   ├── ban_manager/           # 封禁管理模块
│   │   ├── mod.rs
│   │   ├── integration.rs
│   │   └── unit/
│   │       ├── mod.rs
│   │       ├── ban_storage.rs
│   │       └── auto_ban.rs
│   ├── quota/                 # 配额控制模块
│   │   ├── mod.rs
│   │   ├── integration.rs
│   │   └── unit/
│   │       ├── mod.rs
│   │       ├── quota_allocation.rs
│   │       └── overdraft.rs
│   ├── circuit_breaker/       # 熔断器模块
│   │   ├── mod.rs
│   │   ├── integration.rs
│   │   └── unit/
│   │       ├── mod.rs
│   │       ├── state_machine.rs
│   │       └── recovery.rs
│   ├── matchers/              # 匹配器模块
│   │   ├── mod.rs
│   │   ├── integration.rs
│   │   └── unit/
│   │       ├── mod.rs
│   │       ├── ip_matcher.rs
│   │       ├── user_matcher.rs
│   │       └── device_matcher.rs
│   ├── governor/              # 控制器模块
│   │   ├── mod.rs
│   │   ├── integration.rs
│   │   └── unit/
│   │       ├── mod.rs
│   │       └── decision_chain.rs
│   └── cache/                 # 缓存模块
│       ├── mod.rs
│       ├── integration.rs
│       └── unit/
│           ├── mod.rs
│           ├── l2_cache.rs
│           └── l3_cache.rs
├── e2e/                       # E2E 测试（保持不变）
├── common/                    # 公共测试工具（保持不变）
├── fixtures/                  # 测试夹具（保持不变）
└── benches/                   # 基准测试（保持不变）
```

## 实施步骤

1. **分析和规划**: 分析现有测试文件并按功能分类
2. **创建目录结构**: 创建新的模块化目录结构
3. **迁移测试**: 按模块迁移测试文件
4. **更新入口文件**: 更新测试入口文件以使用新结构
5. **验证**: 运行所有测试确保功能正常
6. **文档更新**: 更新项目文档

## 影响范围

- **受影响的规范**: `testing`
- **受影响的代码**: `tests/` 目录结构和所有测试文件
- **迁移策略**: 现有测试将移动到新位置，不改变测试逻辑

## 向后兼容性

- 保持所有现有测试命令（`cargo test`, `cargo test --test integration_tests` 等）
- 保留 E2E 测试结构
- 不改变测试逻辑或覆盖率

## 文件列表

- `proposal.md` - 提案说明
- `tasks.md` - 实施任务清单
- `design.md` - 技术设计文档
- `specs/testing/spec.md` - 测试规范变更

## 下一步

1. 审查提案
2. 批准后开始实施
3. 按照任务清单逐步完成迁移
4. 验证并测试