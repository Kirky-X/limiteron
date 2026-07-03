# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### BREAKING Changes

- **`default = []`**: Cargo.toml 的 `default` feature 从 `["postgres"]` 改为 `[]`。用户必须显式启用 feature 才能使用对应功能。
  - 迁移示例：`cargo build --features standard`（推荐）或 `cargo build --features postgres`（仅存储）
  - 默认构建 `cargo build` 现在只包含核心限流功能，不含 PostgreSQL 存储
- **移除死 feature flags**: `code-review` 和 `advanced-matchers` feature 从 `full` preset 和定义中移除（全仓库零 `#[cfg(feature = "...")]` 引用）
- **oxcache 升级 0.2.0 → 0.3.0**: 适配 oxcache 0.3.0 API。oxcache 0.2.0/0.3.0 有编译 bug（security 模块无 feature gate 但引用 regex crate），启用 `core` feature 作为 workaround

### Security

- **SSRF 防护加固** (`src/webhook_validator.rs`): 修复 IPv4-mapped IPv6 地址绕过（如 `::ffff:10.0.0.1` 不会被私有 IP 检查捕获）、未指定地址（`0.0.0.0`/`::`）、IPv6 链路本地地址（`fe80::/10`）的检查缺失

### Removed

- **Dead code 清理**: 移除 11 个 dead-code 警告对应的代码（`DecisionNodeBuilder`、`MAX_REGEX_NESTING_DEPTH`、`L1Cache::island_stats` 等），基于 gitnexus 影响分析确认无外部调用

### Developer Experience

- **`.gitignore` 加固**: 添加 `*.profraw`、`coverage/`、`tarpaulin/` 规则，防止覆盖率文件污染仓库

## [0.2.0] - 2026-07-03

### BREAKING CHANGES

- **BanManager API 变更**：
  - `ban()` → `add_ban(BanRecord)`，使用 `BanRecord` 结构体替代多个独立参数
  - `unban()` → `delete_ban(&target, unbanned_by: String)`，需要传入操作者标识
  - `is_banned()` 返回 `Result<Option<BanRecord>>` 而非 `bool`，提供更完整的封禁信息
- **限流器导入路径变更**：`use limiteron::{TokenBucketLimiter, ...}` → `use limiteron::limiters::{TokenBucketLimiter, ...}`，限流器类型统一收敛到 `limiters` 模块
- **`QuotaType` 路径变更**：`limiteron::QuotaType` → `limiteron::quota::QuotaType`
- **`MemoryStorage` 路径变更**：`limiteron::MemoryStorage` → `limiteron::storage::MemoryStorage`
- **`FallbackManager::new(cache)` → `FallbackManager::new(Arc::new(cache))`**，统一使用 `Arc` 包装依赖

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
- **examples 覆盖 19 个使用场景**：新增 `redis_storage`、`graceful_shutdown` 等示例
- **AGENTS.md 更新**：添加 RedisStorage 模块说明、LOC 计数、redis 依赖

## [0.1.1] - 2026-01-20

### Added

- **MemoryStorage and MemoryBanStorage**: In-memory storage implementations for `Storage` and `BanStorage` traits. These enable the "out-of-the-box" pattern for quick prototyping and testing.
- **Governor::new()**: New zero-argument constructor for `Governor` that uses default memory storage. Enables quick start without external dependencies.
- **BanManager::new()**: New zero-argument constructor for `BanManager` that uses default memory storage.
- **StorageCreate and BanStorageCreate traits**: Factory traits for creating default storage instances.
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

[0.2.0]: https://github.com/limiteron/limiteron/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/limiteron/limiteron/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/limiteron/limiteron/releases/tag/v0.1.0
