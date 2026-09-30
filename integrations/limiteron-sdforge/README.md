# limiteron-sdforge

limiteron 侧提供的 sdforge 防护桥接 crate：把 limiteron 的限流 / 熔断 / 封禁能力以最小依赖面暴露给 sdforge 应用。

## 结构

- **核心 guard**（`default`）：`Guard` / `GuardConfig` / `GuardDecision` / `GuardIdentity`——协议无关，不依赖 sdforge。
- **sdforge 适配层**（`sdforge` feature）：`GuardForgeAdapter` 实现 `sdforge::domain::ForgeRateLimiter`。

## 用法

```rust
use limiteron::Governor;
use limiteron_sdforge::{Guard, GuardConfig, GuardIdentity};
use std::sync::Arc;

let governor = Arc::new(Governor::builder()
    .with_config(config)          // 规则/熔断/存储在 Governor 侧装配
    .build().await?);

// 三态判定面（Allowed / Throttled / Banned）
let guard = Guard::with_config(governor, GuardConfig::new(GuardIdentity::UserId));

// sdforge feature 下：适配为 ForgeRateLimiter 注入 sdforge 应用
let limiter = limiteron_sdforge::GuardForgeAdapter::new(guard);
```

## 与 sdforge 侧 `LimiteronForgeAdapter` 的分工

| | sdforge 侧 `integrations/limiteron_adapter.rs` | 本 crate |
|---|---|---|
| 视角 | sdforge 侧消费适配（面向 sdforge 内部 trait-kit 装配） | limiteron 侧独立防护层（面向 limiteron 用户接入 sdforge 应用） |
| 依赖方向 | sdforge → limiteron（optional `limiteron-integration`） | 本 crate → limiteron（本体）+ optional sdforge |
| 发布节奏 | 随 sdforge 发版 | 随 limiteron 发版 |

依赖方向与发布节奏相互独立，故不合并为单侧实现。sdforge 仓侧的分工记录由本仓 `docs/CHANGELOG.md` 代持留痕，sdforge 侧后续发版时应同步其 CHANGELOG。

## 菱形依赖与钉版统一策略

```
limiteron-sdforge ──► sdforge ──► limiteron
        │                                 ▲
        └─────────────────────────────────┘
```

菱形合法：Cargo 对同一 semver 兼容区间合并为单一构建（本 workspace 的 limiteron `0.3.0-rc.6` 满足 sdforge 声明的 `0.3.0-rc.4` 下界）。两侧 optional 钉版统一为同一策略——**钉对方已发布的 rc 版本下界 + `default-features = false`**：

- sdforge 侧（既有）：`limiteron = { version = "0.3.0-rc.4", default-features = false }`
- 本 crate：`sdforge = { version = "0.5.0-rc.5", default-features = false, optional = true }`

rc.5 存在性证据：crates.io 解包验证 `sdforge::domain::{ForgeError, ForgeRateLimiter}` 已存在、`domain` 模块无条件编译、`ForgeError::internal(impl Display)` 构造器可用。

## 语义要点（装配前必读）

- **限流桶为规则级共享**：Governor 规则链 `chain.check()` 无键语义，键的隔离作用面是标识符（封禁检查、事件归因、负缓存键），不是限流桶；per-key 限额须按身份维度拆多条规则实现。
- **Banned/Throttled 在 bool 面不可区分**：`GuardDecision::is_allowed()` 把一切拒绝收敛为 `false`——只消费 bool 的调用方无法感知「限流节流」与「封禁升级」的差异；需要区分时必须对 `GuardDecision` 枚举做模式匹配（`Throttled`/`Banned` 变体，各自携带元数据）。
- **身份维度须对齐**：`GuardIdentity` 决定键写入 `RequestContext` 的字段与 header 通道（`X-User-Id` / `X-API-Key` / `ip`+`client_ip`），Governor 规则匹配器与标识符提取器按对应维度命中，两侧类型不一致时规则不命中或提取失败（显性报错，不静默误判）。
- **故障策略显式**：`fail_open = false`（默认）时 Governor 内部错误收敛为拒绝（fail-close，安全优先）；`true` 时放行并 `tracing::warn!` 留痕。需要原始错误用 `Guard::try_check`。
- **熔断来自 Governor**：经 `GovernorBuilder::with_circuit_breaker`（limiteron `circuit-breaker` feature）装配，本层透传裁决。
- **`record` 为文档化 no-op**：`Governor::check` 原子完成 check + consume，无独立 record 步骤（与 sdforge 侧适配器同口径）。

## License

MIT
