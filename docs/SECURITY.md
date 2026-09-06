# 🔒 Limiteron 安全文档

本文档描述 Limiteron 的版本支持策略、漏洞报告流程、安全设计与使用安全最佳实践。安全设计与机制均来自项目实际实现（详见 [架构文档](ARCHITECTURE.md) 与源码），各版本的安全修复记录见根目录 [更新日志](CHANGELOG.md) 的"安全"分类。

## 📋 目录

<details open>
<summary>点击展开</summary>

- [📌 支持的版本](#-支持的版本)
- [🚨 漏洞报告流程](#-漏洞报告流程)
- [🛡️ 安全设计概览](#️-安全设计概览)
- [✅ 安全最佳实践](#-安全最佳实践)

</details>

---

## 📌 支持的版本

| 版本线 | 支持状态 | 说明 |
|--------|----------|------|
| 0.3.0-rc.x | ✅ 支持 | 当前发布线，接收安全修复 |
| 0.2.x | ❌ 不支持 | 已被 0.3.0-rc 取代，请升级到最新版本 |
| 0.1.x | ❌ 不支持 | 早期版本，请升级到最新版本 |

> 建议始终使用 crates.io 上的最新发布版本。历史版本中与安全相关的修复（如 CVE-2025-48369 的 rustls-webpki 版本锁定）不会回移到已停止支持的版本线。

---

## 🚨 漏洞报告流程

1. **报告渠道**：请优先使用 GitHub 的私有漏洞报告功能（仓库页 → Security → Report a vulnerability）私下披露；若该功能不可用，可在 [Issues](https://github.com/Kirky-X/limiteron/issues) 中报告，但请**不要**包含可直接利用的细节、PoC 载荷或影响线上环境的信息。
2. **报告内容**：请尽量包含受影响的版本（`Cargo.toml` 中的 version）、涉及的 feature 组合、复现步骤、影响评估（如可导致限流被绕过、拒绝服务等）。
3. **响应与修复**：维护者确认后会评估严重性并安排修复，修复会先在私有分支进行，随下一个版本发布。
4. **披露约定**：采用协调披露。修复发布后，会在 [CHANGELOG](CHANGELOG.md) 对应版本的"安全"分类中记录（可参考 0.2.8 的 vuln-0001 ~ vuln-0004 系列修复记录），并在讨论中致谢报告者（如报告者同意）。
5. **非安全类问题**：一般性使用问题请走 [Issues](https://github.com/Kirky-X/limiteron/issues) 或 [Discussions](https://github.com/Kirky-X/limiteron/discussions)，不必走安全流程。

---

## 🛡️ 安全设计概览

以下机制均可在源码与架构文档中找到对应实现。

### 语言层内存安全

- 全部核心代码由 Rust 编写，所有权与借用检查在编译期消除悬垂指针、缓冲区越界等内存问题；不安全的代码必须注明安全不变式（见贡献指南）。
- 存储访问统一经 DBNexus / sea-orm 的参数化查询完成，避免字符串拼接 SQL 引入注入风险。

### 限流算法的边界处理

- **令牌桶**（`src/limiters/token_bucket.rs`）：时间差与令牌补充均使用 `saturating_sub` / `saturating_add`，并用 `.min(capacity)` 封顶，杜绝整数溢出/下溢导致的额度计算错误。
- **配额窗口**（`src/quota/`）：内置时钟回退防护——NTP 校时或容器时钟漂移导致 `now < window_start` 时按窗口起点处理，防止窗口重置逻辑被回退时间击穿。
- **时钟抽象**（`src/clock.rs`）：`Clock` trait 统一时间来源，支持 `test-clock` 特性下的 `MockClock` 注入，便于对时间相关安全逻辑做确定性测试。

### 输入校验与注入防护

- **标识符 key 消毒**（`src/limiters/manager.rs` 的 `sanitize_key_component`）：仅保留 ASCII 字母数字与 `_` `-` `.`，截断至 128 字符，防御 key 注入与 Unicode 同形字符攻击。
- **输入校验**（`src/validation.rs`）：对 IP 地址、用户 ID、MAC 地址等标识符做格式校验。
- **Admin API 自身限流**（`src/admin/routes.rs`）：按路径、按客户端分桶限流，并设置分桶内存上限（`RATE_BUCKET_MAX_ENTRIES=10000`）与过期窗口清扫，防止攻击者轮换源 IP 造成无界内存增长的 OOM DoS；Mutex 中毒时恢复而非 panic。

### SSRF 防护

- **Webhook URL 校验**（`src/webhook_validator.rs`）：拒绝私有地址、回环地址、链路本地地址、未指定地址，并显式检查 IPv4-mapped IPv6（如 `::ffff:10.0.0.1`）的内嵌 IPv4，堵住绕过路径。

### 敏感信息保护

- **key 脱敏**（`src/limiters/manager.rs` 的 `redact_key`）：panic 消息与日志中不输出完整标识符，短 key 仅暴露字符数，长 key 暴露前 8 字符 + 总长度，且按字符边界截取避免 UTF-8 panic。
- **日志脱敏**：`log-redaction` 特性提供正则脱敏；`audit-log` 特性使用 `secrecy` 库保护审计链路中的敏感数据。
- **身份与转发头**（`src/admin/routes.rs`、`src/middleware/`）：Admin API operator 身份绑定到请求实际提交的 API token；仅可信代理直连时才信任 `X-Forwarded-For`，防 IP 伪造。

### 资源与可用性防护

- **封禁文件加载**（`src/ban/file_loader.rs`）：YAML 封禁文件大小上限 2MB，防 YAML 炸弹；热重载带 500ms debounce，防止高频文件变更 DoS。
- **限流器 LRU 淘汰**（`src/limiters/manager.rs`）：`LimiterManager` 按容量（`MAX_LIMITER_ENTRIES=100_000`）淘汰最久未用条目，防恶意 key 刷爆内存；cleanup 用 CAS 限流避免阻塞请求路径。
- **任务与背压**：告警通知 spawn 使用 `Semaphore(8)` 背压；`BanManager` / `EventDispatcher` 实现 `Drop` 防止后台任务泄漏；过期封禁清理避免持锁执行阻塞操作，消除死锁风险。

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
6. **启用脱敏与审计**：面向外部的服务建议启用 `log-redaction` 与 `audit-log`，避免标识符与敏感数据进入日志。
7. **保持依赖更新**：跟踪 RustSec 公告（CI 已启用 cargo-deny），升级到携带安全修复的最新版本；升级 `kit` 等集成特性时注意 CHANGELOG 中的破坏性变更说明。
8. **升级前查看更新日志**：安全修复统一记录在 [CHANGELOG](CHANGELOG.md) 各版本的"安全"分类下，升级时优先关注。
