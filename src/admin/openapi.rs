// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Admin API 的 OpenAPI 3.0.3 文档构建
//!
//! [`openapi_document`] 是文档的单一事实来源：路由清单与 `AdminService`
//! 契约、`routes::create_router` 注册表保持一一对应；产物经
//! `docs/openapi.json` 入库并由防漂移测试守卫（见 `tests/openapi_drift.rs`，
//! `UPDATE_OPENAPI=1` 环境变量可重写产物）。
//!
//! schema 手写（无 schemars 依赖）：Admin API 的请求/响应类型是小而稳定的
//! serde 面，手写 schema 使文档不依赖反射宏链，演进由防漂移测试兜底。

/// OpenAPI 文档版本元信息
pub const OPENAPI_VERSION: &str = "3.0.3";
/// Admin API 文档标题
pub const TITLE: &str = "Limiteron Admin API";
/// 与 `CARGO_PKG_VERSION` 同步的 API 版本
pub const API_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 通用响应包装的 schema 引用构造
fn response_schema(data_schema: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["success", "message"],
        "properties": {
            "success": { "type": "boolean" },
            "message": { "type": "string" },
            "data": { "$ref": data_schema }
        }
    })
}

/// JSON 响应的 OpenAPI response 构造
fn json_response(description: &str, data_schema: &str) -> serde_json::Value {
    serde_json::json!({
        "description": description,
        "content": {
            "application/json": {
                "schema": { "allOf": [
                    { "$ref": "#/components/schemas/ApiResponse" },
                    { "type": "object",
                      "properties": { "data": { "$ref": data_schema } } }
                ]}
            }
        }
    })
}

/// 错误响应构造（ApiResponse 包装、data 为 null）
fn error_response(description: &str) -> serde_json::Value {
    serde_json::json!({
        "description": description,
        "content": {
            "application/json": {
                "schema": { "$ref": "#/components/schemas/ApiResponse" }
            }
        }
    })
}

/// 带 Bearer 安全要求的 operation 构造
fn operation(summary: &str, tag: &str, mut operation: serde_json::Value) -> serde_json::Value {
    operation["summary"] = serde_json::json!(summary);
    operation["tags"] = serde_json::json!([tag]);
    if operation.get("responses").is_none() {
        operation["responses"] = serde_json::json!({});
    }
    operation
}

/// 构建完整 OpenAPI 文档
///
/// 路由清单与 `routes::create_router` 一一对应（含探针/指标三个 bypass
/// 端点）；写操作标注 `security: BearerAuth`。
#[must_use]
pub fn openapi_document() -> serde_json::Value {
    let bearer = serde_json::json!([{ "BearerAuth": [] }]);
    let ok_json = |desc: &str, schema: &str| json_response(desc, schema);

    serde_json::json!({
        "openapi": OPENAPI_VERSION,
        "info": {
            "title": TITLE,
            "version": API_VERSION,
            "description": "Limiteron 管理控制面：限流状态、封禁管理、配额调整、熔断器状态与规则热更新。除探针/指标端点外均需 Bearer API key。",
        },
        "paths": {
            "/healthz": operation("存活探针", "probes", serde_json::json!({
                "get": {
                    "responses": {
                        "200": { "description": "进程存活", "content": { "application/json": { "schema": { "type": "object", "properties": { "status": { "type": "string" } } } } } }
                    }
                }
            })),
            "/readyz": operation("就绪探针（组件级聚合）", "probes", serde_json::json!({
                "get": {
                    "responses": {
                        "200": { "description": "全部组件健康" },
                        "503": { "description": "任一组件不健康，响应体含明细" }
                    }
                }
            })),
            "/metrics": operation("Prometheus 文本格式指标", "probes", serde_json::json!({
                "get": {
                    "responses": {
                        "200": { "description": "text/plain; version=0.0.4 exposition" }
                    }
                }
            })),
            "/api/v1/status": operation("系统整体状态", "status", serde_json::json!({
                "get": {
                    "security": bearer,
                    "responses": {
                        "200": ok_json("系统状态", "#/components/schemas/SystemStatus"),
                        "401": error_response("鉴权失败")
                    }
                }
            })),
            "/api/v1/status/circuit-breaker": operation("熔断器状态", "status", serde_json::json!({
                "get": {
                    "security": bearer,
                    "responses": {
                        "200": ok_json("熔断器状态", "#/components/schemas/CircuitBreakerStatus"),
                        "503": error_response("未配置 circuit-breaker")
                    }
                }
            })),
            "/api/v1/introspect": operation("运行时自省快照（规则/决策链/封禁/熔断聚合）", "status", serde_json::json!({
                "get": {
                    "security": bearer,
                    "responses": {
                        "200": { "description": "自省快照 JSON（结构随 feature 组合扩展）",
                                 "content": { "application/json": { "schema": { "type": "object" } } } }
                    }
                }
            })),
            "/api/v1/config": operation("规则热更新（原子换配置，失败保留旧配置）", "config", serde_json::json!({
                "post": {
                    "security": bearer,
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/FlowControlConfig" } } }
                    },
                    "responses": {
                        "200": { "description": "应用报告", "content": { "application/json": { "schema": { "type": "object" } } } },
                        "400": error_response("校验失败，旧配置原样保留")
                    }
                }
            })),
            "/api/v1/check/batch": operation("批量检查（N 个 key 一次决策）", "decision", serde_json::json!({
                "post": {
                    "security": bearer,
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/BatchCheckBody" } } }
                    },
                    "responses": {
                        "200": { "description": "逐项决策结果", "content": { "application/json": { "schema": { "type": "object" } } } },
                        "400": error_response("空批次或超过 1000 条上限")
                    }
                }
            })),
            "/api/v1/tokens/prefetch": operation("批量令牌预取", "decision", serde_json::json!({
                "post": {
                    "security": bearer,
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/TokenPrefetchBody" } } }
                    },
                    "responses": {
                        "200": { "description": "逐项 granted 与汇总", "content": { "application/json": { "schema": { "type": "object" } } } },
                        "400": error_response("空批次或超过 1000 条上限")
                    }
                }
            })),
            "/api/v1/ban": operation("创建封禁（ip/user/mac/geo）", "ban", serde_json::json!({
                "post": {
                    "security": bearer,
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CreateBanRequest" } } }
                    },
                    "responses": {
                        "201": ok_json("封禁详情", "#/components/schemas/BanResponse"),
                        "400": error_response("目标/原因校验失败"),
                        "403": error_response("operator 无授权"),
                        "500": error_response("内部错误"),
                        "503": error_response("未配置 ban-manager")
                    }
                }
            })),
            "/api/v1/ban/{target}": operation("解除封禁（?type=ip|user|mac|geo|cidr）", "ban", serde_json::json!({
                "delete": {
                    "security": bearer,
                    "parameters": [
                        { "name": "target", "in": "path", "required": true, "schema": { "type": "string" } },
                        { "name": "type", "in": "query", "required": false,
                          "schema": { "type": "string", "enum": ["ip", "user", "mac", "geo", "cidr"] } }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/UnbanRequest" } } }
                    },
                    "responses": {
                        "200": ok_json("解除结果", "#/components/schemas/Unit"),
                        "400": error_response("不支持的 type"),
                        "404": error_response("封禁不存在"),
                        "500": error_response("内部错误"),
                        "503": error_response("未配置 ban-manager")
                    }
                }
            })),
            "/api/v1/quota/{tenant_id}": operation("配额重置（new_limit=0）/ 拒绝 per-tenant 上限更新", "quota", serde_json::json!({
                "put": {
                    "security": bearer,
                    "parameters": [
                        { "name": "tenant_id", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/UpdateQuotaRequest" } } }
                    },
                    "responses": {
                        "200": ok_json("操作结果", "#/components/schemas/UpdateQuotaResponse"),
                        "400": error_response("不支持的操作"),
                        "500": error_response("内部错误"),
                        "503": error_response("未配置 quota-control")
                    }
                }
            }))
        },
        "components": {
            "securitySchemes": {
                "BearerAuth": { "type": "http", "scheme": "bearer" }
            },
            "schemas": {
                "ApiResponse": response_schema("#/components/schemas/Unit"),
                "Unit": { "type": "object", "nullable": true },
                "SystemStatus": {
                    "type": "object",
                    "required": ["total_requests", "blocked_requests", "success_rate"],
                    "properties": {
                        "total_requests": { "type": "integer", "format": "int64" },
                        "blocked_requests": { "type": "integer", "format": "int64" },
                        "success_rate": { "type": "number", "format": "double" },
                        "active_bans": { "type": "integer", "description": "ban-manager feature 下存在" },
                        "circuit_breaker": { "type": "string", "description": "circuit-breaker feature 下存在" }
                    }
                },
                "CircuitBreakerStatus": {
                    "type": "object",
                    "required": ["state", "failure_rate", "slow_call_rate"],
                    "properties": {
                        "state": { "type": "string" },
                        "failure_rate": { "type": "number", "format": "double" },
                        "slow_call_rate": { "type": "number", "format": "double" }
                    }
                },
                "FlowControlConfig": {
                    "type": "object",
                    "required": ["version", "global", "rules"],
                    "properties": {
                        "version": { "type": "string" },
                        "global": { "type": "object" },
                        "rules": { "type": "array", "items": { "type": "object" } }
                    },
                    "description": "完整 FlowControlConfig（字段见 config 模块；此处保留顶层骨架）"
                },
                "BatchCheckBody": {
                    "type": "object",
                    "required": ["requests"],
                    "properties": {
                        "requests": {
                            "type": "array",
                            "maxItems": 1000,
                            "items": {
                                "type": "object",
                                "properties": {
                                    "user_id": { "type": "string", "nullable": true },
                                    "ip": { "type": "string", "nullable": true },
                                    "path": { "type": "string", "nullable": true },
                                    "method": { "type": "string", "nullable": true }
                                }
                            }
                        }
                    }
                },
                "TokenPrefetchBody": {
                    "type": "object",
                    "required": ["items"],
                    "properties": {
                        "items": {
                            "type": "array",
                            "maxItems": 1000,
                            "items": {
                                "type": "object",
                                "required": ["key", "tokens"],
                                "properties": {
                                    "key": { "type": "string" },
                                    "tokens": { "type": "integer", "format": "int64" }
                                }
                            }
                        }
                    }
                },
                "BanTarget": {
                    "type": "object",
                    "required": ["type", "value"],
                    "description": "ip/user/mac 为字符串 value；geo 为 {country_code}；cidr 为字符串 value",
                    "properties": {
                        "type": { "type": "string", "enum": ["ip", "user", "mac", "geo", "cidr"] },
                        "value": {}
                    }
                },
                "CreateBanRequest": {
                    "type": "object",
                    "required": ["target", "reason"],
                    "properties": {
                        "target": { "$ref": "#/components/schemas/BanTarget" },
                        "reason": { "type": "string" },
                        "operator": { "type": "string", "nullable": true, "description": "已弃用：服务端忽略，身份由 API key mapping 决定" },
                        "duration_secs": { "type": "integer", "format": "int64", "nullable": true }
                    }
                },
                "BanResponse": {
                    "type": "object",
                    "required": ["id", "ban_times", "expires_at", "is_manual"],
                    "properties": {
                        "id": { "type": "string" },
                        "ban_times": { "type": "integer" },
                        "expires_at": { "type": "integer", "format": "int64" },
                        "is_manual": { "type": "boolean" }
                    }
                },
                "UnbanRequest": {
                    "type": "object",
                    "properties": {
                        "reason": { "type": "string", "nullable": true },
                        "operator": { "type": "string", "nullable": true, "description": "已弃用：服务端忽略" }
                    }
                },
                "UpdateQuotaRequest": {
                    "type": "object",
                    "required": ["resource", "new_limit"],
                    "properties": {
                        "resource": { "type": "string" },
                        "new_limit": { "type": "integer", "format": "int64", "description": "0 = 重置信号" },
                        "duration_secs": { "type": "integer", "format": "int64", "nullable": true }
                    }
                },
                "UpdateQuotaResponse": {
                    "type": "object",
                    "required": ["success"],
                    "properties": {
                        "success": { "type": "boolean" },
                        "expires_at": { "type": "integer", "format": "int64", "nullable": true }
                    }
                }
            }
        }
    })
}

/// 序列化为入库产物格式（pretty JSON + 尾换行）
#[must_use]
pub fn openapi_document_pretty() -> String {
    let mut out =
        serde_json::to_string_pretty(&openapi_document()).expect("openapi document serializes");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_is_valid_structure() {
        let doc = openapi_document();
        assert_eq!(doc["openapi"], OPENAPI_VERSION);
        assert_eq!(doc["info"]["version"], API_VERSION);
        let paths = doc["paths"].as_object().expect("paths object");
        // 12 条路由与 routes::create_router 一一对应
        assert_eq!(
            paths.len(),
            12,
            "路由清单须与 create_router 同步: {paths:?}"
        );
        for key in [
            "/healthz",
            "/readyz",
            "/metrics",
            "/api/v1/status",
            "/api/v1/status/circuit-breaker",
            "/api/v1/introspect",
            "/api/v1/config",
            "/api/v1/check/batch",
            "/api/v1/tokens/prefetch",
            "/api/v1/ban",
            "/api/v1/ban/{target}",
            "/api/v1/quota/{tenant_id}",
        ] {
            assert!(paths.contains_key(key), "缺少路由 {key}");
        }
    }

    #[test]
    fn local_refs_resolve() {
        // 所有内部 $ref 指向已定义 schema
        let doc = openapi_document();
        let text = serde_json::to_string(&doc).unwrap();
        let schemas = doc["components"]["schemas"].as_object().unwrap();
        const REF_PREFIX: &str = "#/components/schemas/";
        let mut search_from = 0;
        while let Some(at) = text[search_from..].find(REF_PREFIX) {
            let start = search_from + at + REF_PREFIX.len();
            let rest = &text[start..];
            let end = rest.find('"').expect("ref terminates with quote");
            let name = &rest[..end];
            assert!(
                name.is_empty() || schemas.contains_key(name),
                "悬空 $ref: {name}"
            );
            search_from = start;
        }
    }
}
