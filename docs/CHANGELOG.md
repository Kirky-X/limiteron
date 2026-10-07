# 更新日志

本文件记录本项目的所有重要变更。

格式基于 [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)，
版本号遵循 [语义化版本](https://semver.org/spec/v2.0.0.html)。

## 📋 目录

<details open>
<summary>📑 目录</summary>

- [Unreleased](#unreleased)
- [0.3.0-rc.6](#030-rc6---2026-09-28)
- [0.3.0-rc.5](#030-rc5---2026-09-21)
- [0.3.0-rc.4](#030-rc4---2026-09-14)
- [0.3.0-rc.3](#030-rc3---2026-09-10)
- [0.3.0-rc.2](#030-rc2---2026-09-03)
- [0.2.10](#0210---2026-07-22)
- [0.2.9](#029---2026-07-18)
- [0.2.8](#028---2026-07-17)
- [0.2.7](#027---2026-07-15)
- [0.2.6](#026---2026-07-13)
- [0.2.5](#025---2026-07-13)
- [0.2.4](#024---2026-07-12)
- [0.2.3](#023---2026-07-11)
- [0.2.1](#021---2026-07-06)
- [0.2.0](#020---2026-07-04)
- [0.1.1](#011---2026-01-20)
- [0.1.0](#010---2026-01-18)

</details>

---

## [Unreleased]

### 新增

- **插件系统（`plugins` feature，入 full preset）**：`Plugin` trait（`on_admit`/`on_reject`/`on_degrade` 决策生命周期钩子，默认空实现——插件只订阅自己关心的终态）+ `PluginRegistry` 编译期注册制（`src/plugins/mod.rs`；BTreeMap 保注册顺序分发；同名注册显性报错 `PluginError::AlreadyRegistered` 而非静默覆盖；卸载按名移除返回存在性）。**动态 .so 明确不做并文档说明边界**（模块文档：Rust 无稳定 ABI、`extern` FFI 面不可审计且与 `#![forbid(unsafe_code)]` 立场冲突、运行时加载任意路径 .so 等于把代码执行权交给文件系统写入者；第三方扩展以独立 crate 依赖 limiteron 实现 `Plugin` 并在同一构建注册）。**分发语义**：单个插件的 panic 经 `futures::FutureExt::catch_unwind` 隔离并计数（`panicked_dispatches` 留痕），不中断其余插件、不上抛裁决路径——插件是观测面，不得反向影响限流判定；分发顺序即注册顺序。内置两个示例插件：`LoggingPlugin`（log 门面记录三类终态）与 `CounterPlugin`（无锁原子计数自省面）。单测 7 项（注册/触发/卸载全生命周期、同名拒绝、分发顺序、panic 隔离计数、空表 no-op、get 查询）+ doctest；feature 决策：入 full preset（运行时功能面，非文档/工具面）
- **只读管理 Web UI（`admin-ui` feature）**：内嵌单页（无构建链，vanilla HTML+JS，fetch `/snapshot` 与 `/circuit-breaker` 渲染聚合统计/组件健康/规则与决策链/熔断状态，5s 自动刷新）+ `WebUiServer`（默认绑定 `127.0.0.1:9091` 可配，非回环绑定发出无认证暴露 warn 日志并要求前置反向代理——README 安全节与 API_REFERENCE 显著警示）。**机械只读守卫三层**：①路由表 `ROUTE_TABLE` 为路径清单唯一事实来源，Router 表驱动注册且仅经 `get()`；②守卫测试对表内全路径 GET 探测（漏注册红灯）+ 全路径 POST/PUT/DELETE 断言 405（方法级守卫）；③数据 handler 仅接受 `Arc<dyn ReadOnlySnapshotSource>`——`AdminService` 的只读投影 trait（async_trait，blanket 委托 status/introspect/circuit-breaker），类型面上不存在写方法。feature 决策：默认关、不入 full preset（UI 面与 openapi/admin-client 同口径）
- **自适应阈值限流器（`adaptive-threshold` feature，默认关）**：滑动观测窗口（样本数驱动，无墙钟依赖、无时钟回拨面）内的错误率与 p95 延迟驱动动态配额——错误率升高或延迟劣化时乘性收紧（`decrease_ratio`，钳制 `min_limit`），指标恢复后加性放宽（`increase_step`，钳制 `max_limit`），下调信号为「或」（错误率 ≥ 阈值 **或** p95 ≥ 阈值），上调信号为「且」（错误率 ≤ 阈值 **且** p95 ≤ 阈值）；**统计启发式非 ML（显式决策）**——全部阈值/步长/冷却期为显式配置项，判定是确定性条件语句；冷却期（`cooldown_samples` 个样本内禁止再调整）防阈值附近振荡，冷启动期（样本数 < `min_samples_for_adjust`）不抖动；配置构造期显式校验（min≤base≤max、ratio∈(0,1)、上下阈值序）fail-loud；实现 `Limiter` trait（`allow` 对动态配额原子消费判定、`peek`/`remaining` 标准限流头快照），反馈经 `report(Feedback)` 显式上报（错误/延迟语义由调用方定义，限流拒绝不是错误信号）；与 AIMD `AdaptiveConcurrencyLimiter`（并发租约窗口）的分工：本限流器为配额型，适配出口配额随下游健康度伸缩与服务端按错误率自降载两类场景。单测 9 项（下调至下界钳制/上调至上界钳制/冷却期抑制再升降/延迟信号独立触发/冷启动跳过/配置校验/快照面）。feature 决策：独立默认关，刻意不入任何 preset（含 full）——与 capacity-dial 同口径，反馈接入点由消费方装配
- **Admin API OpenAPI 化（`openapi` feature + AdminService trait 抽取）**：`handlers` 的管理操作业务逻辑迁入协议无关的 `AdminService` trait（`src/admin/service.rs`，`GovernorAdminService` 实现包装 `LimiteronState` 组件引用），axum handlers 收敛为薄壳（解析 → service 调用 → `AdminServiceError`→状态码唯一映射：NotConfigured→503 / Invalid→400 / NotFound→404 / Forbidden→403 / Internal→500），`AdminServiceError` 携带上下文消息保持既有响应契约逐位一致（97 项既有 admin 测试回归绿）；`openapi` feature（蕴含 admin-api）提供手写 OpenAPI 3.0.3 构建器（`src/admin/openapi.rs`，12 条路由与 `create_router` 一一对应、内部 `$ref` 完整性自检），产物 `docs/openapi.json` 入库并由 `tests/openapi_drift.rs` 防漂移守卫（`UPDATE_OPENAPI=1` 重写、diff 须人工复核）；schema 手写不引 schemars（Admin serde 面小而稳定，防漂移测试兜底演进）；`admin-client` feature（hyper 1.x http1 client + hyper-util tokio 适配，均为传递依赖零新增重量，default-features=false 零 TLS 面）提供 `AdminClient` 薄客户端——与 AdminService 契约一一对应的类型化方法（status/introspect/cb_status/create_ban/delete_ban/reset_quota/apply_config/check_batch/healthz），**临时验证面**：sdforge-R11 SDK 生成器落地后接替（演进注记入 client.rs 模块文档）；per-request 新建连接（管理面低频）、路径段百分号编码防穿越、超时显性（默认 10s）；feature preset 决策：openapi/admin-client 均为文档/消费端工具面，不入 full preset（与 cli/test-clock 同口径）
- **sdforge 防护桥接 crate（新 workspace 成员 `integrations/limiteron-sdforge`）**：limiteron 侧提供的独立防护层——核心 `Guard`（协议无关，包装 `Arc<Governor>` 暴露三态 `GuardDecision`：Allowed / Throttled（原因+重试等待秒数）/ Banned（原因+到期时刻+封禁次数））与 `GuardConfig`（身份维度 `GuardIdentity`：Ip/UserId/ApiKey，键同步写入 `RequestContext` 字段与默认 `CompositeExtractor` 的 header 通道 `X-User-Id`/`X-API-Key`/`client_ip`，覆盖规则匹配与标识符提取两条路径；故障策略 `fail_open` 显式可配，默认 fail-close 拒绝，`true` 时放行并 `tracing::warn!` 留痕）+ `try_check` 显式错误面；`sdforge` feature（optional `sdforge = "0.5.0-rc.5"`，default-features=false）下 `GuardForgeAdapter` 实现 `ForgeRateLimiter`，`record` 为文档化 no-op（`Governor::check` 原子完成 check+consume）。**与 sdforge 侧 `integrations/limiteron_adapter.rs` 的分工**（本条目代持两仓记录义务，sdforge 侧发版时同步其 CHANGELOG）：后者是 sdforge 侧消费适配（面向 sdforge 内部 trait-kit 装配，随 sdforge 发版），本 crate 是 limiteron 侧防护层（面向 limiteron 用户接入 sdforge 应用，随 limiteron 发版），依赖方向与发布节奏独立故不合并。**菱形依赖钉版统一策略**（limiteron-sdforge→sdforge→limiteron 合法，Cargo 合并 semver 兼容区间为单一构建）：两侧 optional 钉版统一为「钉对方已发布 rc 版本下界 + default-features=false」——sdforge 侧既有 `limiteron = "0.3.0-rc.4"`，本 crate `sdforge = "0.5.0-rc.5"`（rc.5 经 crates.io 解包验证 `domain::{ForgeError, ForgeRateLimiter}` 存在、`domain` 模块无条件编译、`ForgeError::internal` 可用）。feature 面决策：`default=[]` 仅核心 guard（不依赖 sdforge）、`sdforge` 开适配层、`full=["sdforge"]`（本 crate 全功能面）。单测 11 项（三态映射/身份通道/预置封禁命中/fail-open/close/`dyn` object-safety），两个示例（`guard_basic` 无 sdforge 可跑、`sdforge_guard` 需 feature）
- **dbnexus 限流端口适配器 crate（新 workspace 成员 `integrations/limiteron-dbnexus`）**：limiteron 侧实现 dbnexus 0.6.0-rc.6 抽出的独立限流端口 `dbnexus-limiter-port`（trait `Limiter`：`check(key) → Result<RateLimitDecision, RateLimitError>`，2 文件、常规依赖仅 async-trait、dbnexus 仓契约测试固化与双方零依赖）——经 `LimiterManager`（feature `manager`）per-key 令牌桶实例缓存适配：`check(key)` 每次消费 1 令牌（引擎 `TokenBucketLimiter::allow(1)`，不经 Governor 决策链），deny 携带 `Retry-After`（引擎快照 `reset_secs` 亚秒向上取整到 1s，与 dbnexus session 层进位规则一致；快照推导失败时无建议、判定不变），后端故障经 `RateLimitError` 显性上报且 key 永不写入消息（防标识符泄漏进消费方日志）；fail-open/closed 由消费方决定（端口契约），适配器不擅自放行或拒绝。**key 归一化与基数约束**：超 256 字节 key 确定性哈希为定长标识（防单条目内存无界，碰撞只合并配额属 fail-closed）；LRU 淘汰即配额重置（高基数洪峰可绕窗口）已在文档显性警示，key 须取有限受信集合。**与 sdforge 桥接 crate 的分工**（本条目代持记录义务，随 limiteron 发版）：`Guard` 面向完整决策链（含封禁/熔断，桶规则级共享），本 crate 面向 per-key 纯限流判定（dbnexus 权限链路按角色独立配额的语义）；两个集成 crate 均不进 crates.io 发布清单（release.yml publish-crates 范围不变），应用经 git 依赖消费。**依赖钉版**：父仓 path+version 双钉 `0.3.0-rc.6`（default-features=false、仅 `manager` feature）；端口 crate 钉 crates.io `dbnexus-limiter-port = "0.6.0-rc.6"` 下界（已发布）。**统一配置防 panic**：所有 key 共用构造期 `(amount, unit_secs)`，结构性规避 `get_rate_limiter` 同 key 异参断言。feature 决策：`default=[]`、`full=[]` 预留位（workspace CI 显式特性清单口径）。单测 10 项（配额内放行/耗尽拒绝+Retry-After 下界/per-key 隔离/`Arc<dyn Limiter>` 对象安全/同 key 重复获取不触发参数断言/`map_retry_after` 亚秒进位与整秒透传/`map_err` 保留错误链/长 key 哈希映射确定性与定长上界/长 key 配额语义）+ 组合根装配示例 `dbnexus_port_basic`
- **SQLite 嵌入式存储后端（`sqlite`）**：dbnexus 嵌入式驱动的本地后端全链路可用——五张表（KV/封禁/配额/速率/事件 outbox）新增 sqlite 方言建表 DDL（`create_all_tables_ddl_sqlite`，随 `sqlite` feature 公开导出）；`StorageFactory::create_schema` 按连接后端自动选择建表方言（PostgreSQL/SQLite，MySQL 无 `BIGSERIAL`/`NOW()` 不受支持，显性报错而非产出错误 SQL）；封禁与配额适配器的原子 SQL 路径（`RETURNING`/UPSERT/GREATEST vs 双参 `MAX`、`$N` vs `?N` 占位符）按后端方言分派，非支持后端显性拒绝；修复 `QuotaStorage for Arc<S>` blanket 委托缺失 `refund` 导致经 `Arc` 的退款静默落入默认 no-op；内存库（`sqlite::memory:`）单测覆盖 KV/封禁/配额/工厂全链路并与 Redis 形后端（cache-redis 适配器 + 内存传输层）行为对拍，沙箱无 Docker 可跑
- **Lua 脚本增强（`lua-script`）**：新增滑动窗口日志脚本（cost 加权精确滑窗，每请求成本可变，对应进程内 `SlidingWindowLogLimiter` 语义，与每请求计 1 的既有滑窗脚本区分）与带突发透支令牌桶脚本（余额不足时在透支额度内借债放行、负余额记账未来偿还，与严格余额的既有令牌桶脚本区分）；`LuaScriptType` 扩至七变体并全部注册进 `OxcacheLuaManager`
- **自定义匹配器扩展**：`HeaderMatcher` 新增前缀/相等双模式（`HeaderMatchMode`，默认精确相等向后兼容，`load_config` 支持 `match_mode` 字段）；新增 `MethodMatcher`（方法名大小写规范化、builder、load_config）；新增 `RegexPathMatcher`（feature `regex-matching`，模式构造期编译显性报错、大小写内联旗标）；三者均实现 `CustomMatcher` trait 并经 `CustomMatcherRegistry` 注册/注销/match_with；lib.rs 平铺导出（RegexPathMatcher 随 feature 门控）；`custom_matchers` 示例补三段演示
- **LeakyBucketLimiter 漏桶限流器**：任意速率入桶、恒定速率漏出的水位计型（meter）限流，判定语义与令牌桶严格对偶（水位 = 容量 − 令牌），无请求排队/延迟放行；漏出积分守恒（亚单位时间滞留累计，与令牌桶补充对称）、漏空时积压积分丢弃、`peek`/`remaining` 纯读虚拟推演；状态为 Mutex 单点串行（高争用场景选无锁 TokenBucketLimiter）；`LimiterConfig::LeakyBucket` 配置变体接入工厂/决策链/自省
- **SlidingWindowLogLimiter 滑动窗口日志限流器**：逐条记录窗口内放行（时间戳 + 成本队列）的精确滑动窗口，无固定窗口边界突刺；过期条目在 allow 路径惰性逐出（摊还 O(1)，空闲期已过期条目驻留队列、队空时收缩缓冲），内存随窗口内请求数线性增长（16 字节/条目，配置上界 `MAX_SLIDING_LOG_REQUESTS`=100K 条 ≈1.6MB/实例，计数器型大配额窗口优先分片滑动窗口）；判定/读路径经单调确认游标摊还 O(1)，拒绝热路径无重复扫描；`LimiterConfig::SlidingWindowLog` 配置变体接入工厂/决策链/自省
- **Governor shutdown 完整实现**：五阶段优雅关闭编排——停止配置热重载 watcher（`register_config_watcher_token` 注册、多 watcher 全取消）、取消 shutdown 令牌、统计快照落盘（`GovernorBuilder::with_shutdown_snapshot_dir` 显式启用，JSON 原子写，临时文件 O_EXCL 独占创建 + 0600 权限防符号链接覆写，冲突退避 pid 后缀）、停止 BanManager 自动解封任务（取消信号优雅排空 5s，超时 abort）、审计日志器摘除（尽力 `Arc::try_unwrap` 优雅排空）
- **`impl Drop for Governor`**：同步兜底无条件取消 shutdown 与已注册 watcher 令牌（幂等）
- **`BanManager::is_auto_unban_running()`**：自动解封后台任务运行状态观测

### 新增

- **监控维度指标与关键路径追踪（`monitoring`/`telemetry`，默认开启可关闭）**：`Metrics` 新增 per-rule 检查计数（`flowguard_rule_checks_total`，标签 rule/outcome）、规则内限流器拒绝计数（`flowguard_rule_limiter_rejections_total`，标签 rule/limiter，Governor 持快照增量导出保持 Counter 单调——导出版本号下沉为链内原子量，放行稳态零锁零分配跳过，拒绝路径快照与基线落账同临界区串行化，基线只随真实新增前进）与降级检查计数（`flowguard_degraded_checks_total`）；负缓存命中的拒绝经缓存内裁决规则 ID 计入 per-rule 序列（缓存条目新增 rule 字段），无归因命中退 `flowguard_negative_cache_hits_total` 对账（opt-out 关闭维度序列时全部命中退此保底；规则被热更新移除后其存量缓存条目在 TTL 内仍按已删 rule_id 递增 per-rule 序列，惰性创建短暂抬高基数，条目到期自愈）；`Governor::builder().with_per_rule_metrics(bool)` 可关闭维度序列（标签基数随规则/限流器数量增长，超大规模配置降级为仅全局计数）；`telemetry` feature 下 `governor_check` span 记录 rule.outcome 属性，拒绝路径（链上与负缓存命中）另携带归因的 rule.id（全放行无单一裁决规则、缓存条目无归因时不虚构，后者仅记 outcome 保留结果信号）；主检查存储类错误触发降级时递增降级计数（配置类错误不计，避免未认证洪水刷高存储健康度告警）

### 修复

- **默认命名空间封禁静默失效修复**：`ban_identifier_for_namespace` 此前无条件加租户前缀，而读侧（`is_identifier_banned` 与热路径键改写）在默认命名空间（global/development，如 `DefaultTenantResolver` 解析结果）下只查无前缀键——该路径写入的封禁在任何查询路径都不可见。现改为：默认命名空间写无前缀键（全局封禁）——强制点为默认命名空间/无 resolver 流量的请求热路径，非默认命名空间流量经 `is_identifier_banned` 的回退查询可见；非默认命名空间写限定键的行为不变。本方法仍是低级写入口（直写封禁存储，不经 `BanManager` 的授权检查、封禁历史累加、退避时长与格式校验链），默认命名空间与 `ban_identifier` 仅键范围一致、记录语义不同。存量说明：此前经该路径写入的限定键记录从未生效，升级后可按前缀 `tenant:global:env:development:` 清理，无生效语义需要迁移
- **bench harness 失效修复**：criterion 0.8 要求 `[[bench]] harness = false`，四个 bench 目标缺失该设置导致 `cargo bench` 落入 libtest harness 空跑（"running 0 tests"），性能基线此前从未真实产出；补齐后 criterion 正常接管并建立首份基线（reviews/perf-baseline.md）

### 变更

- **`Governor::decision_key` 默认命名空间返回值变更（公开 API）**：配置 resolver 且解析为默认命名空间时，返回值由 `tenant:global:env:development:{key}` 改为 `{key}`，与请求热路径的键改写及内部负缓存/事件/封禁键逐位一致；自建缓存键或预热数据的调用方需自查旧前缀格式
- **性能基线建立与匹配器热点削减**：基线口径与环境见 reviews/perf-baseline.md；`HeaderMatcher` 大小写不敏感路径新增 ASCII 零分配快路径（`values_ascii` 逐值门控，非 ASCII 值回退 `to_lowercase` 原路径，语义严格等价），bench 三轮采样中位数 12 项全部改善（-10.0% ~ -33.4%，原热点 header_prefix_miss/100 193.60→128.89ns）；`MethodMatcher` 匹配改 `eq_ignore_ascii_case` 零分配折叠
- **HeaderMatcher 配置解析与校验收紧（行为变更）**：`load_config` 的 allowed_values 非字符串项从静默丢弃改为显性报错（与 MethodMatcher 政策对齐）；空 allowed_values（`new`/builder/`load_config` 全空数组）从静默永不命中改为显性拒绝（`matcher-header-values-empty`）——已部署空列表配置升级后加载被拒

### 变更

- **存量限流类型配置校验收紧（升级注意）**：`LimiterConfig::validate`（配置加载生效校验点）新增上限——TokenBucket/LeakyBucket 容量 ≤10M、补充/漏出速率 ≤1M/s，SlidingWindow/FixedWindow max_requests ≤10M，SlidingWindowLog max_requests ≤100K（日志型独立更严上界），Concurrency ≤100K；超限配置此前可加载（仅工厂层校验未接入生产路径），升级后将在配置加载/规则构建期被拒绝（fail-closed）。迁移检查：升级前扫描现有 YAML/TOML 配置中各限流器数值是否超限，超限项按业务真实需求下调或反馈 issue 评估上限调整
- `BanManager::stop_auto_unban_task()` 由直接 abort 改为先取消信号等待在途清理完成（5s 超时兜底 abort）
- 存储连接释放语义文档化：连接池随最后 `Arc` 引用释放由底层驱动关闭，shutdown 负责停止后台任务不再发起新访问
- `LimiterFactory::create_with_redis`（`distributed` + `lua-script`）对无分布式脚本的类型（LeakyBucket/SlidingWindowLog/Concurrency 等）降级为进程内限流时输出 `tracing::warn` 带配置详情——多实例部署下全局放行量 = 配置值 × 实例数，降级不再静默
- **`monitoring` 特性更名为 `prometheus`（兼容别名保留，无破坏）**：该特性实为 Prometheus 专属（`dep:prometheus` 门控），按「特性名即能力」命名惯例取依赖实名；91 处 `cfg(feature = "monitoring")` 门控同步迁移（src 90 + tests/e2e 1），examples 特性镜像与双语 README / docs 特性表同步更新，`start_prometheus_server` 禁用态错误消息同步更名；旧名 `monitoring = ["prometheus"]` 保留为兼容别名，既有消费者与文档旧引用不受影响；`metrics` 聚合特性与 `full` preset 改引主名

## [0.3.0-rc.6] - 2026-09-28

### 新增

- **ManualCircuitBreaker 手动记录式熔断**：比例触发 + 半开探测 + 时钟注入；`ManualCircuitBreaker` / `ManualCircuitBreakerConfig` 公开导出并进 prelude
- **capacity-dial 六档容量调光模块**：独立默认关 feature
- **tower-middleware 增强**：拒绝响应工厂与 keyed 静态限流器直驱快速路径
- **RetryPolicy 重试前回调**：`execute_notify` 钩子
- **QuotaLimiter 超限窗口查询**：`check_retry_after` 返回剩余秒数
- **manager feature 拆分**：新增 `try_get_quota_limiter` 非 panic 路径
- **文档一致性门禁脚本**：接入 CI doc job，检出文档漂移

### 修复

- **【安全】CodeQL 明文日志四项收敛**：governor 日志脱敏（6 处标识符）与长度指纹结构性消除 cleartext-logging 污点；check 入口不再记录 user_id/ip 派生字段；移除 check_internal 入口 debug 日志
- **limiters 算法层**正确性与健壮性修复
- **quota 配额语义与账本架构归一**
- **distributed**：Lua 脚本接线与分布式传输健壮性
- **governor**：换装原子性/降级路径/XFF 可信代理/审计与孤岛落地
- **resilience**：熔断恢复通道/AIMD 信号/重试风暴防护
- try_get_quota_limiter 慢路径参数一致性校验、tower-middleware keyed 快速路径键卫生、capacity-dial 构造期校验与 quota 边界去时序依赖、默认 feature 下测试编译与空链越界门控修复

### 变更

- **封禁时长口径修正**：四档阶梯封顶（非指数退避）
- CI：文档门禁接入 doc job、codeql-action 升 4.38.0、codecov-action 升 7.1.0、patch 组依赖刷新

## [0.3.0-rc.5] - 2026-09-21

### 新增

- **重试组件**：`retry` feature——`RetryPolicy`（指数退避，`factor`/`max_delay`/`jitter` 可配）+ `delay_for_attempt` 单次延迟计算

### 变更

- **i18n 整改**：迁移 unify-rust-i18n——生产路径中文字面量清零、日志/错误消息键接线（T016/T025）
- 版权头统一改写为 2025-2026

### 修复

- **feature 门控与 preset 解耦**：`standard`/`full` preset 去除钉死的 sqlite/postgres 驱动（改为按需叠加）；`ban-sync` 补蕴含 `event-system`（单开不再静默无效）；`validation` 改模块级门控（修复单开 E0432）；`cache-storage` 正名 `cache-redis`（保留向后兼容别名）；删除死 feature `legacy_tests`/`chaos-testing`/`config-security`；`adaptive-limiting` 文档纠偏（rc4 起为真实实现）；新增 docs.rs 元数据（`full` + postgres 单后端）；修复 3 处 intra-doc 断链；删除 governor 不可达 panic 分支
- 测试确定性修复

### 依赖

- 版本递增至 `0.3.0-rc.5`
- 跨仓 path 依赖改走 crates.io；path-only 依赖补全 `version` 字段

## [0.3.0-rc.4] - 2026-09-14

### 修复

- **OCR 审查系列**：
  - **HTB/inklog Sink**：令牌扣减原子化——单临界区内完成刷新+扣减，借用不足整体回滚，消除并发透支
  - **限流器并发**：自适应/并发/固定窗口/批量预取——release 饱和扣减、注入信号量容量观测、溢出比较、预取校验失败留痕
  - **事件链路**：webhook 共享 `Client` 与常量头名、outbox 事务语义文档纠偏与损坏 payload 告警、ban-sync 解码失败留痕
  - **存储与审计**：审计链批写移入阻塞线程池；ban 存储改原子自增 UPDATE 并显性限定 Postgres 方言；存储契约补 TTL 过期断言
  - **配额控制器**：超限比较去溢出、告警百分比钳制 255、克隆句柄计数防误取消后台任务
  - **telemetry/存储/CLI**：epoch 时间去 panic、metrics 并发占位原子化、CLI 序列化失败显性报错、`Debug`/`PartialEq` 派生补全、OTLP 测试 Content-Length 下溢防护
  - **Admin API**：`into_router` 配置校验、status 饱和运算、config `PartialEq`

### 变更

- 死代码清理 15 处；注释卫生清理（52 个文件移除任务追踪 ID 引用 + 复审补漏悬空导航）
- **文档收敛**：中英双语 README 重写（`Governor::check` 级联决策时序图、21 个示例全量表，舍弃无法核实的覆盖率/性能表述）；docs 套件统一优化（折叠目录、交叉链接网络、架构 mermaid 图）；重复内容收敛至单一 canonical 位置；README 目录统一为 17 章节标准骨架
- CI 门禁收敛：rustdoc 私有项与断链修复

### 依赖

- 版本递增至 `0.3.0-rc.4`
- 移除 `[patch.crates-io]`，跨仓依赖改用 crates.io 上游 rc.4：trait-kit → `0.5.0-rc.4`、oxcache → `0.5.0-rc.4`、dbnexus → `0.6.0-rc.4`、inklog → `0.3.0-rc.4`、confers → `0.6.0-rc.4`

## [0.3.0-rc.3] - 2026-09-10

> 注：0.3.0-rc.3 版本号已跳过、未发布（无 tag、未上 crates.io），本节内容随 0.3.0-rc.4 一并发布。

### 新增

- **K8s 探针与指标端点**：admin API `/healthz` `/readyz` `/metrics`（bypass 认证，T601）
- **多租户贯穿 Governor**：tenant+key 复合决策键，存储/配额/封禁按租户隔离（T602）
- **限流预检**：`Limiter::peek(cost)`/`remaining()` 非消费查询 + IETF 标准 `RateLimit-*` 头数据（T603）
- **CIDR 网段封禁**：IPv4/IPv6 CIDR 段匹配 + 最长前缀匹配（T604）
- **Admin RBAC**：多 key 令牌认证 + admin/viewer 角色矩阵（越权 403，T605）
- **OTLP 追踪导出**：`otlp` feature 的 OTLP/HTTP exporter 替代 stub（mock collector 单测，T606）
- **MySQL 存储后端**：`mysql` feature（dbnexus server-side 驱动，契约测试共享，T607）
- **Governor 自省 API**：`GET /api/v1/introspect` 规则/决策链/配额/封禁/熔断 JSON（T608）
- **审计哈希链**：审计事件 HMAC-SHA256(prev‖payload) 链式签名 + `verify_chain` 篡改检测（T609）
- **AIMD 自适应并发限流**：`adaptive-limiting` feature 真实实现（延迟/错误率反馈调窗，熔断联动，T610）
- **舱壁隔离**：`bulkhead` feature——按资源组分池 + 独立并发预算/熔断/隔离指标（T611）
- **配置 CLI**：`limiteron-cli` 二进制（`cli` feature）——规则文件校验/导出/apply dry-run，JSON 输出 + 退出码 0/1/2（T612）
- **热更新/批量 API**：`POST /api/v1/config` 原子换配置（同步重建规则匹配器与决策链 + L1 失效 + 历史记录）；`POST /api/v1/check/batch` N key 一次决策；`POST /api/v1/tokens/prefetch` 批量令牌预取（`BatchTokenPrefetcher`，T613）
- **Webhook 签名**：外发事件 HMAC-SHA256 签名头（`X-Limiteron-Signature`）+ 时间戳防重放（`X-Limiteron-Timestamp`，默认 300s 窗口；`webhook` feature，T614）
- **HTB 分层令牌桶**：`HierarchicalTokenBucket` 父/子借用（全有或全无、兄弟隔离，T615）
- **宏 throttle 排队**：`on_exceed = "throttle"` 真实实现——有界等待重试（`queue_ms`/`poll_ms` 可配），超时返回 `LimiteronError::Throttled`（T615）
- **事件 Outbox**：`limiteron_event_outbox` 表（Transactional Outbox，append/pending/mark_published，sqlite 本地测试，T616）
- **封禁跨实例同步**：`ban-sync` feature 经 oxcache Pub/Sub 协议层广播封禁变更（自消息豁免，mock 单测，T616）
- **下层端口实现与 kit 对齐**：inklog `SinkRateLimit` 真实实现（按 target 分桶 + ERROR 直通 + 失败归还）；dbnexus `QueryThrottle` 端口语义文档化 + `LimiteronQueryThrottle` 实现；`LimiteronModule` 补 trait-kit `AsyncHealthCheck`/`AsyncLifecycle` 端口并依赖 `OxcacheModule` 注入缓存（dbnexus T413 范式；T617）

### 变更

- `kit` feature 扩展：`dep:trait-kit` + `dep:futures` + `oxcache/kit`；trait-kit 依赖补 `health`/`lifecycle` features（T617）
- `integrations` 模块改为恒编译（子模块保持各自 feature 门控；`query_throttle` 无外部依赖）（T617）
- `LimiteronError` 新增 `Throttled` 变体（宏 throttle 队列超时，T615）

### 新增

- **Redis 分布式限流器** (`RedisDistributedLimiter`)：实现 `DistributedLimiter` trait，5 个 Lua 脚本经 oxcache `eval_lua` 执行，需 `distributed` + `lua-script` 双 feature（T050）
- **confers 配置集成**：新增 `config-confers` / `config-confers-reload` feature，支持从文件加载 `FlowControlConfig` + FsWatcher 热重载 + 验证失败自动回滚（T054）
- **Governor `config_handle()`**：返回 `Arc<RwLock<FlowControlConfig>>` 供外部原子换配置
- **inklog 可配置初始化**：`init_inklog_logger_with_config(InklogConfig)` 支持自定义级别/输出（T053）

### 修复

- **metrics 死接线**：`metrics` feature 隐含 `monitoring`，governor allow/reject/ban 三点指标记录自动激活（T052）
- **sqlite 门控修复**：adapters 与 dbnexus_entities 的 cfg 门控改为 `any(postgres, sqlite)`（T051）

### 依赖

- 版本递增至 `0.3.0-rc.3`
- trait-kit → `0.5.0-rc.3`、oxcache → `0.5.0-rc.4`、dbnexus → `0.6.0-rc.3`、inklog → `0.3.0-rc.3`、confers → `0.6.0-rc.3`
- 新增 `[patch.crates-io]` 本地路径联调

## [0.3.0-rc.2] - 2026-09-03

### 文档

- 同步 README/USER_GUIDE/FAQ/API_REFERENCE 等文档中的版本号 0.2 → 0.3.0-rc.2
- 同步 MSRV 文档描述 1.75+ → 1.85+（与 `Cargo.toml` `rust-version = "1.85"` 对齐）
- 同步 README_EN.md 的 `redis-storage` 特性 / `RedisStorage` 用法示例：v0.2.1 已移除，改用 oxcache 统一管理
- 同步 README_EN.md 路线图：`v0.2.1 Planned` → `Shipped`、`v0.3.0 Planned` → `v0.3.0-rc.2 (Current)`

### 变更

- 依赖路径本地化：`oxcache` / `dbnexus` / `trait-kit` / `inklog` 在 `[workspace.dependencies]` 改用 `path` + `version` 双写，供本地联调与 CI 发布模式共用
- `Cargo.lock` 加入版本控制（库 crate 现在生成并提交）
- 版本号递增至 `0.3.0-rc.2`（下一个 minor 预发布）

## [0.2.10] - 2026-07-22

### 测试

- 新增 `tests/e2e_advanced.rs`（76 个测试）：覆盖 limiter_boundary(12)、gcra_limiter(9)、concurrency_limiter(10)、decision_chain(9)、fallback_strategy(9)、multi_tenant(10)、distributed_limiter(14)、cross_module(4) 共 8 个模块的边界与异常场景

### 维护

- 移除未使用依赖：opentelemetry/opentelemetry-jaeger/opentelemetry_sdk/tracing-opentelemetry（死依赖）、sqlx、tokio-stream、tower-http、validator、proc-macro2/syn/quote（主包）
- 更新 sea-orm 到 2.0 稳定版

## [0.2.9] - 2026-07-18

### 新增

- `limiteron-macros` 版本同步：0.2.7 → 0.2.9（与 workspace 主 crate 版本一致，便于发布）

### 新增

- **[T006]** `#[flow_control]` 宏 `on_exceed` 参数实现：`reject`（默认）超限返回错误，`log_only` 超限继续执行，`throttle` 生成 `compile_error!`（`LimiteronError::Throttled` 变体不存在）。parse 阶段拒绝未知 `on_exceed` 值（Rule 12）
- **[T007]** `#[flow_control]` 宏新增 `key_prefix = "namespace"` 参数，用于多模块同名函数的 key 隔离
- **[T008]** `#[flow_control]` 宏新增 `tracing = false` / `metrics = false` 参数，可独立禁用 span 和 metrics 记录
- `LimiterManager` 全局单例（`GLOBAL_LIMITER_MANAGER`）：按 key 缓存 rate/quota/concurrency 限流器，供 `#[flow_control]` 宏生成的代码使用

### 修复

- 宏生成代码 bug 1：`rate="100/m"` 的 unit 信息丢失（hardcoded unit_secs=1 导致被当作 100/s 处理）
- 宏生成代码 bug 2：`quota_check` 使用 `allow(1)` 不消费配额（改为 `check(&key)` 调用 `check_and_consume`）
- 宏生成代码 bug 3：`concurrency_check` 的 permit 在 match 作用域结束即 drop（改为持有到函数结束）
- 移除未实现的 `get_limiter_status` admin 端点（原返回 501 Not Implemented，无文档承诺，无代码依赖）
- 移除 `test_decision_chain_add_remove_node_disabled` 空占位测试（`remove_node` 未实现且无文档/代码引用）
- 移除 `test_decision_chain_set_short_circuit` 上过时的 TODO 注释和 `legacy_tests` 门控（short_circuit 行为已实现且测试通过）
- **二次收敛**：移除 `tests/on_exceed_modes_test.rs` 中 5 个 `assert!(true)` 占位文档测试（违反 Rule 9：测试必须验证有意义的属性）
- **二次收敛**：移除 `tests/modules/custom_limiter/` 目录（引用的 `CustomLimiterRegistry`/`LimiterStats` 类型在 src/ 中完全不存在）
- **二次收敛**：移除 `tests/modules/l1_cache/` 目录（`integration.rs` 使用过时的同步 API，与当前异步 API 不匹配；src/l1_cache.rs 已有 42 个单元测试覆盖）
- **二次收敛 bug 修复**：原占位 `mod.rs` 未声明 `pub mod integration;`，导致 `fallback`/`telemetry` 目录下的真实集成测试从未被编译运行（Rule 12 违规：死代码隐藏失败）。修复 `fallback/mod.rs` 和 `telemetry/mod.rs` 为正确声明
- **二次收敛**：修复 `tests/modules/fallback/integration.rs` 中 `ComponentType::Storage`（不存在）→ `ComponentType::Redis` + 修复 `test_fallback_config_builder` 错误断言（`Default::default()` 设置 `enabled=true`，原断言 `!config.enabled` 错误）

### 新增

- **[T001]** `#[flow_control]` 宏抽 `build_exceed_handler` 辅助函数，合并 3 个重复的 exceed handler match 表达式（架构-H2 DRY）
- **[T002]** `#[flow_control]` 宏新增 `sanitize_key_component` 辅助函数（ASCII alphanumeric + `_` `-` `.` + take 128），防御性过滤 `key_prefix` 和 `fname` 中的特殊字符（安全-M1，防止 key 注入和 Unicode 同形字符攻击）
- **[T003]** `#[flow_control]` 宏 `key_prefix=None` 时无前导冒号修复（架构-M5 breaking change 恢复）
- **[T005]** `#[flow_control]` 宏 `log_only` 模式下不调用 `allow`/`check`/`acquire`，不消费配额/速率/并发（安全-M2 语义修复）
- **[T006]** `#[flow_control]` 宏条件生成 `#[allow(unreachable_code)]` attr（仅 reject 模式生成，架构-L1）
- **[T008]** `LimiterManager::get_*_limiter` 新增参数一致性 `assert!`（Rule 12 显性化）+ get() 快速路径读锁优化
- **[T009]** `QuotaLimiter` 新增 `max()` / `period()` getter（仅供 manager 参数校验使用）
- **[T010]** `LimiterManager` 新增 LRU 淘汰机制（`MAX_LIMITER_ENTRIES=100_000` / `CLEANUP_THRESHOLD=110_000` / `CLEANUP_RATIO=0.1`），`*_access_times` 用 `AtomicU64` 无锁更新，cleanup 用 `retain` + `select_nth_unstable_by_key`（性能-HIGH-2）
- **[T012]** `LimiterManager::clear()` 改为 `#[cfg(test)] pub fn clear_for_test()`，仅测试可用（安全-L1）
- 新增 13 个单元测试（3 个并发一致性 + 3 个 LRU 淘汰 + 4 个 redact_key + 2 个 QuotaLimiter 边界 + 1 个 sanitize 边界）

### 修复

- **[CRITICAL H-002]** `LimiterManager::get_*_limiter` 慢路径 TOCTOU 限流绕过：并发场景下返回本地 `Arc::new(...)` 而非 DashMap 中的，导致限流被绕过。改用 `entry().or_insert_with(|| Arc::new(...)).clone()` 模式（tiangang 安全审查）
- **[HIGH H-001]** `LimiterManager::get_*_limiter` panic 消息中泄露 key 原文：新增 `redact_key` 函数脱敏（短 key 仅暴露字符数，长 key 暴露前 8 字符 + 总长度），用 `chars().take(8).collect()` 避免 UTF-8 边界切片 panic（tiangang 安全审查）
- **[CRITICAL C-001]** `LimiterManager::get_*_limiter` 快速路径 `access_times.insert()` 引入写锁 + 堆分配：改用 `DashMap<String, AtomicU64>` 存纳秒时间戳，`store(Ordering::Relaxed)` 无锁更新（diting 性能审查）
- **[HIGH H-001]** `LimiterManager::cleanup_*_limiters_to` 期间 `iter().collect()` 持有所有 shard 读锁 + 4.4MB 分配：改用 `retain` 替代 `iter+remove`，`select_nth_unstable_by_key` 替代全排序（diting 性能审查）
- **[HIGH H-002]** `LimiterManager::cleanup` 同步执行阻塞请求 50-200ms：用 `AtomicBool` + `compare_exchange` CAS 限制并发 cleanup（diting 性能审查）
- **[MEDIUM M-001]** `sanitize_key_component` 用 `is_alphanumeric()` 允许 Unicode 同形字符攻击：改用 `is_ascii_alphanumeric()`（tiangang 安全审查）
- **[MEDIUM M-001]** 3 个 `cleanup_*_limiters_to` 高度重复：抽 `cleanup_lru<L>` 泛型函数（diting 架构审查 DRY）
- **[MEDIUM M-003]** 3 个 `*_limiter_count()` 注释"主要用于测试"但未 cfg-gate：改为 `#[cfg(test)] pub`（diting 架构审查）
- **[MEDIUM M-004/M-003]** LRU 全排序 O(n log n)：用 `select_nth_unstable_by_key` 优化为 O(n) 平均（diting 架构/性能审查）
- **[MEDIUM M-005]** `assert!` panic 风险：在 `# Panic` 章节文档化"参数不一致是代码 bug，应在开发阶段发现"（diting 架构审查）
- **[MEDIUM M-008]** 参数一致性校验不完整（`unit_secs` 未校验）：在模块 `# 限制` 章节文档化已知限制（diting 架构审查）
- **[LOW L-001/性能 M-001]** `key.to_string()` 在 `get_*_limiter` 中调用 3 次：函数开头 `let key = key.to_string()` 缓存（diting 架构审查）
- **[LOW L-002]** `sanitize_key_component` 缺边界单元测试：新增 `test_sanitize_key_component_edge_cases` + `test_sanitize_key_component_defense_in_depth`（diting 架构审查）
- **[LOW L-003]** `let _ = rate_limiter;` 写法不够地道：改为 `let _ = &rate_limiter;`（3 处，diting 架构审查）
- **[LOW L-007]** `build_exceed_handler` 的 `error_variant` 参数可简化：签名从 `syn::Ident` 改为 `&str`（diting 架构审查）
- **[LOW L-003]** `QuotaLimiter::new` 未校验 `window_size=0`：新增 `assert!(config.window_size > 0)`（tiangang 安全审查）
- **[MEDIUM M-002]** `QuotaLimiter::max/period` getter 暴露内部配置：添加"仅供 LimiterManager 参数校验使用"注释（diting 架构审查）
- **[MEDIUM M-006]** 缺并发场景 LRU 一致性测试：新增 3 个 `test_concurrent_get_*_limiter_consistency`（10 线程并发 get 同 key 验证 `Arc::ptr_eq`，diting 架构审查）
- **[MEDIUM M-007]** `cleanup_quota/concurrency_limiters_to` 缺测试：新增 `test_lru_eviction_quota` + `test_lru_eviction_concurrency`（diting 架构审查）

### 测试覆盖

- 主 crate：1958 passed / 0 failed / 6 ignored（`cargo test --features full --lib`）
- macros crate：43 passed / 0 failed / 0 ignored（`cargo test --lib`）

## [0.2.8] - 2026-07-17

### 安全修复

- **[vuln-0001]** Admin API operator 身份绑定改用请求中实际提交的 token 查 `api_key_operators` 映射（原实现用全局单一 `api_key` 查，多 key 部署下身份隔离失效）
- **[vuln-0002]** Admin API 按路径分组的速率限制（per-client 分桶）
- **[vuln-0003]** X-Forwarded-For IP 伪造防护（仅可信代理直连才信任转发头）
- **[vuln-0004]** rustls-webpki 升级修复 CVE-2025-48369
- **[HIGH-001]** per-client 速率分桶增加内存上限（`RATE_BUCKET_MAX_ENTRIES=10000`）+ 过期窗口清扫，防止轮换源 IP 导致 map 无限膨胀的 OOM DoS
- **[MEDIUM-002]** `rate_buckets` Mutex 中毒时恢复而非 panic
- **[MEDIUM-003]** 显式锁定 rustls-webpki 版本约束
- 修复 namespace key prefix injection 漏洞
- 修复统计计数器 `Ordering::Relaxed` 高并发下不准确问题

### 重构

- examples/tests/benches：扩展 L1 重导出隔离
- lib.rs：修正 `#[async_trait]` L2 分类文档

## [0.2.7] - 2026-07-15

### 维护

- CI：action-gh-release 3.0.1 → 3.0.2
- release.yml：publish 步骤幂等化（`already exists`/`already published` 视为 warning 而非失败）
- clippy 修复：`unsafe-op-in-unsafe-fn`（benches/memory.rs）、`collapsible_if`（benches/regression.rs、tests/common）、misc clippy fixes
- 文档同步：README 版本号、feature 表补全（distributed/kit/i18n/inklog）

## [0.2.6] - 2026-07-13

### 新增

- `distributed` feature 与 `src/limiters/distributed.rs` 模块（DistributedLimiter trait + InMemoryDistributedLimiter 实现）—— 支持分布式与进程内限流兼容
- 跨平台 CI 矩阵（ubuntu/macos/windows）验证 apple/windows/linux 平台兼容性

### 变更

- `release.yml` publish 步骤幂等化：捕获 cargo publish 输出，若失败但匹配 "already exists"/"already published" 则发 ::warning:: 并继续
- `benches/memory.rs`：修复 `unsafe-op-in-unsafe-fn` clippy lint（unsafe fn 内部操作用 `unsafe { }` 包裹）
- `benches/regression.rs` + `tests/common/mod.rs`：修复 `collapsible_if` clippy lint（嵌套 if-let 用 let-chains 合并）

### 修复

- CI clippy lint 失败（unsafe-op-in-unsafe-fn + collapsible_if）
- `examples/integration-app` governor 私有模块访问错误（改为 re-export `limiteron::Governor`）

### ⚠️ 破坏性变更（仅影响启用 `kit` feature 的用户）

- trait-kit 0.2 → 0.3（pre-1.0 minor bump，Cargo 视为不兼容）；启用 `kit` feature 的用户需同步升级

### 依赖

- trait-kit 0.2 → 0.3（对齐 oxcache/dbnexus/inklog 依赖链）
- inklog 0.1.6 → 0.1.7（传递依赖，经 Cargo.lock 解析）

## [0.2.5] - 2026-07-13

### 依赖

- dbnexus 0.2 → 0.4（解决 oxcache 版本冲突，dbnexus 0.4 现依赖 oxcache 0.3）
- oxcache 0.3.7 → 0.3.8

## [0.2.4] - 2026-07-12

### 变更

- `FlowGuardError` 重命名为 `LimiteronError`，遵循 `ProjectNameError` 命名约定
- 新增 `LimiteronResult<T>` 类型别名
- 跨 crate 导入更新：`oxcache::CacheError` → `oxcache::OxCacheError`（适配 oxcache 0.3.7）
- 导入路径扁平化

## [0.2.3] - 2026-07-11

### 变更

- 移除 `StructuredLogger` trait（YAGNI 清理）
- 对齐 inklog 集成与 sdforge 模式
- 修复 edition 2024 unsafe env 调用

### 变更

- Rust edition 从 2021 升级到 2024
- 设置 rust-version 为 1.85
- 许可证从 Apache-2.0 变更为 MIT

### 修复

- 修复 edition 2024 模式匹配错误（`ref` 关键字、隐式借用）

## [0.2.1] - 2026-07-06

### 破坏性变更

- **移除 RedisStorage**：完全删除 Redis 存储后端实现及 `redis-storage` feature
  - 删除 `src/storage/redis.rs`
  - 删除 `examples/src/bin/redis_storage.rs`
  - 移除 `redis` crate 依赖
  - 所有缓存通过 oxcache 统一管理
- **移除 StorageCreate/BanStorageCreate trait**：改为 `MemoryStorage::create_storage()` 固有方法
- **移除 SlidingWindowLimiter 公开导出**：使用 `ShardedSlidingWindowLimiter` 替代
  - 仍可通过 `limiteron::limiters::sliding_window::SlidingWindowLimiter` 全路径访问（已废弃）

### 新增

- **BanTarget::Geo**：新增地理位置封禁，支持按国家代码（ISO 3166-1 alpha-2）封禁
- **BanFileLoader**：从 YAML 文件加载封禁规则，支持文件变更热重载（500ms debounce）
- **POST /api/v1/ban**：新增 HTTP 端点创建封禁，支持 ip/user/mac/geo 4 种 target 类型
- **DELETE /api/v1/ban/{target}?type=**：扩展支持 MAC/Geo 目标解封

### 安全修复

- YAML 炸弹防护：封禁文件大小限制 2MB
- AdminServer::start() 强制调用 config.validate()
- 热重载 debounce 防止 DoS
- 告警 spawn-fire-and-forget 添加 Semaphore(8) 背压
- BanManager/EventDispatcher 添加 Drop impl 防止任务泄漏
- 授权链路显性化：未配置 provider 时记录警告日志
- 整数下溢防护（list_bans 分页）
- 时钟回退防护（配额窗口重置）
- `as u32`/`as u8` 截断修复
- redact_advanced 正则脱敏逻辑修复

### 改进

- handler 错误响应状态码统一（200/400/403/404/422/500/501/503）
- get_limiter_status 改为 501 Not Implemented（不再返回假数据）
- 测试辅助函数集中到 test_support.rs 消除 3x 重复
- GovernorBuilder 移除 #[allow(dead_code)]
- config TODO 模块状态文档明确
- **GovernorBuilder metrics/tracer 修复**：`with_metrics()`/`with_tracer()` 的值之前在 `build()` 中被静默丢弃（`#[allow(unused_variables)]` 掩盖），现在正确存储到 Governor 并在 `check()` 中消费（`record_check`/`record_ban`/`record_error` + `tracer.start_span`）
- **依赖版本格式统一**：`Cargo.toml` 全部 `[workspace.dependencies]` 移除 `~` 最小版本前缀，统一使用 `"X.Y"` 格式（caret 语义）
  - 主要升级：`criterion` 0.5 → 0.8、`sqlx` 0.7 → 0.9、`sea-orm` 1.0 → 2.0.0-rc.42、`maxminddb` 0.24 → 0.29、`hmac` 0.12 → 0.13、`sha2` 0.10 → 0.11、`opentelemetry` 0.24 → 0.32、`reqwest` 0.11 → 0.13、`axum` 0.7 → 0.8、`tower-http` 0.5 → 0.7、`dashmap` 5.5 → 6.2、`thiserror` 1.0 → 2.0、`validator` 0.18 → 0.20、`rand` 0.8 → 0.10、`notify` 6.5 → 8.2、`secrecy` 0.8 → 0.10、`woothee` 0.11 → 0.13
  - `dbnexus` 保持 `0.2`：`0.3` 依赖 `oxcache 0.2`，与本项目 `oxcache 0.3` 冲突

### 测试覆盖

- 1893 unit tests passing (0 failed)
- 96.16% line coverage (5781/6012 lines)

### 修复

- **API 兼容性修复**（依赖 MAJOR 升级）：
  - `maxminddb 0.29`：`Reader.metadata` 字段私有化，改用 `reader.metadata()` 方法（`src/matchers/geo.rs`）
  - `hmac 0.13`：`new_from_slice` 改为 `KeyInit` trait 方法，补充 `use hmac::KeyInit`（`src/logging/audit.rs`）
  - `sqlx 0.9`：组合 feature `runtime-tokio-rustls` 拆分为 `runtime-tokio` + `tls-rustls`（`Cargo.toml`）
- **clippy 兼容性修复**（rust 1.96 新 lint）：
  - `unnecessary_sort_by`：`sort_by(|a,b| b.x.cmp(&a.x))` → `sort_by_key(|r| Reverse(r.x))`（`src/adapters/dbnexus_ban_storage.rs`）
  - `manual_checked_ops`：手动 `if x > 0 { y / x } else { default }` → `checked_div().unwrap_or(default)`（`src/limiters/gcra.rs`、`tests/chaos/latency.rs`）

### 文档

- **`docs/FAQ.md`**：示例依赖版本 `limiteron = "0.1"` → `"0.2"`（与当前发布版本一致）

## [0.2.0] - 2026-07-04

### 破坏性变更

- **`default = []`**：Cargo.toml 的 `default` feature 从 `["postgres"]` 改为 `[]`。用户必须显式启用 feature 才能使用对应功能。
  - 迁移示例：`cargo build --features standard`（推荐）或 `cargo build --features postgres`（仅存储）
  - 默认构建 `cargo build` 现在只包含核心限流功能，不含 PostgreSQL 存储
- **移除死 feature flags**：`code-review` 和 `advanced-matchers` feature 从 `full` preset 和定义中移除（全仓库零 `#[cfg(feature = "...")]` 引用）
- **oxcache 升级 0.2.0 → 0.3.2**：适配 oxcache 0.3.x API。oxcache 0.2.0/0.3.x 有编译 bug（security 模块无 feature gate 但引用 regex crate），启用 `core` feature 作为 workaround
- **BanManager API 变更**：
  - `ban()` → `add_ban(BanRecord)`，使用 `BanRecord` 结构体替代多个独立参数
  - `unban()` → `delete_ban(&target, unbanned_by: String)`，需要传入操作者标识
  - `is_banned()` 返回 `Result<Option<BanRecord>>` 而非 `bool`，提供更完整的封禁信息
- **限流器导入路径变更**：`use limiteron::{TokenBucketLimiter, ...}` → `use limiteron::limiters::{TokenBucketLimiter, ...}`，限流器类型统一收敛到 `limiters` 模块
- **`QuotaType` 路径变更**：`limiteron::QuotaType` → `limiteron::quota::QuotaType`
- **`MemoryStorage` 路径变更**：`limiteron::MemoryStorage` → `limiteron::storage::MemoryStorage`
- **`FallbackManager::new(cache)` → `FallbackManager::new(Arc::new(cache))`**，统一使用 `Arc` 包装依赖

### 安全

- **SSRF 防护加固** (`src/webhook_validator.rs`)：修复 IPv4-mapped IPv6 地址绕过（如 `::ffff:10.0.0.1` 不会被私有 IP 检查捕获）、未指定地址（`0.0.0.0`/`::`）、IPv6 链路本地地址（`fe80::/10`）的检查缺失

### 移除

- **Dead code 清理**：移除 11 个 dead-code 警告对应的代码（`DecisionNodeBuilder`、`MAX_REGEX_NESTING_DEPTH`、`L1Cache::island_stats` 等），基于 gitnexus 影响分析确认无外部调用

### 新增

- **RedisStorage 存储后端**（`redis-storage` feature）：实现 `Storage`/`BanStorage`/`QuotaStorage` trait，支持多实例分布式场景
- **Governor 优雅关闭**：新增 `shutdown()` / `shutdown_token()` / `is_shutdown()` 方法，支持优雅停止后台任务
- **Governor 健康检测**：新增 `health_check()` / `health_status()` 方法，提供真实的健康状态检测
- **ConfigLoader 环境变量覆盖**：支持 `LIMITERON_GLOBAL_STORAGE` / `LIMITERON_GLOBAL_CACHE` / `LIMITERON_GLOBAL_METRICS` 等环境变量覆盖配置
- **CircuitBreaker `new()` 默认构造方法**：支持开箱即用模式
- **`storage_cleanup_expired_bans` 批量删除优化**：提升过期封禁记录清理性能
- **`redis_storage` example**：RedisStorage 使用示例
- **`graceful_shutdown` example**：优雅关闭使用示例

### 修复

- **clippy 零警告**：`src/` + `tests/` + `benches/` 全部通过 clippy 严格检查
- **修复 `cleanup_expired_bans` 死锁风险**：避免在持有锁时执行可能阻塞的操作
- **修复 Governor 字段 `_storage`/`_ban_storage` 下划线前缀问题**：移除不必要的下划线前缀，字段实际被使用

### 文档

- **README 中英文同步**：版本徽章、特性列表、测试计数、路线图全面更新
- **examples 覆盖 20 个使用场景**：新增 `redis_storage`、`graceful_shutdown` 等示例
- **AGENTS.md 更新**：添加 RedisStorage 模块说明、LOC 计数、redis 依赖

### 开发体验

- **`.gitignore` 加固**：添加 `*.profraw`、`coverage/`、`tarpaulin/` 规则，防止覆盖率文件污染仓库

## [0.1.1] - 2026-01-20

### 新增

- **MemoryStorage 与 MemoryBanStorage**：`Storage` 与 `BanStorage` trait 的内存存储实现。支持"开箱即用"模式，便于快速原型验证与测试。
- **Governor::new()**：`Governor` 新增零参数构造器，使用默认内存存储。无需外部依赖即可快速上手。
- **BanManager::new()**：`BanManager` 新增零参数构造器，使用默认内存存储。
- **特性组件构造模式**：在 AGENTS.md 中新增文档表格，说明各组件支持哪些构造模式。

### 变更

- **Governor::new(config, storage, ban_storage)**：重命名为 `Governor::with_storage(config, storage, ban_storage)`，为新的零参数 `new()` 方法腾出命名。旧签名仍可通过重命名后的方法使用。

### 已弃用

- **config_loader::ConfigBuilder**：请改用 `config::ConfigBuilder`。该类型现为带弃用警告的重导出。
- **config_loader::RuleBuilder**：请改用 `config::RuleBuilder`。该类型现为带弃用警告的重导出。
- **Governor::new(config, storage, ban_storage)**：请改用 `Governor::with_storage()`。此变更为新的开箱即用模式提供了支持。

### 修复

- Governor 现已按 DI 架构文档的规格正确实现三种构造模式。
- BanManager builder 现支持可选存储（默认使用 MemoryBanStorage）。

### 安全

- 无

### 文档

- 在 AGENTS.md 中新增"特性组件构造模式"章节，包含用法示例与迁移说明。
- 新增 API 变更的迁移说明。

### 迁移指南

#### ConfigBuilder 用户

之前（已弃用）：

```rust
use limiteron::config_loader::ConfigBuilder;
let config = ConfigBuilder::new().with_rule(|r| r.id("test")).build();
```

之后（推荐）：

```rust
use limiteron::config::ConfigBuilder;
let config = ConfigBuilder::new().with_rule(|r| r.id("test")).build();
```

#### Governor 用户

之前（已弃用）：

```rust
let governor = Governor::new(config, storage, ban_storage).await.unwrap();
```

之后（推荐）：

```rust
let governor = Governor::with_storage(config, storage, ban_storage).await.unwrap();
```

快速开始（新）：

```rust
let governor = Governor::new().await;
```

#### BanManager 用户

之前：

```rust
let storage: Arc<dyn BanStorage> = Arc::new(custom_storage);
let ban_manager = BanManager::with_dependencies(storage, config).await.unwrap();
```

现在（支持可选存储）：

```rust
let ban_manager = BanManager::builder().build().await.unwrap();
// 或使用自定义存储：
let ban_manager = BanManager::builder()
    .with_storage(custom_storage)
    .build()
    .await
    .unwrap();
```

快速开始（新）：

```rust
let ban_manager = BanManager::new().await.unwrap();
```

## [0.1.0] - 2026-01-18

### 新增

- 首个发布版本：包含限流、配额管理、熔断与封禁管理
- 支持多种限流算法：TokenBucket、SlidingWindow、FixedWindow、Concurrency
- 封禁管理，支持优先级体系（IP > User > MAC > Device > APIKey）
- 配额控制，支持周期性分配与告警
- 熔断器，支持自动故障转移与状态恢复
- L1/L2/L3 缓存层
- 集成 dbnexus 实现 PostgreSQL 持久化
- 集成 oxcache 实现 Redis 缓存
- 集成 confers 实现配置管理
- 声明式宏简化配置
- 监控支持 Prometheus 指标与 OpenTelemetry 追踪
- 并行封禁检查以提升性能

[Unreleased]: https://github.com/Kirky-X/limiteron/compare/v0.3.0-rc.3...HEAD
[0.3.0-rc.3]: https://github.com/Kirky-X/limiteron/compare/v0.3.0-rc.2...v0.3.0-rc.3
[0.3.0-rc.2]: https://github.com/Kirky-X/limiteron/compare/v0.2.10...v0.3.0-rc.2
[0.2.10]: https://github.com/Kirky-X/limiteron/compare/v0.2.9...v0.2.10
[0.2.9]: https://github.com/Kirky-X/limiteron/compare/v0.2.8...v0.2.9
[0.2.8]: https://github.com/Kirky-X/limiteron/compare/v0.2.7...v0.2.8
[0.2.7]: https://github.com/Kirky-X/limiteron/compare/v0.2.6...v0.2.7
[0.2.6]: https://github.com/Kirky-X/limiteron/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/Kirky-X/limiteron/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/Kirky-X/limiteron/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/Kirky-X/limiteron/compare/v0.2.1...v0.2.3
[0.2.1]: https://github.com/Kirky-X/limiteron/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Kirky-X/limiteron/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/Kirky-X/limiteron/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Kirky-X/limiteron/releases/tag/v0.1.0
