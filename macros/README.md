# limiteron-macros

[limiteron](https://github.com/Kirky-X/limiteron) 仓库的过程宏子 crate（目录 `limiteron/macros`）。

## 与父 crate 的关系

| 项 | 内容 |
|---|---|
| 所属仓库 | [limiteron](https://github.com/Kirky-X/limiteron)（[crates.io](https://crates.io/crates/limiteron) · [docs.rs](https://docs.rs/limiteron)） |
| 依赖方向 | `limiteron` → `limiteron-macros`（父 crate 以 `path + version` 双写依赖本 crate） |
| 是否 re-export | 是：父 crate 经 `src/macros.rs` 重导出 `flow_control`，并在 crate 根部再次导出，使用者无需直接依赖本 crate |
| 适用场景 | 通常无需直接依赖；仅在自建宏包装或编译期工具链中直接使用 |

## 提供的属性宏

| 宏 | 作用 |
|---|---|
| `#[flow_control]` | 为函数或方法挂载限流、熔断、配额等流控判定，自动生成配置解析与检查代码 |

## 使用

经父 crate 使用（推荐）：

```rust
use limiteron::flow_control;

#[flow_control(rate = "100/s", quota = "10000/m", concurrency = 50)]
async fn handle_request(user_id: String) {
    // 业务逻辑
}
```

展开代码依赖父 crate 的 `manager` 特性（隐含启用）；`macros` 特性默认关闭，需在父 crate 上显式开启。

## 版本与支持

- 版本经 `version.workspace = true` 随仓库统一，与 `limiteron` 同号发布。
- 仅维护当前 minor 版本线，详见 [limiteron 安全文档 · 支持的版本](https://github.com/Kirky-X/limiteron/blob/main/docs/SECURITY.md#-支持的版本与维护策略)。

## 许可证

MIT
