# 🔒 Limiteron 安全文档

本文档描述 Limiteron 的版本支持策略、漏洞报告流程、安全设计与使用安全最佳实践。安全设计与机制均来自项目实际实现（详见 [架构文档](ARCHITECTURE.md) 与源码），各版本的安全修复记录见 [更新日志](CHANGELOG.md) 的"安全"分类。

## 📋 目录

<details open>
<summary>点击展开</summary>

- [📌 支持的版本与维护策略](#-支持的版本与维护策略)
- [🚨 漏洞报告流程](#-漏洞报告流程)
- [🛡️ 安全设计概览](#️-安全设计概览)
- [✅ 安全最佳实践](#-安全最佳实践)

</details>

---

## 📌 支持的版本与维护策略

本项目处于 0.x 预发布阶段，**仅维护当前 minor 版本线**（`0.3.x`，含该线内的 rc 预发布版本）。

- **当前线**：接收缺陷修复与安全修复，修复随该线的最新发布版本提供。
- **旧 minor 线**：在下一个 minor 线的首个版本发布时即进入 EOL，不再提供任何修复，安全补丁也不回移。
- **不设 LTS**：1.0 之前不承诺跨 minor 的 API 兼容性，minor 位升级可能包含破坏性变更，升级前请阅读 [CHANGELOG](CHANGELOG.md)。
- **crates.io 可见性 ≠ 受支持**：旧版本保持可下载（除非因严重缺陷执行 yank），但不属于受支持范围。

| 版本线 | 状态 | 说明 |
|--------|------|------|
| `0.3.x`（当前） | ✅ 支持 | 唯一接收修复的版本线 |
| `≤ 0.2.x` | ❌ EOL | 已被当前线取代，不再维护，请升级 |

> 历史版本中与安全相关的修复（如 CVE-2025-48369 的 rustls-webpki 版本锁定）不会回移到已 EOL 的版本线。

---

## 🚨 漏洞报告流程

1. **报告渠道**：请优先使用 GitHub 私密漏洞报告通道 **GitHub Security Advisories**（仓库页 → Security → Report a vulnerability）私下披露；该功能不可用时的第二备选为发送邮件至 Kirky-X@outlook.com，一般问题亦可在 [Issues](https://github.com/Kirky-X/limiteron/issues) 中报告，但请**不要**包含可直接利用的细节、PoC 载荷或影响线上环境的信息。
2. **报告内容**：请尽量包含受影响的版本（`Cargo.toml` 中的 version）、涉及的 feature 组合、复现步骤、影响评估（如可导致限流被绕过、拒绝服务等）。
3. **响应与修复**：维护者确认后会评估严重性并安排修复，修复会先在私有分支进行，随下一个版本发布。
4. **披露约定**：采用协调披露。修复发布后，会在 [CHANGELOG](CHANGELOG.md) 对应版本的"安全"分类中记录（可参考 0.2.8 的 vuln-0001 ~ vuln-0004 系列修复记录），并在讨论中致谢报告者（如报告者同意）。
5. **非安全类问题**：一般性使用问题请走 [Issues](https://github.com/Kirky-X/limiteron/issues) 或 [Discussions](https://github.com/Kirky-X/limiteron/discussions)，不必走安全流程。

### ⏱️ 响应承诺

- **48 小时内**确认收到
- **7 天内**给出初步评估（接受/拒绝/需补充信息）
- 修复发布前，报告者将获得补丁的预览验证机会
- 修复发布后，在 CHANGELOG 与 advisory 中致谢报告者（可匿名）

---

## 🛡️ 安全设计概览

以下机制均可在源码与架构文档中找到对应实现。

### 语言层内存安全

- 全部核心代码由 Rust 编写，所有权与借用检查在编译期消除悬垂指针、缓冲区越界等内存问题；不安全的代码必须注明安全不变式（见贡献指南）。
- 存储访问统一经 DBNexus / sea-orm 的参数化查询完成，避免字符串拼接 SQL 引入注入风险。

### 限流算法的边界处理

- **令牌桶**（`src/limiters/token_bucket.rs`）：时间差与令牌补充均使用 `saturating_sub` / `saturating_add`，并用 `.min(capacity)` 封顶，杜绝整数溢出/下溢导致的额度计算错误。
- **配额窗口**（`src/quota/`）：内置时钟回退防护，NTP 校时或容器时钟漂移导致 `now < window_start` 时按窗口起点处理，防止窗口重置逻辑被回退时间击穿。
- **时钟抽象**（`src/clock.rs`）：`Clock` trait 统一时间来源，支持 `test-clock` 特性下的 `MockClock` 注入，便于对时间相关安全逻辑做确定性测试。

### 输入校验与注入防护

- **标识符消毒**（`src/tenant/config.rs` 的 `sanitize_tenant_id` / `sanitize_environment`）：转义标识符中的 `:`，防止多租户命名空间前缀注入与跨租户前缀冲突。
- **租户身份来源**（`src/tenant/resolver.rs`）：租户身份由 garrison（SSO 网关）统一控制——租户 ID 取自请求头，**该头只能由 garrison 在认证后注入，网关必须剥离/覆写终端客户端自带的同名头**；否则用 `HeaderTenantResolver::with_allowed_tenants` 限定可接受的租户 ID（未命中即不解析租户，避免请求方自选租户命名空间而绕开封禁命名空间）。未设白名单且首次解析到该头时告警一次（库侧无认证身份可绑定，只能显性留痕）。白名单配置为空时同样告警一次：空白名单会把所有租户头的流量并入默认命名空间（租户维度隔离失效），属误配，不静默生效。白名单未命中的头值回退默认命名空间——伪造流量会并入全局桶但拿不到自选命名空间；若改为隔离命名空间，全局封禁将不再覆盖该流量（可绕封禁），此取舍由调用方决定。绕过 garrison 直连的场景应由已认证主体（JWT claim / mTLS 证书 / 会话）派生租户，库提供 `TenantResolver` 扩展点与 `DefaultTenantResolver`。
- **输入校验**（`src/validation.rs`）：对 IP 地址、用户 ID、MAC 地址等标识符做格式校验。
- **Admin API 自身限流**（`src/admin/routes.rs`）：按路径、按客户端分桶限流（分桶窗口长度随 entry 存储，跨组不清零他组计数），并设置分桶内存上限（`RATE_BUCKET_MAX_ENTRIES=10000`）与过期窗口清扫，防止攻击者轮换源 IP 造成无界内存增长的 OOM DoS；Mutex 中毒时恢复而非 panic。`/healthz` `/readyz` 完全不限流（K8s 探针被限流会触发 Pod 重启），`/metrics` 免认证但受限流约束。分组默认额度取自单一事实源 `config::default_rate_limit_for`（ban=100/min、quota=50/min、metrics=600/min、default=200/min）：配置只覆盖部分分组时，未覆盖的分组回退到同一额度，不会因缺项而放宽限流。
- **管理面凭证强度**（`src/admin/routes.rs`）：`routes::create_router` 与 `AdminServer::start` / `into_router` 同口径执行 `AdminApiConfig::validate()`——主 `api_key` 短于 16 字符时不得作为有效凭证（直接调用路由构造函数不再能绕过该校验）。`api_key_roles` 中登记的 key 与主 `api_key` 同为活凭证，同口径要求 16 字符下限：过短的角色 key 不作有效凭证（401）并显性记错（只报角色与长度，不输出 key 原文）。公开辅助函数 `AdminApiConfig::is_valid_api_key` 与路由同口径（主 `api_key` 须先过强度校验方视为有效凭证；角色映射 key 须同样满足下限），凭证比对统一走常量时间比较。

### SSRF 防护

- **Webhook URL 校验**（`src/webhook_validator.rs`）：拒绝私有地址、回环地址、链路本地地址、未指定地址，并显式检查 IPv4-mapped IPv6（如 `::ffff:10.0.0.1`）的内嵌 IPv4，堵住绕过路径。禁止段另覆盖**特殊用途段**——`0.0.0.0/8`、`100.64.0.0/10`（运营商级 NAT / Tailscale 等隧道工具的默认段，可直达宿主机上的服务）、`192.0.0.0/24`、`198.18.0.0/15`、`224.0.0.0/4` 及以上保留段（组播 / 广播），v6 侧覆盖 `ff00::/8` 组播、`fec0::/10` 站点本地、IPv4 兼容写法 `::a.b.c.d` 与 NAT64 前缀 `64:ff9b::/96`（按内嵌 IPv4 判定，不整段拉黑以免误伤纯 NAT64 部署）。字面量路径与出站解析器**共用同一判定**（`forbidden_target_reason`），两处口径不会漂移。校验对**每一跳重定向**重复执行（出站客户端用自定义 `redirect` 策略，命中内网即停止跟随），避免「公网地址 302 → 内网」绕过初始 URL 校验；重定向链另有跳数上限（reqwest 的 `Policy::custom` 不自带链长限制），超限以错误终止——跳数按 reqwest 的一基口径计（`attempt.previous().len()` 含初始 URL），故恰好允许 `MAX_WEBHOOK_REDIRECTS` 跳。
- **出站客户端不走环境代理**（`src/webhook_validator.rs`）：环境代理（`ALL_PROXY` / `HTTP_PROXY` / `HTTPS_PROXY`）会让**代理**去解析目标主机名——本地只解析到代理主机，目标地址从此逃过 SSRF 解析器的判定。故出站客户端一律 `.no_proxy()`，所有名字解析都经过 SSRF 安全解析器。
- **重定向目标的传输策略由调用方决定**（`WebhookTransportPolicy`）：事件分发与配额告警的策略不同（后者载荷含敏感数据，重定向目标恒须 HTTPS），策略经工厂参数传入并按策略分桶缓存客户端——用构建 profile 推断会让某一方的策略套到另一方（明文降级或无谓拒绝）。
- **域名面（DNS 解析）**：字面量校验之外，出站客户端装用 SSRF 安全解析器——在**连接所用的那一次解析**上拒绝内网/回环/link-local 目标（解析结果即连接目标，无「先校验后连接」的解析漂移窗口，DNS rebinding 亦被覆盖）。**发送路径不再重复 DNS 预检**：预检与连接是两次解析，预检的瞬时故障会把本可成功的投递判死，故域名面的判定只保留在连接期（权威）与**注册路径**（`validate_webhook_url_async`，回答「该端点此刻是否可用」，故需区分永久拒绝与瞬时故障）。注册路径的预检有耗时上限（`WEBHOOK_DNS_PRECHECK_TIMEOUT`，2s < 请求自身的 5s 超时）：解析器卡死时按瞬时故障返回，不会无限期占用调用方——事件分发的 webhook 扇出是顺序 await，一次卡死即拖住整个分派任务，最终导致广播通道积压并丢事件。
- **Webhook 校验结果的分类上报**（`src/error/mod.rs` 的 `WebhookUrlError`）：校验失败区分「永久拒绝」（`Rejected`：协议不符 / 命中内网禁止段 / 解析结果全落在禁止段）与「瞬时故障」（`DnsTransient`：DNS 解析失败或预检超时），后者可重试。注册路径的 `EventDispatcher::try_add_webhook_url` 把分类结果返回调用方（`add_webhook_url` 仍是只记告警的薄封装），避免一次解析抖动把合法端点变成不可恢复的丢失。
- **Webhook URL 不整条入日志**（`logging::redaction::redact_url`）：webhook 目标的 path/query 可能携带令牌（如带签名的回调 URL），日志只保留 scheme 与 host——重定向目标由远端服务端给出，整条回显等于把远端可控内容写进日志文件。

### 只读管理 Web UI（`admin-ui`）

- **默认无鉴权、默认绑定回环**：UI 面向「本机快照查看」设计，默认 `127.0.0.1`、无鉴权；非回环绑定发出显式告警，要求前置带认证的反向代理。`/snapshot` 的封禁清单降敏（目标按 `logging::redaction` 口径遮罩、原因剔除）。
- **可选访问控制**（`src/admin/web.rs` 的 `WebUiAccess`）：`WebUiAccess::strict()` 启用 Host 白名单（仅放行 IP 字面量、`localhost` 与显式配置的域名，其余 DNS 名返回 421，收敛 DNS rebinding 面；Host 取 `Host` 头，缺失时回退 URI authority——HTTP/2 只有 `:authority`，两者皆缺即拒绝）；`WebUiAccess::with_api_key(...)` 要求 `Authorization: Bearer <key>`（scheme 大小写不敏感；缺失/不符返回 401），key 须非空且长度 ≥ 16（与 `AdminApiConfig` 同口径，构造期即拒绝违规 key），鉴权失败按客户端 IP 限流（窗口内失败次数达上限返回 429）。静态页 `/` 不含数据，豁免凭证校验（Host 校验仍覆盖全部路由）。两项均为 opt-in，默认行为与既有部署一致。
- **凭证不外泄**：`WebUiAccess` 手写 `Debug` 实现（api_key 以 `<redacted; N chars>` 呈现），派生的 `Debug` 会以明文打印 key。
- **已知边界**：默认（宽松 Host + 无鉴权）下 DNS rebinding 面未被路由层闭合，须由前置反向代理强制 Host 白名单。**生产部署提醒**：非本机部署请显式启用 `WebUiAccess::strict()`（或由前置反代强制 Host 白名单），不要长期运行在默认宽松模式。

### 管理操作授权

- **手动封禁/解封的细粒度授权**（`src/ban/types.rs`）：`BanManager` 的 `authorization_provider` 为可选注入；未配置时构造即告警，手动封禁/解封逐次告警（跳过授权链路必须留痕）。`BanManagerBuilder::with_required_authorization(true)` 可令未注入授权提供者的构建直接失败（fail closed），杜绝「以为有细粒度授权、实际没有」的部署状态；管理面另有 API key → 角色的粗粒度 RBAC 兜底。
- **`with_required_authorization` 开启须知**（默认 `false`，按需开启）：开启后仅强制「构建期必须注入 `authorization_provider`」；provider 的授权判定语义（角色/操作映射、拒绝时是否 fail closed）由实现方负责。该开关不替代管理面 API key → 角色的 RBAC，也不覆盖服务面限流决策；开启后未注入 provider 的部署会在构建期显式报错，而非静默运行在无细粒度授权状态。
- **`with_required_authorization` 的作用范围**：fail closed 只在 `BanManagerBuilder::build()` 内生效。`BanManager::new` / `BanManager::with_dependencies` / `BanManager::with_dependencies_and_auth` 不经该构建器，无法开启此约束——未注入授权提供者时它们仍构建成功（仅告警并逐次跳过授权检查）。要求 fail closed 的部署必须经 `BanManager::builder()` 构造。

### 敏感信息保护

- **key 脱敏**（`src/limiters/manager.rs` 的 `redact_key`）：panic 消息与日志中不输出完整标识符，短 key 仅暴露字符数，长 key 暴露前 8 字符 + 总长度，且按字符边界截取避免 UTF-8 panic。
- **日志脱敏**：`log-redaction` 特性提供脱敏引擎（引擎由 inklog 的 `DataMasker` / `MaskRuleRegistry` 承载，规则与优先级的唯一定义处在 inklog 侧）；该特性不在 `full` 预设内，须在依赖声明中显式开启，高级脱敏 API（`RedactionConfig` / `contains_sensitive_info` / `redact_advanced` / `redact_http_content`）随之可用。`audit-log` 特性使用 `secrecy` 库保护审计链路中的敏感数据。基础脱敏函数（`redact_basic` / `redact_user_id` / `redact_ip` / `redact_email`）与敏感字段名判定一律按**字符**（非字节）截取，多字节 UTF-8（中文 / emoji）输入不会触发 str 切片 panic；`redact_advanced` 的短值（≤4 字符）整段脱敏阈值同样按**字符**计（非字节），多字节短值不会被阈值漏过。
- **身份与转发头**（`src/admin/routes.rs`、`src/middleware/`）：Admin API operator 身份绑定到请求实际提交的 API token；仅可信代理直连时才信任 `X-Forwarded-For`，防 IP 伪造。

### 资源与可用性防护

- **封禁文件加载**（`src/ban/file_loader.rs`）：YAML 封禁文件大小上限 2MB，防 YAML 炸弹；热重载带 500ms debounce，防止高频文件变更 DoS。
- **限流器 LRU 淘汰**（`src/limiters/manager.rs`）：`LimiterManager` 按容量（`MAX_LIMITER_ENTRIES=100_000`）淘汰最久未用条目，防恶意 key 刷爆内存；cleanup 用 CAS 限流避免阻塞请求路径。
- **任务与背压**：告警通知 spawn 使用 `Semaphore(8)` 背压；`BanManager` / `EventDispatcher` 实现 `Drop` 防止后台任务泄漏；过期封禁清理避免持锁执行阻塞操作，消除死锁风险。`EventDispatcher` 的 webhook 发送路径每次发送前做 SSRF 预检，预检带 2s 上限——慢解析器不会无限期占用唯一的分派任务。

### 供应链与依赖安全

- CI 内置 **cargo-deny 安全审计**（`.github/workflows/ci.yml` 的 Security Audit 任务，对齐 RustSec Advisory DB）与 **CodeQL 静态分析**（`.github/workflows/codeql.yml`）。
- 对已知漏洞依赖做最低版本锁定，如 `rustls-webpki`（CVE-2025-48369）；依赖一律经 feature 门控（`optional = true`），默认构建（`default = []`）不引入任何外部存储/HTTP 依赖，最小化攻击面。

---

## ✅ 安全最佳实践

部署与使用 Limiteron 时建议遵循：

1. **最小化特性面**：只启用实际需要的 feature（默认 `default = []` 仅含核心限流），减少编译进二进制的代码与依赖。
2. **显式选择存储后端**：`postgres` 与 `sqlite` 互斥，按部署形态二选一；生产环境优先 PostgreSQL 并启用 TLS（`runtime-tokio-rustls`）。
3. **保护 Admin API**：不要将 Admin REST API 直接暴露在公网，使用网络隔离/反向代理访问控制；按需启用按路径限流，并为多 key 部署配置 `api_key_operators` 映射。
4. **正确配置可信代理**：仅在确认前置为可信反向代理时信任 `X-Forwarded-For`，否则客户端 IP 可被伪造绕过基于 IP 的限流与封禁。
5. **校验外部输入**：为 webhook 回调目标启用 URL 校验（内网地址会被拒绝）；使用 YAML 封禁文件时仅加载受控来源的文件（2MB 上限 + 热重载 debounce 已内置，但文件权限应收敛为只读）。
6. **启用脱敏与审计**：面向外部的服务建议显式启用 `log-redaction` 与 `audit-log`（`log-redaction` 不在 `full` 预设内，须在依赖声明中单独开启），避免标识符与敏感数据进入日志。
7. **保持依赖更新**：跟踪 RustSec 公告（CI 已启用 cargo-deny），升级到携带安全修复的最新版本；升级 `kit` 等集成特性时注意 CHANGELOG 中的破坏性变更说明。
8. **升级前查看更新日志**：安全修复统一记录在 [CHANGELOG](CHANGELOG.md) 各版本的"安全"分类下，升级时优先关注。
