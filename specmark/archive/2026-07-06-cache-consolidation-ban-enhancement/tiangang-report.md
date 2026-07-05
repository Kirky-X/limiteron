# tiangang SAST 安全扫描报告

**扫描目标**：/home/dev/projects/limiteron（Rust 项目，约 90 个源文件）
**扫描维度**：9 个安全维度全覆盖
**扫描方法**：Grep 模式匹配 + 关键文件 Read 静态分析

## CRITICAL（漏洞，必须立即修复）

1. **[src/ban/file_loader.rs:110 + src/config/loader.rs:29] YAML 炸弹 / 文件大小无限制**
   `serde_yaml::from_str(&content)` 直接解析整个文件内容，YAML 锚点/别名（billion laughs attack）可造成指数级内存扩展。文件路径由配置项指定，若被攻击者可控则可远程触发 OOM。
   → 在 `read_to_string` 前增加文件大小预检查（如 `MAX_CONFIG_FILE_SIZE = 1 MB`），并启用 `serde_yaml` 的递归深度限制。

2. **[src/admin/server.rs:74-94] `AdminServer::start()` 未调用 `config.validate()`**
   `AdminApiConfig::new()` 会设置 `enabled=true`，但 `start()` 不验证 `api_key` 是否非空或长度 ≥16。若用户手动构造 `AdminApiConfig { enabled: true, api_key: String::new(), .. }` 并启动，会以空 API key 暴露管理 API。
   → 在 `start()` 入口处添加 `self.config.validate().map_err(...)?` 强制校验。

## HIGH（高危风险）

3. **[src/ban/file_loader.rs:176-219] 热重载无 debounce**
   `notify::RecommendedWatcher` 在编辑器保存（原子写入：写临时文件 + 重命名）时会短时间内触发多个事件，每次触发都同步读文件 + 解析 YAML + 写存储，可造成性能抖动甚至 DoS。
   → 增加事件防抖（如 500ms `tokio::time::sleep` + coalescing）。

4. **[src/logging/redaction.rs:153-203] `redact_advanced` 逻辑缺陷**
   regex 替换结果 `result` 被丢弃，最终输出基于原始 `value` 的 prefix/suffix。例如输入 `"contact me at test@example.com"` 返回 `"co***om"`，泄露了原始字符串首尾字符。
   → 在 regex 替换后，对 `result` 计算最终输出，或当任何 regex 命中时直接返回 `"***"`。

5. **[src/admin/handlers.rs:204-250 + src/ban/types.rs:677-682] create_ban 授权链路依赖可选的 `authorization_provider`**
   `BanManager::new()` 默认不安装 provider，导致任何持有 admin API key 的人都能封禁任意目标，无细粒度角色检查。
   → 在 admin handler 层强制注入 `AuthorizationProvider`，或 `BanManager::new()` 默认安装 `SimpleAuthorizationProvider`。

6. **[src/matchers/custom.rs:600, 612] `as u8` 截断绕过范围校验**
   `let start_hour: u8 = start_hour_u64 as u8;` 然后 `if start_hour > 23 { return Err }`。当 `start_hour_u64 = 256` 时截断为 `0`，绕过 `> 23` 检查。
   → 改为 `u8::try_from(start_hour_u64).map_err(...)?` 再做范围检查。

## MEDIUM（中危风险）

7. [src/ban/file_loader.rs:110] async 函数中同步阻塞 I/O（`std::fs::read_to_string`）
8. [src/validation.rs:323-343] `validate_geo_country_code` 仅校验格式不校验有效性（"ZZ" 等无效国家码通过）
9. [src/matchers/custom.rs:521-523] `TimeWindowMatcher::new` 使用 `assert!`（release panic）
10. [src/admin/routes.rs] Admin API 未显式配置 HTTP body 限制
11. [src/logging/redaction.rs:94-148] `log-redaction` feature 非默认启用
12. [src/admin/routes.rs:13-22] `constant_time_eq` 长度不等时立即返回（侧信道）

## LOW（低危/信息性）

13. [src/admin/routes.rs:174 等] 测试中硬编码 `"test-api-key-16chars!!"`（仅 `#[cfg(test)]`，符合预期）
14. [src/logging/audit.rs:924 等] 测试中使用 `"test-secret-key-32-bytes-long!"`（仅 `#[cfg(test)]`）
15. [src/admin/server.rs:85-89] `log::info!` 记录服务器监听地址（可接受）
16. [src/ban/types.rs:688-691] `info!("Creating ban: target={:?}")` 可能泄露被封禁的 IP/用户 ID
17. 未发现 CSPRNG 使用（当前 API key 由用户提供，无需生成）
18. **未发现 `unsafe` 代码** ✅ 优秀
19. **未发现命令注入风险** ✅ 优秀
20. **未发现 SQL 注入风险** ✅ 优秀
21. **未发现弱哈希** ✅ 优秀
22. [src/webhook_validator.rs] SSRF 防护完善 ✅ 优秀

## 统计

| 严重程度 | 数量 |
|---------|------|
| CRITICAL | 2 |
| HIGH | 4 |
| MEDIUM | 6 |
| LOW | 9 |

**整体安全评分：7.0 / 10**

## 优势

- 无 unsafe 代码、无命令注入、无 SQL 注入、无弱哈希
- SSRF 防护完善
- Bearer token 使用 constant_time_eq
- API key 最小长度校验
- 默认绑定 127.0.0.1
- HMAC 密钥用 `secrecy::SecretString` 保护

## 优先修复顺序

1. CRITICAL: YAML 文件大小限制 + AdminServer::start() 增加 validate()
2. HIGH: 热重载 debounce + redact_advanced 修复 + BanManager 默认安装授权 provider + `as u8` 截断修复
3. MEDIUM: async I/O 改造 + Geo 国家码白名单 + TimeWindowMatcher 返回 Result + 显式 body limit
