# diting 代码质量审计报告

**审计范围**：/home/dev/projects/limiteron 全代码库（100+ Rust 源文件）
**审计维度**：过度工程、技术债务、代码重复、死代码、命名一致性、错误处理一致性、文档完整性、DI 合规性

## CRITICAL（必须修复）

1. **[src/admin/handlers.rs:110-124] 公开 API 端点返回假数据**
   `get_limiter_status` handler 标注 `// TODO: 实现从存储中获取限流状态`，但已上线暴露为 `GET /api/v1/status/limiter/{key}`，永远返回 `limit:0, remaining:0, reset_at:0`。
   → 返回 `501 Not Implemented` + `ApiResponse::error("not implemented")`，禁止用 200 OK 掩盖未完成功能（Rule 12）。

2. **[src/limiters/sliding_window.rs + src/limiters/mod.rs:43 + src/decision_chain/types.rs] 已废弃 API 仍被内部使用并公开导出**
   `SlidingWindowLimiter` 标注 `#[deprecated]` 但 `mod.rs` 仍 `pub use` 公开导出，`decision_chain/types.rs` 4 处测试调用。
   → 从 `mod.rs` 移除公开导出；测试改用 `ShardedSlidingWindowLimiter`；用 `#[cfg(test)]` + `#[allow(deprecated)]` 限定。

3. **[src/admin/handlers.rs:140-165, 281-323, 337-357] handler 错误响应 HTTP 状态码不一致**
   `delete_ban`/`update_quota`/`get_circuit_breaker_status` 返回 `Json<ApiResponse<()>>` 无 StatusCode，"not configured"/"not found"/"failed" 全部返回 HTTP 200。对比 `create_ban` 正确返回 `(StatusCode, Json<...>)`。
   → 所有 handler 统一返回 `(StatusCode, Json<ApiResponse<T>>)`；"not configured"→503，"not found"→404，验证错误→400，内部错误→500。

## HIGH（应该修复）

4. **[src/admin/handlers.rs:378,402 + src/admin/routes.rs:99,123 + src/admin/server.rs:112,136] 测试辅助函数 3x 重复**
   `make_valid_config()` 与 `make_governor()` 在 3 个 admin 模块文件中逐字复制（共 6 个函数体，约 70 行重复代码）。
   → 在 `src/admin/mod.rs` 或 `src/admin/test_support.rs`（`#[cfg(test)]`）中定义一次。

5. **[src/storage/mod.rs:191-213, 215-216] `StorageCreate` / `BanStorageCreate` 是幽灵 trait（过度工程）**
   每个 trait 仅 1 个空实现，默认方法体硬编码 `MemoryStorage`，多态价值为零。
   → 删除两个 trait，改为 `impl MemoryStorage { pub fn create_storage() -> ... }`；同步移除 `lib.rs:215-218` 公开导出。

6. **[src/governor.rs:186] `#[allow(dead_code)]` 标注整个 `GovernorBuilder` 结构体**
   会抑制所有字段的未使用警告，可能掩盖真实死代码。
   → 移除 `#[allow(dead_code)]`，逐字段排查；对特定字段用 `#[allow(dead_code)]` 并附 reason。

7. **[src/config/mod.rs:12,32,37] 三个 TODO 标注核心模块未实现**
   `config-watcher` 是 `BanFileLoader::start_watching` 依赖的特性，`config-security` 完全缺失。
   → 创建 issue 跟踪或文档中明确"未实现"状态。

## MEDIUM（建议修复）

8. [src/admin/handlers.rs:445 vs src/admin/routes.rs:135] 测试辅助函数命名不一致（`make_minimal_state` vs `make_state`）
9. [src/cache/cache_service.rs:20 + src/cache/memory_cache.rs:99] `CacheService` trait 仅 1 个生产实现，文档声称支持多 backend
10. [src/events/types.rs:179 + src/events/dispatcher.rs] `EventHandler` trait 仅有测试实现
11. [src/storage/mod.rs:84-97] `BanTarget::UserId` serde rename 为 "user" 与变体名不一致
12. [src/decision_chain/types.rs:930] `#[cfg(feature = "legacy_tests")]` TODO 标注 short_circuit 行为未实现
13. [src/ban/types.rs:1230,1236,1446] 测试模块用 `#[allow(dead_code)]` 抑制未用辅助函数

## LOW（可选优化）

14. [src/governor.rs:2769 + src/error/mod.rs:632] `unreachable!()` 在测试代码中使用
15. [src/admin/handlers.rs:286-311] `update_quota` 端点对 `new_limit > 0` 返回 "not supported"
16. [src/logging/redaction.rs:294-297] `redact_ban_target` 与 `BanPriority::from_target` 重复 match 模式

## DI 合规性专项检查

**Trait Send+Sync 合规**：✅ 全部 16 个 pub trait 均声明 `Send + Sync`

**三种构造模式合规**：

| 组件 | new() | builder() | with_dependencies() | 状态 |
|------|-------|-----------|---------------------|------|
| Governor | ✅ | ✅ | ✅ | 合规 |
| QuotaController | ✅ | ✅ | ✅ | 合规 |
| ConcurrencyLimiter | ✅ | ✅ | ✅ | 合规 |
| BanManager | ✅ | ✅ | ✅ | 合规 |
| CircuitBreaker | ✅ | N/A | ✅ | 合规 |
| AdminServer | ✅ | ❌（链式 with_xxx） | ❌ | 偏离（应用容器可豁免） |

## 统计

- CRITICAL: 3, HIGH: 4, MEDIUM: 6, LOW: 3
- 整体代码质量评分: 6.5/10

## 裁决

**Changes Requested** —— 需修复 3 个 CRITICAL 项方可合并/发布。HIGH 项建议在同一 PR 修复。
