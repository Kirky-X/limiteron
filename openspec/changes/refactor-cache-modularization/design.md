## Context

### 当前状态

当前 `src/` 目录包含以下缓存相关文件：
- `l2_cache.rs` - L2 缓存实现（基于 DashMap 和 LRU Cache）
- `l3_cache.rs` - L3 缓存实现（基于 Redis，依赖 L2 缓存）
- `smart_cache.rs` - 智能缓存策略（依赖 L2 缓存）

这些文件与核心业务逻辑文件（如 `governor.rs`、`limiters.rs`、`ban_manager.rs` 等）混在一起，导致：
1. 目录结构不够清晰
2. 模块边界不明确
3. 难以快速定位缓存相关代码
4. 不符合项目的模块化架构约定（已有 `factory/`、`bin/` 等子模块）

### 约束条件

1. **向后兼容**：公共 API 必须保持不变，不能破坏现有用户代码
2. **测试覆盖**：所有现有测试必须继续通过
3. **编译通过**：不能引入任何编译错误
4. **代码质量**：必须通过 clippy 检查和格式化检查

### 利益相关者

- **开发者**：需要更清晰的代码组织结构
- **用户**：不应感知到内部重构，公共 API 保持不变
- **维护者**：需要更容易定位和修改缓存相关功能

## Goals / Non-Goals

### Goals

1. 将所有缓存相关代码组织到 `src/cache/` 子模块中
2. 保持公共 API 不变，确保向后兼容
3. 所有现有测试通过
4. 提高代码的可读性和可维护性
5. 为未来添加新的缓存实现提供清晰的扩展点

### Non-Goals

1. 不改变缓存的实现逻辑或性能
2. 不修改公共 API 的行为
3. 不添加新的缓存功能
4. 不改变缓存层的接口设计

## Decisions

### Decision 1: 模块结构设计

**选择**：创建 `src/cache/` 子模块，包含三个子文件：
- `l2.rs` - L2 缓存实现
- `l3.rs` - L3 缓存实现
- `smart.rs` - 智能缓存策略
- `mod.rs` - 模块入口和公共导出

**理由**：
- 符合 Rust 的模块组织惯例
- 与现有的 `factory/` 和 `bin/` 子模块保持一致
- 清晰的模块边界和职责分离
- 便于未来扩展（如添加 `l4.rs`、`distributed.rs` 等）

**替代方案**：
1. 保持扁平结构（`l2_cache.rs`、`l3_cache.rs`、`smart_cache.rs`）
   - ❌ 不符合模块化架构约定
   - ❌ 难以扩展新的缓存实现
2. 创建多个独立的子模块（`src/l2/`、`src/l3/`、`src/smart/`）
   - ❌ 过度设计，增加不必要的嵌套层级
   - ❌ 缓存层应该作为一个整体模块

### Decision 2: 文件命名

**选择**：使用简短名称（`l2.rs`、`l3.rs`、`smart.rs`）而非完整名称（`l2_cache.rs`、`l3_cache.rs`、`smart_cache.rs`）

**理由**：
- 父模块 `cache/` 已经提供了上下文，不需要重复
- 更简洁的文件名
- 符合 Rust 社区惯例（如 `std::collections::hash_map` 而非 `std::collections::hash_map_module`）

**替代方案**：
1. 使用完整名称（`l2_cache.rs`、`l3_cache.rs`、`smart_cache.rs`）
   - ❌ 冗余，父模块已提供上下文
   - ❌ 文件名过长

### Decision 3: 公共导出策略

**选择**：在 `src/cache/mod.rs` 中重新导出所有公共类型和常量，在 `src/lib.rs` 中再次重新导出

**理由**：
- 用户可以通过 `limiteron::cache::L2Cache` 或 `limiteron::L2Cache` 访问
- 提供灵活的导入选项
- 符合 Rust 库的常见模式

**示例**：
```rust
// src/cache/mod.rs
pub use l2::{L2Cache, L2CacheConfig, CacheEntry, ...};
pub use l3::{L3Cache, L3CacheConfig, L3CacheStats};
pub use smart::{SmartCacheStrategy, CacheStats as SmartCacheStats};

// src/lib.rs
pub use cache::{L2Cache, L2CacheConfig, L3Cache, L3CacheConfig, L3CacheStats, SmartCacheStrategy};
```

**替代方案**：
1. 只在 `src/lib.rs` 中导出
   - ❌ 用户无法通过 `limiteron::cache::` 访问
   - ❌ 不符合模块化设计原则
2. 只在 `src/cache/mod.rs` 中导出
   - ❌ 用户必须使用完整路径 `limiteron::cache::L2Cache`
   - ❌ 不够灵活

### Decision 4: 内部依赖处理

**选择**：更新模块内部导入路径，使用相对路径或绝对路径

**理由**：
- `l3.rs` 和 `smart.rs` 依赖 `l2.rs` 中的类型
- 使用 `use crate::cache::l2::L2Cache` 明确依赖关系
- 避免循环依赖

**示例**：
```rust
// src/cache/l3.rs
use crate::cache::l2::{L2Cache, L2CacheConfig};

// src/cache/smart.rs
use crate::cache::l2::{CacheEntry, L2Cache};
```

**替代方案**：
1. 使用 `super::l2::L2Cache`
   - ✅ 也可以，但 `crate::cache::l2::L2Cache` 更明确
2. 使用 `use crate::l2_cache::L2Cache`（旧路径）
   - ❌ 路径已失效，会导致编译错误

## Risks / Trade-offs

### Risk 1: 导入路径更新遗漏

**风险**：可能遗漏某些文件中的导入路径更新，导致编译错误

**缓解措施**：
1. 使用 `rg` 搜索所有引用 `l2_cache`、`l3_cache`、`smart_cache` 的文件
2. 运行 `cargo check` 确保所有编译错误都被修复
3. 运行完整的测试套件确保没有遗漏

### Risk 2: 循环依赖

**风险**：如果其他模块依赖缓存模块，而缓存模块又依赖其他模块，可能导致循环依赖

**缓解措施**：
1. 检查当前依赖关系，确保没有循环依赖
2. 使用 `use crate::` 绝对路径避免歧义
3. 如果发现循环依赖，考虑引入 trait 来解耦

### Trade-off: 文件移动的复杂性

**权衡**：移动文件需要更新所有导入路径，工作量较大

**理由**：
- 一次性完成所有更新，避免增量更新的混乱
- 使用自动化工具（如 `rg`）辅助查找需要更新的文件
- 长期收益大于短期工作量

## Migration Plan

### 步骤 1：准备工作
1. 创建 git commit 或 stash 保存当前状态
2. 运行 `cargo test --all-features` 确保所有测试通过

### 步骤 2：创建新结构
1. 创建 `src/cache/` 目录
2. 创建 `src/cache/mod.rs` 文件
3. 在 `src/cache/mod.rs` 中声明子模块

### 步骤 3：移动文件
1. 移动 `src/l2_cache.rs` → `src/cache/l2.rs`
2. 移动 `src/l3_cache.rs` → `src/cache/l3.rs`
3. 移动 `src/smart_cache.rs` → `src/cache/smart.rs`

### 步骤 4：更新导入
1. 更新 `src/cache/l3.rs` 中的导入路径
2. 更新 `src/cache/smart.rs` 中的导入路径
3. 更新 `src/lib.rs` 中的模块声明和导出
4. 更新所有测试文件和示例文件中的导入路径

### 步骤 5：验证
1. 运行 `cargo check --all-features`
2. 运行 `cargo test --all-features`
3. 运行 `cargo clippy --all-targets --all-features --workspace -- -D warnings`
4. 运行 `cargo fmt --all`

### 步骤 6：文档更新
1. 更新 `IFLOW.md` 中的项目结构说明
2. 更新 `docs/API_REFERENCE.md` 中的模块路径（如果需要）

### 步骤 7：提交
1. 运行 `git status` 和 `git diff` 确认变更
2. 提交变更：`git commit -m "refactor(cache): modularize cache layer into standalone module"`

### 回滚计划

如果重构失败或引入问题：
1. 使用 `git reset --hard HEAD^` 回滚到重构前的状态
2. 或使用 `git stash pop` 恢复之前保存的状态
3. 重新评估问题并制定新的计划

## Open Questions

无。所有设计决策已经明确。

## Implementation Notes

### 搜索需要更新的文件

使用以下命令搜索需要更新导入路径的文件：

```bash
# 搜索引用 l2_cache 的文件
rg "use crate::l2_cache" --type rust

# 搜索引用 l3_cache 的文件
rg "use crate::l3_cache" --type rust

# 搜索引用 smart_cache 的文件
rg "use crate::smart_cache" --type rust
```

### 验证公共 API 不变

运行以下命令确保公共 API 不变：

```bash
# 生成公共 API 文档
cargo doc --all-features --no-deps --open

# 检查是否有任何公共 API 被移除或修改
cargo semver-checks (如果可用)
```

### 性能验证

运行基准测试确保性能不受影响：

```bash
cargo bench --all-features
```

## Future Considerations

### 可能的扩展

1. **L4 缓存**：未来可以添加分布式缓存（如 Memcached、etcd）
2. **缓存策略**：可以添加更多智能缓存策略（如自适应 TTL、预测性预取）
3. **缓存监控**：可以添加更详细的缓存监控和指标收集

### 模块演进

随着缓存模块的成熟，可以考虑：
1. 将缓存模块提取为独立的 crate
2. 添加更多的配置选项和自定义能力
3. 支持插件式的缓存策略