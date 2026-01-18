# Change: 重构匹配器模块化

## Why

当前 `src/` 目录下的匹配器相关文件（`matchers.rs`、`geo_matcher.rs`、`device_matcher.rs`、`custom_matcher.rs`）分散在根目录中，缺乏清晰的模块化组织。这些文件功能高度相关，都是用于标识符提取和规则匹配的组件。

随着项目规模增长，这种扁平结构会导致：

1. **代码组织混乱**：匹配器相关代码与核心业务逻辑混在一起
2. **可维护性下降**：难以快速定位和修改匹配器相关功能
3. **扩展性受限**：添加新的匹配器类型时需要修改多个文件
4. **职责不清晰**：匹配器层应该作为一个独立的模块存在

通过将匹配器相关代码组织到独立的 `matchers/` 子模块中，可以：

- 提高代码的可读性和可维护性
- 明确模块边界和职责
- 便于未来扩展新的匹配器实现
- 符合项目的模块化架构约定

## What Changes

- **创建 `src/matchers/` 子模块**：将所有匹配器相关代码组织到独立模块中
- **移动文件**：
  - `src/matchers.rs` → `src/matchers/mod.rs`（核心匹配器）
  - `src/geo_matcher.rs` → `src/matchers/geo.rs`（地理位置匹配器）
  - `src/device_matcher.rs` → `src/matchers/device.rs`（设备类型匹配器）
  - `src/custom_matcher.rs` → `src/matchers/custom.rs`（自定义匹配器）
- **更新导入路径**：修改所有引用这些模块的文件中的导入语句
- **更新 `src/lib.rs`**：更新匹配器相关的公共类型导出

**BREAKING CHANGES**: 无（仅内部重构，公共 API 保持不变）

## Impact

- **Affected specs**: `matchers`（新增）
- **Affected code**:
  - `src/lib.rs` - 更新 matchers 模块声明和导出
  - `src/governor.rs` - 引用 matchers 的路径需要更新
  - `src/limiter_manager.rs` - 引用 matchers 的路径需要更新
  - 所有测试文件中的导入路径需要更新
  - 示例代码中的导入路径可能需要更新

- **Benefits**:
  - 更清晰的代码组织结构
  - 更好的模块边界和职责分离
  - 更容易添加新的匹配器实现
  - 符合项目的模块化架构约定