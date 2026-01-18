## 1. 准备工作
- [x] 1.1 备份当前代码状态（git commit 或 stash）
- [x] 1.2 确认所有测试通过（`cargo test --all-features`）
- [x] 1.3 检查是否有其他分支依赖这些文件

## 2. 创建匹配器模块结构
- [x] 2.1 创建 `src/matchers/` 目录（如果不存在）
- [x] 2.2 创建 `src/matchers/mod.rs` 文件（核心匹配器）
- [x] 2.3 在 `src/matchers/mod.rs` 中声明子模块：
  - `pub mod geo;`
  - `pub mod device;`
  - `pub mod custom;`

## 3. 移动匹配器文件
- [x] 3.1 将 `src/matchers.rs` 重命名为 `src/matchers/mod.rs`（保留核心匹配器代码）
- [x] 3.2 移动 `src/geo_matcher.rs` → `src/matchers/geo.rs`
- [x] 3.3 移动 `src/device_matcher.rs` → `src/matchers/device.rs`
- [x] 3.4 移动 `src/custom_matcher.rs` → `src/matchers/custom.rs`

## 4. 更新模块内部导入
- [x] 4.1 更新 `src/matchers/geo.rs` 中的导入路径（如果需要）
- [x] 4.2 更新 `src/matchers/device.rs` 中的导入路径（如果需要）
- [x] 4.3 更新 `src/matchers/custom.rs` 中的导入路径（如果需要）

## 5. 创建匹配器模块公共导出
- [x] 5.1 在 `src/matchers/mod.rs` 中重新导出公共 API：
  - 核心匹配器类型（Identifier, RequestContext, Rule, RuleMatcher 等）
  - `pub use geo::{GeoMatcher, GeoCondition, GeoInfo, GeoCacheStats};`
  - `pub use device::{DeviceMatcher, DeviceCondition, DeviceInfo, DeviceType, DeviceCacheStats};`
  - `pub use custom::{CustomMatcher, CustomMatcherRegistry, HeaderMatcher, TimeWindowMatcher};`

## 6. 更新 lib.rs
- [x] 6.1 更新 `src/lib.rs` 中的 matchers 模块声明（如果需要）
- [x] 6.2 更新 `src/lib.rs` 中的公共导出（如果需要）

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
- [x] 10.3 提交变更：`git commit -m "refactor(matchers): modularize matchers into standalone module"`