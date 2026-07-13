# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.6] - 2026-07-13

### Added

- `distributed` feature 与 `src/limiters/distributed.rs` 模块（DistributedLimiter trait + InMemoryDistributedLimiter 实现）—— 支持分布式与进程内限流兼容
- 跨平台 CI 矩阵（ubuntu/macos/windows）验证 apple/windows/linux 平台兼容性

### Changed

- `release.yml` publish 步骤幂等化：捕获 cargo publish 输出，若失败但匹配 "already exists"/"already published" 则发 ::warning:: 并继续
- `benches/memory.rs`: 修复 `unsafe-op-in-unsafe-fn` clippy lint（unsafe fn 内部操作用 `unsafe { }` 包裹）
- `benches/regression.rs` + `tests/common/mod.rs`: 修复 `collapsible_if` clippy lint（嵌套 if-let 用 let-chains 合并）

### Fixed

- CI clippy lint 失败（unsafe-op-in-unsafe-fn + collapsible_if）
- `examples/integration-app` governor 私有模块访问错误（改为 re-export `limiteron::Governor`）

### ⚠️ BREAKING CHANGES（仅影响启用 `kit` feature 的用户）

- trait-kit 0.2 → 0.3（pre-1.0 minor bump，Cargo 视为不兼容）；启用 `kit` feature 的用户需同步升级

### Dependencies

- trait-kit 0.2 → 0.3（对齐 oxcache/dbnexus/inklog 依赖链）
- inklog 0.1.6 → 0.1.7（transitive, via Cargo.lock resolution）

## [0.2.5] - 2026-07-13

### Dependencies

- dbnexus 0.2 → 0.4（解决 oxcache 版本冲突，dbnexus 0.4 现依赖 oxcache 0.3）
- oxcache 0.3.7 → 0.3.8

## [0.2.4] - 2026-07-12

### Changed

- `FlowGuardError` renamed to `LimiteronError`, following `ProjectNameError` naming convention
- Added `LimiteronResult<T>` type alias
- Cross-crate imports updated: `oxcache::CacheError` → `oxcache::OxCacheError`（适配 oxcache 0.3.7）
- 导入路径扁平化

## [0.2.3] - 2026-07-11

### Changed

- 移除 `StructuredLogger` trait（YAGNI 清理）
- 对齐 inklog 集成与 sdforge 模式
- 修复 edition 2024 unsafe env 调用

### Changed（Phase 6 前置）

- Rust edition 从 2021 升级到 2024
- 设置 rust-version 为 1.85
- 许可证从 Apache-2.0 变更为 MIT

### Fixed

- 修复 edition 2024 模式匹配错误（`ref` 关键字、隐式借用）

## [0.2.1] - 2026-07-06

### Breaking Changes

- **移除 RedisStorage**: 完全删除 Redis 存储后端实现及 `redis-storage` feature
  - 删除 `src/storage/redis.rs`
  - 删除 `examples/src/bin/redis_storage.rs`
  - 移除 `redis` crate 依赖
  - 所有缓存通过 oxcache 统一管理
- **移除 StorageCreate/BanStorageCreate trait**: 改为 `MemoryStorage::create_storage()` 固有方法
- **移除 SlidingWindowLimiter 公开导出**: 使用 `ShardedSlidingWindowLimiter` 替代
  - 仍可通过 `limiteron::limiters::sliding_window::SlidingWindowLimiter` 全路径访问（已废弃）

### New Features

- **BanTarget::Geo**: 新增地理位置封禁，支持按国家代码（ISO 3166-1 alpha-2）封禁
- **BanFileLoader**: 从 YAML 文件加载封禁规则，支持文件变更热重载（500ms debounce）
- **POST /api/v1/ban**: 新增 HTTP 端点创建封禁，支持 ip/user/mac/geo 4 种 target 类型
- **DELETE /api/v1/ban/{target}?type=**: 扩展支持 MAC/Geo 目标解封

### Security Fixes

- YAML 炸弹防护：封禁文件大小限制 2MB
- AdminServer::start() 强制调用 config.validate()
- 热重载 debounce 防止 DoS
- 告警 spawn-fire-and-forget 添加 Semaphore(8) 背压
- BanManager/EventDispatcher 添加 Drop impl 防止任务泄漏
- 授权链路显性化：未配置 provider 时记录警告日志
- 整数下溢防护（list_bans 分页）
- 时钟回退防护（配额窗口重置）
- `as u32`/`as u8` 截断修复
- redact_advanced 正则脱敏逻辑修复

### Improvements

- handler 错误响应状态码统一（200/400/403/404/422/500/501/503）
- get_limiter_status 改为 501 Not Implemented（不再返回假数据）
- 测试辅助函数集中到 test_support.rs 消除 3x 重复
- GovernorBuilder 移除 #[allow(dead_code)]
- config TODO 模块状态文档明确
- **GovernorBuilder metrics/tracer 修复**：`with_metrics()`/`with_tracer()` 的值之前在 `build()` 中被静默丢弃（`#[allow(unused_variables)]` 掩盖），现在正确存储到 Governor 并在 `check()` 中消费（`record_check`/`record_ban`/`record_error` + `tracer.start_span`）
- **依赖版本格式统一**：`Cargo.toml` 全部 `[workspace.dependencies]` 移除 `~` 最小版本前缀，统一使用 `"X.Y"` 格式（caret 语义）
  - 主要升级：`criterion` 0.5 → 0.8、`sqlx` 0.7 → 0.9、`sea-orm` 1.0 → 2.0.0-rc.42、`maxminddb` 0.24 → 0.29、`hmac` 0.12 → 0.13、`sha2` 0.10 → 0.11、`opentelemetry` 0.24 → 0.32、`reqwest` 0.11 → 0.13、`axum` 0.7 → 0.8、`tower-http` 0.5 → 0.7、`dashmap` 5.5 → 6.2、`thiserror` 1.0 → 2.0、`validator` 0.18 → 0.20、`rand` 0.8 → 0.10、`notify` 6.5 → 8.2、`secrecy` 0.8 → 0.10、`woothee` 0.11 → 0.13
  - `dbnexus` 保持 `0.2`：`0.3` 依赖 `oxcache 0.2`，与本项目 `oxcache 0.3` 冲突

### Test Coverage

- 1893 unit tests passing (0 failed)
- 96.16% line coverage (5781/6012 lines)

### Fixed

- **API 兼容性修复**（依赖 MAJOR 升级）：
  - `maxminddb 0.29`：`Reader.metadata` 字段私有化，改用 `reader.metadata()` 方法（`src/matchers/geo.rs`）
  - `hmac 0.13`：`new_from_slice` 改为 `KeyInit` trait 方法，补充 `use hmac::KeyInit`（`src/logging/audit.rs`）
  - `sqlx 0.9`：组合 feature `runtime-tokio-rustls` 拆分为 `runtime-tokio` + `tls-rustls`（`Cargo.toml`）
- **clippy 兼容性修复**（rust 1.96 新 lint）：
  - `unnecessary_sort_by`：`sort_by(|a,b| b.x.cmp(&a.x))` → `sort_by_key(|r| Reverse(r.x))`（`src/adapters/dbnexus_ban_storage.rs`）
  - `manual_checked_ops`：手动 `if x > 0 { y / x } else { default }` → `checked_div().unwrap_or(default)`（`src/limiters/gcra.rs`、`tests/chaos/latency.rs`）

### Documentation

- **`docs/FAQ.md`**：示例依赖版本 `limiteron = "0.1"` → `"0.2"`（与当前发布版本一致）

## [0.2.0] - 2026-07-04

### BREAKING CHANGES

- **`default = []`**: Cargo.toml 的 `default` feature 从 `["postgres"]` 改为 `[]`。用户必须显式启用 feature 才能使用对应功能。
  - 迁移示例：`cargo build --features standard`（推荐）或 `cargo build --features postgres`（仅存储）
  - 默认构建 `cargo build` 现在只包含核心限流功能，不含 PostgreSQL 存储
- **移除死 feature flags**: `code-review` 和 `advanced-matchers` feature 从 `full` preset 和定义中移除（全仓库零 `#[cfg(feature = "...")]` 引用）
- **oxcache 升级 0.2.0 → 0.3.2**: 适配 oxcache 0.3.x API。oxcache 0.2.0/0.3.x 有编译 bug（security 模块无 feature gate 但引用 regex crate），启用 `core` feature 作为 workaround
- **BanManager API 变更**：
  - `ban()` → `add_ban(BanRecord)`，使用 `BanRecord` 结构体替代多个独立参数
  - `unban()` → `delete_ban(&target, unbanned_by: String)`，需要传入操作者标识
  - `is_banned()` 返回 `Result<Option<BanRecord>>` 而非 `bool`，提供更完整的封禁信息
- **限流器导入路径变更**：`use limiteron::{TokenBucketLimiter, ...}` → `use limiteron::limiters::{TokenBucketLimiter, ...}`，限流器类型统一收敛到 `limiters` 模块
- **`QuotaType` 路径变更**：`limiteron::QuotaType` → `limiteron::quota::QuotaType`
- **`MemoryStorage` 路径变更**：`limiteron::MemoryStorage` → `limiteron::storage::MemoryStorage`
- **`FallbackManager::new(cache)` → `FallbackManager::new(Arc::new(cache))`**，统一使用 `Arc` 包装依赖

### Security

- **SSRF 防护加固** (`src/webhook_validator.rs`): 修复 IPv4-mapped IPv6 地址绕过（如 `::ffff:10.0.0.1` 不会被私有 IP 检查捕获）、未指定地址（`0.0.0.0`/`::`）、IPv6 链路本地地址（`fe80::/10`）的检查缺失

### Removed

- **Dead code 清理**: 移除 11 个 dead-code 警告对应的代码（`DecisionNodeBuilder`、`MAX_REGEX_NESTING_DEPTH`、`L1Cache::island_stats` 等），基于 gitnexus 影响分析确认无外部调用

### Added

- **RedisStorage 存储后端**（`redis-storage` feature）：实现 `Storage`/`BanStorage`/`QuotaStorage` trait，支持多实例分布式场景
- **Governor 优雅关闭**：新增 `shutdown()` / `shutdown_token()` / `is_shutdown()` 方法，支持优雅停止后台任务
- **Governor 健康检测**：新增 `health_check()` / `health_status()` 方法，提供真实的健康状态检测
- **ConfigLoader 环境变量覆盖**：支持 `LIMITERON_GLOBAL_STORAGE` / `LIMITERON_GLOBAL_CACHE` / `LIMITERON_GLOBAL_METRICS` 等环境变量覆盖配置
- **CircuitBreaker `new()` 默认构造方法**：支持开箱即用模式
- **`storage_cleanup_expired_bans` 批量删除优化**：提升过期封禁记录清理性能
- **`redis_storage` example**：RedisStorage 使用示例
- **`graceful_shutdown` example**：优雅关闭使用示例

### Fixed

- **clippy 零警告**：`src/` + `tests/` + `benches/` 全部通过 clippy 严格检查
- **修复 `cleanup_expired_bans` 死锁风险**：避免在持有锁时执行可能阻塞的操作
- **修复 Governor 字段 `_storage`/`_ban_storage` 下划线前缀问题**：移除不必要的下划线前缀，字段实际被使用

### Documentation

- **README 中英文同步**：版本徽章、特性列表、测试计数、路线图全面更新
- **examples 覆盖 20 个使用场景**：新增 `redis_storage`、`graceful_shutdown` 等示例
- **AGENTS.md 更新**：添加 RedisStorage 模块说明、LOC 计数、redis 依赖

### Developer Experience

- **`.gitignore` 加固**: 添加 `*.profraw`、`coverage/`、`tarpaulin/` 规则，防止覆盖率文件污染仓库

## [0.1.1] - 2026-01-20

### Added

- **MemoryStorage and MemoryBanStorage**: In-memory storage implementations for `Storage` and `BanStorage` traits. These enable the "out-of-the-box" pattern for quick prototyping and testing.
- **Governor::new()**: New zero-argument constructor for `Governor` that uses default memory storage. Enables quick start without external dependencies.
- **BanManager::new()**: New zero-argument constructor for `BanManager` that uses default memory storage.
- **Feature Components Construction Patterns**: Documentation table in AGENTS.md showing which patterns each component supports.

### Changed

- **Governor::new(config, storage, ban_storage)**: Renamed to `Governor::with_storage(config, storage, ban_storage)` to make room for the new zero-argument `new()` method. The old signature is still available via the renamed method.

### Deprecated

- **config_loader::ConfigBuilder**: Use `config::ConfigBuilder` instead. The type is now a re-export with a deprecation warning.
- **config_loader::RuleBuilder**: Use `config::RuleBuilder` instead. The type is now a re-export with a deprecation warning.
- **Governor::new(config, storage, ban_storage)**: Use `Governor::with_storage()` instead. This change enables the new out-of-the-box pattern.

### Fixed

- Governor now properly implements the three construction patterns as specified in the DI architecture documentation.
- BanManager builder now supports optional storage (uses MemoryBanStorage as default).

### Security

- None

### Documentation

- Added "Feature Components Construction Patterns" section to AGENTS.md with usage examples and migration notes.
- Added migration notes for API changes.

### Migration Guide

#### For ConfigBuilder Users

Before (deprecated):
```rust
use limiteron::config_loader::ConfigBuilder;
let config = ConfigBuilder::new().with_rule(|r| r.id("test")).build();
```

After (recommended):
```rust
use limiteron::config::ConfigBuilder;
let config = ConfigBuilder::new().with_rule(|r| r.id("test")).build();
```

#### For Governor Users

Before (deprecated):
```rust
let governor = Governor::new(config, storage, ban_storage).await.unwrap();
```

After (recommended):
```rust
let governor = Governor::with_storage(config, storage, ban_storage).await.unwrap();
```

Quick start (new):
```rust
let governor = Governor::new().await;
```

#### For BanManager Users

Before:
```rust
let storage: Arc<dyn BanStorage> = Arc::new(custom_storage);
let ban_manager = BanManager::with_dependencies(storage, config).await.unwrap();
```

Now (with optional storage):
```rust
let ban_manager = BanManager::builder().build().await.unwrap();
// Or with custom storage:
let ban_manager = BanManager::builder()
    .with_storage(custom_storage)
    .build()
    .await
    .unwrap();
```

Quick start (new):
```rust
let ban_manager = BanManager::new().await.unwrap();
```

## [0.1.0] - 2026-01-18

### Added

- Initial release with rate limiting, quota management, circuit breaking, and ban management
- Support for multiple rate limiting algorithms: TokenBucket, SlidingWindow, FixedWindow, Concurrency
- Ban management with priority system (IP > User > MAC > Device > APIKey)
- Quota control with periodic allocation and alerting
- Circuit breaker for automatic failover and state recovery
- L1/L2/L3 caching layers
- Integration with dbnexus for PostgreSQL persistence
- Integration with oxcache for Redis caching
- Integration with confers for configuration management
- Declarative macros for simplified configuration
- Monitoring with Prometheus metrics and OpenTelemetry tracing
- Parallel ban checking for improved performance

[Unreleased]: https://github.com/Kirky-X/limiteron/compare/v0.2.6...HEAD
[0.2.6]: https://github.com/Kirky-X/limiteron/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/Kirky-X/limiteron/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/Kirky-X/limiteron/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/Kirky-X/limiteron/compare/v0.2.1...v0.2.3
[0.2.1]: https://github.com/Kirky-X/limiteron/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Kirky-X/limiteron/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/Kirky-X/limiteron/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Kirky-X/limiteron/releases/tag/v0.1.0
