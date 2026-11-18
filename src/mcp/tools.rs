// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! MCP 只读工具集（feature `mcp`）
//!
//! 三工具：`limiteron_check`（假设性检查，零消费）/ `limiteron_ban_list`（脱敏封禁列表）
//! / `limiteron_stats`（规则统计）。授权在工具层强制：无授权角色返回 `-32001`。

use crate::governor::{Governor, NodePeekStatus, RuleStatsSnapshot};
use crate::mcp::protocol::UNAUTHORIZED;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

/// 工具种类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpToolKind {
    /// 假设性检查
    Check,
    /// 封禁列表
    BanList,
    /// 规则统计
    Stats,
}

impl McpToolKind {
    /// 工具名
    pub fn name(&self) -> &'static str {
        match self {
            McpToolKind::Check => "limiteron_check",
            McpToolKind::BanList => "limiteron_ban_list",
            McpToolKind::Stats => "limiteron_stats",
        }
    }
}

/// MCP 工具配置
#[derive(Debug, Clone)]
pub struct McpConfig {
    /// 允许调用工具的角色（工具层强制）
    pub authorized_roles: Vec<String>,
    /// 启用的工具集
    pub enabled_tools: Vec<McpToolKind>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            authorized_roles: Vec::new(), // 默认无角色授权（fail-closed）
            enabled_tools: vec![McpToolKind::Check, McpToolKind::BanList, McpToolKind::Stats],
        }
    }
}

/// 工具执行上下文
pub struct ToolContext {
    /// Governor（只读查询）
    pub governor: Arc<Governor>,
    /// 封禁管理器（可选，ban-manager feature 下装配）
    #[cfg(feature = "ban-manager")]
    pub ban_manager: Option<Arc<crate::BanManager>>,
}

/// 工具调用结果（JSON-RPC result 载荷）
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ToolResult {
    /// 成功载荷
    Ok(Value),
    /// 授权/执行失败（含 JSON-RPC code）
    Err { code: i32, message: String },
}

/// 标识脱敏：仅保留哈希前缀，绝不回显明文 key
fn redact(raw: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut hasher);
    format!("fp:{:016x}", hasher.finish())
}

fn authorize(config: &McpConfig, role: Option<&str>) -> Result<(), (i32, String)> {
    let permitted = role.is_some_and(|r| config.authorized_roles.iter().any(|x| x == r));
    if permitted {
        Ok(())
    } else {
        Err((UNAUTHORIZED, "mcp-unauthorized".to_string()))
    }
}

/// 调用一个工具
///
/// `role` 为调用方角色（经外部认证/授权链解析后传入）；工具层再次强制校验。
pub async fn call_tool(
    config: &McpConfig,
    ctx: &ToolContext,
    name: &str,
    args: &Value,
    role: Option<&str>,
) -> ToolResult {
    let kind = match name {
        "limiteron_check" => McpToolKind::Check,
        "limiteron_ban_list" => McpToolKind::BanList,
        "limiteron_stats" => McpToolKind::Stats,
        _ => {
            return ToolResult::Err {
                code: crate::mcp::protocol::METHOD_NOT_FOUND,
                message: format!("Unknown tool: {name}"),
            };
        }
    };
    if !config.enabled_tools.contains(&kind) {
        return ToolResult::Err {
            code: crate::mcp::protocol::METHOD_NOT_FOUND,
            message: format!("Tool disabled: {name}"),
        };
    }
    if let Err((code, msg)) = authorize(config, role) {
        return ToolResult::Err { code, message: msg };
    }

    match kind {
        McpToolKind::Check => tool_check(ctx, args).await,
        McpToolKind::BanList => tool_ban_list(ctx, args).await,
        McpToolKind::Stats => tool_stats(ctx, args).await,
    }
}

async fn tool_check(ctx: &ToolContext, args: &Value) -> ToolResult {
    use crate::matchers::traits::RequestContext;
    let mut request = RequestContext::new();
    let ip = args
        .get("key")
        .and_then(|v| v.as_str())
        .unwrap_or("127.0.0.1");
    request.client_ip = Some(ip.to_string());
    request.ip = Some(ip.to_string());
    request.path = "/".to_string();
    request.method = "GET".to_string();
    let cost = args.get("cost").and_then(|v| v.as_u64()).unwrap_or(1);

    match ctx.governor.explain_check(&request).await {
        Ok(explanation) => {
            let mut value = serde_json::to_value(&explanation).unwrap_or(Value::Null);
            // 脱敏：dimension 中可能含租户+key，回显指纹
            if let Some(dim) = explanation.dimension.as_deref() {
                value["dimension"] = Value::String(redact(dim));
            }
            // peek 判定可行性摘要（remaining >= cost）
            let feasible = explanation.rules.iter().any(|r| {
                r.nodes.iter().any(|n| match &n.status {
                    NodePeekStatus::Remaining(s) => s.allows(cost),
                    _ => false,
                })
            });
            value["feasible"] = Value::Bool(feasible);
            ToolResult::Ok(value)
        }
        Err(e) => ToolResult::Err {
            code: crate::mcp::protocol::INVALID_REQUEST,
            message: redact(&e.to_string()),
        },
    }
}

async fn tool_ban_list(ctx: &ToolContext, args: &Value) -> ToolResult {
    #[cfg(feature = "ban-manager")]
    {
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(100)
            .min(1000);
        let filter = crate::ban::types::BanFilter {
            target_type: args
                .get("target_type")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            active_only: true,
            limit: Some(limit),
            ..Default::default()
        };
        if let Some(ref manager) = ctx.ban_manager {
            match manager.list_bans(filter).await {
                Ok(list) => {
                    // 目标标识脱敏：仅回显类型 + 指纹
                    let redacted: Vec<Value> = list
                        .iter()
                        .map(|b| {
                            serde_json::json!({
                                "target": redact(&format!("{:?}", b.target)),
                                "reason_hash": redact(&b.reason),
                            })
                        })
                        .collect();
                    ToolResult::Ok(Value::Array(redacted))
                }
                Err(e) => ToolResult::Err {
                    code: crate::mcp::protocol::INVALID_REQUEST,
                    message: redact(&e.to_string()),
                },
            }
        } else {
            ToolResult::Ok(Value::Array(vec![]))
        }
    }
    #[cfg(not(feature = "ban-manager"))]
    {
        let _ = (ctx, args);
        ToolResult::Ok(Value::Array(vec![]))
    }
}

async fn tool_stats(ctx: &ToolContext, args: &Value) -> ToolResult {
    let filter = args
        .get("rule_name")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let stats: Vec<RuleStatsSnapshot> = ctx
        .governor
        .rule_stats()
        .await
        .into_iter()
        .filter(|s| filter.as_ref().is_none_or(|f| &s.rule_id == f))
        .collect();
    ToolResult::Ok(serde_json::to_value(stats).unwrap_or(Value::Null))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(role: &str) -> McpConfig {
        McpConfig {
            authorized_roles: vec![role.to_string()],
            ..Default::default()
        }
    }

    #[test]
    fn unauthorized_all_tools_return_minus_32001() {
        let config = config("mcp-admin");
        // 无 role → 授权失败（无需真实 ctx，授权在工具执行前）
        for name in ["limiteron_check", "limiteron_ban_list", "limiteron_stats"] {
            match authorize(&config, None) {
                Err((code, _)) => assert_eq!(code, UNAUTHORIZED),
                other => panic!("expected unauthorized, got {:?}", other),
            }
            let _ = name;
        }
        assert!(authorize(&config, Some("mcp-admin")).is_ok());
        assert!(authorize(&config, Some("other")).is_err());
    }

    #[test]
    fn redaction_never_echoes_plaintext() {
        let secret = "super-secret-api-key-32chars!!";
        let r = redact(secret);
        assert!(!r.contains(secret));
        assert!(r.starts_with("fp:"));
    }

    #[tokio::test]
    async fn call_tool_stats_and_unauthorized() {
        use crate::config::FlowControlConfig;

        let cfg: FlowControlConfig = serde_json::from_str(
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
        #[cfg(feature = "ban-manager")]
        let ctx = ToolContext {
            governor,
            ban_manager: Some(Arc::new(crate::BanManager::new().await.unwrap())),
        };
        #[cfg(not(feature = "ban-manager"))]
        let ctx = ToolContext { governor };

        // 无授权角色 → -32001
        let deny = call_tool(
            &config("mcp-admin"),
            &ctx,
            "limiteron_stats",
            &Value::Null,
            None,
        )
        .await;
        match deny {
            ToolResult::Err { code, .. } => assert_eq!(code, UNAUTHORIZED),
            other => panic!("expected unauthorized, got {:?}", other),
        }

        // 授权后 stats 返回成功载荷
        let ok = call_tool(
            &config("mcp-admin"),
            &ctx,
            "limiteron_stats",
            &Value::Null,
            Some("mcp-admin"),
        )
        .await;
        match ok {
            ToolResult::Ok(v) => assert!(v.is_array()),
            other => panic!("expected Ok, got {:?}", other),
        }

        // 授权后 check 零消费检查返回 feasible 字段
        let check = call_tool(
            &config("mcp-admin"),
            &ctx,
            "limiteron_check",
            &serde_json::json!({"key": "10.0.0.9", "cost": 1}),
            Some("mcp-admin"),
        )
        .await;
        match check {
            ToolResult::Ok(v) => assert!(v.get("feasible").is_some(), "result: {v}"),
            other => panic!("expected Ok, got {:?}", other),
        }
    }

    #[test]
    fn tool_kind_names() {
        assert_eq!(McpToolKind::Check.name(), "limiteron_check");
        assert_eq!(McpToolKind::BanList.name(), "limiteron_ban_list");
        assert_eq!(McpToolKind::Stats.name(), "limiteron_stats");
    }
}
