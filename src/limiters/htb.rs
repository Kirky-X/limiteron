// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! HTB 分层令牌桶（T615）：层次令牌桶，父/子借用。
//!
//! 语义对齐 Linux HTB（Hierarchical Token Bucket）的核心思想：
//! - 树状桶结构：根桶（总量）→ 分类桶（子类）→ 叶子桶（具体限流对象）；
//! - 消费优先取**自身令牌**；不足时向**祖先借**（borrow）——叶子长期速率
//!   因此被祖先容量约束（子类突发不得超过父类剩余预算）；
//! - 兄弟桶之间相互隔离：A 借用只消耗**祖先**令牌，不触碰兄弟；
//! - 借用是全有或全无：祖先不足以补齐缺口则整笔拒绝（无部分消费，
//!   决策可重试），调用方无需回滚。
//!
//! # Example
//!
//! ```
//! use limiteron::limiters::HierarchicalTokenBucket;
//!
//! # tokio_test::block_on(async {
//! // 根 100；子类 api 60；叶子 premium 40
//! // refill_rate = 0（静态预算）保证示例确定性
//! let htb = HierarchicalTokenBucket::new(20, 0);
//! htb.add_class(&["api"], 20, 0).unwrap();
//! htb.add_class(&["api", "premium"], 40, 0).unwrap();
//!
//! assert!(htb.allow(&["api", "premium"], 10).await.unwrap());
//! assert_eq!(htb.available(&["api", "premium"]).unwrap(), 30);
//! // 子桶只剩 30：请求 40 → 缺口 10 向父桶借
//! assert!(htb.allow(&["api", "premium"], 40).await.unwrap());
//! // 链上剩余 0 + 20 + 20 = 40 < 50 → 全有或全无拒绝
//! assert!(!htb.allow(&["api", "premium"], 50).await.unwrap());
//! # });
//! ```

use dashmap::DashMap;
use parking_lot::Mutex;

use crate::error::LimiteronError;

use super::traits::Limiter;

/// 单个桶（节点）：容量 + 补充速率 + 当前令牌
#[derive(Debug)]
struct HtbBucket {
    capacity: u64,
    /// 每秒补充速率
    refill_rate: u64,
    state: Mutex<BucketState>,
}

#[derive(Debug)]
struct BucketState {
    tokens: f64,
    last_refill: std::time::Instant,
}

impl HtbBucket {
    fn new(capacity: u64, refill_rate: u64) -> Self {
        Self {
            capacity,
            refill_rate,
            state: Mutex::new(BucketState {
                tokens: capacity as f64,
                last_refill: std::time::Instant::now(),
            }),
        }
    }

    /// 按流逝时间补充令牌（上限容量）；返回当前可用
    fn refill_and_available(&self) -> u64 {
        let mut st = self.state.lock();
        let elapsed = st.last_refill.elapsed().as_secs_f64();
        if elapsed > 0.0 && self.refill_rate > 0 {
            st.tokens = (st.tokens + elapsed * self.refill_rate as f64).min(self.capacity as f64);
            st.last_refill = std::time::Instant::now();
        }
        st.tokens as u64
    }

    fn available(&self) -> u64 {
        self.refill_and_available()
    }

    fn take(&self, amount: u64) {
        let mut st = self.state.lock();
        st.tokens = (st.tokens - amount as f64).max(0.0);
    }

    fn give(&self, amount: u64) {
        let mut st = self.state.lock();
        st.tokens = (st.tokens + amount as f64).min(self.capacity as f64);
    }
}

/// HTB 分层令牌桶
///
/// 根桶 + 命名分类桶树；决策热路径仅哈希查找 + 无竞争 Mutex，无后台线程。
pub struct HierarchicalTokenBucket {
    root: std::sync::Arc<HtbBucket>,
    children: DashMap<String, ClassNode>,
}

#[derive(Clone)]
struct ClassNode {
    bucket: std::sync::Arc<HtbBucket>,
    children: std::sync::Arc<DashMap<String, ClassNode>>,
}

impl Default for HierarchicalTokenBucket {
    fn default() -> Self {
        Self::new(1_000_000, 1_000_000)
    }
}

impl HierarchicalTokenBucket {
    /// 以根桶容量/速率创建
    pub fn new(root_capacity: u64, root_refill_rate: u64) -> Self {
        Self {
            root: std::sync::Arc::new(HtbBucket::new(root_capacity, root_refill_rate)),
            children: DashMap::new(),
        }
    }

    /// 根桶容量
    pub fn root_capacity(&self) -> u64 {
        self.root.capacity
    }

    /// 注册分类桶（`path` 不得为空，根桶由构造函数给定）。
    ///
    /// 同名重复注册按 last-config-wins 重建（控制面操作，容量即语义）。
    /// # Errors
    /// 路径中间节点不存在时返回 `ConfigError`（须先注册父类）。
    pub fn add_class(
        &self,
        path: &[&str],
        capacity: u64,
        refill_rate: u64,
    ) -> Result<(), LimiteronError> {
        if path.is_empty() {
            return Err(LimiteronError::ConfigError(
                "HTB: class path cannot be empty (root is implicit)".to_string(),
            ));
        }
        let mut current: Option<ClassNode> = None;
        for (i, seg) in path.iter().enumerate() {
            let is_last = i == path.len() - 1;
            let map: &DashMap<String, ClassNode> = match &current {
                Some(n) => &n.children,
                None => &self.children,
            };
            // 先以 owned Option 提取，结束 map 借用后再写入 current
            let existing: Option<ClassNode> = map.get(*seg).map(|e| e.value().clone());
            match existing {
                Some(next) => {
                    if is_last {
                        // last-config-wins：重建叶子桶
                        map.insert((*seg).to_string(), Self::make_node(capacity, refill_rate));
                        return Ok(());
                    }
                    current = Some(next);
                }
                None => {
                    if is_last {
                        map.insert((*seg).to_string(), Self::make_node(capacity, refill_rate));
                        return Ok(());
                    }
                    return Err(LimiteronError::ConfigError(format!(
                        "HTB: parent class '{seg}' not registered yet (path must be added top-down)"
                    )));
                }
            }
        }
        Ok(())
    }

    fn make_node(capacity: u64, refill_rate: u64) -> ClassNode {
        ClassNode {
            bucket: std::sync::Arc::new(HtbBucket::new(capacity, refill_rate)),
            children: std::sync::Arc::new(DashMap::new()),
        }
    }

    /// 沿路径定位叶子桶；路径为空 = 根桶本身
    fn find(&self, path: &[&str]) -> Option<ClassNode> {
        let mut node = ClassNode {
            bucket: self.root.clone(),
            children: std::sync::Arc::new(self.children.clone()),
        };
        for seg in path {
            let next = node.children.get(*seg)?.value().clone();
            node = next;
        }
        Some(node)
    }

    /// 消费 `cost`：叶子自身令牌优先，缺口向祖先借用（全有或全无）。
    ///
    /// 返回 `false` 表示任一环节预算不足（状态不变）。
    pub async fn allow(&self, class_path: &[&str], cost: u64) -> Result<bool, LimiteronError> {
        if cost == 0 {
            return Err(LimiteronError::ConfigError(
                "HTB cost cannot be zero".to_string(),
            ));
        }

        // 自底向上收集链：leaf → ... → root
        let mut chain: Vec<std::sync::Arc<HtbBucket>> = Vec::with_capacity(class_path.len() + 1);
        let mut node = ClassNode {
            bucket: self.root.clone(),
            children: std::sync::Arc::new(self.children.clone()),
        };
        chain.push(node.bucket.clone());
        for seg in class_path {
            // 先以 owned Option 提取，结束借用后再替换 node
            let next: Option<ClassNode> = node.children.get(*seg).map(|e| e.value().clone());
            match next {
                Some(n) => {
                    chain.push(n.bucket.clone());
                    node = n;
                }
                None => {
                    return Err(LimiteronError::ConfigError(format!(
                        "HTB: class '{seg}' not registered"
                    )));
                }
            }
        }

        // 令牌刷新 + 自身可用
        let leaf = chain[chain.len() - 1].clone();
        let own = leaf.available();
        let deficit = cost.saturating_sub(own);
        if deficit == 0 {
            leaf.take(cost);
            return Ok(true);
        }

        // 向祖先逐级借：全有或全无（不足则整体拒绝，零副作用）
        let mut remaining = deficit;
        let mut plan: Vec<(std::sync::Arc<HtbBucket>, u64)> = Vec::new();
        for and in chain[..chain.len() - 1].iter().rev() {
            if remaining == 0 {
                break;
            }
            let avail = and.available();
            let lend = avail.min(remaining);
            if lend > 0 {
                plan.push((and.clone(), lend));
                remaining -= lend;
            }
        }
        if remaining > 0 {
            return Ok(false);
        }

        // 执行：祖先扣减 → 叶子补齐 → 叶子消费 cost
        for (and, lend) in &plan {
            and.take(*lend);
        }
        leaf.give(deficit);
        leaf.take(cost);
        Ok(true)
    }

    /// 查询叶子（或根）的可用令牌（非消费）
    pub fn available(&self, class_path: &[&str]) -> Result<u64, LimiteronError> {
        self.find(class_path)
            .map(|n| n.bucket.available())
            .ok_or_else(|| {
                LimiteronError::ConfigError("HTB: class path not registered".to_string())
            })
    }
}

#[async_trait::async_trait]
impl Limiter for HierarchicalTokenBucket {
    /// 消费 1 个令牌（默认分类：根桶）
    async fn allow(&self, cost: u64) -> Result<bool, LimiteronError> {
        self.allow(&[], cost).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 注：除 refill 专项测试外，全部用 refill_rate=0（静态预算）保证断言确定性

    /// 叶子自身令牌充足 → 直接消费，父桶不受影响
    #[tokio::test]
    async fn test_t615_htb_consumes_own_tokens_first() {
        let htb = HierarchicalTokenBucket::new(1000, 0);
        htb.add_class(&["api"], 600, 0).unwrap();
        htb.add_class(&["api", "premium"], 100, 0).unwrap();

        assert!(htb.allow(&["api", "premium"], 40).await.unwrap());
        assert_eq!(htb.available(&["api", "premium"]).unwrap(), 60);
        assert_eq!(htb.available(&["api"]).unwrap(), 600, "父桶未被触碰");
        assert_eq!(htb.available(&[]).unwrap(), 1000, "根桶未被触碰");
    }

    /// 叶子耗尽 → 向父借用补齐（父扣减，叶子清零后消费）
    #[tokio::test]
    async fn test_t615_htb_borrows_from_parent() {
        let htb = HierarchicalTokenBucket::new(1000, 0);
        htb.add_class(&["api"], 50, 0).unwrap();
        htb.add_class(&["api", "premium"], 10, 0).unwrap();

        assert!(htb.allow(&["api", "premium"], 10).await.unwrap());
        assert_eq!(htb.available(&["api", "premium"]).unwrap(), 0);

        // 缺口 5，父桶有 50 → 借用成功
        assert!(htb.allow(&["api", "premium"], 5).await.unwrap());
        assert_eq!(htb.available(&["api"]).unwrap(), 45, "父桶被借走 5");
    }

    /// 祖先也不足 → 全有或全无拒绝（零副作用，可重试）
    #[tokio::test]
    async fn test_t615_htb_all_or_nothing_denial() {
        // 链总预算 = 叶 10 + 父 5 + 根 5 = 20 < 30 → 拒绝（零副作用）
        let htb = HierarchicalTokenBucket::new(5, 0);
        htb.add_class(&["api"], 5, 0).unwrap();
        htb.add_class(&["api", "premium"], 10, 0).unwrap();

        assert!(htb.allow(&["api", "premium"], 10).await.unwrap());
        // 叶子 0 + 父 5 + 根 5 = 10 < 30 → 拒绝
        assert!(!htb.allow(&["api", "premium"], 30).await.unwrap());
        // 状态未变：父桶仍 5、根桶仍 5（可重试语义）
        assert_eq!(htb.available(&["api"]).unwrap(), 5);
        assert_eq!(htb.available(&[]).unwrap(), 5);
    }

    /// 兄弟隔离：A 借用只消耗祖先令牌，不触碰兄弟 B
    #[tokio::test]
    async fn test_t615_htb_sibling_isolation() {
        let htb = HierarchicalTokenBucket::new(1000, 0);
        htb.add_class(&["api"], 100, 0).unwrap();
        htb.add_class(&["api", "a"], 10, 0).unwrap();
        htb.add_class(&["api", "b"], 10, 0).unwrap();

        assert!(htb.allow(&["api", "a"], 10).await.unwrap()); // a 耗尽
        assert!(htb.allow(&["api", "a"], 5).await.unwrap()); // a 借父 5

        assert_eq!(
            htb.available(&["api", "b"]).unwrap(),
            10,
            "兄弟桶 b 的令牌不得被 a 的借用触碰"
        );
    }

    /// 多级借用：叶子沿祖先逐级借（父 → 根），祖先耗尽后拒绝
    #[tokio::test]
    async fn test_t615_htb_multi_level_borrow_order() {
        let htb = HierarchicalTokenBucket::new(10, 0);
        htb.add_class(&["api"], 10, 0).unwrap();
        htb.add_class(&["api", "premium"], 10, 0).unwrap();

        // 叶 10（自身）+ 借父 10 + 借根 5 = 25
        assert!(htb.allow(&["api", "premium"], 25).await.unwrap());
        assert_eq!(htb.available(&["api", "premium"]).unwrap(), 0);
        assert_eq!(htb.available(&["api"]).unwrap(), 0);
        assert_eq!(htb.available(&[]).unwrap(), 5);

        // 请求 10 > 剩余祖先预算 5 → 拒绝
        assert!(!htb.allow(&["api", "premium"], 10).await.unwrap());
        // 请求 5 = 剩余根预算 → 恰好借尽
        assert!(htb.allow(&["api", "premium"], 5).await.unwrap());
        assert_eq!(htb.available(&[]).unwrap(), 0);
    }

    /// 未注册路径 → ConfigError（显性失败）
    #[tokio::test]
    async fn test_t615_htb_unregistered_path_is_error() {
        let htb = HierarchicalTokenBucket::new(100, 100);
        let err = htb.allow(&["ghost"], 1).await.unwrap_err();
        assert!(matches!(err, LimiteronError::ConfigError(_)));
        assert!(
            htb.add_class(&["a", "ghost"], 10, 10).is_err(),
            "须先注册父类"
        );
    }

    /// 补充速率：耗尽后按 rate/s 随时间恢复（上限容量）
    #[tokio::test]
    async fn test_t615_htb_refills_over_time() {
        // 100/s → 100ms 恢复 10 个令牌（恰达容量上限，任何时钟偏移只会更多）
        let htb = HierarchicalTokenBucket::new(100, 0);
        htb.add_class(&["api"], 10, 100).unwrap();
        assert!(htb.allow(&["api"], 10).await.unwrap());
        assert_eq!(htb.available(&["api"]).unwrap(), 0);
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert_eq!(
            htb.available(&["api"]).unwrap(),
            10,
            "150ms @ 100/s → 至少 10 个令牌（封顶容量）"
        );
    }

    /// cost=0 → ConfigError（与 Limiter 校验语义一致）
    #[tokio::test]
    async fn test_t615_htb_zero_cost_rejected() {
        let htb = HierarchicalTokenBucket::new(100, 100);
        assert!(htb.allow(&[], 0).await.is_err());
    }

    /// Limiter trait 桥接：默认根桶路径消费
    #[tokio::test]
    async fn test_t615_htb_limiter_trait_bridge() {
        let htb = HierarchicalTokenBucket::new(5, 5);
        Limiter::allow(&htb, 5).await.unwrap();
        assert!(!Limiter::allow(&htb, 1).await.unwrap());
    }
}
