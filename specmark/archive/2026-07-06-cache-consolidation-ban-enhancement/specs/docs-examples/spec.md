# Spec — docs-examples

> Delta spec for change `cache-consolidation-ban-enhancement`. 覆盖此变更引入/修改的文档与示例能力域需求。

## Requirements

### R-docs-examples-001: README 更新

README.md 和 README_EN.md 反映所有架构变更和新功能。

**验收标准：**
- README.md 移除 RedisStorage 相关说明
- README.md 新增 BanTarget::Geo 章节说明
- README.md 新增文件加载 ban 章节（YAML 格式 + 热重载）
- README.md 新增 POST /api/v1/ban 端点说明
- README.md 更新 features 列表移除 redis-storage
- README_EN.md 同步更新（与中文版一致）
- Feature 列表与 Cargo.toml 实际 features 一致

### R-docs-examples-002: CHANGELOG 更新

CHANGELOG.md 新增 v0.3.0 section，列出全部变更。

**验收标准：**
- 新增 `## [0.3.0] - 2026-07-XX` section
- Breaking changes 列表含：移除 redis-storage feature、移除 RedisStorage 类型、BanTarget 新增 Geo 变体
- New features 列表含：BanTarget::Geo、BanFileLoader、POST /api/v1/ban、依赖升级
- Migration guide 说明从 RedisStorage 迁移到 CacheStorage 的步骤

### R-docs-examples-003: USER_GUIDE + API_REFERENCE 更新

docs/USER_GUIDE.md 和 docs/API_REFERENCE.md 同步更新。

**验收标准：**
- USER_GUIDE.md 新增「文件加载 Ban」章节（YAML 示例 + 配置说明）
- USER_GUIDE.md 新增「Geo Ban」章节
- USER_GUIDE.md 更新「存储后端」章节移除 RedisStorage，说明 CacheStorage + oxcache Redis 后端
- API_REFERENCE.md 新增 `POST /api/v1/ban` 端点完整文档（请求/响应/错误码/示例）
- API_REFERENCE.md 更新 `BanTarget` 枚举文档含 Geo 变体
- API_REFERENCE.md 新增 `BanFileLoader` API 文档

### R-docs-examples-004: 删除 redis_storage 示例

examples/src/bin/redis_storage.rs 已在 cache-backend spec 删除，本 spec 确认 examples 结构更新。

**验收标准：**
- examples/src/bin/redis_storage.rs 不存在
- examples/Cargo.toml 移除 redis_storage bin 配置（如存在）
- examples/README.md（如存在）移除 redis_storage 引用

### R-docs-examples-005: 新增 ban_file_loader 示例

新增 examples/src/bin/ban_file_loader.rs 演示 YAML 文件加载 + 热重载。

**验收标准：**
- 文件存在并编译通过（`cargo build --example ban_file_loader`）
- 演示创建 YAML 文件、启动 BanFileLoader、触发热重载
- 含中文注释说明关键步骤
- 演示错误处理（无效 YAML、文件不存在）

### R-docs-examples-006: 新增 ban_http_api 示例

新增 examples/src/bin/ban_http_api.rs 演示 POST /api/v1/ban（覆盖 4 种 target_type）。

**验收标准：**
- 文件存在并编译通过（`cargo build --example ban_http_api`）
- 演示启动 AdminServer、调用 POST /api/v1/ban 创建 ip/user/mac/geo 4 种 ban
- 含中文注释说明请求/响应格式
- 演示错误响应处理（验证错误、认证错误）

### R-docs-examples-007: Examples 全覆盖审计

审计现有 examples/src/bin/ 全部样例，对照 src/lib.rs 公开 API 补齐遗漏场景。

**验收标准：**
- 审计清单输出到 specmark/changes/cache-consolidation-ban-enhancement/examples-audit.md
- 清单含每个 example 的覆盖能力
- 对照 src/lib.rs 公开 API，列出未覆盖的 API
- 补齐遗漏的 example（如关键 API 无对应示例）
- 所有 examples 通过 `cargo build --examples --features full` + clippy 检查

## Constraints

- 文档语言：README.md 中文为主，README_EN.md 英文同步
- 示例代码遵循项目惯例（4 空格缩进、100 字符行宽、no wildcard imports）
- 示例必须可编译运行（不允许 `// compile_fail` 示例）
- 不创建无意义的 hello-world 示例（每个示例演示真实场景）

## Out of Scope

- 不重写现有 19 个 example（仅审计补漏）
- 不创建视频/动画教程
- 不创建交互式 playground
- 不为 macros crate 单独创建 examples（已在主 examples 中覆盖）
