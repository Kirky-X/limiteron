# Spec — ban-target

> Delta spec for change `cache-consolidation-ban-enhancement`. 覆盖此变更引入/修改的 BanTarget 类型扩展能力域需求。

## Requirements

### R-ban-target-001: 新增 BanTarget::Geo 变体

`BanTarget` 枚举新增 `Geo { country_code: String }` 变体，使用 serde 标签 `#[serde(rename = "geo")]`。

**验收标准：**
- `src/storage/mod.rs` 中 `BanTarget` 枚举含 `Geo { country_code: String }` 变体
- `serde_json::to_string(&BanTarget::Geo { country_code: "CN".into() })` 输出 `{"type":"geo","value":{"country_code":"CN"}}`
- `serde_json::from_str::<BanTarget>(r#"{"type":"geo","value":{"country_code":"CN"}}"#)` 返回 `Ok(BanTarget::Geo { country_code: "CN" })`
- 单元测试覆盖序列化/反序列化往返

### R-ban-target-002: 新增 BanPriority::Geo = 6

`BanPriority` 枚举新增 `Geo = 6` 变体，作为最低优先级（geo 是粗粒度封禁）。

**验收标准：**
- `src/ban/types.rs` 中 `BanPriority` 枚举含 `Geo = 6` 变体
- `BanPriority::from_target(&BanTarget::Geo { country_code: "CN".into() })` 返回 `BanPriority::Geo`
- `BanPriority::Geo < BanPriority::ApiKey` 为 true（Geo 优先级最低）
- 单元测试覆盖优先级排序

### R-ban-target-003: validate_ban_target 支持 Geo 分支

`validate_ban_target` 函数对 `BanTarget::Geo` 验证 country_code 为有效 ISO 3166-1 alpha-2 国家代码（2 字母大写）。

**验收标准：**
- `validate_ban_target(&BanTarget::Geo { country_code: "CN".into() })` 返回 `Ok(())`
- `validate_ban_target(&BanTarget::Geo { country_code: "cn".into() })` 返回 Err（需大写）
- `validate_ban_target(&BanTarget::Geo { country_code: "CHN".into() })` 返回 Err（需 2 字母）
- `validate_ban_target(&BanTarget::Geo { country_code: "".into() })` 返回 Err（空字符串）
- `validate_ban_target(&BanTarget::Geo { country_code: "ZZ".into() })` 返回 Err（非 ISO 国家代码，可选严格模式）

### R-ban-target-004: redact_ban_target 支持 Geo 分支

`redact_ban_target` 函数对 `BanTarget::Geo` 进行脱敏，保留前 2 字符（国家代码本身就是 2 字符，整体保留但标记为已脱敏）。

**验收标准：**
- `redact_ban_target(&BanTarget::Geo { country_code: "CN".into() })` 返回脱敏后的字符串（如 "Geo(**)" 或 "CN→**"）
- 单元测试覆盖脱敏行为

### R-ban-target-005: 更新所有 match BanTarget 模式

所有 `match BanTarget { ... }` 模式增加 `Geo` 分支，无 `_ =>` 通配符（强制穷尽匹配）。

**验收标准：**
- `src/adapters/dbnexus_ban_storage.rs` 所有 match BanTarget 模式含 Geo 分支
- `src/cache/ban_storage.rs` 所有 match BanTarget 模式含 Geo 分支
- `cargo build --features full` 无"non-exhaustive patterns"警告
- `cargo clippy --features full -- -D warnings` 零警告

## Constraints

- BanTarget::Geo 的 country_code 必须是 ISO 3166-1 alpha-2（2 字母大写）
- 不引入 country-code 验证 crate（手动维护白名单或正则）
- 保持与 Ip/UserId/Mac 一致的类型安全设计

## Out of Scope

- 不实现 GeoMatcher 与 BanTarget::Geo 的自动联动（后续变更）
- 不实现 IP → Geo 自动封禁（后续变更）
- 不存储 Geo ban 的地理坐标
