// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 决策链编排测试（自 tests/e2e_advanced.rs 下沉）
//!
//! 下沉理由：进程内编排验证（无真实后端链路），调用计数与错误注入替身
//! 属单元层正当场景；e2e 面禁 mock。断言与原用例一致。

mod decision_chain {
    use limiteron::decision_chain::{DecisionChain, DecisionChainBuilder, DecisionNode};
    use limiteron::error::{Decision, LimiteronError};
    use limiteron::limiters::Limiter;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    /// 自定义 Mock 限流器，可控制 allow 返回值
    struct MockLimiter {
        allowed: Arc<AtomicBool>,
        calls: Arc<AtomicUsize>,
    }
    impl MockLimiter {
        fn new(allowed: bool) -> (Self, Arc<AtomicBool>, Arc<AtomicUsize>) {
            let allowed = Arc::new(AtomicBool::new(allowed));
            let calls = Arc::new(AtomicUsize::new(0));
            (
                Self {
                    allowed: allowed.clone(),
                    calls: calls.clone(),
                },
                allowed,
                calls,
            )
        }
    }
    #[async_trait::async_trait]
    impl Limiter for MockLimiter {
        async fn allow(&self, _cost: u64) -> Result<bool, LimiteronError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.allowed.load(Ordering::SeqCst))
        }
    }

    /// 空链应返回 Allowed
    #[tokio::test]
    async fn empty_chain_returns_allowed() {
        let chain = DecisionChain::with_dependencies(vec![]);
        let decision = chain.check().await.unwrap();
        assert_eq!(decision, Decision::allowed_default());
    }

    /// 单节点允许 → Allowed
    #[tokio::test]
    async fn single_node_allowed() {
        let (limiter, _, _) = MockLimiter::new(true);
        let node = DecisionNode::with_dependencies(
            "n1".to_string(),
            "mock".to_string(),
            Arc::new(limiter),
            100,
        );
        let chain = DecisionChain::with_dependencies(vec![node]);
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Allowed(_)));
    }

    /// 单节点短路拒绝 → 立即 Rejected
    #[tokio::test]
    async fn single_node_short_circuit_rejected() {
        let (limiter, _, _) = MockLimiter::new(false);
        let node = DecisionNode::with_dependencies(
            "n1".to_string(),
            "mock".to_string(),
            Arc::new(limiter),
            100,
        )
        .with_short_circuit(true);
        let chain = DecisionChain::with_dependencies(vec![node]);
        let decision = chain.check().await.unwrap();
        match decision {
            Decision::Rejected(meta) => {
                assert!(
                    meta.reason.contains("mock"),
                    "reason should mention node name"
                );
            }
            other => panic!("Expected Rejected, got {:?}", other),
        }
    }

    /// 短路模式下，第一个节点拒绝后，后续节点不应被调用
    #[tokio::test]
    async fn short_circuit_skips_subsequent_nodes() {
        let (reject_limiter, _, _) = MockLimiter::new(false);
        let (allow_limiter, _, allow_calls) = MockLimiter::new(true);

        let n1 = DecisionNode::with_dependencies(
            "reject".to_string(),
            "rejector".to_string(),
            Arc::new(reject_limiter),
            100,
        )
        .with_short_circuit(true);
        let n2 = DecisionNode::with_dependencies(
            "allow".to_string(),
            "allower".to_string(),
            Arc::new(allow_limiter),
            50,
        );

        let chain = DecisionChain::with_dependencies(vec![n1, n2]);
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
        assert_eq!(
            allow_calls.load(Ordering::SeqCst),
            0,
            "Subsequent node should NOT be called under short-circuit"
        );
    }

    /// 非短路模式下，所有节点都会被调用，最终返回最后的拒绝
    #[tokio::test]
    async fn non_short_circuit_calls_all_nodes() {
        let (reject_limiter, _, _) = MockLimiter::new(false);
        let (allow_limiter, _, allow_calls) = MockLimiter::new(true);

        let n1 = DecisionNode::with_dependencies(
            "reject".to_string(),
            "rejector".to_string(),
            Arc::new(reject_limiter),
            100,
        )
        .with_short_circuit(false);
        let n2 = DecisionNode::with_dependencies(
            "allow".to_string(),
            "allower".to_string(),
            Arc::new(allow_limiter),
            50,
        )
        .with_short_circuit(false);

        let chain = DecisionChain::with_dependencies(vec![n1, n2]);
        let decision = chain.check().await.unwrap();
        // 非短路拒绝后，最终仍返回 Rejected（last_rejection）
        assert!(
            matches!(decision, Decision::Rejected(_)),
            "Should return Rejected when any non-short-circuit node rejects"
        );
        assert_eq!(
            allow_calls.load(Ordering::SeqCst),
            1,
            "All nodes should be called under non-short-circuit"
        );
    }

    /// 禁用节点应被跳过
    #[tokio::test]
    async fn disabled_node_is_skipped() {
        let (reject_limiter, _, reject_calls) = MockLimiter::new(false);
        let node = DecisionNode::with_dependencies(
            "disabled".to_string(),
            "rejector".to_string(),
            Arc::new(reject_limiter),
            100,
        )
        .with_enabled(false);
        let chain = DecisionChain::with_dependencies(vec![node]);
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Allowed(_)));
        assert_eq!(reject_calls.load(Ordering::SeqCst), 0);
    }

    /// 节点返回错误时，链应传播错误
    #[tokio::test]
    async fn node_error_propagates() {
        struct ErrorLimiter;
        #[async_trait::async_trait]
        impl Limiter for ErrorLimiter {
            async fn allow(&self, _cost: u64) -> Result<bool, LimiteronError> {
                Err(LimiteronError::LimitError("node error".to_string()))
            }
        }
        let node = DecisionNode::with_dependencies(
            "err".to_string(),
            "error_node".to_string(),
            Arc::new(ErrorLimiter),
            100,
        );
        let chain = DecisionChain::with_dependencies(vec![node]);
        let result = chain.check().await;
        match result {
            Err(LimiteronError::LimitError(msg)) => {
                assert!(msg.contains("node error"));
            }
            other => panic!("Expected LimitError, got {:?}", other),
        }
    }

    /// 统计信息正确记录
    #[tokio::test]
    async fn stats_track_allowed_and_rejected() {
        let (allow_limiter, _, _) = MockLimiter::new(true);
        let (reject_limiter, _, _) = MockLimiter::new(false);

        let allow_node = DecisionNode::with_dependencies(
            "allow".to_string(),
            "allower".to_string(),
            Arc::new(allow_limiter),
            100,
        );
        let reject_node = DecisionNode::with_dependencies(
            "reject".to_string(),
            "rejector".to_string(),
            Arc::new(reject_limiter),
            50,
        )
        .with_short_circuit(true);

        let chain = DecisionChain::with_dependencies(vec![allow_node, reject_node]);

        // 第一次：allow_node 允许，reject_node 拒绝（短路）
        let _ = chain.check().await.unwrap();
        // 第二次：allow_node 允许
        let (a2, _, _) = MockLimiter::new(true);
        let chain2 = DecisionChain::with_dependencies(vec![DecisionNode::with_dependencies(
            "a2".to_string(),
            "a2".to_string(),
            Arc::new(a2),
            100,
        )]);
        let _ = chain2.check().await.unwrap();

        let stats = chain.stats().await;
        assert_eq!(stats.total_checks, 1);
        assert_eq!(stats.rejected_count, 1);
        assert_eq!(stats.allowed_count, 0);

        let stats2 = chain2.stats().await;
        assert_eq!(stats2.total_checks, 1);
        assert_eq!(stats2.allowed_count, 1);
    }

    /// 使用 builder 链式构建
    #[tokio::test]
    async fn builder_chaining() {
        let (l1, _, _) = MockLimiter::new(true);
        let (l2, _, _) = MockLimiter::new(true);
        let chain = DecisionChainBuilder::new()
            .add_node(DecisionNode::with_dependencies(
                "n1".to_string(),
                "first".to_string(),
                Arc::new(l1),
                100,
            ))
            .add_node(DecisionNode::with_dependencies(
                "n2".to_string(),
                "second".to_string(),
                Arc::new(l2),
                50,
            ))
            .build();
        assert_eq!(chain.node_count(), 2);
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Allowed(_)));
    }
}
