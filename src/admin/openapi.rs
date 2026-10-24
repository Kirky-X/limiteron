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

/// Path Item Object 的 operation 键（HTTP 方法）
const HTTP_METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

/// 为路径项内每个 operation 附加 summary/tags/responses
///
/// 写入目标是 operation 层而非 Path Item 层：Path Item 的合法键只有 HTTP 方法与
/// `$ref`/`summary`/`description`/`servers`/`parameters`，`tags`/`responses` 挂在
/// Path Item 上会让整份文档不符合 OpenAPI 3.0（且 Swagger UI/Redoc 的接口分组
/// 依赖 operation 级 `tags`）。
fn operation(summary: &str, tag: &str, mut path_item: serde_json::Value) -> serde_json::Value {
    let obj = path_item.as_object_mut().expect("path item object");
    for (method, op) in obj.iter_mut() {
        if !HTTP_METHODS.contains(&method.as_str()) {
            continue;
        }
        op["summary"] = serde_json::json!(summary);
        op["tags"] = serde_json::json!([tag]);
        if op.get("responses").is_none() {
            op["responses"] = serde_json::json!({});
        }
    }
    path_item
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
            "description": "Limiteron admin control plane: rate limit status, ban management, quota adjustment, circuit breaker status and rule hot reload. All endpoints except probes/metrics require a Bearer API key.",
        },
        "paths": {
            "/healthz": operation("Liveness probe", "probes", serde_json::json!({
                "get": {
                    "responses": {
                        "200": { "description": "Process is alive", "content": { "application/json": { "schema": { "type": "object", "properties": { "status": { "type": "string" } } } } } }
                    }
                }
            })),
            "/readyz": operation("Readiness probe (component-level aggregation)", "probes", serde_json::json!({
                "get": {
                    "responses": {
                        "200": { "description": "All components healthy" },
                        "503": { "description": "One or more components unhealthy; response body contains details" }
                    }
                }
            })),
            "/metrics": operation("Prometheus text-format metrics", "probes", serde_json::json!({
                "get": {
                    "responses": {
                        "200": { "description": "text/plain; version=0.0.4 exposition" }
                    }
                }
            })),
            "/api/v1/status": operation("Overall system status", "status", serde_json::json!({
                "get": {
                    "security": bearer,
                    "responses": {
                        "200": ok_json("System status", "#/components/schemas/SystemStatus"),
                        "401": error_response("Authentication failed")
                    }
                }
            })),
            "/api/v1/status/circuit-breaker": operation("Circuit breaker status", "status", serde_json::json!({
                "get": {
                    "security": bearer,
                    "responses": {
                        "200": ok_json("Circuit breaker status", "#/components/schemas/CircuitBreakerStatus"),
                        "503": error_response("circuit-breaker not configured")
                    }
                }
            })),
            "/api/v1/introspect": operation("Runtime introspection snapshot (rules, decision chains, bans and circuit breakers aggregated)", "status", serde_json::json!({
                "get": {
                    "security": bearer,
                    "responses": {
                        "200": { "description": "Introspection snapshot JSON (structure extends with the feature combination)",
                                 "content": { "application/json": { "schema": { "type": "object" } } } }
                    }
                }
            })),
            "/api/v1/config": operation("Rule hot reload (atomic config swap; previous config retained on failure)", "config", serde_json::json!({
                "post": {
                    "security": bearer,
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/FlowControlConfig" } } }
                    },
                    "responses": {
                        "200": { "description": "Application report", "content": { "application/json": { "schema": { "type": "object" } } } },
                        "400": error_response("Validation failed; previous config retained")
                    }
                }
            })),
            "/api/v1/check/batch": operation("Batch check (N keys decided in one request)", "decision", serde_json::json!({
                "post": {
                    "security": bearer,
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/BatchCheckBody" } } }
                    },
                    "responses": {
                        "200": { "description": "Per-item decision results", "content": { "application/json": { "schema": { "type": "object" } } } },
                        "400": error_response("Empty batch or above the 1000-item limit")
                    }
                }
            })),
            "/api/v1/tokens/prefetch": operation("Batch token prefetch", "decision", serde_json::json!({
                "post": {
                    "security": bearer,
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/TokenPrefetchBody" } } }
                    },
                    "responses": {
                        "200": { "description": "Per-item granted results and summary", "content": { "application/json": { "schema": { "type": "object" } } } },
                        "400": error_response("Empty batch or above the 1000-item limit")
                    }
                }
            })),
            "/api/v1/ban": operation("Create ban (ip/user/mac/geo)", "ban", serde_json::json!({
                "post": {
                    "security": bearer,
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CreateBanRequest" } } }
                    },
                    "responses": {
                        "201": ok_json("Ban details", "#/components/schemas/BanResponse"),
                        "400": error_response("Target or reason validation failed"),
                        "403": error_response("Operator not authorized"),
                        "500": error_response("Internal error"),
                        "503": error_response("ban-manager not configured")
                    }
                }
            })),
            "/api/v1/ban/{target}": operation("Remove ban (?type=ip|user|mac|geo|cidr)", "ban", serde_json::json!({
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
                        "200": ok_json("Removal result", "#/components/schemas/Unit"),
                        "400": error_response("Unsupported type or invalid target format"),
                        "403": error_response("Operator not authorized"),
                        "404": error_response("Ban not found"),
                        "500": error_response("Internal error"),
                        "503": error_response("ban-manager not configured")
                    }
                }
            })),
            "/api/v1/quota/{user_id}": operation("Reset a user's quota usage (new_limit=0); rejects per-tenant quota limit updates", "quota", serde_json::json!({
                "put": {
                    "security": bearer,
                    "parameters": [
                        { "name": "user_id", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/UpdateQuotaRequest" } } }
                    },
                    "responses": {
                        "200": ok_json("Operation result", "#/components/schemas/UpdateQuotaResponse"),
                        "400": error_response("Unsupported operation; user_id/resource must not contain ':'"),
                        "500": error_response("Internal error"),
                        "503": error_response("quota-control not configured")
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
                        "active_bans": { "type": "integer", "description": "Present when the ban-manager feature is enabled" },
                        "circuit_breaker": { "type": "string", "description": "Present when the circuit-breaker feature is enabled" }
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
                    "description": "Full FlowControlConfig (fields are defined in the config module; only the top-level skeleton is kept here)"
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
                    "description": "ip/user/mac use a string value; geo uses an object with a country_code field; cidr uses a string value",
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
                        "operator": { "type": "string", "nullable": true, "description": "Deprecated: ignored by the server; identity is determined by the API key mapping" },
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
                        "operator": { "type": "string", "nullable": true, "description": "Deprecated: ignored by the server" }
                    }
                },
                "UpdateQuotaRequest": {
                    "type": "object",
                    "required": ["resource", "new_limit"],
                    "properties": {
                        "resource": { "type": "string" },
                        "new_limit": { "type": "integer", "format": "int64", "description": "0 = reset signal" },
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

    /// OpenAPI 3.0 Path Item Object 的合法键（HTTP 方法与 Path Item 字段）
    const PATH_ITEM_KEYS: [&str; 13] = [
        "$ref",
        "summary",
        "description",
        "servers",
        "parameters",
        "get",
        "put",
        "post",
        "delete",
        "options",
        "head",
        "patch",
        "trace",
    ];

    /// 断言路径项只带合法键、operation 自带 summary/tags/responses
    ///
    /// `tags`/`responses` 是 operation 级概念：挂在 Path Item 上会让文档不符合
    /// OpenAPI 3.0（Path Item 的 `additionalProperties` 为 false），同时让
    /// Swagger UI/Redoc 丢失接口分组。
    fn assert_path_items_are_legal(paths: &serde_json::Map<String, serde_json::Value>) {
        assert!(!paths.is_empty(), "document must expose at least one path");
        for (path, path_item) in paths {
            let item = path_item.as_object().expect("path item object");
            for key in item.keys() {
                assert!(
                    PATH_ITEM_KEYS.contains(&key.as_str()) || key.starts_with("x-"),
                    "path item {path} carries illegal key {key}: Path Item Object allows only HTTP methods, $ref/summary/description/servers/parameters and x- extensions"
                );
            }
            for (method, op) in item {
                if !HTTP_METHODS.contains(&method.as_str()) {
                    continue;
                }
                let operation = op.as_object().expect("operation object");
                let summary = operation["summary"].as_str().unwrap_or_default();
                assert!(
                    !summary.is_empty(),
                    "operation {method} {path} carries no summary"
                );
                let tags = operation["tags"].as_array().expect("operation tags array");
                assert!(
                    !tags.is_empty(),
                    "operation {method} {path} carries no tags (interface grouping is lost)"
                );
                let responses = operation["responses"]
                    .as_object()
                    .expect("operation responses object");
                assert!(
                    !responses.is_empty(),
                    "operation {method} {path} declares no response code"
                );
            }
        }
    }

    #[test]
    fn path_items_carry_no_operation_level_keys() {
        let doc = openapi_document();
        let paths = doc["paths"].as_object().expect("paths object");
        assert_path_items_are_legal(paths);
    }

    #[test]
    fn committed_artifact_carry_no_operation_level_keys() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/openapi.json");
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read artifact {path}: {e}"));
        let doc: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("artifact is not valid JSON: {e}"));
        let paths = doc["paths"].as_object().expect("paths object");
        assert_path_items_are_legal(paths);
    }

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
            "route list must stay in sync with create_router: {paths:?}"
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
            "/api/v1/quota/{user_id}",
        ] {
            assert!(paths.contains_key(key), "missing route {key}");
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
                "dangling $ref: {name}"
            );
            search_from = start;
        }
    }
}
