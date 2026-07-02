//! Decision Chain 示例
//!
//! 演示责任链模式的决策链：组合多个限流器，按优先级执行，支持短路。
//!
//! # 涵盖 API
//!
//! - `DecisionNode` 构造（`with_dependencies`、`with_short_circuit`、`with_cost`）
//! - `DecisionChainBuilder` 构建器（`add_node`、`build`）
//! - `DecisionChain::check().await` 执行检查
//! - `DecisionChain::stats().await` 获取统计
//! - `ChainStats` 字段（`total_checks`、`allowed_count`、`node_rejections` 等）
//! - `enable_node` / `disable_node` / `set_short_circuit` 动态控制
//!
//! # 运行方式
//!
//! ```bash
//! cargo run --bin decision_chain
//! ```

use limiteron::decision_chain::{DecisionChain, DecisionNode};
use limiteron::limiters::TokenBucketLimiter;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Decision Chain Demo ===\n");

    demo_basic_chain().await?;
    demo_short_circuit().await?;
    demo_dynamic_control().await?;
    demo_chain_stats().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

/// 演示基本决策链：组合多个限流器节点
async fn demo_basic_chain() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 1. Basic Decision Chain ---\n");

    // 节点 1：令牌桶限流（容量 10，每秒补充 2）
    let node1 = DecisionNode::with_dependencies(
        "token-bucket".to_string(),
        "Token Bucket Limiter".to_string(),
        Arc::new(TokenBucketLimiter::new(10, 2)),
        100,
    );

    // 节点 2：另一个令牌桶限流（容量 5，每秒补充 1）
    let node2 = DecisionNode::with_dependencies(
        "strict-bucket".to_string(),
        "Strict Token Bucket".to_string(),
        Arc::new(TokenBucketLimiter::new(5, 1)),
        50,
    );

    let chain = DecisionChain::builder()
        .add_node(node1)
        .add_node(node2)
        .build();

    println!("  Chain has {} nodes", chain.node_count());
    println!("  Enabled nodes: {}", chain.enabled_node_count());

    // 执行检查
    for i in 0..6 {
        let decision = chain.check().await?;
        let result = match &decision {
            limiteron::Decision::Allowed(_) => "✅ Allowed",
            limiteron::Decision::Rejected(m) => {
                println!("  Request {}: ❌ Rejected (reason={})", i, m.reason);
                continue;
            }
            limiteron::Decision::Banned(_) => "🚫 Banned",
        };
        println!("  Request {}: {}", i, result);
    }
    println!();
    Ok(())
}

/// 演示短路行为：节点拒绝时立即返回，不执行后续节点
async fn demo_short_circuit() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 2. Short-circuit Behavior ---\n");

    // 第一个节点容量为 2，短路启用
    let strict_node = DecisionNode::with_dependencies(
        "strict".to_string(),
        "Strict Limiter (capacity=2)".to_string(),
        Arc::new(TokenBucketLimiter::new(2, 1)),
        100,
    )
    .with_short_circuit(true);

    // 第二个节点容量为 100（应该不会被触发）
    let loose_node = DecisionNode::with_dependencies(
        "loose".to_string(),
        "Loose Limiter (capacity=100)".to_string(),
        Arc::new(TokenBucketLimiter::new(100, 10)),
        50,
    )
    .with_short_circuit(false);

    let chain = DecisionChain::builder()
        .add_node(strict_node)
        .add_node(loose_node)
        .build();

    println!("  Sending 4 requests (strict node capacity=2, short_circuit=true):");
    for i in 0..4 {
        let decision = chain.check().await?;
        match &decision {
            limiteron::Decision::Allowed(_) => println!("  Request {}: ✅ Allowed", i),
            limiteron::Decision::Rejected(m) => {
                println!("  Request {}: ❌ Rejected ({} - short-circuited)", i, m.reason);
            }
            limiteron::Decision::Banned(_) => println!("  Request {}: 🚫 Banned", i),
        }
    }

    let stats = chain.stats().await;
    println!(
        "\n  Stats: total={}, allowed={}, rejected={}",
        stats.total_checks, stats.allowed_count, stats.rejected_count
    );
    println!("  Node rejections:");
    for (node_id, count) in &stats.node_rejections {
        println!("    - {}: {}", node_id, count);
    }
    println!();
    Ok(())
}

/// 演示动态控制：运行时启用/禁用节点，调整短路行为
async fn demo_dynamic_control() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 3. Dynamic Node Control ---\n");

    let strict_node = DecisionNode::with_dependencies(
        "strict".to_string(),
        "Strict Limiter".to_string(),
        Arc::new(TokenBucketLimiter::new(1, 1)),
        100,
    );
    let loose_node = DecisionNode::with_dependencies(
        "loose".to_string(),
        "Loose Limiter".to_string(),
        Arc::new(TokenBucketLimiter::new(100, 10)),
        50,
    );

    let mut chain = DecisionChain::with_dependencies(vec![strict_node, loose_node]);

    println!("  Initial: {} nodes, {} enabled", chain.node_count(), chain.enabled_node_count());

    // 禁用 strict 节点
    let disabled = chain.disable_node("strict");
    println!("\n  Disabled 'strict' node: success={}", disabled);
    println!("  Enabled nodes: {}", chain.enabled_node_count());

    // 检查应该全部通过（因为只剩 loose 节点）
    for i in 0..3 {
        let decision = chain.check().await?;
        match &decision {
            limiteron::Decision::Allowed(_) => println!("  Request {}: ✅ Allowed", i),
            limiteron::Decision::Rejected(_) => println!("  Request {}: ❌ Rejected", i),
            limiteron::Decision::Banned(_) => println!("  Request {}: 🚫 Banned", i),
        }
    }

    // 重新启用 strict 节点
    let enabled = chain.enable_node("strict");
    println!("\n  Re-enabled 'strict' node: success={}", enabled);

    // 关闭 strict 节点的短路行为
    let short_circuit_set = chain.set_short_circuit("strict", false);
    println!("  Set 'strict' short_circuit=false: success={}", short_circuit_set);

    let stats = chain.stats_sync();
    println!("\n  Stats: total_checks={}", stats.total_checks);
    println!();
    Ok(())
}

/// 演示 ChainStats 统计信息
async fn demo_chain_stats() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 4. ChainStats ---\n");

    let node1 = DecisionNode::with_dependencies(
        "node-a".to_string(),
        "Limiter A".to_string(),
        Arc::new(TokenBucketLimiter::new(3, 1)),
        100,
    )
    .with_cost(1);

    let node2 = DecisionNode::with_dependencies(
        "node-b".to_string(),
        "Limiter B".to_string(),
        Arc::new(TokenBucketLimiter::new(2, 1)),
        50,
    )
    .with_cost(1);

    let chain = DecisionChain::builder().add_node(node1).add_node(node2).build();

    // 发送 5 个请求
    for _ in 0..5 {
        let _ = chain.check().await?;
    }

    let stats = chain.stats().await;
    println!("  Total checks:   {}", stats.total_checks);
    println!("  Allowed count:  {}", stats.allowed_count);
    println!("  Rejected count: {}", stats.rejected_count);
    println!("  Error count:    {}", stats.error_count);
    println!("  Node rejections:");
    for (node_id, count) in &stats.node_rejections {
        println!("    - {}: {}", node_id, count);
    }

    // 重置统计
    chain.reset_stats().await;
    let after = chain.stats().await;
    println!("\n  After reset: total_checks={}", after.total_checks);
    println!();
    Ok(())
}
