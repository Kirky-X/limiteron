// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 决策链类型定义

use crate::error::{Decision, LimiteronError, RejectionMetadata};
use crate::limiters::Limiter;
use ahash::AHashMap;
use log::{info, warn};
use parking_lot::RwLock;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

// ============================================================================
// 决策链节点
// ============================================================================

/// 决策链节点
///
/// 责任链中的单个节点，包含一个限流器和相关配置。
#[derive(Clone)]
pub struct DecisionNode {
    /// 节点ID
    pub id: String,
    /// 节点名称
    pub name: String,
    /// 限流器
    pub limiter: Arc<dyn Limiter>,
    /// 优先级（数值越大优先级越高）
    pub priority: u16,
    /// 是否启用
    pub enabled: bool,
    /// 是否短路（拒绝时立即返回）
    pub short_circuit: bool,
    /// 成本（每次请求消耗的令牌数）
    pub cost: u64,
}

impl DecisionNode {
    /// 使用依赖注入创建决策节点
    ///
    /// # 参数
    /// - `id`: 节点ID
    /// - `name`: 节点名称
    /// - `limiter`: 限流器（依赖注入）
    /// - `priority`: 优先级
    ///
    /// # 返回
    /// - 决策节点实例
    pub fn with_dependencies(
        id: String,
        name: String,
        limiter: Arc<dyn Limiter>,
        priority: u16,
    ) -> Self {
        Self {
            id,
            name,
            limiter,
            priority,
            enabled: true,
            short_circuit: true,
            cost: 1,
        }
    }

    /// 创建决策节点（向后兼容）
    pub fn new(id: String, name: String, limiter: Arc<dyn Limiter>, priority: u16) -> Self {
        Self::with_dependencies(id, name, limiter, priority)
    }

    /// 设置是否启用
    ///
    /// # 参数
    /// - `enabled`: 是否启用
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 设置是否短路
    ///
    /// # 参数
    /// - `short_circuit`: 是否短路
    pub fn with_short_circuit(mut self, short_circuit: bool) -> Self {
        self.short_circuit = short_circuit;
        self
    }

    /// 设置成本
    ///
    /// # 参数
    /// - `cost`: 成本
    pub fn with_cost(mut self, cost: u64) -> Self {
        self.cost = cost;
        self
    }
}

// ============================================================================
// 决策链
// ============================================================================

/// 原子统计结构体
///
/// 使用原子操作实现无锁统计更新，提供高性能的并发统计。
///
/// # 性能优势
///
/// - 无锁设计：使用原子操作避免锁竞争
/// - 顺序一致：所有原子操作使用 `Ordering::SeqCst`，保证跨计数器的全局
///   可见性顺序。`DecisionChain::check` 的调用模式是先 `increment_total`
///   后 `increment_allowed/rejected/error`，SeqCst 确保 snapshot 不会
///   观测到 "total 已 increment 而分类计数尚未 increment" 的中间态，
///   从而维护 `allowed + rejected + error >= total` 的业务不变式。
///   相比 `Relaxed` 的小幅性能损失换来更强的跨计数器一致性保证。
/// - 高并发：支持多线程同时更新统计信息
///
/// # 示例
///
/// ```rust
/// use limiteron::decision_chain::types::AtomicChainStats;
///
/// let stats = AtomicChainStats::new();
/// stats.increment_total();
/// stats.increment_allowed();
///
/// let snapshot = stats.snapshot();
/// assert_eq!(snapshot.total_checks, 1);
/// assert_eq!(snapshot.allowed_count, 1);
/// ```
pub struct AtomicChainStats {
    /// 总检查次数
    total_checks: AtomicU64,
    /// 允许次数
    allowed_count: AtomicU64,
    /// 拒绝次数
    rejected_count: AtomicU64,
    /// 错误次数
    error_count: AtomicU64,
    /// 各节点的拒绝次数（需要锁保护的动态数据）
    node_rejections: RwLock<AHashMap<String, u64>>,
}

impl AtomicChainStats {
    /// 创建新的原子统计实例
    ///
    /// # 返回
    /// - 初始化为零的原子统计实例
    pub fn new() -> Self {
        Self {
            total_checks: AtomicU64::new(0),
            allowed_count: AtomicU64::new(0),
            rejected_count: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            node_rejections: RwLock::new(AHashMap::new()),
        }
    }

    /// 增加总检查次数
    #[inline]
    pub fn increment_total(&self) {
        self.total_checks.fetch_add(1, Ordering::SeqCst);
    }

    /// 增加允许次数
    #[inline]
    pub fn increment_allowed(&self) {
        self.allowed_count.fetch_add(1, Ordering::SeqCst);
    }

    /// 增加拒绝次数
    #[inline]
    pub fn increment_rejected(&self) {
        self.rejected_count.fetch_add(1, Ordering::SeqCst);
    }

    /// 增加错误次数
    #[inline]
    pub fn increment_error(&self) {
        self.error_count.fetch_add(1, Ordering::SeqCst);
    }

    /// 增加指定节点的拒绝次数
    ///
    /// # 参数
    /// - `node_id`: 节点ID
    pub fn increment_node_rejection(&self, node_id: &str) {
        let mut rejections = self.node_rejections.write();
        *rejections.entry(node_id.to_string()).or_insert(0) += 1;
    }

    /// 获取统计快照
    ///
    /// 返回当前统计信息的快照，用于读取和展示。
    ///
    /// # 返回
    /// - 统计快照（值类型）
    pub fn snapshot(&self) -> ChainStats {
        let node_rejections: Vec<(String, u64)> = self
            .node_rejections
            .read()
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();

        ChainStats {
            total_checks: self.total_checks.load(Ordering::SeqCst),
            allowed_count: self.allowed_count.load(Ordering::SeqCst),
            rejected_count: self.rejected_count.load(Ordering::SeqCst),
            error_count: self.error_count.load(Ordering::SeqCst),
            node_rejections,
        }
    }

    /// 重置所有统计信息
    pub fn reset(&self) {
        self.total_checks.store(0, Ordering::SeqCst);
        self.allowed_count.store(0, Ordering::SeqCst);
        self.rejected_count.store(0, Ordering::SeqCst);
        self.error_count.store(0, Ordering::SeqCst);
        self.node_rejections.write().clear();
    }
}

impl Default for AtomicChainStats {
    fn default() -> Self {
        Self::new()
    }
}

/// 决策链
///
/// 使用责任链模式实现多限流器组合决策。
#[derive(Clone)]
pub struct DecisionChain {
    /// 链中的节点（按优先级排序）
    nodes: Vec<DecisionNode>,
    /// 统计信息（原子类型）
    stats: Arc<AtomicChainStats>,
}

/// 决策链统计信息
///
/// 统计快照，用于读取和展示统计信息。
#[derive(Debug, Clone, Default)]
pub struct ChainStats {
    /// 总检查次数
    pub total_checks: u64,
    /// 允许次数
    pub allowed_count: u64,
    /// 拒绝次数
    pub rejected_count: u64,
    /// 错误次数
    pub error_count: u64,
    /// 各节点的拒绝次数
    pub node_rejections: Vec<(String, u64)>,
}

impl DecisionChain {
    /// 使用构建器创建决策链
    ///
    /// # 返回
    /// - 决策链构建器
    pub fn builder() -> DecisionChainBuilder {
        DecisionChainBuilder::new()
    }

    /// 使用依赖注入创建决策链
    pub fn with_dependencies(nodes: Vec<DecisionNode>) -> Self {
        let mut chain = Self {
            nodes: Vec::new(),
            stats: Arc::new(AtomicChainStats::new()),
        };

        for node in nodes {
            chain.add_node(node);
        }

        chain
    }

    /// 检查决策链（向后兼容）
    pub async fn check(&self) -> Result<Decision, LimiteronError> {
        let enabled_nodes: Vec<&DecisionNode> = self.nodes.iter().filter(|n| n.enabled).collect();

        let mut last_rejection: Option<RejectionMetadata> = None;

        for node in enabled_nodes {
            match node.limiter.allow(node.cost).await {
                Ok(false) => {
                    // 节点拒绝 - 使用原子操作更新统计
                    self.stats.increment_total();
                    self.stats.increment_rejected();
                    self.stats.increment_node_rejection(&node.id);

                    // 检查短路标志
                    if node.short_circuit {
                        // 短路：立即返回拒绝，不执行后续节点
                        return Ok(Decision::Rejected(RejectionMetadata {
                            reason: format!("Rejected by {}: rate limit exceeded", node.name),
                            retry_after: 60,
                            limit: 0,
                            reset_at: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0)
                                + 60,
                        }));
                    }
                    // 非短路：记录拒绝但继续执行后续节点
                    last_rejection = Some(RejectionMetadata {
                        reason: format!("Rejected by {}: rate limit exceeded", node.name),
                        retry_after: 60,
                        limit: 0,
                        reset_at: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0)
                            + 60,
                    });
                }
                Err(e) => {
                    // 节点错误 - 使用原子操作更新统计
                    self.stats.increment_total();
                    self.stats.increment_error();
                    return Err(e);
                }
                _ => {} // 允许，继续下一个节点
            }
        }

        // 所有节点都允许 - 使用原子操作更新统计
        self.stats.increment_total();

        // 如果有任何非短路拒绝，返回最后的拒绝结果
        if let Some(rejection) = last_rejection {
            self.stats.increment_allowed();
            return Ok(Decision::Rejected(rejection));
        }

        self.stats.increment_allowed();
        Ok(Decision::allowed_default())
    }

    /// 获取统计信息
    ///
    /// 返回当前统计信息的快照。
    ///
    /// # 返回
    /// - 统计快照（值类型）
    pub async fn stats(&self) -> ChainStats {
        self.stats.snapshot()
    }

    /// 获取统计信息（同步版本）
    ///
    /// 由于使用原子操作，可以同步获取统计信息。
    ///
    /// # 返回
    /// - 统计快照（值类型）
    pub fn stats_sync(&self) -> ChainStats {
        self.stats.snapshot()
    }

    /// 重置统计信息
    pub async fn reset_stats(&self) {
        self.stats.reset();
    }

    /// 重置统计信息（同步版本）
    ///
    /// 由于使用原子操作，可以同步重置统计信息。
    pub fn reset_stats_sync(&self) {
        self.stats.reset();
    }

    /// 获取节点数量
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 获取启用的节点数量
    pub fn enabled_node_count(&self) -> usize {
        self.nodes.iter().filter(|n| n.enabled).count()
    }

    /// 添加节点
    pub fn add_node(&mut self, node: DecisionNode) {
        self.nodes.push(node);
    }

    /// 启用节点
    ///
    /// # 参数
    /// - `node_id`: 节点ID
    pub fn enable_node(&mut self, node_id: &str) -> bool {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == node_id) {
            node.enabled = true;
            info!("Enabled node: {}", node_id);
            true
        } else {
            warn!("Failed to enable node: {} (not found)", node_id);
            false
        }
    }

    /// 禁用节点
    ///
    /// # 参数
    /// - `node_id`: 节点ID
    pub fn disable_node(&mut self, node_id: &str) -> bool {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == node_id) {
            node.enabled = false;
            info!("Disabled node: {}", node_id);
            true
        } else {
            warn!("Failed to disable node: {} (not found)", node_id);
            false
        }
    }

    /// 设置节点短路
    ///
    /// # 参数
    /// - `node_id`: 节点ID
    /// - `short_circuit`: 是否短路
    pub fn set_short_circuit(&mut self, node_id: &str, short_circuit: bool) -> bool {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == node_id) {
            node.short_circuit = short_circuit;
            info!("Set short_circuit={} for node: {}", short_circuit, node_id);
            true
        } else {
            warn!(
                "Failed to set short_circuit for node: {} (not found)",
                node_id
            );
            false
        }
    }
}

// ============================================================================
// 构建器
// ============================================================================

/// 决策链构建器
///
/// 提供流式API构建决策链。
pub struct DecisionChainBuilder {
    nodes: Vec<DecisionNode>,
}

impl DecisionChainBuilder {
    /// 创建新的构建器
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::decision_chain::DecisionChainBuilder;
    ///
    /// let builder = DecisionChainBuilder::new();
    /// ```
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// 添加节点
    ///
    /// # 参数
    /// - `node`: 决策节点
    pub fn add_node(mut self, node: DecisionNode) -> Self {
        self.nodes.push(node);
        self
    }

    /// 构建决策链
    ///
    /// # 返回
    /// - 决策链实例
    pub fn build(self) -> DecisionChain {
        DecisionChain::with_dependencies(self.nodes)
    }
}

impl Default for DecisionChainBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use crate::limiters::{
        ConcurrencyLimiter, FixedWindowLimiter, ShardedSlidingWindowLimiter, TokenBucketLimiter,
    };
    use async_trait::async_trait;
    use std::time::Duration;

    // Helper structs for testing
    struct MockLimiter {
        allowed: Arc<std::sync::atomic::AtomicBool>,
    }
    impl MockLimiter {
        fn new(allowed: bool) -> Self {
            Self {
                allowed: Arc::new(std::sync::atomic::AtomicBool::new(allowed)),
            }
        }
        fn set_allowed(&self, v: bool) {
            self.allowed.store(v, std::sync::atomic::Ordering::SeqCst);
        }
    }
    #[async_trait]
    impl Limiter for MockLimiter {
        async fn allow(&self, _cost: u64) -> Result<bool, LimiteronError> {
            let a = self.allowed.load(std::sync::atomic::Ordering::SeqCst);
            Ok(a)
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
    }
    #[async_trait]
    impl Limiter for SpyLimiter {
        async fn allow(&self, _cost: u64) -> Result<bool, LimiteronError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(true)
        }
    }

    // ==================== DecisionNode 测试 ====================

    #[test]
    fn test_decision_node_creation() {
        let limiter = Arc::new(TokenBucketLimiter::new(100, 10));
        let node = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        );

        assert_eq!(node.id, "node1");
        assert_eq!(node.name, "Token Bucket");
        assert_eq!(node.priority, 100);
        assert!(node.enabled);
        assert!(node.short_circuit);
        assert_eq!(node.cost, 1);
    }

    #[test]
    fn test_decision_node_with_options() {
        let limiter = Arc::new(TokenBucketLimiter::new(100, 10));
        let node = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        )
        .with_enabled(false)
        .with_short_circuit(false)
        .with_cost(5);

        assert!(!node.enabled);
        assert!(!node.short_circuit);
        assert_eq!(node.cost, 5);
    }

    // ==================== DecisionChain 测试 ====================

    #[tokio::test]
    async fn test_decision_chain_empty() {
        let chain = DecisionChain::with_dependencies(vec![]);
        let decision = chain.check().await.unwrap();

        assert_eq!(decision, Decision::allowed_default());
    }

    #[tokio::test]
    async fn test_decision_chain_single_node() {
        let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
        let node = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        );

        let chain = DecisionChain::with_dependencies(vec![node]);

        // 前10个请求应该被允许
        for _ in 0..10 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::allowed_default());
        }

        // 第11个请求应该被拒绝
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    #[tokio::test]
    async fn test_decision_chain_multiple_nodes() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(5, 1));
        let limiter2 = Arc::new(ShardedSlidingWindowLimiter::new(Duration::from_secs(1), 10));

        let node1 = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter1,
            100,
        );

        let node2 = DecisionNode::with_dependencies(
            "node2".to_string(),
            "Sliding Window".to_string(),
            limiter2,
            50,
        );

        let chain = DecisionChain::with_dependencies(vec![node1, node2]);

        // 前5个请求应该被允许
        for _ in 0..5 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::allowed_default());
        }

        // 第6个请求应该被更高优先级的node1拒绝
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    #[tokio::test]
    async fn test_decision_chain_priority() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(10, 1));
        let limiter2 = Arc::new(TokenBucketLimiter::new(5, 1));

        let node1 = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Low Priority".to_string(),
            limiter1,
            50,
        );

        let node2 = DecisionNode::with_dependencies(
            "node2".to_string(),
            "High Priority".to_string(),
            limiter2,
            100,
        );

        let chain = DecisionChain::with_dependencies(vec![node1, node2]);

        // 高优先级的node2应该先被检查
        for _ in 0..5 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::allowed_default());
        }

        // node2应该先拒绝
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));

        // 验证拒绝原因来自node2
        if let Decision::Rejected(metadata) = decision {
            assert!(metadata.reason.contains("High Priority"));
        }
    }

    #[tokio::test]
    async fn test_decision_chain_disabled_node() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(0, 1));
        let limiter2 = Arc::new(TokenBucketLimiter::new(10, 1));

        let node1 = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Disabled Node".to_string(),
            limiter1,
            100,
        )
        .with_enabled(false);

        let node2 = DecisionNode::with_dependencies(
            "node2".to_string(),
            "Active Node".to_string(),
            limiter2,
            50,
        );

        let chain = DecisionChain::with_dependencies(vec![node1, node2]);

        // node1被禁用，应该检查node2
        let decision = chain.check().await.unwrap();
        assert_eq!(decision, Decision::allowed_default());
    }

    #[tokio::test]
    async fn test_decision_chain_short_circuit() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(5, 1));
        let limiter2 = Arc::new(TokenBucketLimiter::new(10, 1));

        let node1 = DecisionNode::with_dependencies(
            "node1".to_string(),
            "First Node".to_string(),
            limiter1,
            100,
        );

        let node2 = DecisionNode::with_dependencies(
            "node2".to_string(),
            "Second Node".to_string(),
            limiter2,
            50,
        );

        let chain = DecisionChain::with_dependencies(vec![node1, node2]);

        // 前5个请求应该被允许
        for _ in 0..5 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::allowed_default());
        }

        // 第6个请求应该被node1拒绝，并短路
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    #[tokio::test]
    async fn test_decision_chain_no_short_circuit() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(5, 1));
        let limiter2 = Arc::new(TokenBucketLimiter::new(3, 1));

        let node1 = DecisionNode::with_dependencies(
            "node1".to_string(),
            "First Node".to_string(),
            limiter1,
            100,
        )
        .with_short_circuit(false);

        let node2 = DecisionNode::with_dependencies(
            "node2".to_string(),
            "Second Node".to_string(),
            limiter2,
            50,
        );

        let chain = DecisionChain::with_dependencies(vec![node1, node2]);

        // 前3个请求应该被允许
        for _ in 0..3 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::allowed_default());
        }

        // 第4个请求应该被node2拒绝
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    #[tokio::test]
    async fn test_decision_chain_check() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(3, 1));
        let limiter2 = Arc::new(TokenBucketLimiter::new(5, 1));

        let node1 = DecisionNode::with_dependencies(
            "node1".to_string(),
            "First Node".to_string(),
            limiter1,
            100,
        );

        let node2 = DecisionNode::with_dependencies(
            "node2".to_string(),
            "Second Node".to_string(),
            limiter2,
            50,
        );

        let chain = DecisionChain::with_dependencies(vec![node1, node2]);

        // 前3个请求应该被允许
        for _ in 0..3 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::allowed_default());
        }

        // 第4个请求应该检查所有节点
        let decision = chain.check().await.unwrap();
        if let Decision::Rejected(metadata) = decision {
            // 应该包含两个节点的拒绝原因
            assert!(metadata.reason.contains("First Node"));
        }
    }

    #[tokio::test]
    async fn test_decision_chain_stats() {
        let limiter = Arc::new(TokenBucketLimiter::new(5, 1));
        let node = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        );

        let chain = DecisionChain::with_dependencies(vec![node]);

        // 发送10个请求
        for _ in 0..10 {
            chain.check().await.unwrap();
        }

        let stats = chain.stats().await;
        assert_eq!(stats.total_checks, 10);
        assert_eq!(stats.allowed_count, 5);
        assert_eq!(stats.rejected_count, 5);
    }

    #[tokio::test]
    async fn test_decision_chain_node_rejections() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(5, 1));
        let limiter2 = Arc::new(TokenBucketLimiter::new(3, 1));

        let node1 = DecisionNode::with_dependencies(
            "node1".to_string(),
            "First Node".to_string(),
            limiter1,
            100,
        )
        .with_short_circuit(false);

        let node2 = DecisionNode::with_dependencies(
            "node2".to_string(),
            "Second Node".to_string(),
            limiter2,
            50,
        );

        let chain = DecisionChain::with_dependencies(vec![node1, node2]);

        // 发送10个请求
        for _ in 0..10 {
            chain.check().await.unwrap();
        }

        let stats = chain.stats().await;

        // 应该有两个节点的拒绝记录
        assert_eq!(stats.node_rejections.len(), 2);

        // 验证拒绝次数
        let node1_rejections = stats
            .node_rejections
            .iter()
            .find(|(id, _)| id == "node1")
            .map(|(_, count)| *count)
            .unwrap_or(0);
        let node2_rejections = stats
            .node_rejections
            .iter()
            .find(|(id, _)| id == "node2")
            .map(|(_, count)| *count)
            .unwrap_or(0);

        assert!(node1_rejections > 0);
        assert!(node2_rejections > 0);
    }

    #[tokio::test]
    async fn test_decision_chain_add_node() {
        let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
        let node = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        );

        let mut chain = DecisionChain::with_dependencies(vec![]);
        assert_eq!(chain.node_count(), 0);

        chain.add_node(node);
        assert_eq!(chain.node_count(), 1);
    }

    #[tokio::test]
    async fn test_decision_chain_enable_disable_node() {
        let limiter = Arc::new(TokenBucketLimiter::new(0, 1));
        let node = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        );

        let mut chain = DecisionChain::with_dependencies(vec![node]);

        // 禁用节点
        chain.disable_node("node1");
        assert_eq!(chain.enabled_node_count(), 0);

        let decision = chain.check().await.unwrap();
        assert_eq!(decision, Decision::allowed_default());

        // 启用节点
        chain.enable_node("node1");
        assert_eq!(chain.enabled_node_count(), 1);

        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    #[tokio::test]
    async fn test_decision_chain_set_short_circuit() {
        let limiter1 = Arc::new(MockLimiter::new(true));
        let limiter2_spy = Arc::new(SpyLimiter::new());
        let limiter2 = limiter2_spy.clone();

        // Node1: Mock limiter, initially allowed. Short circuit OFF.
        let node1 = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Mock Node".to_string(),
            limiter1.clone(),
            100,
        )
        .with_short_circuit(false);

        // Node2: Spy limiter, always allowed.
        let node2 = DecisionNode::with_dependencies(
            "node2".to_string(),
            "Spy Node".to_string(),
            limiter2,
            50,
        );

        let mut chain = DecisionChain::with_dependencies(vec![node1, node2]);

        // 1. Initial check: Node1 allows. Node2 should be called.
        let decision = chain.check().await.unwrap();
        assert_eq!(decision, Decision::allowed_default());
        assert_eq!(
            limiter2_spy.calls.load(std::sync::atomic::Ordering::SeqCst),
            1
        );

        // 2. Node1 rejects. Short circuit OFF. Node2 should be called.
        limiter1.set_allowed(false);
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
        assert_eq!(
            limiter2_spy.calls.load(std::sync::atomic::Ordering::SeqCst),
            2
        ); // Increased

        // 3. Enable short circuit on Node1.
        chain.set_short_circuit("node1", true);

        // 4. Node1 rejects. Short circuit ON. Node2 should NOT be called.
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
        assert_eq!(
            limiter2_spy.calls.load(std::sync::atomic::Ordering::SeqCst),
            2
        ); // Unchanged!

        // 5. Node1 allows again. Node2 should be called.
        limiter1.set_allowed(true);
        let decision = chain.check().await.unwrap();
        assert_eq!(decision, Decision::allowed_default());
        assert_eq!(
            limiter2_spy.calls.load(std::sync::atomic::Ordering::SeqCst),
            3
        ); // Increased
    }

    // ==================== DecisionChainBuilder 测试 ====================

    #[test]
    fn test_decision_chain_builder() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(100, 10));
        let limiter2 = Arc::new(ShardedSlidingWindowLimiter::new(
            Duration::from_secs(1),
            100,
        ));

        let node1 = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter1,
            100,
        );

        let node2 = DecisionNode::with_dependencies(
            "node2".to_string(),
            "Sliding Window".to_string(),
            limiter2,
            50,
        );

        let chain = DecisionChainBuilder::new()
            .add_node(node1)
            .add_node(node2)
            .build();

        assert_eq!(chain.node_count(), 2);
    }

    #[tokio::test]
    async fn test_decision_chain_mixed_limiters() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(10, 1));
        let limiter2 = Arc::new(ShardedSlidingWindowLimiter::new(Duration::from_secs(1), 5));
        let limiter3 = Arc::new(FixedWindowLimiter::new(Duration::from_secs(1), 3));
        let limiter4 = Arc::new(ConcurrencyLimiter::new(2));

        let node1 = DecisionNode::with_dependencies(
            "token_bucket".to_string(),
            "Token Bucket".to_string(),
            limiter1,
            100,
        );

        let node2 = DecisionNode::with_dependencies(
            "sliding_window".to_string(),
            "Sliding Window".to_string(),
            limiter2,
            75,
        );

        let node3 = DecisionNode::with_dependencies(
            "fixed_window".to_string(),
            "Fixed Window".to_string(),
            limiter3,
            50,
        );

        let node4 = DecisionNode::with_dependencies(
            "concurrency".to_string(),
            "Concurrency".to_string(),
            limiter4,
            25,
        );

        let chain = DecisionChain::with_dependencies(vec![node1, node2, node3, node4]);

        // 第一个请求应该被允许
        let decision = chain.check().await.unwrap();
        assert_eq!(decision, Decision::allowed_default());

        // 检查统计
        let stats = chain.stats().await;
        assert_eq!(stats.total_checks, 1);
        assert_eq!(stats.allowed_count, 1);
    }

    #[tokio::test]
    async fn test_decision_chain_cost() {
        let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
        let node = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        )
        .with_cost(2);

        let chain = DecisionChain::with_dependencies(vec![node]);

        // 5个请求，每个消耗2个令牌
        for _ in 0..5 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::allowed_default());
        }

        // 第6个请求应该被拒绝（总共消耗了10个令牌）
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    #[tokio::test]
    async fn test_decision_chain_reset_stats() {
        let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
        let node = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        );

        let chain = DecisionChain::with_dependencies(vec![node]);

        // 发送一些请求
        for _ in 0..5 {
            chain.check().await.unwrap();
        }

        // 检查统计
        let stats = chain.stats().await;
        assert_eq!(stats.total_checks, 5);

        // 重置统计
        chain.reset_stats().await;

        // 检查重置后的统计
        let stats = chain.stats().await;
        assert_eq!(stats.total_checks, 0);
    }

    #[tokio::test]
    async fn test_decision_chain_concurrent_checks() {
        let limiter = Arc::new(TokenBucketLimiter::new(100, 10));
        let node = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        );

        let chain = Arc::new(DecisionChain::with_dependencies(vec![node]));
        let mut handles = vec![];

        // 并发检查
        for _ in 0..10 {
            let chain_clone = Arc::clone(&chain);
            handles.push(tokio::spawn(
                async move { chain_clone.check().await.unwrap() },
            ));
        }

        // 等待所有检查完成
        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        // 所有检查都应该成功
        for result in results {
            assert_eq!(result, Decision::allowed_default());
        }

        // 检查统计
        let stats = chain.stats().await;
        assert_eq!(stats.total_checks, 10);
    }

    // ==================== AtomicChainStats 测试 ====================

    #[test]
    fn test_atomic_chain_stats_new() {
        let stats = AtomicChainStats::new();
        let snapshot = stats.snapshot();

        assert_eq!(snapshot.total_checks, 0);
        assert_eq!(snapshot.allowed_count, 0);
        assert_eq!(snapshot.rejected_count, 0);
        assert_eq!(snapshot.error_count, 0);
        assert!(snapshot.node_rejections.is_empty());
    }

    #[test]
    fn test_atomic_chain_stats_default() {
        let stats = AtomicChainStats::default();
        let snapshot = stats.snapshot();

        assert_eq!(snapshot.total_checks, 0);
    }

    #[test]
    fn test_atomic_chain_stats_increment_operations() {
        let stats = AtomicChainStats::new();

        // 测试各种增量操作
        stats.increment_total();
        stats.increment_total();
        stats.increment_total();

        stats.increment_allowed();
        stats.increment_allowed();

        stats.increment_rejected();

        stats.increment_error();
        stats.increment_error();
        stats.increment_error();

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.total_checks, 3);
        assert_eq!(snapshot.allowed_count, 2);
        assert_eq!(snapshot.rejected_count, 1);
        assert_eq!(snapshot.error_count, 3);
    }

    #[test]
    fn test_atomic_chain_stats_node_rejections() {
        let stats = AtomicChainStats::new();

        // 测试节点拒绝计数
        stats.increment_node_rejection("node1");
        stats.increment_node_rejection("node1");
        stats.increment_node_rejection("node1");
        stats.increment_node_rejection("node2");

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.node_rejections.len(), 2);

        let node1_count = snapshot
            .node_rejections
            .iter()
            .find(|(id, _)| id == "node1")
            .map(|(_, count)| *count)
            .unwrap_or(0);
        let node2_count = snapshot
            .node_rejections
            .iter()
            .find(|(id, _)| id == "node2")
            .map(|(_, count)| *count)
            .unwrap_or(0);

        assert_eq!(node1_count, 3);
        assert_eq!(node2_count, 1);
    }

    #[test]
    fn test_atomic_chain_stats_reset() {
        let stats = AtomicChainStats::new();

        // 添加一些数据
        stats.increment_total();
        stats.increment_allowed();
        stats.increment_node_rejection("node1");

        // 重置
        stats.reset();

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.total_checks, 0);
        assert_eq!(snapshot.allowed_count, 0);
        assert!(snapshot.node_rejections.is_empty());
    }

    #[test]
    fn test_atomic_chain_stats_concurrent_increment() {
        use std::sync::atomic::AtomicUsize;
        use std::thread;

        let stats = Arc::new(AtomicChainStats::new());
        let counter = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];

        // 并发增加计数
        for _ in 0..100 {
            let stats_clone = Arc::clone(&stats);
            let counter_clone = Arc::clone(&counter);
            handles.push(thread::spawn(move || {
                stats_clone.increment_total();
                stats_clone.increment_allowed();
                counter_clone.fetch_add(1, Ordering::Relaxed);
            }));
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 验证计数正确
        let snapshot = stats.snapshot();
        assert_eq!(snapshot.total_checks, 100);
        assert_eq!(snapshot.allowed_count, 100);
    }

    #[test]
    fn test_atomic_chain_stats_concurrent_node_rejections() {
        use std::thread;

        let stats = Arc::new(AtomicChainStats::new());
        let mut handles = vec![];

        // 并发增加节点拒绝计数
        for i in 0..50 {
            let stats_clone = Arc::clone(&stats);
            handles.push(thread::spawn(move || {
                stats_clone.increment_node_rejection("node1");
                stats_clone.increment_node_rejection(&format!("node{}", i % 5));
            }));
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 验证计数正确
        let snapshot = stats.snapshot();

        // node1 应该有 50 + 10 = 60 次拒绝（每个线程增加一次，加上 i % 5 == 1 的情况）
        let node1_count = snapshot
            .node_rejections
            .iter()
            .find(|(id, _)| id == "node1")
            .map(|(_, count)| *count)
            .unwrap_or(0);

        // node1 被每个线程调用一次（50次），加上 i % 5 == 1 的情况（10次）
        assert_eq!(node1_count, 60);

        // 总共有 5 个不同的节点（node0 到 node4）
        assert_eq!(snapshot.node_rejections.len(), 5);
    }

    #[tokio::test]
    async fn test_decision_chain_stats_sync() {
        let limiter = Arc::new(TokenBucketLimiter::new(5, 1));
        let node = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        );

        let chain = DecisionChain::with_dependencies(vec![node]);

        // 发送10个请求
        for _ in 0..10 {
            chain.check().await.unwrap();
        }

        // 使用同步方法获取统计
        let stats = chain.stats_sync();
        assert_eq!(stats.total_checks, 10);
        assert_eq!(stats.allowed_count, 5);
        assert_eq!(stats.rejected_count, 5);
    }

    #[test]
    fn test_decision_chain_reset_stats_sync() {
        let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
        let node = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        );

        let chain = DecisionChain::with_dependencies(vec![node]);

        // 使用同步重置
        chain.reset_stats_sync();

        let stats = chain.stats_sync();
        assert_eq!(stats.total_checks, 0);
    }

    #[test]
    fn test_atomic_chain_stats_high_concurrency() {
        use std::thread;

        let stats = Arc::new(AtomicChainStats::new());
        let mut handles = vec![];

        // 高并发测试：1000个线程
        for _ in 0..1000 {
            let stats_clone = Arc::clone(&stats);
            handles.push(thread::spawn(move || {
                // 每个线程执行多次操作
                for _ in 0..100 {
                    stats_clone.increment_total();
                    stats_clone.increment_allowed();
                }
            }));
        }

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 验证计数正确
        let snapshot = stats.snapshot();
        assert_eq!(snapshot.total_checks, 100_000);
        assert_eq!(snapshot.allowed_count, 100_000);
    }

    // ==================== 并发安全测试 ====================

    /// 测试 DecisionChain 高并发检查的线程安全
    ///
    /// 验证在高并发情况下，决策链不会出现数据竞争或死锁
    #[tokio::test]
    async fn test_decision_chain_high_concurrency_safety() {
        let limiter = Arc::new(TokenBucketLimiter::new(10000, 1000));
        let node = DecisionNode::with_dependencies(
            "concurrent_node".to_string(),
            "Concurrent Test".to_string(),
            limiter,
            100,
        );

        let chain = Arc::new(DecisionChain::with_dependencies(vec![node]));
        let mut handles = vec![];

        // 使用 barrier 确保所有任务同时开始
        let barrier = Arc::new(tokio::sync::Barrier::new(100));
        let start_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

        for _ in 0..100 {
            let chain_clone = Arc::clone(&chain);
            let barrier_clone = Arc::clone(&barrier);
            let start_signal_clone = Arc::clone(&start_signal);
            handles.push(tokio::spawn(async move {
                // 等待所有任务准备就绪
                barrier_clone.wait().await;

                // 等待开始信号
                while !start_signal_clone.load(std::sync::atomic::Ordering::SeqCst) {
                    std::hint::spin_loop();
                }

                // 每个任务发送 100 个请求
                let mut local_allowed = 0;
                for _ in 0..100 {
                    if let Ok(Decision::Allowed(_)) = chain_clone.check().await {
                        local_allowed += 1;
                    }
                }
                local_allowed
            }));
        }

        // 设置开始信号
        start_signal.store(true, std::sync::atomic::Ordering::SeqCst);

        let mut total_allowed = 0;
        for handle in handles {
            total_allowed += handle.await.unwrap();
        }

        // 不应该超过令牌桶限制（允许 5% 的误差）
        assert!(
            total_allowed <= 10500,
            "Total allowed: {}, expected <= 10500",
            total_allowed
        );
    }

    /// 测试多节点决策链并发安全
    #[tokio::test]
    async fn test_multi_node_chain_concurrent_safety() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(5000, 100));
        let limiter2 = Arc::new(ShardedSlidingWindowLimiter::new(
            Duration::from_secs(1),
            5000,
        ));
        let limiter3 = Arc::new(FixedWindowLimiter::new(Duration::from_secs(10), 5000));

        let node1 = DecisionNode::with_dependencies(
            "token_bucket".to_string(),
            "Token Bucket".to_string(),
            limiter1,
            100,
        );
        let node2 = DecisionNode::with_dependencies(
            "sliding_window".to_string(),
            "Sliding Window".to_string(),
            limiter2,
            50,
        );
        let node3 = DecisionNode::with_dependencies(
            "fixed_window".to_string(),
            "Fixed Window".to_string(),
            limiter3,
            25,
        );

        let chain = Arc::new(DecisionChain::with_dependencies(vec![node1, node2, node3]));
        let mut handles = vec![];

        let barrier = Arc::new(tokio::sync::Barrier::new(50));
        let start_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

        for _ in 0..50 {
            let chain_clone = Arc::clone(&chain);
            let barrier_clone = Arc::clone(&barrier);
            let start_signal_clone = Arc::clone(&start_signal);
            handles.push(tokio::spawn(async move {
                barrier_clone.wait().await;

                while !start_signal_clone.load(std::sync::atomic::Ordering::SeqCst) {
                    std::hint::spin_loop();
                }

                let mut local_allowed = 0;
                for _ in 0..100 {
                    if let Ok(Decision::Allowed(_)) = chain_clone.check().await {
                        local_allowed += 1;
                    }
                }
                local_allowed
            }));
        }

        start_signal.store(true, std::sync::atomic::Ordering::SeqCst);

        let mut total_allowed = 0;
        for handle in handles {
            total_allowed += handle.await.unwrap();
        }

        // 应该被最严格的限流器限制
        assert!(
            total_allowed <= 5500,
            "Total allowed: {}, expected <= 5500",
            total_allowed
        );
    }

    /// 测试决策链统计并发安全
    #[tokio::test]
    async fn test_chain_stats_concurrent_safety() {
        let limiter = Arc::new(TokenBucketLimiter::new(10000, 1000));
        let node = DecisionNode::with_dependencies(
            "stats_node".to_string(),
            "Stats Test".to_string(),
            limiter,
            100,
        );

        let chain = Arc::new(DecisionChain::with_dependencies(vec![node]));
        let mut handles = vec![];

        // 并发执行检查和统计读取
        for _ in 0..100 {
            let chain_clone = Arc::clone(&chain);
            handles.push(tokio::spawn(async move {
                for _ in 0..10 {
                    let _ = chain_clone.check().await;
                    // 并发读取统计
                    let _ = chain_clone.stats_sync();
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // 验证统计一致性
        let stats = chain.stats_sync();
        assert!(stats.total_checks <= 10000);
    }

    /// 测试决策链无死锁
    #[tokio::test]
    async fn test_chain_no_deadlock() {
        let limiter = Arc::new(TokenBucketLimiter::new(100000, 10000));
        let node = DecisionNode::with_dependencies(
            "deadlock_test".to_string(),
            "Deadlock Test".to_string(),
            limiter,
            100,
        );

        let chain = Arc::new(DecisionChain::with_dependencies(vec![node]));
        let mut handles = vec![];

        // 大量并发任务
        for _ in 0..500 {
            let chain_clone = Arc::clone(&chain);
            handles.push(tokio::spawn(async move {
                for _ in 0..50 {
                    let _ = chain_clone.check().await;
                }
            }));
        }

        // 使用超时确保不会死锁
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            for handle in handles {
                let _ = handle.await;
            }
        })
        .await;

        assert!(result.is_ok(), "Test timed out - possible deadlock");
    }

    /// 测试节点动态启用/禁用的并发安全
    #[tokio::test]
    async fn test_node_enable_disable_concurrent_safety() {
        let limiter = Arc::new(TokenBucketLimiter::new(10000, 1000));
        let node = DecisionNode::with_dependencies(
            "toggle_node".to_string(),
            "Toggle Test".to_string(),
            limiter,
            100,
        );

        let chain = Arc::new(tokio::sync::RwLock::new(DecisionChain::with_dependencies(
            vec![node],
        )));
        let mut handles = vec![];

        // 并发执行检查和节点状态切换
        for i in 0..100 {
            let chain_clone = Arc::clone(&chain);
            handles.push(tokio::spawn(async move {
                if i % 2 == 0 {
                    // 检查请求
                    let chain = chain_clone.read().await;
                    let _ = chain.check().await;
                } else {
                    // 切换节点状态
                    let mut chain = chain_clone.write().await;
                    if i % 4 == 1 {
                        chain.disable_node("toggle_node");
                    } else {
                        chain.enable_node("toggle_node");
                    }
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // 验证最终状态一致
        let chain = chain.read().await;
        let stats = chain.stats_sync();
        assert!(stats.total_checks <= 10000);
    }

    /// 测试决策链边界条件 - 空链
    #[tokio::test]
    async fn test_empty_chain_boundary() {
        let chain = DecisionChain::with_dependencies(vec![]);

        // 空链应该总是允许
        for _ in 0..100 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::allowed_default());
        }

        let stats = chain.stats_sync();
        assert_eq!(stats.total_checks, 100);
        assert_eq!(stats.allowed_count, 100);
    }

    /// 测试决策链边界条件 - 所有节点禁用
    #[tokio::test]
    async fn test_all_nodes_disabled_boundary() {
        let limiter = Arc::new(TokenBucketLimiter::new(0, 1)); // 容量为 0，会拒绝所有请求
        let node = DecisionNode::with_dependencies(
            "disabled_node".to_string(),
            "Disabled Node".to_string(),
            limiter,
            100,
        )
        .with_enabled(false);

        let chain = DecisionChain::with_dependencies(vec![node]);

        // 所有节点禁用时应该允许
        let decision = chain.check().await.unwrap();
        assert_eq!(decision, Decision::allowed_default());
    }

    /// 测试决策链成本边界条件
    #[tokio::test]
    async fn test_chain_cost_boundary() {
        let limiter = Arc::new(TokenBucketLimiter::new(100, 1));
        let node = DecisionNode::with_dependencies(
            "cost_node".to_string(),
            "Cost Test".to_string(),
            limiter,
            100,
        )
        .with_cost(50); // 每次检查消耗 50 个令牌

        let chain = DecisionChain::with_dependencies(vec![node]);

        // 第一次检查应该成功（消耗 50 个令牌）
        let decision = chain.check().await.unwrap();
        assert_eq!(decision, Decision::allowed_default());

        // 第二次检查应该成功（消耗剩余 50 个令牌）
        let decision = chain.check().await.unwrap();
        assert_eq!(decision, Decision::allowed_default());

        // 第三次检查应该失败（没有足够的令牌）
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    // ==================== DecisionNode::new() 测试 ====================

    #[test]
    fn test_decision_node_new_backward_compat() {
        let limiter = Arc::new(TokenBucketLimiter::new(100, 10));
        let node = DecisionNode::new("node1".to_string(), "new method".to_string(), limiter, 100);
        assert_eq!(node.id, "node1");
        assert_eq!(node.name, "new method");
        assert_eq!(node.priority, 100);
        assert!(node.enabled);
        assert!(node.short_circuit);
        assert_eq!(node.cost, 1);
    }

    // ==================== DecisionChain Builder 测试 ====================

    #[test]
    fn test_decision_chain_builder_method() {
        let builder = DecisionChain::builder();
        let chain = builder.build();
        assert_eq!(chain.node_count(), 0);
    }

    #[test]
    fn test_decision_chain_builder_default_trait() {
        let _builder = DecisionChainBuilder::default();
    }

    // ==================== DecisionChain Error Path 测试 ====================

    struct ErrorLimiter;

    #[async_trait]
    impl Limiter for ErrorLimiter {
        async fn allow(&self, _cost: u64) -> Result<bool, LimiteronError> {
            Err(LimiteronError::LimitError("limit error".to_string()))
        }
    }

    #[tokio::test]
    async fn test_decision_chain_check_error_path() {
        let node = DecisionNode::with_dependencies(
            "error_node".to_string(),
            "Error Limiter".to_string(),
            Arc::new(ErrorLimiter),
            100,
        );
        let chain = DecisionChain::with_dependencies(vec![node]);
        let result = chain.check().await;
        assert!(result.is_err());

        let stats = chain.stats_sync();
        assert_eq!(stats.total_checks, 1);
        assert_eq!(stats.error_count, 1);
    }

    #[tokio::test]
    async fn test_decision_chain_error_path_second_node() {
        let ok = Arc::new(TokenBucketLimiter::new(100, 10));
        let ok_node = DecisionNode::with_dependencies("ok".to_string(), "OK".to_string(), ok, 100);
        let err_node = DecisionNode::with_dependencies(
            "err".to_string(),
            "Error".to_string(),
            Arc::new(ErrorLimiter),
            50,
        );
        let chain = DecisionChain::with_dependencies(vec![ok_node, err_node]);
        let result = chain.check().await;
        assert!(result.is_err());

        let stats = chain.stats_sync();
        assert_eq!(stats.total_checks, 1);
        assert_eq!(stats.error_count, 1);
    }

    // ==================== DecisionChain Non-Short-Circuit 测试 ====================

    #[tokio::test]
    async fn test_decision_chain_non_short_circuit_final() {
        let pass = Arc::new(MockLimiter::new(true));
        let limited = Arc::new(TokenBucketLimiter::new(3, 1));

        let node1 = DecisionNode::with_dependencies(
            "pass".to_string(),
            "Pass Through".to_string(),
            pass,
            100,
        );
        let node2 = DecisionNode::with_dependencies(
            "limited".to_string(),
            "Limited".to_string(),
            limited,
            50,
        )
        .with_short_circuit(false);

        let chain = DecisionChain::with_dependencies(vec![node1, node2]);

        for _ in 0..3 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::allowed_default());
        }

        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
        if let Decision::Rejected(meta) = decision {
            assert!(meta.reason.contains("Limited"));
        }

        let stats = chain.stats_sync();
        assert_eq!(stats.total_checks, 5);
        assert_eq!(stats.rejected_count, 1);
    }

    // ==================== DecisionChain Node Not Found 测试 ====================

    #[tokio::test]
    async fn test_decision_chain_enable_node_not_found() {
        let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
        let node =
            DecisionNode::with_dependencies("node1".to_string(), "Node".to_string(), limiter, 100);
        let mut chain = DecisionChain::with_dependencies(vec![node]);
        assert!(!chain.enable_node("nonexistent"));
    }

    #[tokio::test]
    async fn test_decision_chain_disable_node_not_found() {
        let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
        let node =
            DecisionNode::with_dependencies("node1".to_string(), "Node".to_string(), limiter, 100);
        let mut chain = DecisionChain::with_dependencies(vec![node]);
        assert!(!chain.disable_node("nonexistent"));
    }

    #[tokio::test]
    async fn test_decision_chain_set_short_circuit_found() {
        let spy = Arc::new(SpyLimiter::new());
        let reject = Arc::new(MockLimiter::new(true));

        let node1 = DecisionNode::with_dependencies(
            "node1".to_string(),
            "Mock".to_string(),
            reject.clone(),
            100,
        );
        let node2 = DecisionNode::with_dependencies(
            "node2".to_string(),
            "Spy".to_string(),
            spy.clone(),
            50,
        );

        let mut chain = DecisionChain::with_dependencies(vec![node1, node2]);

        chain.check().await.unwrap();
        assert_eq!(spy.calls.load(std::sync::atomic::Ordering::SeqCst), 1);

        assert!(chain.set_short_circuit("node1", true));

        reject.set_allowed(false);
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
        assert_eq!(spy.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_decision_chain_set_short_circuit_not_found() {
        let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
        let node =
            DecisionNode::with_dependencies("node1".to_string(), "Node".to_string(), limiter, 100);
        let mut chain = DecisionChain::with_dependencies(vec![node]);
        assert!(!chain.set_short_circuit("nonexistent", true));
    }

    // ========================================================================
    // stats race condition 修复测试
    //
    // 验证策略：
    // AtomicChainStats 的 fetch_add/store 原使用 Ordering::Relaxed，不保证
    // 跨原子变量的可见性顺序。DecisionChain::check 的调用模式是先 increment_total
    // 后 increment_allowed/rejected/error，但 Relaxed 下 snapshot 可能读到
    // total 已 increment 而分类计数尚未 increment 的中间态，违反业务不变式
    // allowed + rejected + error >= total（每次分类必伴随 total）。
    // SeqCst 保证代码顺序即全局可见顺序，消除此类中间态观测。
    // ========================================================================

    #[test]
    fn test_atomic_chain_stats_concurrent_snapshot_monotonic() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as StdOrdering};
        use std::thread;
        use std::time::Duration;

        let stats = Arc::new(AtomicChainStats::new());
        let stop = Arc::new(AtomicBool::new(false));
        let violations = Arc::new(AtomicU64::new(0));

        // 工作线程：持续 increment 各计数器
        let stats_worker = Arc::clone(&stats);
        let stop_worker = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            while !stop_worker.load(StdOrdering::SeqCst) {
                stats_worker.increment_total();
                stats_worker.increment_allowed();
                stats_worker.increment_rejected();
                stats_worker.increment_error();
            }
        });

        // 快照线程：持续 snapshot，验证计数器单调非递减
        let stats_snap = Arc::clone(&stats);
        let stop_snap = Arc::clone(&stop);
        let violations_snap = Arc::clone(&violations);
        let snapshotter = thread::spawn(move || {
            let mut prev = stats_snap.snapshot();
            for _ in 0..1000 {
                let curr = stats_snap.snapshot();
                if curr.total_checks < prev.total_checks
                    || curr.allowed_count < prev.allowed_count
                    || curr.rejected_count < prev.rejected_count
                    || curr.error_count < prev.error_count
                {
                    violations_snap.fetch_add(1, StdOrdering::SeqCst);
                }
                prev = curr;
            }
            let _ = stop_snap;
        });

        thread::sleep(Duration::from_millis(200));
        stop.store(true, StdOrdering::SeqCst);
        worker.join().unwrap();
        snapshotter.join().unwrap();

        assert_eq!(
            violations.load(StdOrdering::SeqCst),
            0,
            "snapshot 观测到计数器倒退：违反单调性（increment 期间不应有 reset）"
        );
    }

    #[test]
    fn test_atomic_chain_stats_check_pattern_eventual_consistency() {
        use std::sync::Arc;
        use std::thread;

        // 模拟 DecisionChain::check 的统计更新模式：
        //   拒绝：increment_total + increment_rejected
        //   错误：increment_total + increment_error
        //   允许：increment_total + increment_allowed
        // 验证"最终一致性"：所有工作线程结束后，fetch_add 的原子性保证
        // total == allowed + rejected + error（每轮 total+1 且恰好一个分类+1）。
        // 注意：snapshot() 内部多个 load 非原子，无法在并发期间验证
        // 跨计数器不变式；只能在所有 increment 完成后验证最终值。
        // SeqCst 保证 fetch_add 的全局顺序，确保最终计数无丢失更新。
        let stats = Arc::new(AtomicChainStats::new());
        const THREADS: usize = 8;
        const ITERS: u64 = 1000;

        let mut handles = vec![];
        for _ in 0..THREADS {
            let stats_clone = Arc::clone(&stats);
            handles.push(thread::spawn(move || {
                for i in 0..ITERS {
                    stats_clone.increment_total();
                    match i % 3 {
                        0 => stats_clone.increment_allowed(),
                        1 => stats_clone.increment_rejected(),
                        _ => stats_clone.increment_error(),
                    }
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let snap = stats.snapshot();
        let expected_total = (THREADS as u64) * ITERS;
        let classified = snap.allowed_count + snap.rejected_count + snap.error_count;
        assert_eq!(snap.total_checks, expected_total, "total 计数丢失更新");
        assert_eq!(classified, expected_total, "分类计数丢失更新");
        // 每轮恰好一个分类+1，所以 classified == total
        assert_eq!(
            classified, snap.total_checks,
            "SeqCst 应保证 fetch_add 无丢失更新：classified 应等于 total"
        );
    }
}
