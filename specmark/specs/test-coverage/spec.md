# Spec — test-coverage

> Main spec for the test-coverage capability domain. 初始版本由 change `cache-consolidation-ban-enhancement` 引入（2026-07-06）。

## Requirements

### R-test-coverage-001: 覆盖率基线测量

运行 `cargo tarpaulin` 测量当前覆盖率基线，输出 HTML 报告供分析。

**验收标准：**
- 执行命令：`cargo tarpaulin --features full --lib --out Html --output-dir specmark/changes/cache-consolidation-ban-enhancement/`
- 生成 `tarpaulin-report.html` 文件
- 报告含每个文件/模块的覆盖率百分比
- 基线数值记录到 specmark/changes/cache-consolidation-ban-enhancement/coverage-baseline.txt

### R-test-coverage-002: 覆盖率达标 95%

补充测试使整体覆盖率 ≥ 95%（tarpaulin `fail-under = 95` 配置已存在）。

**验收标准：**
- `cargo tarpaulin --features full --lib --fail-under 95` 退出码 0
- 重点新增模块覆盖率达标：
  - `src/ban/file_loader.rs` ≥ 95%
  - `src/admin/handlers.rs::create_ban` ≥ 95%
  - `src/storage/mod.rs::BanTarget::Geo` ≥ 95%
  - `src/validation.rs::validate_ban_target` Geo 分支 ≥ 95%
- 遵循 Rule 9：测试验证有意义的属性（值、结构、副作用、错误类型），不是"有返回值"

### R-test-coverage-003: 测试质量保障

新增测试必须验证有意义的行为属性，避免弱测试。

**验收标准：**
- 新增测试断言具体值/结构/错误类型，不仅是 `assert!(result.is_ok())`
- 错误路径测试断言错误变体和错误消息关键词
- 边界条件测试覆盖（空输入、最大值、无效格式）
- 无 `#[ignore]` 测试除非有明确原因（注释说明）

## Constraints

- 不修改 tarpaulin.toml 排除列表（已排除 Redis/Postgres/MaxMind 基础设施模块）
- 不为覆盖率而写无意义的 `assert!(true)` 测试
- 测试代码遵循项目惯例（ahash, parking_lot, no wildcard imports）
- 测试基础设施辅助类型保留 `#[allow(dead_code)]`（项目惯例）

## Out of Scope

- 不实现 E2E 测试覆盖率（仅 unit + integration lib 测试）
- 不修改 e2e_tests.rs 中已知的 GlobalConfig/RequestContext 兼容性问题
- 不为 benchmark 代码测量覆盖率
- 不为 examples 测量覆盖率（仅 lib）
