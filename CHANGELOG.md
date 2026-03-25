# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.1]: https://github.com/limiteron/limiteron/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/limiteron/limiteron/releases/tag/v0.1.0
