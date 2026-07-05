# Spec — cache-backend

> Main spec for the cache-backend capability domain. 初始版本由 change `cache-consolidation-ban-enhancement` 引入（2026-07-06）。

## Requirements

### R-cache-backend-001: 删除 RedisStorage 实现

`src/storage/redis.rs` 文件完全删除，`RedisStorage` 类型不再存在。

**验收标准：**
- `src/storage/redis.rs` 文件不存在
- `cargo build --features full` 不报"cannot find RedisStorage"错误
- `src/storage/mod.rs` 中无 `pub mod redis;` 声明
- `src/lib.rs` 中无 `pub use storage::redis::RedisStorage;` 导出

### R-cache-backend-002: 删除 RedisStorage 相关测试与示例

依赖 RedisStorage 的测试和示例文件删除。

**验收标准：**
- `tests/integration/real_storage/redis_storage.rs` 不存在
- `tests/integration/real_storage/distributed_consistency.rs` 不存在（如依赖 RedisStorage）
- `examples/src/bin/redis_storage.rs` 不存在
- `cargo test --features full --lib` 不引用已删除文件

### R-cache-backend-003: Redis 后端能力通过 oxcache 提供

用户配置 Redis 后端时使用 `cache-storage` feature + `oxcache/redis` + `CacheStorage::new(Arc::new(RedisBackend::new(...)))`。

**验收标准：**
- `cache-storage` feature 保留 `oxcache/redis` 依赖
- 文档（USER_GUIDE.md）说明 Redis 后端配置方式
- `examples/src/bin/cache_storage_redis.rs`（如存在）演示 oxcache Redis 后端用法

## Constraints

- 不删除 `cache-storage` feature（这是 Redis 后端的唯一路径）
- 不修改 oxcache 源码（external dependency）
- Breaking change：下游使用 RedisStorage 的代码需迁移到 CacheStorage

## Out of Scope

- 不实现 RedisCluster 支持（oxcache 后续能力）
- 不实现 Redis Sentinel 支持
- 不优化 Redis 连接池配置（oxcache 内部职责）
