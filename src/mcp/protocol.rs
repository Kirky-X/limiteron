// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! MCP JSON-RPC 2.0 最小协议面（serde_json 手工实现，不引入 MCP SDK）
//!
//! 行分隔 JSON-RPC over stdio：方法集 `initialize` / `tools/list` / `tools/call`，
//! 未知方法返回 `-32601`（Method not found）。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcRequest {
    /// 协议版本（必须为 "2.0"）
    pub jsonrpc: String,
    /// 请求 ID（通知可为 None）
    pub id: Option<Value>,
    /// 方法名
    pub method: String,
    /// 参数对象
    #[serde(default)]
    pub params: Value,
}

/// JSON-RPC 2.0 错误
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// 错误码
    pub code: i32,
    /// 错误消息
    pub message: String,
}

/// JSON-RPC 2.0 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// 协议版本
    pub jsonrpc: String,
    /// 请求 ID
    pub id: Option<Value>,
    /// 成功结果
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// 失败错误
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// 工具描述（`tools/list` 返回项）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// 工具名
    pub name: String,
    /// 描述
    pub description: String,
    /// JSON Schema（inputSchema）
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// 错误码：方法不存在
pub const METHOD_NOT_FOUND: i32 = -32601;
/// 错误码：请求无效
pub const INVALID_REQUEST: i32 = -32600;
/// 错误码：工具未授权（MCP 扩展）
pub const UNAUTHORIZED: i32 = -32001;

fn ok(id: Option<Value>, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(result),
        error: None,
    }
}

fn err(id: Option<Value>, code: i32, message: impl Into<String>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.into(),
        }),
    }
}

/// 分派一个 JSON-RPC 请求
///
/// `tools` 为 `tools/list` 的工具清单；`call` 为 `tools/call` 的处理器回调，
/// 接收工具名与 params，返回 `Ok(Value)` 或 `Err((code, message))`。
pub fn dispatch<F>(req: &JsonRpcRequest, tools: &[ToolDescriptor], call: F) -> JsonRpcResponse
where
    F: FnOnce(&str, &Value) -> Result<Value, (i32, String)>,
{
    if req.jsonrpc != "2.0" {
        return err(req.id.clone(), INVALID_REQUEST, "jsonrpc must be \"2.0\"");
    }
    match req.method.as_str() {
        "initialize" => ok(
            req.id.clone(),
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": { "name": "limiteron", "version": env!("CARGO_PKG_VERSION") },
                "capabilities": { "tools": {} }
            }),
        ),
        "tools/list" => ok(req.id.clone(), serde_json::json!({ "tools": tools })),
        "tools/call" => {
            let name = req
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = req.params.get("arguments").unwrap_or(&Value::Null);
            match call(name, args) {
                Ok(value) => ok(req.id.clone(), value),
                Err((code, msg)) => err(req.id.clone(), code, msg),
            }
        }
        other => err(
            req.id.clone(),
            METHOD_NOT_FOUND,
            format!("Method not found: {other}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(method: &str, params: Value) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(1)),
            method: method.to_string(),
            params,
        }
    }

    #[test]
    fn initialize_round_trip() {
        let resp = dispatch(&req("initialize", Value::Null), &[], |_, _| unreachable!());
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["serverInfo"]["name"], serde_json::json!("limiteron"));
        assert_eq!(result["protocolVersion"], serde_json::json!("2024-11-05"));
    }

    #[test]
    fn tools_list_returns_enabled_tools() {
        let tools = vec![ToolDescriptor {
            name: "limiteron_stats".to_string(),
            description: "stats".to_string(),
            input_schema: serde_json::json!({ "type": "object" }),
        }];
        let resp = dispatch(
            &req("tools/list", Value::Null),
            &tools,
            |_, _| unreachable!(),
        );
        let listed = resp.result.unwrap()["tools"].as_array().unwrap().clone();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["name"], serde_json::json!("limiteron_stats"));
        assert!(listed[0]["inputSchema"].is_object());
    }

    #[test]
    fn tools_call_delegates_handler() {
        let params =
            serde_json::json!({ "name": "limiteron_stats", "arguments": { "rule_name": "api" } });
        let resp = dispatch(&req("tools/call", params), &[], |name, args| {
            assert_eq!(name, "limiteron_stats");
            assert_eq!(args["rule_name"], serde_json::json!("api"));
            Ok(serde_json::json!({ "ok": true }))
        });
        assert_eq!(resp.result.unwrap()["ok"], serde_json::json!(true));
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        let resp = dispatch(&req("frobnicate", Value::Null), &[], |_, _| unreachable!());
        assert_eq!(resp.error.unwrap().code, METHOD_NOT_FOUND);
    }

    #[test]
    fn invalid_protocol_returns_invalid_request() {
        let mut bad = req("initialize", Value::Null);
        bad.jsonrpc = "1.0".to_string();
        let resp = dispatch(&bad, &[], |_, _| unreachable!());
        assert_eq!(resp.error.unwrap().code, INVALID_REQUEST);
    }
}
