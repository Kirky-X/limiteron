// Copyright (c) 2025-2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 组合根装配示例：dbnexus 权限链路（消费方）只依赖 `dbnexus-limiter-port`
//! 端口；应用在组合根把 limiteron 引擎（经本适配器）注入端口。
//!
//! 运行：`cargo run -p limiteron-dbnexus --example dbnexus_port_basic`

use std::sync::Arc;

use dbnexus_limiter_port::Limiter;
use limiteron_dbnexus::DbnexusLimiter;

/// 消费方同款形态（对照 dbnexus 的 PermissionContext）：只持有端口，不感知引擎
struct PermissionGate {
    limiter: Arc<dyn Limiter>,
}

impl PermissionGate {
    async fn enter(&self, role: &str) -> String {
        match self.limiter.check(role).await {
            Ok(decision) if decision.allowed => "allowed".to_string(),
            Ok(decision) => match decision.retry_after {
                Some(retry_after) => format!("rate-limited (retry after {retry_after:?})"),
                None => "rate-limited".to_string(),
            },
            // fail-closed 属消费方策略；此处仅演示错误面
            Err(err) => format!("backend error: {err}"),
        }
    }
}

#[tokio::main]
async fn main() {
    // 组合根：limiteron per-key 令牌桶 → 端口对象 → 消费方
    let gate = PermissionGate {
        limiter: Arc::new(DbnexusLimiter::new(3, 1)),
    };

    for i in 0..5 {
        let verdict = gate.enter("role-a").await;
        println!("check #{i} for role-a: {verdict}");
    }

    // 独立配额：换 key 立即恢复
    println!("check for role-b: {}", gate.enter("role-b").await);
}
