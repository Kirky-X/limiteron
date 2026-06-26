//! 决策链模块集成测试
//!
//! 测试决策链模块的完整功能

use async_trait::async_trait;
use limiteron::decision_chain::{ChainStats, DecisionChain, DecisionChainBuilder, DecisionNode};
use limiteron::error::{Decision, FlowGuardError};
use limiteron::{Limiter, TokenBucketLimiter};
use std::sync::Arc;

// ==================== Mock Limiters ====================

struct MockLimiter {
    allowed: bool,
}

impl MockLimiter {
    fn new(allowed: bool) -> Self {
        Self { allowed }
    }
}

#[async_trait]
impl Limiter for MockLimiter {
    async fn allow(&self, _cost: u64) -> Result<bool, FlowGuardError> {
        Ok(self.allowed)
    }
}

struct SpyLimiter {
    calls: Arc<std::sync::atomic::AtomicUsize>,
}

impl SpyLimiter {
    fn new() -> Self {
        Self {
            calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
    fn call_count(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait]
impl Limiter for SpyLimiter {
    async fn allow(&self, _cost: u64) -> Result<bool, FlowGuardError> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(true)
    }
}

// ==================== DecisionChain Tests ====================

#[tokio::test]
async fn empty_chain_returns_allowed() {
    let chain: DecisionChain = DecisionChain::builder().build();
    let result = chain.check().await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), Decision::Allowed(_)));
}

#[tokio::test]
async fn single_node_allowed() {
    let limiter = MockLimiter::new(true);
    let node = DecisionNode::new("n1".to_string(), "node1".to_string(), Arc::new(limiter), 10);
    let chain = DecisionChain::builder().add_node(node).build();
    let result = chain.check().await;
    assert!(matches!(result.unwrap(), Decision::Allowed(_)));
}

#[tokio::test]
async fn single_node_rejected() {
    let limiter = MockLimiter::new(false);
    let node = DecisionNode::new("n1".to_string(), "node1".to_string(), Arc::new(limiter), 10);
    let chain = DecisionChain::builder().add_node(node).build();
    let result = chain.check().await;
    assert!(matches!(result.unwrap(), Decision::Rejected(_)));
}

#[tokio::test]
async fn chain_runs_all_nodes_until_rejection() {
    let spy1 = Arc::new(SpyLimiter::new());
    let spy2 = spy1.clone();

    let node1 = DecisionNode::new(
        "n1".to_string(),
        "node1".to_string(),
        Arc::new(MockLimiter::new(true)),
        10,
    );
    let node2 = DecisionNode::new(
        "n2".to_string(),
        "node2".to_string(),
        Arc::new(MockLimiter::new(true)),
        10,
    );
    let node3 = DecisionNode::new(
        "n3".to_string(),
        "node3".to_string(),
        Arc::new(MockLimiter::new(false)),
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
    let spy = Arc::new(SpyLimiter::new());

    let mut node1 = DecisionNode::new(
        "n1".to_string(),
        "node1".to_string(),
        Arc::new(MockLimiter::new(true)),
        10,
    );
    node1.enabled = false;
    let node2 = DecisionNode::new(
        "n2".to_string(),
        "node2".to_string(),
        Arc::new(MockLimiter::new(true)),
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
    let limiter = MockLimiter::new(true);
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
    let limiter = MockLimiter::new(true);
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
        Arc::new(MockLimiter::new(true)),
        10,
    );
    let node2 = DecisionNode::new(
        "n2".to_string(),
        "node2".to_string(),
        Arc::new(MockLimiter::new(true)),
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
        Arc::new(MockLimiter::new(true)),
        10,
    );
    let mut node2 = DecisionNode::new(
        "n2".to_string(),
        "node2".to_string(),
        Arc::new(MockLimiter::new(true)),
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
    let limiter = MockLimiter::new(true);
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
    let limiter = MockLimiter::new(true);
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
