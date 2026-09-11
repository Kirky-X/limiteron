// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 舱壁隔离（T611，feature `bulkhead`）
//!
//! 按资源组（下游服务/租户/规则组）划分独立并发池：单池饱和只影响自身
//! （快速失败 [`BulkheadError::Full`]），不拖垮其他池——补齐弹性模式
//! 最后一块（熔断✅ / 重试✅ / 限流✅ / 舱壁✅）。
//!
//! - **分池**：[`BulkheadRegistry`] 按名 get-or-create [`Bulkhead`]，
//!   每池独立并发预算（[`BulkheadConfig::max_concurrent`]）
//! - **独立熔断**（`circuit-breaker` feature）：每池可选注入
//!   [`crate::CircuitBreaker`]，打开时该池快速失败
//! - **隔离指标**：每池 active / admitted / rejected 计数
//!   （[`BulkheadStats`]），供自省/监控消费
//!
//! # Example
//!
//! ```rust
//! use limiteron::bulkhead::{BulkheadConfig, BulkheadRegistry};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let registry = BulkheadRegistry::new()
//!     .with_pool_config("downstream-payment", BulkheadConfig { max_concurrent: 2 });
//!
//! let out = registry
//!     .execute("downstream-payment", async { 42 })
//!     .await?;
//! assert_eq!(out, 42);
//! # Ok(())
//! # }
//! ```

use crate::error::LimiteronError;
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// 舱壁池配置
#[derive(Debug, Clone, Copy)]
pub struct BulkheadConfig {
    /// 池内最大并发数（独立并发预算）
    pub max_concurrent: usize,
}

impl Default for BulkheadConfig {
    fn default() -> Self {
        Self { max_concurrent: 32 }
    }
}

/// 舱壁错误
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BulkheadError {
    /// 池已满：快速失败（不排队，避免线程/任务堆积）
    #[error("bulkhead '{0}' is full (max_concurrent exhausted)")]
    Full(String),
    /// 该池熔断器打开（circuit-breaker feature 联动）
    #[cfg(feature = "circuit-breaker")]
    #[error("bulkhead '{0}' circuit breaker is open")]
    CircuitOpen(String),
}

/// 舱壁池统计（隔离指标）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BulkheadStats {
    /// 当前在途
    pub active: i64,
    /// 累计放行
    pub admitted: u64,
    /// 累计快速失败（池满/熔断）
    pub rejected: u64,
    /// 并发预算
    pub max_concurrent: usize,
}

/// 单个舱壁池
pub struct Bulkhead {
    /// 池名（资源组标识）
    name: String,
    config: BulkheadConfig,
    /// 在途计数
    active: AtomicI64,
    /// 放行计数
    admitted: AtomicU64,
    /// 拒绝计数
    rejected: AtomicU64,
    /// 该池熔断器（可选；circuit-breaker feature）
    #[cfg(feature = "circuit-breaker")]
    circuit_breaker: parking_lot::RwLock<Option<Arc<crate::circuit::CircuitBreaker>>>,
}

impl Bulkhead {
    /// 池名
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 并发预算
    pub fn max_concurrent(&self) -> usize {
        self.config.max_concurrent
    }

    /// 当前统计快照
    pub fn stats(&self) -> BulkheadStats {
        BulkheadStats {
            active: self.active.load(Ordering::Relaxed),
            admitted: self.admitted.load(Ordering::Relaxed),
            rejected: self.rejected.load(Ordering::Relaxed),
            max_concurrent: self.config.max_concurrent,
        }
    }

    /// 注入/替换该池熔断器（circuit-breaker feature）
    #[cfg(feature = "circuit-breaker")]
    pub fn set_circuit_breaker(&self, cb: Option<Arc<crate::circuit::CircuitBreaker>>) {
        *self.circuit_breaker.write() = cb;
    }

    /// 尝试获取一个执行槽位（成功返回释放守卫）
    fn try_acquire(self: &Arc<Self>) -> Result<BulkheadPermit, BulkheadError> {
        loop {
            let current = self.active.load(Ordering::Relaxed);
            if current >= self.config.max_concurrent as i64 {
                self.rejected.fetch_add(1, Ordering::Relaxed);
                return Err(BulkheadError::Full(self.name.clone()));
            }
            match self.active.compare_exchange(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.admitted.fetch_add(1, Ordering::Relaxed);
                    return Ok(BulkheadPermit {
                        bulkhead: Arc::clone(self),
                    });
                }
                Err(_) => continue,
            }
        }
    }
}

/// 舱壁执行许可（Drop 归还槽位）
pub struct BulkheadPermit {
    bulkhead: Arc<Bulkhead>,
}

impl Drop for BulkheadPermit {
    fn drop(&mut self) {
        self.bulkhead.active.fetch_sub(1, Ordering::AcqRel);
    }
}

/// 舱壁注册中心（按资源组分池）
pub struct BulkheadRegistry {
    pools: DashMap<String, Arc<Bulkhead>>,
    /// 新池的默认配置
    default_config: BulkheadConfig,
}

impl Default for BulkheadRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl BulkheadRegistry {
    /// 创建注册中心
    pub fn new() -> Self {
        Self {
            pools: DashMap::new(),
            default_config: BulkheadConfig::default(),
        }
    }

    /// 设置新池默认配置（链式）
    pub fn with_default_config(mut self, config: BulkheadConfig) -> Self {
        self.default_config = config;
        self
    }

    /// 预建/覆盖指定池配置（链式）
    pub fn with_pool_config(mut self, name: impl Into<String>, config: BulkheadConfig) -> Self {
        let name = name.into();
        self.pools.insert(
            name.clone(),
            Arc::new(Bulkhead {
                name,
                config,
                active: AtomicI64::new(0),
                admitted: AtomicU64::new(0),
                rejected: AtomicU64::new(0),
                #[cfg(feature = "circuit-breaker")]
                circuit_breaker: parking_lot::RwLock::new(None),
            }),
        );
        self
    }

    /// get-or-create 池
    pub fn bulkhead(&self, name: &str) -> Arc<Bulkhead> {
        if let Some(existing) = self.pools.get(name) {
            return Arc::clone(existing.value());
        }
        // DashMap entry API 持有分片锁，避免重复构造
        let mut entry = self.pools.entry(name.to_string()).or_insert_with(|| {
            Arc::new(Bulkhead {
                name: name.to_string(),
                config: self.default_config,
                active: AtomicI64::new(0),
                admitted: AtomicU64::new(0),
                rejected: AtomicU64::new(0),
                #[cfg(feature = "circuit-breaker")]
                circuit_breaker: parking_lot::RwLock::new(None),
            })
        });
        Arc::clone(entry.value_mut())
    }

    /// 在指定池内执行操作：满池快速失败，完成（含 Err）后归还槽位
    ///
    /// 熔断联动（circuit-breaker feature）：该池熔断打开时快速失败
    /// [`BulkheadError::CircuitOpen`]（计入 rejected）。
    pub async fn execute<F, T>(&self, pool: &str, operation: F) -> Result<T, BulkheadError>
    where
        F: std::future::Future<Output = T>,
    {
        let bulkhead = self.bulkhead(pool);

        #[cfg(feature = "circuit-breaker")]
        {
            let cb = bulkhead.circuit_breaker.read().clone();
            if let Some(cb) = cb {
                if cb.is_open().await {
                    bulkhead.rejected.fetch_add(1, Ordering::Relaxed);
                    return Err(BulkheadError::CircuitOpen(bulkhead.name().to_string()));
                }
            }
        }

        let _permit = bulkhead.try_acquire()?;
        Ok(operation.await)
    }

    /// 池统计快照
    pub fn stats(&self, pool: &str) -> Option<BulkheadStats> {
        self.pools.get(pool).map(|b| b.stats())
    }

    /// 全部池统计（按池名）
    pub fn all_stats(&self) -> Vec<(String, BulkheadStats)> {
        let mut out: Vec<_> = self
            .pools
            .iter()
            .map(|e| (e.key().clone(), e.value().stats()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// 池名列表
    pub fn pool_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.pools.iter().map(|e| e.key().clone()).collect();
        names.sort();
        names
    }
}

/// 舱壁错误 → 库错误的便捷转换（`Full` 映射并发限制超出语义）
impl From<BulkheadError> for LimiteronError {
    fn from(e: BulkheadError) -> Self {
        LimiteronError::ConcurrencyLimitExceeded(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> BulkheadRegistry {
        BulkheadRegistry::new().with_default_config(BulkheadConfig { max_concurrent: 2 })
    }

    #[tokio::test]
    async fn test_t611_pool_isolation_group_a_full_group_b_unaffected() {
        let registry = registry();

        // 池 A 占满（直接持许可，确定性饱和）
        let bh_a = registry.bulkhead("group-a");
        let p1 = bh_a.try_acquire().expect("空载应可占用");
        let p2 = bh_a.try_acquire().expect("第二个槽位应可占用");

        // 池 A 已满 → 新请求快速失败
        let err = registry
            .execute("group-a", async { 3 })
            .await
            .err()
            .expect("池 A 满后应快速失败");
        assert_eq!(err, BulkheadError::Full("group-a".to_string()));

        // 池 B 独立预算，不受 A 饱和影响
        let out = registry
            .execute("group-b", async { "ok" })
            .await
            .expect("池 B 不应受池 A 饱和影响");
        assert_eq!(out, "ok");

        drop((p1, p2));
        assert_eq!(registry.stats("group-a").unwrap().active, 0);
    }

    #[tokio::test]
    async fn test_t611_permit_released_after_completion() {
        let registry = registry();
        registry.execute("p", async {}).await.unwrap();
        registry
            .execute("p", async { Err::<(), _>(BulkheadError::Full("x".into())) })
            .await
            .ok(); // Err 也要归还槽位
        // 槽位应全部归还（max=2，连续执行 3 次不失败）
        registry.execute("p", async {}).await.unwrap();
        let stats = registry.stats("p").unwrap();
        assert_eq!(stats.active, 0, "完成后在途应归零");
        assert_eq!(stats.admitted, 3);
    }

    #[tokio::test]
    async fn test_t611_rejected_counter_and_stats() {
        let registry = registry();
        let bh = registry.bulkhead("s");
        let hold1 = bh.try_acquire().unwrap();
        let hold2 = bh.try_acquire().unwrap();

        let _ = registry.execute("s", async {}).await; // 池满 → rejected
        let stats = registry.stats("s").unwrap();
        assert_eq!(stats.active, 2);
        assert_eq!(stats.rejected, 1);
        assert_eq!(stats.max_concurrent, 2);

        drop((hold1, hold2));
    }

    #[test]
    fn test_t611_get_or_create_identity() {
        let registry = registry();
        let a = registry.bulkhead("x");
        let b = registry.bulkhead("x");
        assert!(Arc::ptr_eq(&a, &b), "同名池应 get-or-create 同一实例");
        assert_eq!(registry.pool_names(), vec!["x".to_string()]);
    }

    #[test]
    fn test_t611_all_stats_sorted() {
        let registry = registry().with_pool_config("b-pool", BulkheadConfig { max_concurrent: 4 });
        let _ = registry.bulkhead("a-pool");
        let all = registry.all_stats();
        let names: Vec<_> = all.iter().map(|(n, _)| n.clone()).collect();
        assert_eq!(names, vec!["a-pool".to_string(), "b-pool".to_string()]);
        assert_eq!(all[1].1.max_concurrent, 4);
    }

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test]
    async fn test_t611_pool_circuit_open_fails_fast() {
        use crate::CircuitState;
        use crate::circuit::{CircuitBreaker, CircuitBreakerConfig};

        let registry = BulkheadRegistry::new()
            .with_pool_config("cb-pool", BulkheadConfig { max_concurrent: 4 });
        let pool = registry.bulkhead("cb-pool");
        let cb = Arc::new(CircuitBreaker::with_dependencies(CircuitBreakerConfig {
            failure_threshold: 1,
            ..CircuitBreakerConfig::default()
        }));
        pool.set_circuit_breaker(Some(cb.clone()));

        // 打开该池熔断
        let _ = cb
            .execute(|| async { Err::<(), _>(LimiteronError::Other("boom".to_string())) })
            .await;
        assert_eq!(cb.get_state().await, CircuitState::Open);

        let err = registry
            .execute("cb-pool", async { 1 })
            .await
            .err()
            .expect("熔断打开的池应快速失败");
        assert!(
            matches!(err, BulkheadError::CircuitOpen(ref n) if n == "cb-pool"),
            "应返回 CircuitOpen，实际: {err:?}"
        );

        // 其他池不受该池熔断影响
        let out = registry.execute("other-pool", async { 2 }).await.unwrap();
        assert_eq!(out, 2);
    }
}
