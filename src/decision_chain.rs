//! 决策链模块
//!
//! 使用责任链模式实现多限流器组合决策。
//!
//! # 特性
//!
//! - 责任链模式：支持链式调用多个限流器
//! - 短路逻辑：任一拒绝则立即返回拒绝
//! - 优先级排序：按优先级顺序执行限流器
//! - 决策聚合：聚合所有限流器的决策结果
//! - 可扩展：易于添加新的限流器类型

use crate::error::{Decision, FlowGuardError};
use crate::limiters::Limiter;
use std::sync::Arc;
use tracing::{debug, info, trace, warn};

// ============================================================================
// 决策链节点
// ============================================================================

/// 决策链节点
///
/// 责任链中的单个节点，包含一个限流器和相关配置。
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
    /// 创建新的决策节点
    ///
    /// # 参数
    /// - `id`: 节点ID
    /// - `name`: 节点名称
    /// - `limiter`: 限流器
    /// - `priority`: 优先级
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::decision_chain::DecisionNode;
    /// use limiteron::limiters::TokenBucketLimiter;
    /// use std::sync::Arc;
    ///
    /// let limiter = Arc::new(TokenBucketLimiter::new(100, 10));
    /// let node = DecisionNode::new(
    ///     "node1".to_string(),
    ///     "Token Bucket".to_string(),
    ///     limiter,
    ///     100,
    /// );
    /// ```
    pub fn new(id: String, name: String, limiter: Arc<dyn Limiter>, priority: u16) -> Self {
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

    /// 执行限流检查
    ///
    /// # 返回
    /// - `Ok(allowed)`: 是否允许
    /// - `Err(_)`: 错误
    async fn check(&self) -> Result<bool, FlowGuardError> {
        if !self.enabled {
            debug!("DecisionNode {} is disabled, skipping", self.id);
            return Ok(true);
        }

        trace!(
            "Checking decision node: {} (cost: {})",
            self.name,
            self.cost
        );
        self.limiter.allow(self.cost).await
    }
}

// ============================================================================
// 决策链
// ============================================================================

/// 决策链
///
/// 使用责任链模式实现多限流器组合决策。
pub struct DecisionChain {
    /// 链中的节点（按优先级排序）
    nodes: Vec<DecisionNode>,
    /// 统计信息
    stats: Arc<std::sync::RwLock<ChainStats>>,
}

/// 决策链统计信息
#[derive(Debug, Clone, Default)]
pub struct ChainStats {
    /// 总检查次数
    pub total_checks: u64,
    /// 允许次数
    pub allowed_count: u64,
    /// 拒绝次数
    pub rejected_count: u64,
    /// 各节点的拒绝次数
    pub node_rejections: Vec<(String, u64)>,
}

impl DecisionChain {
    /// 创建新的决策链
    ///
    /// # 参数
    /// - `nodes`: 决策节点列表
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::decision_chain::{DecisionChain, DecisionNode};
    /// use limiteron::limiters::TokenBucketLimiter;
    /// use std::sync::Arc;
    ///
    /// let limiter = Arc::new(TokenBucketLimiter::new(100, 10));
    /// let node = DecisionNode::new(
    ///     "node1".to_string(),
    ///     "Token Bucket".to_string(),
    ///     limiter,
    ///     100,
    /// );
    ///
    /// let chain = DecisionChain::new(vec![node]);
    /// ```
    pub fn new(nodes: Vec<DecisionNode>) -> Self {
        let mut chain = Self {
            nodes: Vec::new(),
            stats: Arc::new(std::sync::RwLock::new(ChainStats::default())),
        };

        for node in nodes {
            chain.add_node(node);
        }

        chain
    }

    /// 添加节点
    ///
    /// # 参数
    /// - `node`: 决策节点
    pub fn add_node(&mut self, node: DecisionNode) {
        // 按优先级排序（降序）
        let pos = self
            .nodes
            .binary_search_by(|n| n.priority.cmp(&node.priority).reverse())
            .unwrap_or_else(|pos| pos);

        self.nodes.insert(pos, node);
    }

    /// 移除节点
    ///
    /// # 参数
    /// - `node_id`: 节点ID
    pub fn remove_node(&mut self, node_id: &str) -> Option<DecisionNode> {
        if let Some(pos) = self.nodes.iter().position(|n| n.id == node_id) {
            Some(self.nodes.remove(pos))
        } else {
            None
        }
    }

    /// 执行决策链检查
    ///
    /// 按优先级顺序执行所有节点，任一节点拒绝则立即返回（如果启用了短路）。
    ///
    /// # 返回
    /// - `Ok(Decision::Allowed(None))`: 所有节点都允许
    /// - `Ok(Decision::Rejected)`: 至少一个节点拒绝
    /// - `Err(_)`: 发生错误
    ///
    /// # 示例
    /// ```rust
    /// use limiteron::decision_chain::DecisionChain;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let chain = DecisionChain::new(vec![]);
    ///     let decision = chain.check().await.unwrap();
    /// }
    /// ```
    pub async fn check(&self) -> Result<Decision, FlowGuardError> {
        {
            let mut stats = self.stats.write().unwrap();
            stats.total_checks += 1;
        }

        debug!(
            "Starting decision chain check with {} nodes",
            self.nodes.len()
        );

        // 按优先级顺序检查每个节点
        for node in &self.nodes {
            if !node.enabled {
                trace!("Skipping disabled node: {}", node.id);
                continue;
            }

            trace!("Checking node: {}", node.name);

            match node.check().await {
                Ok(true) => {
                    trace!("Node {} allowed", node.name);
                    // 继续检查下一个节点
                }
                Ok(false) => {
                    // 节点拒绝
                    warn!("Node {} rejected request", node.name);

                    {
                        let mut stats = self.stats.write().unwrap();
                        stats.rejected_count += 1;

                        // 更新节点拒绝统计
                        if let Some(pos) = stats
                            .node_rejections
                            .iter()
                            .position(|(id, _)| id == &node.id)
                        {
                            stats.node_rejections[pos].1 += 1;
                        } else {
                            stats.node_rejections.push((node.id.clone(), 1));
                        }
                    }

                    // 如果启用了短路，立即返回
                    if node.short_circuit {
                        info!("Decision chain short-circuited by node: {}", node.name);
                        return Ok(Decision::Rejected(format!(
                            "Rejected by {}: rate limit exceeded",
                            node.name
                        )));
                    }
                }
                Err(e) => {
                    // 发生错误
                    warn!("Node {} check failed: {:?}", node.name, e);
                    return Err(e);
                }
            }
        }

        // 所有节点都允许
        {
            let mut stats = self.stats.write().unwrap();
            stats.allowed_count += 1;
        }

        debug!("Decision chain: all nodes allowed");
        Ok(Decision::Allowed(None))
    }

    /// 执行完整检查（不短路）
    ///
    /// 检查所有节点，聚合所有拒绝结果。
    ///
    /// # 返回
    /// - `Ok(Decision::Allowed(None))`: 所有节点都允许
    /// - `Ok(Decision::Rejected)`: 至少一个节点拒绝，包含所有拒绝原因
    /// - `Err(_)`: 发生错误
    pub async fn check_all(&self) -> Result<Decision, FlowGuardError> {
        {
            let mut stats = self.stats.write().unwrap();
            stats.total_checks += 1;
        }

        debug!(
            "Starting full decision chain check with {} nodes",
            self.nodes.len()
        );

        let mut rejection_reasons = Vec::new();

        // 检查所有节点
        for node in &self.nodes {
            if !node.enabled {
                trace!("Skipping disabled node: {}", node.id);
                continue;
            }

            trace!("Checking node: {}", node.name);

            match node.check().await {
                Ok(true) => {
                    trace!("Node {} allowed", node.name);
                }
                Ok(false) => {
                    warn!("Node {} rejected request", node.name);
                    rejection_reasons.push(format!("{}: rate limit exceeded", node.name));

                    // 更新统计
                    {
                        let mut stats = self.stats.write().unwrap();
                        stats.rejected_count += 1;

                        if let Some(pos) = stats
                            .node_rejections
                            .iter()
                            .position(|(id, _)| id == &node.id)
                        {
                            stats.node_rejections[pos].1 += 1;
                        } else {
                            stats.node_rejections.push((node.id.clone(), 1));
                        }
                    }
                }
                Err(e) => {
                    warn!("Node {} check failed: {:?}", node.name, e);
                    return Err(e);
                }
            }
        }

        // 返回结果
        if rejection_reasons.is_empty() {
            {
                let mut stats = self.stats.write().unwrap();
                stats.allowed_count += 1;
            }

            debug!("Decision chain: all nodes allowed");
            Ok(Decision::Allowed(None))
        } else {
            let reason = rejection_reasons.join("; ");
            info!("Decision chain rejected: {}", reason);
            Ok(Decision::Rejected(reason))
        }
    }

    /// 获取统计信息
    pub fn stats(&self) -> ChainStats {
        self.stats.read().unwrap().clone()
    }

    /// 重置统计信息
    pub fn reset_stats(&self) {
        let mut stats = self.stats.write().unwrap();
        *stats = ChainStats::default();
    }

    /// 获取节点数量
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 获取启用的节点数量
    pub fn enabled_node_count(&self) -> usize {
        self.nodes.iter().filter(|n| n.enabled).count()
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
        DecisionChain::new(self.nodes)
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
mod tests {
    use super::*;
    use crate::limiters::{
        ConcurrencyLimiter, FixedWindowLimiter, SlidingWindowLimiter, TokenBucketLimiter,
    };
    use std::time::Duration;

    // ==================== DecisionNode 测试 ====================

    #[test]
    fn test_decision_node_creation() {
        let limiter = Arc::new(TokenBucketLimiter::new(100, 10));
        let node = DecisionNode::new(
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
        let node = DecisionNode::new(
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
        let chain = DecisionChain::new(vec![]);
        let decision = chain.check().await.unwrap();

        assert_eq!(decision, Decision::Allowed(None));
    }

    #[tokio::test]
    async fn test_decision_chain_single_node() {
        let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
        let node = DecisionNode::new(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        );

        let chain = DecisionChain::new(vec![node]);

        // 前10个请求应该被允许
        for _ in 0..10 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::Allowed(None));
        }

        // 第11个请求应该被拒绝
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    #[tokio::test]
    async fn test_decision_chain_multiple_nodes() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(5, 1));
        let limiter2 = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(1), 10));

        let node1 = DecisionNode::new(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter1,
            100,
        );

        let node2 = DecisionNode::new(
            "node2".to_string(),
            "Sliding Window".to_string(),
            limiter2,
            50,
        );

        let chain = DecisionChain::new(vec![node1, node2]);

        // 前5个请求应该被允许
        for _ in 0..5 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::Allowed(None));
        }

        // 第6个请求应该被更高优先级的node1拒绝
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    #[tokio::test]
    async fn test_decision_chain_priority() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(10, 1));
        let limiter2 = Arc::new(TokenBucketLimiter::new(5, 1));

        let node1 = DecisionNode::new(
            "node1".to_string(),
            "Low Priority".to_string(),
            limiter1,
            50,
        );

        let node2 = DecisionNode::new(
            "node2".to_string(),
            "High Priority".to_string(),
            limiter2,
            100,
        );

        let chain = DecisionChain::new(vec![node1, node2]);

        // 高优先级的node2应该先被检查
        for _ in 0..5 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::Allowed(None));
        }

        // node2应该先拒绝
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));

        // 验证拒绝原因来自node2
        if let Decision::Rejected(reason) = decision {
            assert!(reason.contains("High Priority"));
        }
    }

    #[tokio::test]
    async fn test_decision_chain_disabled_node() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(0, 1));
        let limiter2 = Arc::new(TokenBucketLimiter::new(10, 1));

        let node1 = DecisionNode::new(
            "node1".to_string(),
            "Disabled Node".to_string(),
            limiter1,
            100,
        )
        .with_enabled(false);

        let node2 = DecisionNode::new("node2".to_string(), "Active Node".to_string(), limiter2, 50);

        let chain = DecisionChain::new(vec![node1, node2]);

        // node1被禁用，应该检查node2
        let decision = chain.check().await.unwrap();
        assert_eq!(decision, Decision::Allowed(None));
    }

    #[tokio::test]
    async fn test_decision_chain_short_circuit() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(5, 1));
        let limiter2 = Arc::new(TokenBucketLimiter::new(10, 1));

        let node1 = DecisionNode::new("node1".to_string(), "First Node".to_string(), limiter1, 100);

        let node2 = DecisionNode::new("node2".to_string(), "Second Node".to_string(), limiter2, 50);

        let chain = DecisionChain::new(vec![node1, node2]);

        // 前5个请求应该被允许
        for _ in 0..5 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::Allowed(None));
        }

        // 第6个请求应该被node1拒绝，并短路
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    #[tokio::test]
    async fn test_decision_chain_no_short_circuit() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(5, 1));
        let limiter2 = Arc::new(TokenBucketLimiter::new(3, 1));

        let node1 = DecisionNode::new("node1".to_string(), "First Node".to_string(), limiter1, 100)
            .with_short_circuit(false);

        let node2 = DecisionNode::new("node2".to_string(), "Second Node".to_string(), limiter2, 50);

        let chain = DecisionChain::new(vec![node1, node2]);

        // 前3个请求应该被允许
        for _ in 0..3 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::Allowed(None));
        }

        // 第4个请求应该被node2拒绝
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    #[tokio::test]
    async fn test_decision_chain_check_all() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(3, 1));
        let limiter2 = Arc::new(TokenBucketLimiter::new(5, 1));

        let node1 = DecisionNode::new("node1".to_string(), "First Node".to_string(), limiter1, 100);

        let node2 = DecisionNode::new("node2".to_string(), "Second Node".to_string(), limiter2, 50);

        let chain = DecisionChain::new(vec![node1, node2]);

        // 前3个请求应该被允许
        for _ in 0..3 {
            let decision = chain.check_all().await.unwrap();
            assert_eq!(decision, Decision::Allowed(None));
        }

        // 第4个请求应该检查所有节点
        let decision = chain.check_all().await.unwrap();
        if let Decision::Rejected(reason) = decision {
            // 应该包含两个节点的拒绝原因
            assert!(reason.contains("First Node"));
        }
    }

    #[tokio::test]
    async fn test_decision_chain_stats() {
        let limiter = Arc::new(TokenBucketLimiter::new(5, 1));
        let node = DecisionNode::new(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        );

        let chain = DecisionChain::new(vec![node]);

        // 发送10个请求
        for _ in 0..10 {
            chain.check().await.unwrap();
        }

        let stats = chain.stats();
        assert_eq!(stats.total_checks, 10);
        assert_eq!(stats.allowed_count, 5);
        assert_eq!(stats.rejected_count, 5);
    }

    #[tokio::test]
    async fn test_decision_chain_node_rejections() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(5, 1));
        let limiter2 = Arc::new(TokenBucketLimiter::new(3, 1));

        let node1 = DecisionNode::new("node1".to_string(), "First Node".to_string(), limiter1, 100)
            .with_short_circuit(false);

        let node2 = DecisionNode::new("node2".to_string(), "Second Node".to_string(), limiter2, 50);

        let chain = DecisionChain::new(vec![node1, node2]);

        // 发送10个请求
        for _ in 0..10 {
            chain.check_all().await.unwrap();
        }

        let stats = chain.stats();

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
    async fn test_decision_chain_add_remove_node() {
        let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
        let node = DecisionNode::new(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        );

        let mut chain = DecisionChain::new(vec![]);
        assert_eq!(chain.node_count(), 0);

        chain.add_node(node);
        assert_eq!(chain.node_count(), 1);

        chain.remove_node("node1");
        assert_eq!(chain.node_count(), 0);
    }

    #[tokio::test]
    async fn test_decision_chain_enable_disable_node() {
        let limiter = Arc::new(TokenBucketLimiter::new(0, 1));
        let node = DecisionNode::new(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        );

        let mut chain = DecisionChain::new(vec![node]);

        // 禁用节点
        chain.disable_node("node1");
        assert_eq!(chain.enabled_node_count(), 0);

        let decision = chain.check().await.unwrap();
        assert_eq!(decision, Decision::Allowed(None));

        // 启用节点
        chain.enable_node("node1");
        assert_eq!(chain.enabled_node_count(), 1);

        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    #[tokio::test]
    async fn test_decision_chain_set_short_circuit() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(5, 1));
        let limiter2 = Arc::new(TokenBucketLimiter::new(0, 1));

        let node1 = DecisionNode::new("node1".to_string(), "First Node".to_string(), limiter1, 100)
            .with_short_circuit(false);

        let node2 = DecisionNode::new("node2".to_string(), "Second Node".to_string(), limiter2, 50);

        let mut chain = DecisionChain::new(vec![node1, node2]);

        // 前5个请求应该被允许
        for _ in 0..5 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::Allowed(None));
        }

        // 第6个请求，node1拒绝但不短路，node2也会拒绝
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));

        // 启用node1的短路
        chain.set_short_circuit("node1", true);

        // 重置统计
        chain.reset_stats();

        // 前5个请求应该被允许
        for _ in 0..5 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::Allowed(None));
        }

        // 第6个请求，node1拒绝并短路
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    // ==================== DecisionChainBuilder 测试 ====================

    #[test]
    fn test_decision_chain_builder() {
        let limiter1 = Arc::new(TokenBucketLimiter::new(100, 10));
        let limiter2 = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(1), 100));

        let node1 = DecisionNode::new(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter1,
            100,
        );

        let node2 = DecisionNode::new(
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
        let limiter2 = Arc::new(SlidingWindowLimiter::new(Duration::from_secs(1), 5));
        let limiter3 = Arc::new(FixedWindowLimiter::new(Duration::from_secs(1), 3));
        let limiter4 = Arc::new(ConcurrencyLimiter::new(2));

        let node1 = DecisionNode::new(
            "token_bucket".to_string(),
            "Token Bucket".to_string(),
            limiter1,
            100,
        );

        let node2 = DecisionNode::new(
            "sliding_window".to_string(),
            "Sliding Window".to_string(),
            limiter2,
            75,
        );

        let node3 = DecisionNode::new(
            "fixed_window".to_string(),
            "Fixed Window".to_string(),
            limiter3,
            50,
        );

        let node4 = DecisionNode::new(
            "concurrency".to_string(),
            "Concurrency".to_string(),
            limiter4,
            25,
        );

        let chain = DecisionChain::new(vec![node1, node2, node3, node4]);

        // 第一个请求应该被允许
        let decision = chain.check().await.unwrap();
        assert_eq!(decision, Decision::Allowed(None));

        // 检查统计
        let stats = chain.stats();
        assert_eq!(stats.total_checks, 1);
        assert_eq!(stats.allowed_count, 1);
    }

    #[tokio::test]
    async fn test_decision_chain_cost() {
        let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
        let node = DecisionNode::new(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        )
        .with_cost(2);

        let chain = DecisionChain::new(vec![node]);

        // 5个请求，每个消耗2个令牌
        for _ in 0..5 {
            let decision = chain.check().await.unwrap();
            assert_eq!(decision, Decision::Allowed(None));
        }

        // 第6个请求应该被拒绝（总共消耗了10个令牌）
        let decision = chain.check().await.unwrap();
        assert!(matches!(decision, Decision::Rejected(_)));
    }

    #[tokio::test]
    async fn test_decision_chain_reset_stats() {
        let limiter = Arc::new(TokenBucketLimiter::new(10, 1));
        let node = DecisionNode::new(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        );

        let chain = DecisionChain::new(vec![node]);

        // 发送一些请求
        for _ in 0..5 {
            chain.check().await.unwrap();
        }

        // 检查统计
        let stats = chain.stats();
        assert_eq!(stats.total_checks, 5);

        // 重置统计
        chain.reset_stats();

        // 检查重置后的统计
        let stats = chain.stats();
        assert_eq!(stats.total_checks, 0);
    }

    #[tokio::test]
    async fn test_decision_chain_concurrent_checks() {
        let limiter = Arc::new(TokenBucketLimiter::new(100, 10));
        let node = DecisionNode::new(
            "node1".to_string(),
            "Token Bucket".to_string(),
            limiter,
            100,
        );

        let chain = Arc::new(DecisionChain::new(vec![node]));
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
            assert_eq!(result, Decision::Allowed(None));
        }

        // 检查统计
        let stats = chain.stats();
        assert_eq!(stats.total_checks, 10);
    }
}
