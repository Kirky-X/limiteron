# Changelog

本文件记录 limiteron 的破坏性变更与迁移指引。完整版本历史见 [docs/CHANGELOG.md](docs/CHANGELOG.md)。

## Unreleased

### Breaking Changes

- **移除 `inklog` feature 与 `limiteron::integrations::inklog` 模块**：limiteron 不再向
  应用层转发 inklog 结构化日志的初始化入口。该模块整体删除，公开 API
  `init_inklog_logger` / `init_inklog_logger_with_config` / `LimiteronSinkRateLimit`
  一并消失。`full` 预设中的 `inklog` 条目同步移除。

  结构化日志的正主 API 始终在 inklog 侧（`inklog::init_inklog_logger`，
  inklog/src/lib.rs:335）；limiteron 侧保留的只是对 inklog 能力的薄封装，
  移除转发不改变 limiteron 自身 `log::`/`tracing::` 调用的路由语义。

- **`inklog` feature 名称不再存在**：下游 `Cargo.toml` 中的
  `features = ["inklog"]` 会因未知 feature 报错，需删除该条目。需要结构化日志
  的应用改为直接依赖 inklog 并调用其正主 API。

- **`log-redaction` 特性保留**：日志脱敏能力不变，脱敏引擎仍由 inklog 的
  `DataMasker` / `MaskRule` / `MaskRuleRegistry` 承载（规则与优先级的唯一定义处
  仍在 inklog 侧，limiteron 不自建第二套实现）。该依赖是 `log-redaction` 的实现
  细节，不对外暴露为独立 feature。

### Changed

- `full` 预设不再包含 `log-redaction`：掩码能力是对 inklog 掩码引擎的可选借用，不应让 `full` 传递拉入 inklog。需要时显式启用 `log-redaction`。
- 跨仓自研依赖（oxcache / dbnexus / trait-kit / inklog / confers）一律经 crates.io registry 引入：CI 与发布流程只检出本仓，清单内指向仓外兄弟目录的 `path` 依赖会让清单解析直接失败（`failed to load source for dependency`）。本地联调未发布版本时，用工作区外的路径覆盖（`<工作区根>/.cargo/config.toml` 的 `paths` 项）指向兄弟仓工作树，该覆盖文件不入本仓。
- `POST /api/v1/check/batch` 逐条 `error` 字段由原始错误文本改为稳定类别（`validation` / `storage` / `config` / `authorization` / `throttled` / `ban` / `circuit_breaker` / `internal`）：原始错误文本只落服务端日志，不进响应体。**破坏性变更**——按错误文本解析的下游须改按类别判定。

### 迁移指引

- 批量检查端点的逐条错误判定：改按稳定类别判定，不再匹配错误文本。

  ```json
  // 变更前（逐条 error 为原始错误文本）
  { "index": 0, "allowed": false, "decision": "Error", "error": "Storage error: ..." }

  // 变更后（error 为稳定类别，原始文本只落服务端日志）
  { "index": 0, "allowed": false, "decision": "Error", "error": "storage" }
  ```

- 初始化结构化日志：改用 inklog 正主 API。

  ```rust
  // 变更前（limiteron 转发入口，已移除）
  let _manager = limiteron::integrations::inklog::init_inklog_logger().await?;

  // 变更后（inklog 正主 API）
  inklog::init_inklog_logger().await?;
  ```

  自定义配置时对应 `inklog::init_inklog_logger_with_config(config)`
  （inklog/src/lib.rs:367）。注意 inklog 正主入口是进程级单例语义：重复调用
  返回 `Err(InklogError::ConfigError)`。

- 依赖声明：应用层直接声明 inklog 依赖（与 limiteron 内部脱敏引擎同一版本
  下界 `0.3.0-rc.10`，`default-features = false`），并删除 limiteron 的
  `features = ["inklog"]`。

- sink 写入限流：`LimiteronSinkRateLimit`（按 target 分桶 + ERROR 直通 + 写失败
  归还预算）已随模块删除。需要该限流端口的实现方，直接实现 inklog 的
  `SinkRateLimit` 端口（inklog/src/support/io/sink/rate_limit.rs:47）；仅需全局
  令牌桶时可用 inklog 自带的 `TokenBucketRateLimit`
  （inklog/src/support/io/sink/rate_limit.rs:84），不需要限流时用
  `NoOpRateLimit`（inklog/src/support/io/sink/rate_limit.rs:65）。
