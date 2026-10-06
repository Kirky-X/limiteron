// Copyright (c) 2025-2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! limiteron 对 `dbnexus-limiter-port` 限流端口的适配器。
//!
//! 经 limiteron 的 [`limiteron::LimiterManager`]（per-key 限流器实例缓存）实现 dbnexus 的
//! `Limiter` 端口契约：`check(key)` 检查并消费一次配额，判定携带 HTTP 429
//! 语义的 `Retry-After` 建议；后端故障经 `RateLimitError` 显性上报——
//! fail-open/closed 由消费方策略决定，适配器不擅自放行或拒绝。
//!
//! dbnexus 侧经 `RateLimitBackend::External(Arc<dyn Limiter>)` 注入本适配器，
//! 装配发生在应用组合根（见 `examples/dbnexus_port_basic.rs`）。dbnexus 与
//! limiteron 互为消费方（Cargo 禁包级循环依赖），故适配器落在 limiteron 侧、
//! 只依赖双方的小端口 crate。
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
// limiteron 自身引擎 trait 与端口 trait 同名，别名消解：
// EngineLimiter 为调用面（allow/remaining），PortLimiter 为实现目标
use dbnexus_limiter_port::{Limiter as PortLimiter, RateLimitDecision, RateLimitError};
use limiteron::{Limiter as EngineLimiter, LimiterManager, LimiteronError};

/// 明文保留的 key 长度上界（字节）；超出走确定性哈希映射
const MAX_PLAIN_KEY_BYTES: usize = 256;

/// manager key 归一化：超长 key 映射为定长标识，防单条目内存无界
///
/// 端口契约允许外部输入作 key（如 IP），长度无界时管理器每条目的内存从
/// 常数放大为无界（条目数有上限但单 key 以全量 String 存储）。哈希映射后
/// 判定语义不变；不同长 key 哈希碰撞只会共享同一桶（配额合并，fail-closed
/// 方向），且构造碰撞前提是攻击者已知原 key——等价于身份伪造，属消费方
/// 认证面而非限流器面。
fn manager_key(key: &str) -> String {
    if key.len() <= MAX_PLAIN_KEY_BYTES {
        return key.to_string();
    }
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    format!("hashed:{:016x}", hasher.finish())
}

/// dbnexus 限流端口适配器：把 limiteron 的 per-key 令牌桶装进 `dbnexus_limiter_port::Limiter`
///
/// 单实例内所有 key 共用构造期统一的 `(amount, unit_secs)`——`LimiterManager`
/// 对同 key 异参数有参数一致性断言，实例内统一配置使该 panic 面结构性不可达；
/// 经 [`with_manager`](Self::with_manager) 共享管理器时须守同款约束（见其文档）。
/// 引擎桶容量为 `amount`、稳态速率为 `max(1, amount/unit_secs)` 令牌/秒。
pub struct DbnexusLimiter {
    manager: Arc<LimiterManager>,
    amount: u64,
    unit_secs: u64,
}

impl DbnexusLimiter {
    /// 创建适配器（内部独立 `LimiterManager`，纯内存引擎）
    ///
    /// `amount` 为每窗口配额（即桶容量）；`unit_secs` 为窗口秒数。
    /// 例：`new(3, 3)` = 每 key 3 秒 3 次。
    ///
    /// # 参数约定
    ///
    /// 两值须 ≥1：`amount = 0` 得零容量桶（恒 deny，fail-closed 的可用性
    /// 自杀）；`unit_secs = 0` 被引擎静默按「每秒 `amount` 次」处理（语义
    /// 漂移）。适配器不做运行时校验，参数把关在组合根。
    #[must_use]
    pub fn new(amount: u64, unit_secs: u64) -> Self {
        Self::with_manager(Arc::new(LimiterManager::new()), amount, unit_secs)
    }

    /// 以注入的 [`LimiterManager`] 创建（DI 模式，供组合根共享引擎或注入测试替身）
    ///
    /// # 共享约束
    ///
    /// 共享同一管理器的多个实例必须配置一致或 key 空间不相交：引擎参数
    /// 一致性断言比对 `(capacity, refill_rate)` 且**不校验 `unit_secs`**——
    /// 不同配置实例写入同一 key 时，轻则断言 panic（消息已脱敏），重则派生
    /// 参数恰好相同（如 `(100,60)` 与 `(100,120)` 派生均为 capacity=100、
    /// refill=1）而**静默共享同一桶**，配额合并削弱限流。
    ///
    /// # key 基数与淘汰
    ///
    /// 管理器条目上限 100_000（清理阈值 110_000）带 LRU 清理：**被淘汰 key
    /// 的配额随之归零**，高基数 key 洪峰可把活跃 key 挤出缓存以重置其配额
    /// （引擎文档点名的绕窗口手段）——key 须取有限受信集合（如角色 ID），
    /// 基数按清理阈值留余量评估，安全敏感场景调大上限。超 256 字节的 key
    /// 经 [`manager_key`] 确定性哈希为定长标识，防单条目内存无界。
    #[must_use]
    pub fn with_manager(manager: Arc<LimiterManager>, amount: u64, unit_secs: u64) -> Self {
        Self {
            manager,
            amount,
            unit_secs,
        }
    }

    /// deny 路径的 `Retry-After` 推导：亚秒等待向上取整到 1s
    ///
    /// 引擎给出 `reset_secs = 0` 表示亚秒内即时恢复，取整为 1s（与 dbnexus
    /// session 层对亚秒 `Retry-After` 的进位规则一致）。deny 的建议由快照
    /// 推导；快照调用失败时调用方得到 `None`（判定不变，不升级为故障）。
    fn map_retry_after(reset_secs: u64) -> Option<Duration> {
        Some(Duration::from_secs(reset_secs.max(1)))
    }

    /// 后端故障显性映射：保留 limiteron 错误链文本，便于消费方排障
    ///
    /// key 永不写入消息（防用户标识符经适配器泄漏进消费方日志；
    /// 消费方在调用点自行知晓 key 上下文）。
    fn map_err(err: LimiteronError) -> RateLimitError {
        RateLimitError::new(format!("limiteron token bucket check failed: {err}"))
    }
}

#[async_trait]
impl PortLimiter for DbnexusLimiter {
    async fn check(&self, key: &str) -> Result<RateLimitDecision, RateLimitError> {
        let key = manager_key(key);
        let engine = self
            .manager
            .get_rate_limiter(&key, self.amount, self.unit_secs);
        match engine.allow(1).await {
            Ok(true) => Ok(RateLimitDecision::allow()),
            // 判定已成立：建议推导失败不改变 deny，也不升级为后端故障
            // （当前引擎快照恒成功，该分支为防御性深度；若未来快照可失败，
            // 此处静默降级为无建议，接入方应留意该取舍）
            Ok(false) => {
                let retry_after = match engine.remaining().await {
                    Ok(snapshot) => Self::map_retry_after(snapshot.reset_secs),
                    Err(_) => None,
                };
                Ok(RateLimitDecision::deny(retry_after))
            }
            Err(err) => Err(Self::map_err(err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // refill_rate = amount/unit_secs = 1/s：耗尽后下一令牌恰好 1s 后恢复，
    // 毫秒级测试窗口内不会意外回填，判定确定
    const AMOUNT: u64 = 3;
    const UNIT_SECS: u64 = 3;

    #[tokio::test]
    async fn allow_under_quota() {
        let limiter = DbnexusLimiter::new(AMOUNT, UNIT_SECS);
        for _ in 0..AMOUNT {
            let decision = limiter.check("role-a").await.unwrap();
            assert!(decision.allowed);
            assert_eq!(decision.retry_after, None);
        }
    }

    #[tokio::test]
    async fn deny_after_exhaustion_with_retry_after() {
        let limiter = DbnexusLimiter::new(AMOUNT, UNIT_SECS);
        for _ in 0..AMOUNT {
            assert!(limiter.check("role-a").await.unwrap().allowed);
        }
        let decision = limiter.check("role-a").await.unwrap();
        assert!(!decision.allowed);
        let retry_after = decision
            .retry_after
            .expect("deny 必须携带 Retry-After 建议");
        assert!(retry_after >= Duration::from_secs(1));
    }

    #[tokio::test]
    async fn per_key_isolation() {
        let limiter = DbnexusLimiter::new(AMOUNT, UNIT_SECS);
        for _ in 0..AMOUNT {
            assert!(limiter.check("role-a").await.unwrap().allowed);
        }
        assert!(!limiter.check("role-a").await.unwrap().allowed);
        // key-a 耗尽不影响 key-b 的独立配额
        let decision = limiter.check("role-b").await.unwrap();
        assert!(decision.allowed);
    }

    /// 端口对象安全：`Arc<dyn Limiter>` 可注入消费方并分发调用
    #[tokio::test]
    async fn port_object_safety() {
        let limiter: Arc<dyn PortLimiter> = Arc::new(DbnexusLimiter::new(1, 100));
        assert!(limiter.check("role-a").await.unwrap().allowed);
        assert!(!limiter.check("role-a").await.unwrap().allowed);
    }

    /// 统一配置下同 key 重复获取不触发 manager 参数一致性断言
    #[tokio::test]
    async fn same_key_repeated_check_no_param_panic() {
        let limiter = DbnexusLimiter::new(AMOUNT, UNIT_SECS);
        for _ in 0..AMOUNT * 2 {
            let _ = limiter.check("role-a").await;
        }
    }

    #[test]
    fn map_retry_after_sub_second_ceils_to_one_sec() {
        assert_eq!(
            DbnexusLimiter::map_retry_after(0),
            Some(Duration::from_secs(1))
        );
    }

    #[test]
    fn map_retry_after_whole_seconds_pass_through() {
        assert_eq!(
            DbnexusLimiter::map_retry_after(5),
            Some(Duration::from_secs(5))
        );
    }

    #[test]
    fn map_err_keeps_error_chain() {
        let err =
            DbnexusLimiter::map_err(LimiteronError::ConfigError("storage broken".to_string()));
        let message = err.message();
        assert!(message.starts_with("limiteron token bucket check failed:"));
        assert!(message.contains("storage broken"));
        // “key 不入消息”由 map_err 签名结构性保证（不接收 key 参数），
        // 非运行时行为，故无对应断言；若未来加重载须同步补泄漏测试。
    }

    #[test]
    fn long_keys_map_deterministically_to_bounded_ids() {
        let long_a = "a".repeat(300);
        let long_b = "b".repeat(300);
        let id_a1 = manager_key(&long_a);
        assert_eq!(id_a1, manager_key(&long_a), "同 key 映射到同一桶标识");
        assert_ne!(manager_key(&long_b), id_a1, "不同长 key 不得合并配额");
        assert!(id_a1.len() <= 32, "映射后定长，单条目内存回归常数");
        assert_eq!(manager_key("role-a"), "role-a", "短 key 原样保留");
    }

    #[tokio::test]
    async fn long_key_keeps_quota_semantics() {
        // 400 字节 key 走哈希映射，判定与普通 key 同款
        let long_key = "role-".repeat(80);
        let limiter = DbnexusLimiter::new(AMOUNT, UNIT_SECS);
        for _ in 0..AMOUNT {
            assert!(limiter.check(&long_key).await.unwrap().allowed);
        }
        assert!(!limiter.check(&long_key).await.unwrap().allowed);
    }
}
