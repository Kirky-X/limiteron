# cache-consolidation-ban-enhancement

## Motivation

Limiteron 当前存在两条平行的存储后端路径：`redis-storage` feature 直连 `redis` crate（`RedisStorage` 870+ 行），而 `cache-storage` feature 通过 `oxcache/redis` 走统一抽象。这导致：

1. **架构分裂**：Redis 后端有两套实现，维护成本高，行为不一致（Lua 脚本 vs oxcache 抽象）
2. **依赖冲突**：`redis 1.2` 锁版本以兼容 `dbnexus/oxcache ~1.2`，阻碍依赖升级（Rule 17：项目依赖优先使用最新稳定版本）
3. **BanTarget 缺失 Geo 维度**：`BanTarget` 枚举仅支持 `Ip/UserId/Mac`，但 `matchers/geo.rs` 已实现 geo 查询，封禁能力与匹配能力不对称
4. **无文件加载 ban 机制**：运维场景下需通过文件批量加载 ban 列表（黑名单），当前仅能通过 API/代码创建
5. **Admin API 不对称**：`DELETE /api/v1/ban/{target}` 存在，但缺少 `POST /api/v1/ban` 创建端点
6. **测试覆盖率不足 95%**：tarpaulin 配置 `fail-under = 95`，当前未达标
7. **代码审计未系统化**：未应用 `diting`/`tiangang` 进行全维度审计

本变更解决上述全部问题，作为 v0.3.0 发布前的关键收敛。

## Scope

1. **Cargo.toml 依赖治理**（用户需求 1, 2）
   - 升级所有 workspace 依赖到最新稳定版本（Rule 17）
   - 移除 `redis` crate 直接依赖
   - 移除未使用的依赖（`cargo machete` + `cargo udeps` 验证）
   - 解决 `dbnexus 0.2` vs `oxcache 0.3` 版本冲突

2. **Redis 直连移除**（用户需求 3）
   - 删除 `src/storage/redis.rs`（870+ 行）
   - 删除 `tests/integration/real_storage/redis_storage.rs`
   - 删除 `tests/integration/real_storage/distributed_consistency.rs`（依赖 RedisStorage）
   - 删除 `examples/src/bin/redis_storage.rs`
   - 移除 `redis-storage` feature
   - Redis 后端能力通过 `cache-storage` + `oxcache/redis` 提供
   - 更新 `src/storage/mod.rs`、`src/lib.rs` 移除 RedisStorage 导出

3. **BanTarget Geo 扩展**（用户需求 4）
   - 新增 `BanTarget::Geo { country_code: String }` 变体
   - 更新 `BanPriority` 增加 `Geo = 6`
   - 更新所有 `match` 模式（ban/types.rs, validation.rs, logging/redaction.rs, admin/handlers.rs, adapters/dbnexus_*.rs, cache/ban_storage.rs, storage/redis.rs 即将删除）
   - 更新 serde 标签 `#[serde(rename = "geo")]`

4. **文件加载 Ban**（用户需求 4）
   - 新增 `src/ban/file_loader.rs` 模块
   - YAML 格式（与项目 config 一致）
   - 热重载（复用 `notify` crate + `config-watcher` 模式）
   - 集成到 `BanManager`（启动时加载 + 文件变更时重载）

5. **HTTP 创建 Ban 端点**（用户需求 4）
   - 新增 `POST /api/v1/ban` 端点
   - 请求体：`{ target_type, target_value, reason, duration_secs?, operator?, metadata? }`
   - target_type 支持 `ip/user/mac/geo`
   - 与现有 `DELETE /api/v1/ban/{target}` 对称

6. **Kueiku 分析**（用户需求 5）
   - 应用 `kueiku` 方法论路由到「问题诊断」+「编程与架构」类别
   - 识别隐性 bug 与可优化点，输出优化清单

7. **测试覆盖率 ≥ 95%**（用户需求 6）
   - 运行 `cargo tarpaulin` 测量基线
   - 补充测试到 95% 以上（tarpaulin 配置已排除 Redis/Postgres/MaxMind 基础设施模块）
   - 遵循 Rule 9：测试验证有意义的属性，不是"有返回值"

8. **Diting + Tiangang 全代码审计**（用户需求 7）
   - 应用 `diting` skill 进行全维度代码质量审查
   - 应用 `tiangang` skill 进行 SAST 安全扫描
   - 修复所有 CRITICAL/HIGH 问题

9. **文档同步**（用户需求 8）
   - 更新 README.md, README_EN.md
   - 更新 CHANGELOG.md（v0.3.0 section）
   - 更新 docs/USER_GUIDE.md, docs/API_REFERENCE.md
   - 反映：Redis 移除、Geo ban、文件加载、新 HTTP 端点

10. **Examples 全覆盖**（用户需求 9）
    - 删除 `examples/src/bin/redis_storage.rs`
    - 新增 `examples/src/bin/ban_file_loader.rs`
    - 新增 `examples/src/bin/ban_http_api.rs`（含 geo ban 示例）
    - 审计现有 19 个 example，补齐遗漏场景

## Non-Goals

- **不实现新的限流算法**：本变更不涉及 limiters/ 模块新算法
- **不修改 GCRA 实现**：gcra.rs 保持现状
- **不重构 decision_chain**：决策链保持现有架构
- **不升级 Rust edition**：保持 edition = "2021"
- **不实现分布式限流**：分布式场景留给 v0.3.0+ 后续变更
- **不为 RedisStorage 提供迁移工具**：用户已确认完全删除

## Clarifications

- **[scope]** Q: 移除 redis 后，如何处理 RedisStorage 文件（redis.rs 870行代码、测试、示例）？
  A: 完全删除 — 删除 src/storage/redis.rs、tests/integration/real_storage/redis_storage.rs、examples/src/bin/redis_storage.rs。Redis 后端能力通过 oxcache/redis feature 提供，不再有独立 RedisStorage 类型

- **[scope]** Q: geo ban 的设计方式（影响 BanTarget 序列化、存储 schema、matchers）？
  A: 扩展 BanTarget 枚举 — 新增 BanTarget::Geo { country_code: String } 变体，类型安全，与现有 Ip/UserId/Mac 一致。需要更新所有 match 模式和 storage 实现

- **[scope]** Q: 文件加载 ban IP 的格式与加载时机？
  A: YAML + 热重载 — 复用现有 config-watcher (notify crate) 机制，YAML 格式与项目 config 一致。支持运行时热更新

- **[scope]** Q: HTTP 创建 ban 端点的设计？
  A: 统一 POST /api/v1/ban — 单一端点，请求体含 target_type (ip/user/mac/geo) + target_value + reason + duration + operator。与现有 DELETE /api/v1/ban/{target} 对称

## NEEDS CLARIFICATION

无未解决问题。所有需求已转化为具体任务。
