# limiteron-dbnexus

[![Crates.io](https://img.shields.io/crates/v/limiteron.svg)](https://crates.io/crates/limiteron) [![Docs.rs](https://docs.rs/limiteron/badge.svg)](https://docs.rs/limiteron) [![License](https://img.shields.io/crates/l/limiteron.svg)](../../LICENSE)

limiteron 对 [`dbnexus-limiter-port`](https://crates.io/crates/dbnexus-limiter-port) 限流端口的适配器：把 limiteron 的 per-key 令牌桶（`LimiterManager`）装进 dbnexus 的 `Limiter` 端口契约，供 dbnexus 权限检查链路经 `RateLimitBackend::External` 注入。

> 本 crate 不发布 crates.io（集成 crate 与父仓 path 双钉，与 limiteron-sdforge 同款口径），应用经 git 依赖消费。

## 为什么存在

dbnexus 与 limiteron 互为消费方（limiteron optional 依赖 dbnexus 作存储后端），Cargo 禁止包级循环依赖，故限流接口抽为独立小 crate `dbnexus-limiter-port`（2 文件、常规依赖仅 async-trait）。本 crate 在 limiteron 侧实现该端口，是两条产品线的唯一无环接缝；装配发生在应用组合根。

## 语义映射

| dbnexus 侧习惯 | 本适配器 | limiteron 引擎事实 |
| --- | --- | --- |
| `max_requests`（窗口配额） | `amount` | 令牌桶容量（突发上限 = `amount`） |
| `window_secs`（窗口秒数） | `unit_secs` | 稳态速率 `max(1, amount/unit_secs)` 令牌/秒 |
| 按 key 独立配额（如角色 ID） | `check(key)` 每次消费 1 令牌 | `LimiterManager` per-key 实例缓存（上限 100_000，LRU 清理） |

端口不规定算法；dbnexus 内置后端与本适配器同为令牌桶，但注意突发上限与窗口配额在本适配器中相等（`amount`），与固定窗口语义有差。

## 决策与错误语义

- **allow** → `RateLimitDecision::allow()`（`retry_after: None`）。
- **deny** → `RateLimitDecision::deny(...)`，`Retry-After` 由引擎快照 `reset_secs` 推导，亚秒等待向上取整到 1s（与 dbnexus session 层进位规则一致）；快照推导失败时为 `None`（判定不变，不升级为故障）。
- **后端故障** → `Err(RateLimitError)`，消息保留 limiteron 错误链文本；**key 永不写入消息**（防用户标识符泄漏进消费方日志）。
- **fail-open / fail-closed 由消费方决定**：适配器不擅自把故障放行或拒绝，端口契约如此约定。

## 使用

```rust
use std::sync::Arc;
use dbnexus_limiter_port::Limiter;
use limiteron_dbnexus::DbnexusLimiter;

// 组合根装配：应用同时依赖 dbnexus 与本 crate
let limiter: Arc<dyn Limiter> = Arc::new(DbnexusLimiter::new(3, 1)); // 每 key 每秒 3 次
// dbnexus 侧：PermissionContext::with_rate_limit_backend(RateLimitBackend::External(limiter), ...)
```

完整组合根示例：`examples/dbnexus_port_basic.rs`（`cargo run -p limiteron-dbnexus --example dbnexus_port_basic`）。

## 约束与注意

- **所有 key 共用 `(amount, unit_secs)`**：`LimiterManager` 对同 key 异参数有参数一致性断言（panic，消息脱敏）；统一配置从结构上消除该面。per-key 差异化配置暂不支持。经 `with_manager` 共享管理器时须守同款约束：共享实例必须配置一致或 key 空间不相交——引擎断言比对 `(capacity, refill_rate)` 且不校验 `unit_secs`，派生参数恰好相同的不同配置会**静默共享同一桶**（配额合并削弱限流）。
- **key 取有限受信集合**（角色/用户 ID 等）：管理器条目上限 100_000（清理阈值 110_000）带 LRU 清理，**被淘汰 key 的配额随之归零**——高基数 key 洪峰可把活跃 key 挤出缓存以重置其配额（绕窗口手段），基数按清理阈值留余量评估，安全敏感场景调大上限。超 256 字节的 key 经确定性哈希映射为定长标识（防单条目内存无界；哈希碰撞只会合并配额，fail-closed 方向）。
- **禁用 `--all-features`**：workspace 中 dbnexus 的嵌入式与服务端驱动互斥，构建用显式特性清单（CI 口径 `--features full,postgres`）。
- **纯内存 per-key 引擎**：端口契约下本适配器无持久化注入路径（limiteron 的存储后端接入的是 Governor `Storage` 抽象，与 `LimiterManager` 零关联）；持久化/分布式限流不在本 crate 能力范围。
- limiteron 侧依赖仅 `manager` feature。
