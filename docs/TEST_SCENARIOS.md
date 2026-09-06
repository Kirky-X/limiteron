# limiteron 测试场景固化（TEST_SCENARIOS）

> 阶段 2 验收产物。记录测试金字塔基线、测试目标落点、E2E 场景定义、本轮核正发现、
> 组合矩阵与静态门槛。验证口径全部为 `cargo test`（CI 口径
> `--workspace --no-default-features --features full`）。

## §1 测试金字塔基线

| 层级 | 承载 | 数量基线 |
| --- | --- | --- |
| L1 lib 单元测试 | `src/**` 内 `#[cfg(test)]` | 1964 passed / 6 ignored（governor/limiters/ban/quota/circuit/fallback/matchers 等模块自测） |
| L2 集成测试 | `tests/**` 顶层目标 + 子目录模块 | integration_tests 302+14i / e2e_tests 239 / common_tests 213 / unified_tests 211 / security_tests 57 / e2e_advanced 52 / property_tests_runner 24 / chaos_tests 16 / unit_tests 13 / admin_security_tests 6 / on_exceed_modes_test 1 |
| L3 E2E 场景 | `tests/e2e/`（目录承载：e2e_tests + e2e_advanced 双目标） | 291 passed（239+52） |
| L4 宏与文档 | limiteron_macros（过程宏自测）+ Doc-tests | 43 passed / 157 passed + 22 ignored |
| L5 examples | `examples/` 工作区成员（--workspace 收录） | 空测试目标 24 个（编译级验证） |

CI 口径全量结果：**38 目标 / 3298 passed / 0 failed / 42 ignored**（ignored 为
manual/容器依赖门控 + 文档测试 doctest 门控，与阶段 1 基线一致）。

## §2 测试目标落点

- 顶层自动发现（11 个入口文件，均通过 `mod` 引用子目录模块）：
  unit_tests→unit/、integration_tests→integration/+common/+modules/、
  unified_tests→modules/、common_tests→common/+modules/、security_tests→security/、
  chaos_tests→chaos/、property_tests_runner→property_tests/（含 .proptest-regressions）、
  e2e_tests→e2e/scenarios/、admin_security_tests/on_exceed_modes_test（自包含）
- `[[test]]` 显式注册：**e2e_advanced**（本轮从顶层迁入 tests/e2e/ 目录承载——
  e2e_* 不得裸放顶层；迁入子目录后脱离自动发现范围，不注册则 52 测试静默消失。
  文件自包含内嵌 mod、无 include_str 相对引用，迁移零损失实证）
- test-clock feature：MockClock 公开门控，不入生产 preset；tests/ 经 self dev-dep
  开启（CI 无需显式追加，无静默跳过风险）

## §3 E2E 场景定义（11 域，291 测试）

### tests/e2e/scenarios/（e2e_tests 目标，239 测试 ×4 业务域）

| 域 | 场景要点 |
| --- | --- |
| rate_limiting | Governor 低阈值创建→请求超限被拒完整流程/窗口边界/多策略 |
| ban_management | 违规累计→自动封禁→解封流程/封禁查询/持久化 |
| quota_control | 配额分配→消耗→耗尽拒绝→重置周期 |
| circuit_breaker | 失败累计→熔断开启→半开探测→恢复全状态机 |

### tests/e2e/e2e_advanced.rs（e2e_advanced 目标，52 测试 ×7 mod 域）

| 域 | 场景要点 |
| --- | --- |
| limiter_boundary | 各 limiter 类型边界值（零/最大/溢出/负） |
| gcra_limiter | GCRA 突发/持续速率语义（cfg(feature="gcra") 门控） |
| concurrency_limiter | 并发许可获取/释放/耗尽排队 |
| fallback_strategy | 降级策略触发/切换/恢复 |
| multi_tenant | 多租户隔离/独立配额/租户级封禁 |
| distributed_limiter | 分布式实例协调（存储共享语义） |
| cross_module | 跨模块集成（限流+ban+quota+熔断联动） |

## §4 本轮核正与发现（阶段 2）

1. **e2e_advanced 目录承载迁移**：tests/e2e_advanced.rs 裸放顶层（违反 e2e_* 目录
   承载约定）→ git mv 迁入 tests/e2e/ 并 `[[test]]` 显式注册；迁移前后 52 passed
   一致（零损失）。该仓 11 个顶层入口此前的自动发现正常，深目录均有归属，无
   inklog 式"死文件"问题。
2. **deny licenses clarify ×2**：limiteron/oxcache（工作区 license-file 形态不被
   cargo-deny 识别）→ clarify 绑定 MIT hash（path 相对被 clarify crate 目录解析，
   与 inklog 修复口径一致）。advisories/bans/sources 原本即 ok。

## §5 组合矩阵

| 组合 | 覆盖 | 结果 |
| --- | --- | --- |
| full（CI 口径） | 全 38 目标 | 3298 passed / 0 failed |
| no-default-features | 最小面编译 + 测试 | CI check/build job 覆盖，本地 fmt/clippy 同口径实证 |
| standard（sqlite preset） | 核心限流+ban+quota+熔断 | feature 组合编译验证 |
| gcra 单独门控 | e2e_advanced gcra 域 | full 下激活（52 含 gcra 用例） |

## §6 静态门槛

| 门槛 | 命令口径 | 结果 |
| --- | --- | --- |
| fmt | `cargo fmt --all -- --check` | 净 |
| clippy | `cargo clippy --workspace --all-targets --no-default-features --features full -- -D warnings` | 零告警 |
| doc | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --no-default-features --features full` | 零告警 |
| deny | `cargo deny check` | 4 项 ok（licenses clarify ×2） |
| audit | `cargo audit` | rc=0 |
| MSRV | rust-version（workspace 统一 1.97.1） | 一致 |
