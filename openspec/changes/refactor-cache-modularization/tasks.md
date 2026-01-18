## 1. 准备工作
- [x] 1.1 备份当前代码状态（git commit 或 stash）
- [x] 1.2 确认所有测试通过（`cargo test --all-features`）
- [x] 1.3 检查是否有其他分支依赖这些文件

## 2. 创建缓存模块结构
- [x] 2.1 创建 `src/cache/` 目录
- [x] 2.2 创建 `src/cache/mod.rs` 文件
- [x] 2.3 在 `src/cache/mod.rs` 中声明子模块：
  - `pub mod l2;`
  - `pub mod l3;`
  - `pub mod smart;`

## 3. 移动缓存文件
- [x] 3.1 移动 `src/l2_cache.rs` → `src/cache/l2.rs`
- [x] 3.2 移动 `src/l3_cache.rs` → `src/cache/l3.rs`
- [x] 3.3 移动 `src/smart_cache.rs` → `src/cache/smart.rs`

## 4. 更新模块内部导入
- [x] 4.1 更新 `src/cache/l3.rs` 中的导入路径：
  - `use crate::l2_cache::L2Cache` → `use crate::cache::l2::L2Cache`
  - `use crate::l2_cache::L2CacheConfig` → `use crate::cache::l2::L2CacheConfig`
- [x] 4.2 更新 `src/cache/smart.rs` 中的导入路径：
  - `use crate::l2_cache::{CacheEntry, L2Cache}` → `use crate::cache::l2::{CacheEntry, L2Cache}`

## 5. 创建缓存模块公共导出
- [x] 5.1 在 `src/cache/mod.rs` 中重新导出公共 API：
  - `pub use l2::{L2Cache, L2CacheConfig, CacheEntry, DEFAULT_CACHE_CAPACITY, DEFAULT_TTL_SECS, DEFAULT_CLEANUP_INTERVAL_SECS, DEFAULT_EVICTION_THRESHOLD};`
  - `pub use l3::{L3Cache, L3CacheConfig, L3CacheStats};`
  - `pub use smart::{SmartCacheStrategy, CacheStats as SmartCacheStats};`

## 6. 更新 lib.rs
- [x] 6.1 在 `src/lib.rs` 中添加 `pub mod cache;`
- [x] 6.2 更新 `src/lib.rs` 中的公共导出：
  - `pub use cache::{L2Cache, L2CacheConfig, L3Cache, L3CacheConfig, L3CacheStats, SmartCacheStrategy};`

## 7. 更新测试文件
- [x] 7.1 检查并更新 `tests/` 目录下的测试文件中的导入路径
- [x] 7.2 检查并更新 `examples/` 目录下的示例文件中的导入路径
- [x] 7.3 检查并更新其他源文件中的导入路径

## 8. 验证和测试
- [x] 8.1 运行 `cargo check --all-features` 确保编译通过
- [x] 8.2 运行 `cargo test --all-features` 确保所有测试通过
- [x] 8.3 运行 `cargo clippy --all-targets --all-features --workspace -- -D warnings` 确保代码质量
- [x] 8.4 运行 `cargo fmt --all` 确保代码格式正确

## 9. 文档更新
- [x] 9.1 更新 `IFLOW.md` 中的项目结构说明
- [x] 9.2 更新 `docs/API_REFERENCE.md` 中的模块路径（如果需要）

## 10. 提交变更
- [x] 10.1 运行 `git status` 查看变更
- [x] 10.2 运行 `git diff` 确认变更内容
- [x] 10.3 提交变更：`git commit -m "refactor(cache): modularize cache layer into standalone module"`