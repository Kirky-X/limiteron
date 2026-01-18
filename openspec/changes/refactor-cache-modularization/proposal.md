# Change: 重构缓存模块化

## Why

当前 `src/` 目录下的缓存相关文件（`l2_cache.rs`、`l3_cache.rs`、`smart_cache.rs`）分散在根目录中，缺乏清晰的模块化组织。随着项目规模增长，这种扁平结构会导致：

1. **代码组织混乱**：缓存相关代码与核心业务逻辑混在一起
2. **可维护性下降**：难以快速定位和修改缓存相关功能
3. **扩展性受限**：添加新的缓存实现或策略时需要修改多个文件
4. **职责不清晰**：缓存层应该作为一个独立的模块存在

通过将缓存相关代码组织到独立的 `cache/` 子模块中，可以：

- 提高代码的可读性和可维护性
- 明确模块边界和职责
- 便于未来扩展新的缓存实现
- 符合项目的模块化架构约定

## What Changes

- **创建 `src/cache/` 子模块**：将所有缓存相关代码组织到独立模块中
- **移动文件**：
  - `src/l2_cache.rs` → `src/cache/l2.rs`
  - `src/l3_cache.rs` → `src/cache/l3.rs`
  - `src/smart_cache.rs` → `src/cache/smart.rs`
- **创建 `src/cache/mod.rs`**：作为缓存模块的入口，重新导出公共 API
- **更新导入路径**：修改所有引用这些模块的文件中的导入语句
- **更新 `src/lib.rs`**：添加 `pub mod cache;` 并重新导出缓存相关的公共类型

**BREAKING CHANGES**: 无（仅内部重构，公共 API 保持不变）

## Impact

- **Affected specs**: `cache`（新增）
- **Affected code**:
  - `src/lib.rs` - 添加 cache 模块声明
  - `src/l3_cache.rs` - 引用 `l2_cache` 的路径需要更新
  - `src/smart_cache.rs` - 引用 `l2_cache` 的路径需要更新
  - 所有测试文件中的导入路径需要更新
  - 示例代码中的导入路径可能需要更新

- **Benefits**:
  - 更清晰的代码组织结构
  - 更好的模块边界和职责分离
  - 更容易添加新的缓存实现
  - 符合项目的模块化架构约定