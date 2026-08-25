// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 决策链模块集成测试
//!
//! 测试决策链模块的完整功能

use limiteron::decision_chain::{ChainStats, DecisionChain, DecisionChainBuilder, DecisionNode};
use limiteron::error::Decision;
use limiteron::limiters::{Limiter, TokenBucketLimiter};
use std::sync::Arc;

#[tokio::test]
async fn empty_chain_returns_allowed() {
    let chain: DecisionChain = DecisionChain::builder().build();
    let result = chain.check().await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), Decision::Allowed(_)));
}

#[tokio::test]
async fn single_node_allowed() {
    let limiter = TokenBucketLimiter::new(100, 100);
    let node = DecisionNode::new("n1".to_string(), "node1".to_string(), Arc::new(limiter), 10);
    let chain = DecisionChain::builder().add_node(node).build();
    let result = chain.check().await;
    assert!(matches!(result.unwrap(), Decision::Allowed(_)));
}

#[tokio::test]
async fn single_node_rejected() {
    let limiter = TokenBucketLimiter::new(0, 0);
    let node = DecisionNode::new("n1".to_string(), "node1".to_string(), Arc::new(limiter), 10);
    let chain = DecisionChain::builder().add_node(node).build();
    let result = chain.check().await;
    assert!(matches!(result.unwrap(), Decision::Rejected(_)));
}

#[tokio::test]
async fn chain_runs_all_nodes_until_rejection() {
    let node1 = DecisionNode::new(
        "n1".to_string(),
        "node1".to_string(),
        Arc::new(TokenBucketLimiter::new(100, 100)),
        10,
    );
    let node2 = DecisionNode::new(
        "n2".to_string(),
        "node2".to_string(),
        Arc::new(TokenBucketLimiter::new(100, 100)),
        10,
    );
    let node3 = DecisionNode::new(
        "n3".to_string(),
        "node3".to_string(),
        Arc::new(TokenBucketLimiter::new(0, 0)),
        10,
    );

    let chain = DecisionChain::builder()
        .add_node(node1)
        .add_node(node2)
        .add_node(node3)
        .build();
    let _ = chain.check().await;
    // Node 3 rejects, so chain stops at node 3
}

#[tokio::test]
async fn disabled_node_is_skipped() {
    let mut node1 = DecisionNode::new(
        "n1".to_string(),
        "node1".to_string(),
        Arc::new(TokenBucketLimiter::new(100, 100)),
        10,
    );
    node1.enabled = false;
    let node2 = DecisionNode::new(
        "n2".to_string(),
        "node2".to_string(),
        Arc::new(TokenBucketLimiter::new(100, 100)),
        10,
    );

    let chain = DecisionChain::builder()
        .add_node(node1)
        .add_node(node2)
        .build();
    let result = chain.check().await;
    assert!(matches!(result.unwrap(), Decision::Allowed(_)));
}

#[tokio::test]
async fn chain_stats_tracked() {
    let limiter = TokenBucketLimiter::new(100, 100);
    let node = DecisionNode::new("n1".to_string(), "node1".to_string(), Arc::new(limiter), 10);
    let chain = DecisionChain::builder().add_node(node).build();

    for _ in 0..5 {
        chain.check().await.unwrap();
    }

    let stats = chain.stats().await;
    assert_eq!(stats.total_checks, 5);
    assert_eq!(stats.allowed_count, 5);
}

#[tokio::test]
async fn chain_stats_reset() {
    let limiter = TokenBucketLimiter::new(100, 100);
    let node = DecisionNode::new("n1".to_string(), "node1".to_string(), Arc::new(limiter), 10);
    let chain = DecisionChain::builder().add_node(node).build();

    chain.check().await.unwrap();
    chain.reset_stats().await;
    let stats = chain.stats().await;
    assert_eq!(stats.total_checks, 0);
}

#[tokio::test]
async fn node_count() {
    let node1 = DecisionNode::new(
        "n1".to_string(),
        "node1".to_string(),
        Arc::new(TokenBucketLimiter::new(100, 100)),
        10,
    );
    let node2 = DecisionNode::new(
        "n2".to_string(),
        "node2".to_string(),
        Arc::new(TokenBucketLimiter::new(100, 100)),
        20,
    );
    let chain = DecisionChain::builder()
        .add_node(node1)
        .add_node(node2)
        .build();
    assert_eq!(chain.node_count(), 2);
}

#[tokio::test]
async fn enabled_node_count() {
    let node1 = DecisionNode::new(
        "n1".to_string(),
        "node1".to_string(),
        Arc::new(TokenBucketLimiter::new(100, 100)),
        10,
    );
    let mut node2 = DecisionNode::new(
        "n2".to_string(),
        "node2".to_string(),
        Arc::new(TokenBucketLimiter::new(100, 100)),
        20,
    );
    node2.enabled = false;
    let chain = DecisionChain::builder()
        .add_node(node1)
        .add_node(node2)
        .build();
    assert_eq!(chain.enabled_node_count(), 1);
}

#[tokio::test]
async fn concurrent_checks() {
    let limiter = TokenBucketLimiter::new(100, 100);
    let node = DecisionNode::new("n1".to_string(), "node1".to_string(), Arc::new(limiter), 10);
    let chain = DecisionChain::builder().add_node(node).build();

    let mut handles = vec![];
    for _ in 0..20 {
        let c = chain.clone();
        handles.push(tokio::spawn(async move { c.check().await }));
    }
    for h in handles {
        let result = h.await.unwrap().unwrap();
        assert!(matches!(result, Decision::Allowed(_)));
    }
}

#[tokio::test]
async fn token_bucket_in_chain() {
    let limiter = TokenBucketLimiter::new(100, 0);
    let node = DecisionNode::new(
        "tb".to_string(),
        "token-bucket".to_string(),
        Arc::new(limiter),
        10,
    );
    let chain = DecisionChain::builder().add_node(node).build();
    let result = chain.check().await;
    assert!(matches!(result.unwrap(), Decision::Allowed(_)));
}

#[tokio::test]
async fn with_dependencies_shortcut() {
    let limiter = TokenBucketLimiter::new(100, 100);
    let node = DecisionNode::with_dependencies(
        "n1".to_string(),
        "node1".to_string(),
        Arc::new(limiter),
        10,
    );
    let chain = DecisionChain::with_dependencies(vec![node]);
    let result = chain.check().await;
    assert!(matches!(result.unwrap(), Decision::Allowed(_)));
}

#[tokio::test]
async fn node_builder_pattern() {
    let limiter = Arc::new(TokenBucketLimiter::new(100, 10));
    let node = DecisionNode::with_dependencies(
        "built_node".to_string(),
        "Built Node".to_string(),
        limiter,
        50,
    );

    assert_eq!(node.id, "built_node");
    assert_eq!(node.name, "Built Node");
    assert_eq!(node.priority, 50);
    assert!(node.enabled);
}

#[tokio::test]
async fn chain_builder_default() {
    let chain = DecisionChainBuilder::default().build();
    assert_eq!(chain.node_count(), 0);
}
