# Spec — ban-http-api

> Delta spec for change `cache-consolidation-ban-enhancement`. 覆盖此变更引入/修改的 HTTP ban 创建 API 能力域需求。

## Requirements

### R-ban-http-api-001: POST /api/v1/ban 端点

新增 `POST /api/v1/ban` HTTP 端点，与现有 `DELETE /api/v1/ban/{target}` 对称。请求体为 JSON，响应为 `ApiResponse<BanDetailResponse>`。

**验收标准：**
- `curl -X POST http://localhost:PORT/api/v1/ban -d '{"target_type":"ip","target_value":"192.168.1.100","reason":"恶意请求","duration_secs":3600,"operator":"admin"}'` 返回 201 Created
- 响应体包含创建的 ban 详情（target_type, target_value, ban_id, expires_at）
- 路由注册到 src/admin/routes.rs

### R-ban-http-api-002: CreateBanRequest 请求体结构

请求体支持 ip/user/mac/geo 4 种 target_type，含 reason/duration_secs/operator/metadata 可选字段。

**验收标准：**
- `CreateBanRequest` 结构体定义在 src/admin/handlers.rs
- 字段：`target_type: String`, `target_value: String`, `reason: String`, `duration_secs: Option<u64>`, `operator: Option<String>`, `metadata: Option<serde_json::Value>`
- target_type 接受 "ip"/"user"/"mac"/"geo"（小写）
- duration_secs 缺省时表示永久封禁
- operator 缺省时使用 "system"

### R-ban-http-api-003: target_type 解析与验证

handler 解析 target_type 字符串为 BanTarget 枚举，验证 target_value 格式。

**验收标准：**
- `target_type="ip"` + `target_value="192.168.1.100"` → `BanTarget::Ip("192.168.1.100")`
- `target_type="user"` + `target_value="user123"` → `BanTarget::UserId("user123")`
- `target_type="mac"` + `target_value="AA:BB:CC:DD:EE:FF"` → `BanTarget::Mac("AA:BB:CC:DD:EE:FF")`
- `target_type="geo"` + `target_value="CN"` → `BanTarget::Geo { country_code: "CN" }`
- `target_type="invalid"` → 返回 400 ValidationError
- `target_type="ip"` + `target_value="invalid-ip"` → 返回 400 ValidationError

### R-ban-http-api-004: 错误响应映射

handler 错误映射到 HTTP 状态码，错误信息显性化（Rule 12）。

**验收标准：**
- 验证错误（无效 target_type/target_value）→ 400 Bad Request + 错误详情
- 授权错误（API key 无效）→ 403 Forbidden
- 重复 ban（target 已被封禁且未过期）→ 409 Conflict
- 内部错误（storage 故障）→ 500 Internal Server Error + 错误 ID（不泄露内部细节）
- 错误响应体格式：`{"error": {"code": "...", "message": "...", "details": {...}}}`

### R-ban-http-api-005: API key 认证

POST /api/v1/ban 端点要求 API key 认证，复用现有 `require_api_key` 中间件。

**验收标准：**
- 无 `Authorization: Bearer <key>` 头 → 401 Unauthorized
- 错误 API key → 403 Forbidden（使用 `constant_time_eq` 防止时序攻击）
- 正确 API key → 进入 handler
- 认证逻辑复用 src/admin/routes.rs 现有 `require_api_key` 函数

### R-ban-http-api-006: 单元测试覆盖

handler 单元测试覆盖所有 target_type + 错误情况。

**验收标准：**
- 测试覆盖 ip/user/mac/geo 4 种成功路径
- 测试覆盖无效 target_type、无效 target_value、缺失字段 3 种验证错误
- 测试覆盖无 API key、错误 API key 2 种认证错误
- 测试覆盖重复 ban 场景
- 测试使用 mock BanManager（依赖注入）

## Constraints

- 复用现有 axum 框架和 ApiResponse 类型
- 不引入新的 HTTP 中间件
- API key 认证逻辑必须复用现有实现（不重复造轮子）
- 响应体格式与现有 admin API 一致

## Out of Scope

- 不实现批量创建 ban（POST /api/v1/bans/batch）
- 不实现 ban 列表查询（GET /api/v1/bans）
- 不实现 ban 详情查询（GET /api/v1/ban/{id}）
- 不实现 WebSocket 实时推送 ban 事件
