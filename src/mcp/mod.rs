// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! MCP（Model Context Protocol）只读工具面
//!
//! 受 RBAC 约束的 stdio JSON-RPC 工具集。

pub mod protocol;
pub mod tools;

use crate::governor::Governor;
use crate::mcp::protocol::{JsonRpcRequest, JsonRpcResponse, ToolDescriptor, dispatch};
use crate::mcp::tools::{McpConfig, McpToolKind, ToolContext, call_tool};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// MCP stdio 服务器
pub struct McpServer {
    governor: Arc<Governor>,
    #[cfg(feature = "ban-manager")]
    ban_manager: Option<Arc<crate::BanManager>>,
    config: McpConfig,
}

impl McpServer {
    /// 构造服务器
    pub fn new(governor: Arc<Governor>, config: McpConfig) -> Self {
        Self {
            governor,
            #[cfg(feature = "ban-manager")]
            ban_manager: None,
            config,
        }
    }

    /// 注入封禁管理器（ban-manager feature）
    #[cfg(feature = "ban-manager")]
    pub fn with_ban_manager(mut self, manager: Arc<crate::BanManager>) -> Self {
        self.ban_manager = Some(manager);
        self
    }

    fn tool_descriptors(&self) -> Vec<ToolDescriptor> {
        self.config
            .enabled_tools
            .iter()
            .map(|kind| {
                let (description, schema) = match kind {
                    McpToolKind::Check => (
                        "Hypothetical rate-limit check without consuming quota",
                        serde_json::json!({"type":"object","properties":{"key":{"type":"string"},"cost":{"type":"integer"}},"required":["key"]}),
                    ),
                    McpToolKind::BanList => (
                        "Read-only ban list with redacted identifiers",
                        serde_json::json!({"type":"object","properties":{"limit":{"type":"integer"}}}),
                    ),
                    McpToolKind::Stats => (
                        "Per-rule decision statistics snapshot",
                        serde_json::json!({"type":"object","properties":{"rule_name":{"type":"string"}}}),
                    ),
                };
                ToolDescriptor {
                    name: kind.name().to_string(),
                    description: description.to_string(),
                    input_schema: schema,
                }
            })
            .collect()
    }

    fn context(&self) -> ToolContext {
        ToolContext {
            governor: self.governor.clone(),
            #[cfg(feature = "ban-manager")]
            ban_manager: self.ban_manager.clone(),
        }
    }

    /// 在给定输入输出流上运行行分隔 JSON-RPC 事件循环
    pub async fn serve<R, W>(&self, reader: R, mut writer: W) -> Result<(), crate::LimiteronError>
    where
        R: tokio::io::AsyncRead + Unpin,
        W: tokio::io::AsyncWrite + Unpin,
    {
        let mut lines = BufReader::new(reader).lines();
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(crate::LimiteronError::IoError)?
        {
            if line.trim().is_empty() {
                continue;
            }
            let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
                Ok(req) => self.dispatch_async(&req).await,
                Err(_) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: None,
                    result: None,
                    error: Some(protocol::JsonRpcError {
                        code: protocol::INVALID_REQUEST,
                        message: "Invalid JSON-RPC request".to_string(),
                    }),
                },
            };
            let mut out = serde_json::to_string(&response).unwrap_or_default();
            out.push('\n');
            writer
                .write_all(out.as_bytes())
                .await
                .map_err(crate::LimiteronError::IoError)?;
            writer
                .flush()
                .await
                .map_err(crate::LimiteronError::IoError)?;
        }
        Ok(())
    }

    /// 异步分派：initialize / tools/list 走同步 dispatch，tools/call 走异步工具层
    async fn dispatch_async(&self, req: &JsonRpcRequest) -> JsonRpcResponse {
        if req.method == "tools/call" {
            let name = req
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = req
                .params
                .get("arguments")
                .unwrap_or(&serde_json::Value::Null);
            let role = args.get("_role").and_then(|v| v.as_str());
            let ctx = self.context();
            let result = call_tool(&self.config, &ctx, name, args, role).await;
            let (value, err) = match result {
                tools::ToolResult::Ok(v) => (Some(v), None),
                tools::ToolResult::Err { code, message } => {
                    (None, Some(protocol::JsonRpcError { code, message }))
                }
            };
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id.clone(),
                result: value,
                error: err,
            }
        } else {
            let tools = self.tool_descriptors();
            dispatch(req, &tools, |_, _| {
                unreachable!("only tools/call uses handler")
            })
        }
    }

    /// 从 stdin/stdout 运行（生产入口）
    pub async fn serve_stdio(&self) -> Result<(), crate::LimiteronError> {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        self.serve(stdin, stdout).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    async fn make_server() -> McpServer {
        let cfg: crate::config::FlowControlConfig = serde_json::from_str(
            r#"{"version":"1.0.0","global":{"storage":"memory","cache":"memory","metrics":"none"},"rules":[{"id":"r1","name":"r","priority":100,"matchers":[{"type":"User","user_ids":["*"]}],"limiters":[{"type":"TokenBucket","capacity":10,"refill_rate":1}],"action":{"on_exceed":"reject"}}]}"#,
        )
        .unwrap();
        let governor = Arc::new(
            crate::Governor::builder()
                .with_config(cfg)
                .with_storage(Arc::new(crate::storage::MemoryStorage::new()))
                .with_ban_storage(Arc::new(crate::storage::MemoryBanStorage::new()))
                .build()
                .await
                .expect("governor builds"),
        );
        let config = McpConfig {
            authorized_roles: vec!["mcp-admin".to_string()],
            ..Default::default()
        };
        let server = McpServer::new(governor, config);
        #[cfg(feature = "ban-manager")]
        let server = server.with_ban_manager(Arc::new(crate::BanManager::new().await.unwrap()));
        server
    }

    fn rpc(id: u64, method: &str, params: Value) -> String {
        serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params
        })
        .to_string()
    }

    #[tokio::test]
    async fn duplex_three_step_round_trip() {
        let server = make_server().await;
        let (client_read, server_write) = tokio::io::duplex(64 * 1024);
        let (server_read, mut client_write) = tokio::io::duplex(64 * 1024);

        let handle = tokio::spawn(async move {
            server.serve(server_read, server_write).await.unwrap();
        });

        let input = format!(
            "{}\n{}\n{}\n",
            rpc(1, "initialize", Value::Null),
            rpc(2, "tools/list", Value::Null),
            rpc(
                3,
                "tools/call",
                serde_json::json!({
                    "name": "limiteron_stats",
                    "arguments": { "_role": "mcp-admin" }
                })
            ),
        );
        client_write.write_all(input.as_bytes()).await.unwrap();
        client_write.shutdown().await.unwrap();

        use tokio::io::AsyncBufReadExt;
        let mut reader = BufReader::new(client_read);
        let mut responses = Vec::new();
        let mut line = String::new();
        while reader.read_line(&mut line).await.unwrap() > 0 {
            responses.push(serde_json::from_str::<Value>(line.trim()).unwrap());
            line.clear();
        }
        handle.await.unwrap();

        assert_eq!(responses.len(), 3);
        assert_eq!(
            responses[0]["result"]["serverInfo"]["name"],
            serde_json::json!("limiteron")
        );
        assert_eq!(responses[1]["result"]["tools"].as_array().unwrap().len(), 3);
        assert!(
            responses[2]["result"].is_array(),
            "stats result: {}",
            responses[2]
        );
    }
}
