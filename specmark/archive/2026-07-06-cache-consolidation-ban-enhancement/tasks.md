# Tasks — cache-consolidation-ban-enhancement

按 TDD 粒度拆分（Red → Green → Commit → Analyze → Next），每个任务 2-5 分钟可完成。
所有任务含具体文件路径，按执行顺序排列，apply 阶段严格顺序执行不跳过。

## Phase 1: Cargo.toml 依赖治理（用户需求 1, 2, 3）

- [x] [T001] [P0] 运行 `cargo machete` 检测未使用依赖，输出报告到 specmark/changes/cache-consolidation-ban-enhancement/machete-report.txt
- [x] [T002] [P0] 升级 Cargo.toml 所有 workspace 依赖到最新稳定版本（tokio/serde/axum/sqlx/sea-orm/oxcache/notify 等），保持 edition = "2021"
- [x] [T003] [P0] 移除 Cargo.toml L86-87 redis workspace 依赖 + L169 `redis = { workspace = true, optional = true }`
- [x] [T004] [P0] 移除 Cargo.toml L277 `redis-storage = ["dep:redis"]` feature 定义，并从 L239 `full` feature 移除 `redis-storage`

## Phase 2: RedisStorage 文件删除

- [x] [T005] [P0] 删除 src/storage/redis.rs（870+ 行 RedisStorage 实现）
- [x] [T006] [P0] 删除 tests/integration/real_storage/redis_storage.rs + tests/integration/real_storage/distributed_consistency.rs（依赖 RedisStorage）
- [x] [T007] [P0] 删除 examples/src/bin/redis_storage.rs
- [x] [T008] [P0] 更新 src/storage/mod.rs 移除 L18-23 redis 模块声明 + RedisStorage 相关 re-export
- [x] [T009] [P0] 更新 src/lib.rs 移除 L222-224 `#[cfg(feature = "redis-storage")] pub use storage::redis::RedisStorage;`

## Phase 3: BanTarget Geo 扩展（TDD）

- [x] [T010] [P0] [RED] 写 BanTarget::Geo + BanPriority::Geo 单元测试到 src/storage/mod.rs 和 src/ban/types.rs（覆盖序列化/反序列化、优先级排序、相等性）
- [x] [T011] [P0] [GREEN] 实现 BanTarget::Geo { country_code: String } 变体 + BanPriority::Geo = 6（src/storage/mod.rs, src/ban/types.rs），更新 from_target/validate_ban_target
- [x] [T012] [P0] 实现 validate_ban_target Geo 分支（src/validation.rs），验证 ISO 3166-1 alpha-2 国家代码（2 字母大写）
- [x] [T013] [P0] 实现 redact_ban_target Geo 分支（src/logging/redaction.rs），保留前 2 字符后脱敏
- [x] [T014] [P0] 更新 src/adapters/dbnexus_ban_storage.rs 所有 match BanTarget 模式增加 Geo 分支
- [x] [T015] [P0] 更新 src/cache/ban_storage.rs 所有 match BanTarget 模式增加 Geo 分支

## Phase 4: 文件加载 Ban（TDD）

- [x] [T016] [P0] [RED] 写 BanFileLoader 单元测试到 src/ban/file_loader.rs（覆盖 YAML 解析、load_once 计数、热重载触发、错误格式）
- [x] [T017] [P0] [GREEN] 实现 BanFileLoader::{new, load_once, start_watching, stop_watching}（src/ban/file_loader.rs），复用 notify crate 热重载
- [x] [T018] [P0] 集成到 BanManager（src/ban/types.rs）新增 `with_file_loader(path)` builder 方法，启动时加载 + 文件变更重载

## Phase 5: HTTP POST /api/v1/ban（TDD）

- [x] [T019] [P0] [RED] 写 create_ban handler 测试到 src/admin/routes.rs（覆盖 ip/user/mac/geo 4 种 target_type + 验证错误 400/422 + 503 无 manager + 重复 ban + 自定义 duration + operator + 鉴权 401）
- [x] [T020] [P0] [GREEN] 实现 CreateBanRequest 结构体 + create_ban handler（src/admin/handlers.rs），错误映射 ValidationError→400, AuthorizationError→403, 其他→500
- [x] [T021] [P0] 注册 POST /api/v1/ban 路由到 src/admin/routes.rs，与现有 DELETE /api/v1/ban/{target} 对称

## Phase 6: Kueiku + Diting + Tiangang 分析（用户需求 5, 7）

- [x] [T022] [P1] 应用 kueiku skill 路由到「问题诊断」+「编程与架构」类别，分析隐性 bug 与可优化点，输出清单到 specmark/changes/cache-consolidation-ban-enhancement/kueiku-analysis.md
- [x] [T023] [P1] 应用 diting skill 对 src/ 全代码质量审计，输出报告到 specmark/changes/cache-consolidation-ban-enhancement/diting-report.md
- [x] [T024] [P1] 应用 tiangang skill 进行 SAST 安全扫描（Semgrep/CodeQL），输出报告到 specmark/changes/cache-consolidation-ban-enhancement/tiangang-report.md
- [x] [T025] [P1] 修复 kueiku/diting/tiangang 报告中所有 CRITICAL/HIGH 问题，每修复一项更新对应报告状态
  - [x] CRITICAL: 整数下溢 panic（cache/ban_storage.rs + storage/mod.rs）→ saturating_sub
  - [x] CRITICAL: 时钟回退导致配额永久失效（quota/controller.rs）→ elapsed < 0 检测
  - [x] CRITICAL: YAML 炸弹（ban/file_loader.rs）→ 文件大小限制 2MB
  - [x] CRITICAL: AdminServer::start() 未 validate（admin/server.rs）→ 加 validate 调用
  - [x] CRITICAL: get_limiter_status 返回假数据（admin/handlers.rs）→ 改为 501
  - [x] CRITICAL: handler 错误响应状态码不一致（admin/handlers.rs）→ 统一 (StatusCode, Json)
  - [x] CRITICAL: SlidingWindowLimiter 已废弃仍公开导出 → 移除 pub use (commit cc9b6f4)
  - [x] HIGH: BanManager/EventDispatcher 无 Drop impl → 已加 Drop (commit 1f956f7)
  - [x] HIGH: QuotaController Drop impl feature gate → 已加 (commit 1f956f7)
  - [x] HIGH: `as u32` 截断 ban_times → u32::try_from (commit 1f956f7)
  - [x] HIGH: chrono::Duration::from_std().unwrap() panic → ? 传播 (commit 1f956f7)
  - [x] HIGH: `as u8` 截断绕过范围校验 → 重排校验顺序 (commit 1f956f7)
  - [x] HIGH: redact_advanced 逻辑缺陷 → regex 命中返回 "***" (commit 1f956f7)
  - [x] HIGH: 告警 spawn-fire-and-forget 无背压 → Semaphore(8) (commit f678fca)
  - [x] HIGH: DELETE /ban 无法解封 MAC/Geo → ?type= query param (commit f678fca)
  - [x] HIGH: 测试辅助函数 3x 重复 → test_support.rs (commit f678fca)
  - [x] HIGH: 幽灵 trait StorageCreate/BanStorageCreate → 固有方法 (commit f678fca)
  - [x] HIGH: GovernorBuilder #[allow(dead_code)] → 移除 (commit f678fca)
  - [x] HIGH: config TODO 模块未实现 → 文档明确 (commit f678fca)
  - [x] HIGH: 热重载无 debounce → 500ms debounce (commit f678fca)
  - [x] HIGH: create_ban 授权链路依赖可选 provider → 显式警告日志 (commit cc9b6f4)

## Phase 7: 测试覆盖率 ≥ 95%（用户需求 6）

- [x] [T026] [P0] 运行 `cargo tarpaulin --features full --lib` 测量基线覆盖率 → 96.16% (5781/6012 行)
- [x] [T027] [P0] 覆盖率已达 96.16% > 95% 目标，无需补充测试（覆盖率在 Phase 5/6 TDD 中已达标准）

## Phase 8: 文档同步（用户需求 8）

- [x] [T028] [P1] 更新 README.md，反映：Redis 移除、Geo ban、文件加载、POST /api/v1/ban 端点、依赖升级 (commit 4155ac1)
- [x] [T029] [P1] 更新 CHANGELOG.md 新增 v0.3.0 section，列出全部 breaking changes 和新功能 (commit 4155ac1)
- [x] [T030] [P1] 更新 docs/USER_GUIDE.md + docs/API_REFERENCE.md，新增 BanTarget::Geo、BanFileLoader、POST /api/v1/ban 文档 (commit 4155ac1)

## Phase 9: Examples 全覆盖（用户需求 9）

- [x] [T031] [P1] 新增 examples/src/bin/ban_file_loader.rs 演示 YAML 文件加载 + 热重载 (commit 4155ac1)
- [x] [T032] [P1] 新增 examples/src/bin/ban_http_api.rs 演示 POST /api/v1/ban（覆盖 ip/user/mac/geo 4 种类型）(commit 4155ac1)
- [x] [T033] [P1] 审计现有 examples/src/bin/ 全部样例，输出审计清单到 specmark/changes/cache-consolidation-ban-enhancement/examples-audit.md (commit 4155ac1)

## Phase 10: 最终验证

- [x] [T034] [P0] cargo test --features full --lib → 1892 passed, 0 failed; clippy → 零警告
- [x] [T035] [P0] cargo tarpaulin --features full --lib → 96.16% (5781/6012 行) ≥ 95% ✅
- [x] [T036] [P0] cargo build --bins --features full (examples) → 编译通过; clippy → 零警告

## Phase 11: Convergence

_由 /specmark converge 于 2026-07-06 生成。_

**发现缺口：** 1 (CRITICAL: 0 | HIGH: 0 | MEDIUM: 1 | LOW: 0)
**追加任务：** 1（跳过：0 个 LOW/unrequested）
**未请求范围（按原样接受）：** GovernorBuilder 的 `metrics`/`tracer` 字段有 setter 但 `build()` 未消费（LOW，超出本次 dead_code 修复范围，另行跟踪）

**缺口详情（MEDIUM）：** Phase 1 T001/T002 要求移除 `cargo machete` 检出的未使用依赖，但 `darling`/`anyhow`/`crc32fast`/`lazy_static` 4 项仍在 Cargo.toml / macros/Cargo.toml 中。已通过 grep 验证源码零引用，且 `validator`/`tracing` 等 machete 误报项确认被使用（feature-gated）予以保留。

- [x] [T037] [P1] 移除 4 个未使用依赖：`darling`（macros/Cargo.toml）、`anyhow` + `lazy_static` + `crc32fast`（Cargo.toml workspace deps），并验证 `cargo check --features full --lib` + `cargo check -p limiteron-macros` + `cargo clippy --features full --lib -- -D warnings` 全部通过 — file: Cargo.toml, macros/Cargo.toml
