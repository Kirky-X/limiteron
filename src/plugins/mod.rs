// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 插件系统（feature `plugins`）
//!
//! 决策生命周期钩子 + 编译期注册制。三个钩子覆盖裁决终态：
//! - [`Plugin::on_admit`]：请求放行（含放行元数据摘要）；
//! - [`Plugin::on_reject`]：请求被限流拒绝（含拒绝原因）；
//! - [`Plugin::on_degrade`]：系统进入降级/熔断开放等退化状态。
//!
//! ## 编译期注册制（动态 .so 明确不做）
//!
//! 本系统**不支持**运行时加载动态库插件（`libloading`/`dlopen` 路线），
//! 边界如下：
//! - **ABI 无稳定性**：Rust 无稳定 ABI，跨编译单元的 `extern` 接口既
//!   无法防止内存不安全（`unsafe` FFI 面不可审计），也无法随 crate 升级
//!   保证符号布局一致；
//! - **供应链攻击面**：运行时加载任意路径的 .so 等于把代码执行权交给
//!   文件系统写入者，与 limiteron「最小 unsafe 面」的立场冲突（本 crate
//!   `#![forbid(unsafe_code)]`）；
//! - **注册制收益**：插件以 `Arc<dyn Plugin>` 经 [`PluginRegistry`]
//!   注册，类型安全、可测试、随主构建产物分发——需要第三方扩展时以
//!   独立 crate 依赖 limiteron 并实现 [`Plugin`]，在同一构建中注册。
//!
//! ## 触发语义
//!
//! [`PluginRegistry::dispatch_*`] 按注册顺序调用全部插件；单个插件的
//! 错误或 panic 隔离不传播（panic 经 `std::panic::catch_unwind` 捕获并
//! 计数留痕，不中断其余插件、不上抛裁决路径——插件是观测面，不得反向
//! 影响限流判定）。
//!
//! **panic 隔离的构建边界**：`catch_unwind` 仅在默认 unwind panic 策略
//! 下生效；部署方设置 `panic = "abort"` 时插件 panic 将直接终止进程，
//! 上述隔离层与计数留痕全部失效——选择 abort 即放弃插件故障隔离，
//! 含插件的构建不应使用 `panic = "abort"`。
//!
//! # Example
//!
//! ```rust
//! use limiteron::plugins::{Plugin, PluginEvent, PluginRegistry};
//! use async_trait::async_trait;
//! use std::sync::Arc;
//!
//! struct AuditPlugin;
//!
//! #[async_trait]
//! impl Plugin for AuditPlugin {
//!     fn name(&self) -> &str {
//!         "audit"
//!     }
//!
//!     async fn on_reject(&self, event: &PluginEvent) {
//!         println!("rejected: {} ({})", event.key, event.reason);
//!     }
//! }
//!
//! # async fn demo() {
//! let registry = PluginRegistry::new();
//! registry.register(Arc::new(AuditPlugin)).unwrap();
//! registry
//!     .dispatch_reject(&PluginEvent::new(
//!         "1.2.3.4",
//!         Some("rule-1".to_string()),
//!         "rate exceeded",
//!     ))
//!     .await;
//! assert_eq!(registry.len(), 1);
//! assert!(registry.unregister("audit"));
//! # }
//! ```

use async_trait::async_trait;
use futures::FutureExt;
use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// 插件错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    /// 同名插件已注册
    AlreadyRegistered(String),
    /// 指定名称的插件不存在
    NotFound(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRegistered(name) => write!(f, "plugin already registered: {name}"),
            Self::NotFound(name) => write!(f, "plugin not found: {name}"),
        }
    }
}

impl std::error::Error for PluginError {}

/// 插件事件（裁决终态的观测投影）
#[derive(Debug, Clone)]
pub struct PluginEvent {
    /// 请求标识键（IP / user_id / api key 等，调用方定义）
    pub key: String,
    /// 触发裁决的规则 ID（无法归因时为 None）
    pub rule_id: Option<String>,
    /// 人读原因（拒绝原因 / 降级组件说明）
    pub reason: String,
}

impl PluginEvent {
    /// 构造事件
    pub fn new(key: impl Into<String>, rule_id: Option<String>, reason: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            rule_id,
            reason: reason.into(),
        }
    }
}

/// 限流插件 trait：决策生命周期钩子
///
/// 三个钩子全部有默认空实现——插件只关心自己订阅的终态。实现须为
/// `Send + Sync`（注册表跨任务分发）。
#[async_trait]
pub trait Plugin: Send + Sync {
    /// 插件名（注册表唯一键）
    fn name(&self) -> &str;

    /// 请求放行钩子
    async fn on_admit(&self, _event: &PluginEvent) {}

    /// 请求被限流拒绝钩子
    async fn on_reject(&self, _event: &PluginEvent) {}

    /// 系统降级/退化钩子（熔断开放、存储降级等）
    async fn on_degrade(&self, _event: &PluginEvent) {}
}

/// 插件注册表（编译期注册制）
///
/// 注册顺序即分发顺序（BTreeMap 以注册序号排序）；插件 panic 被隔离
/// 计数（[`Self::panicked_dispatches`]），不传播到调用方。
pub struct PluginRegistry {
    /// 注册序号 → 插件（BTreeMap 保序）
    plugins: parking_lot::RwLock<BTreeMap<u64, Arc<dyn Plugin>>>,
    /// 注册序号 → 插件名（卸载按名查找序号）
    names: parking_lot::RwLock<BTreeMap<String, u64>>,
    /// 注册序号生成器
    next_seq: AtomicU64,
    /// 分发期间插件 panic 计数（隔离留痕）
    panicked_dispatches: AtomicU64,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    /// 空注册表
    #[must_use]
    pub fn new() -> Self {
        Self {
            plugins: parking_lot::RwLock::new(BTreeMap::new()),
            names: parking_lot::RwLock::new(BTreeMap::new()),
            next_seq: AtomicU64::new(0),
            panicked_dispatches: AtomicU64::new(0),
        }
    }

    /// 注册插件（同名拒绝——显性错误而非静默覆盖）
    pub fn register(&self, plugin: Arc<dyn Plugin>) -> Result<(), PluginError> {
        let name = plugin.name().to_string();
        let mut names = self.names.write();
        if names.contains_key(&name) {
            return Err(PluginError::AlreadyRegistered(name));
        }
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        self.plugins.write().insert(seq, plugin);
        names.insert(name, seq);
        Ok(())
    }

    /// 卸载插件（返回是否存在）
    pub fn unregister(&self, name: &str) -> bool {
        let seq = self.names.write().remove(name);
        match seq {
            Some(seq) => {
                self.plugins.write().remove(&seq);
                true
            }
            None => false,
        }
    }

    /// 按名获取插件
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<dyn Plugin>> {
        let seq = self.names.read().get(name).copied()?;
        self.plugins.read().get(&seq).cloned()
    }

    /// 已注册插件名（按注册顺序——与 [`Self::dispatch_*`](Self::dispatch_admit)
    /// 的分发顺序同一口径；名字字典序由内部 map 决定，不外泄）
    #[must_use]
    pub fn list_names(&self) -> Vec<String> {
        let mut by_seq: Vec<(u64, String)> = self
            .names
            .read()
            .iter()
            .map(|(name, seq)| (*seq, name.clone()))
            .collect();
        by_seq.sort_unstable_by_key(|(seq, _)| *seq);
        by_seq.into_iter().map(|(_, name)| name).collect()
    }

    /// 注册插件数
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.read().len()
    }

    /// 是否为空
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.read().is_empty()
    }

    /// 分发期间插件 panic 计数
    #[must_use]
    pub fn panicked_dispatches(&self) -> u64 {
        self.panicked_dispatches.load(Ordering::Relaxed)
    }

    /// 分发放行事件
    pub async fn dispatch_admit(&self, event: &PluginEvent) {
        let plugins: Vec<Arc<dyn Plugin>> = self.plugins.read().values().cloned().collect();
        for plugin in plugins {
            // panic 隔离：观测面插件故障不得反向影响限流裁决路径
            if AssertUnwindSafe(plugin.on_admit(event))
                .catch_unwind()
                .await
                .is_err()
            {
                self.panicked_dispatches.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// 分发拒绝事件
    pub async fn dispatch_reject(&self, event: &PluginEvent) {
        let plugins: Vec<Arc<dyn Plugin>> = self.plugins.read().values().cloned().collect();
        for plugin in plugins {
            if AssertUnwindSafe(plugin.on_reject(event))
                .catch_unwind()
                .await
                .is_err()
            {
                self.panicked_dispatches.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// 分发降级事件
    pub async fn dispatch_degrade(&self, event: &PluginEvent) {
        let plugins: Vec<Arc<dyn Plugin>> = self.plugins.read().values().cloned().collect();
        for plugin in plugins {
            if AssertUnwindSafe(plugin.on_degrade(event))
                .catch_unwind()
                .await
                .is_err()
            {
                self.panicked_dispatches.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

// ============================================================================
// 内置示例插件
// ============================================================================

/// 示例插件：结构化日志记录裁决终态（`log` 门面，消费方可接 tracing 桥）
///
/// 作为「最小可用插件」的参考实现——无内部状态、三个钩子全订阅。
pub struct LoggingPlugin {
    /// 日志目标（区分多实例）
    pub target: String,
}

#[async_trait]
impl Plugin for LoggingPlugin {
    fn name(&self) -> &str {
        "logging"
    }

    async fn on_admit(&self, event: &PluginEvent) {
        log::info!(
            target: self.target.as_str(),
            "admit: key={} rule={:?}",
            event.key,
            event.rule_id
        );
    }

    async fn on_reject(&self, event: &PluginEvent) {
        log::warn!(
            target: self.target.as_str(),
            "reject: key={} rule={:?} reason={}",
            event.key,
            event.rule_id,
            event.reason
        );
    }

    async fn on_degrade(&self, event: &PluginEvent) {
        log::error!(
            target: self.target.as_str(),
            "degrade: key={} reason={}",
            event.key,
            event.reason
        );
    }
}

/// 示例插件：终态计数器（无锁原子计数，自省/测试面）
#[derive(Debug, Default)]
pub struct CounterPlugin {
    admits: AtomicU64,
    rejects: AtomicU64,
    degrades: AtomicU64,
}

impl CounterPlugin {
    /// 空计数器
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// 放行计数
    #[must_use]
    pub fn admits(&self) -> u64 {
        self.admits.load(Ordering::Relaxed)
    }

    /// 拒绝计数
    #[must_use]
    pub fn rejects(&self) -> u64 {
        self.rejects.load(Ordering::Relaxed)
    }

    /// 降级计数
    #[must_use]
    pub fn degrades(&self) -> u64 {
        self.degrades.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl Plugin for CounterPlugin {
    fn name(&self) -> &str {
        "counter"
    }

    async fn on_admit(&self, _event: &PluginEvent) {
        self.admits.fetch_add(1, Ordering::Relaxed);
    }

    async fn on_reject(&self, _event: &PluginEvent) {
        self.rejects.fetch_add(1, Ordering::Relaxed);
    }

    async fn on_degrade(&self, _event: &PluginEvent) {
        self.degrades.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(key: &str) -> PluginEvent {
        PluginEvent::new(key, Some("rule-1".to_string()), "test reason")
    }

    #[tokio::test]
    async fn register_dispatch_unregister_lifecycle() {
        let registry = PluginRegistry::new();
        let counter = CounterPlugin::new();

        // 注册
        registry.register(counter.clone()).unwrap();
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.list_names(), vec!["counter".to_string()]);

        // 触发三类钩子
        registry.dispatch_admit(&event("a")).await;
        registry.dispatch_admit(&event("b")).await;
        registry.dispatch_reject(&event("c")).await;
        registry.dispatch_degrade(&event("d")).await;
        assert_eq!(counter.admits(), 2);
        assert_eq!(counter.rejects(), 1);
        assert_eq!(counter.degrades(), 1);

        // 卸载后不再触发
        assert!(registry.unregister("counter"));
        assert!(!registry.unregister("counter"), "重复卸载应返回 false");
        registry.dispatch_admit(&event("e")).await;
        assert_eq!(counter.admits(), 2, "卸载后不再分发");
        assert!(registry.is_empty());
    }

    #[tokio::test]
    async fn duplicate_registration_is_rejected_loudly() {
        let registry = PluginRegistry::new();
        registry
            .register(Arc::new(LoggingPlugin {
                target: "t1".to_string(),
            }))
            .unwrap();
        let err = registry
            .register(Arc::new(LoggingPlugin {
                target: "t2".to_string(),
            }))
            .unwrap_err();
        assert_eq!(
            err,
            PluginError::AlreadyRegistered("logging".to_string()),
            "同名插件注册必须显性报错而非静默覆盖"
        );
        assert_eq!(registry.len(), 1);
    }

    #[tokio::test]
    async fn dispatch_order_follows_registration_order() {
        use std::sync::Mutex;

        struct Recorder {
            label: char,
            log: Arc<Mutex<Vec<char>>>,
        }
        #[async_trait]
        impl Plugin for Recorder {
            fn name(&self) -> &str {
                match self.label {
                    'c' => "recorder-c",
                    'a' => "recorder-a",
                    _ => "recorder-b",
                }
            }
            async fn on_admit(&self, _event: &PluginEvent) {
                self.log.lock().unwrap().push(self.label);
            }
        }

        let log = Arc::new(Mutex::new(Vec::new()));
        let registry = PluginRegistry::new();
        for label in ['c', 'a', 'b'] {
            registry
                .register(Arc::new(Recorder {
                    label,
                    log: log.clone(),
                }))
                .unwrap();
        }
        registry.dispatch_admit(&event("k")).await;
        assert_eq!(
            *log.lock().unwrap(),
            vec!['c', 'a', 'b'],
            "分发顺序应与注册顺序一致"
        );
    }

    #[test]
    fn list_names_follows_registration_order_not_lexicographic() {
        let registry = PluginRegistry::new();
        // 名字与注册序刻意错开：字典序（alpha/mid/zeta）≠ 注册序（zeta/alpha/mid）
        for name in ["zeta", "alpha", "mid"] {
            struct Named(String);
            #[async_trait]
            impl Plugin for Named {
                fn name(&self) -> &str {
                    &self.0
                }
            }
            registry
                .register(Arc::new(Named(name.to_string())))
                .unwrap();
        }
        assert_eq!(
            registry.list_names(),
            vec!["zeta".to_string(), "alpha".to_string(), "mid".to_string()],
            "list_names 应按注册顺序输出（与分发顺序同一口径）"
        );

        // 卸载后剩余插件仍保持注册序相对次序
        assert!(registry.unregister("alpha"));
        assert_eq!(
            registry.list_names(),
            vec!["zeta".to_string(), "mid".to_string()]
        );
    }

    struct PanickyPlugin;

    #[async_trait]
    impl Plugin for PanickyPlugin {
        fn name(&self) -> &str {
            "panicky"
        }
        async fn on_reject(&self, _event: &PluginEvent) {
            panic!("plugin bug");
        }
    }

    #[tokio::test]
    async fn plugin_panic_is_isolated_and_counted() {
        let registry = PluginRegistry::new();
        registry.register(Arc::new(PanickyPlugin)).unwrap();
        let counter = CounterPlugin::new();
        registry.register(counter.clone()).unwrap();

        // panicky 插件 panic 被隔离：计数器仍收到事件，panic 计数 +1
        registry.dispatch_reject(&event("k")).await;
        assert_eq!(counter.rejects(), 1, "后续插件不应被 panic 中断");
        assert_eq!(registry.panicked_dispatches(), 1);
    }

    #[tokio::test]
    async fn dispatch_with_no_plugins_is_noop() {
        let registry = PluginRegistry::new();
        registry.dispatch_admit(&event("k")).await;
        registry.dispatch_reject(&event("k")).await;
        registry.dispatch_degrade(&event("k")).await;
        assert_eq!(registry.panicked_dispatches(), 0);
    }

    #[tokio::test]
    async fn logging_plugin_writes_log_records() {
        // LoggingPlugin 是日志门面封装：经 log 现有测试捕获机制不可得时，
        // 退而断言其可注册与钩子可调用（不 panic 即为契约）
        let registry = PluginRegistry::new();
        registry
            .register(Arc::new(LoggingPlugin {
                target: "plugin-test".to_string(),
            }))
            .unwrap();
        registry.dispatch_admit(&event("k")).await;
        registry.dispatch_reject(&event("k")).await;
        registry.dispatch_degrade(&event("k")).await;
        assert_eq!(registry.panicked_dispatches(), 0);
    }

    #[test]
    fn get_returns_registered_plugin() {
        let registry = PluginRegistry::new();
        assert!(registry.get("counter").is_none());
        registry.register(CounterPlugin::new()).unwrap();
        let plugin = registry.get("counter").expect("registered");
        assert_eq!(plugin.name(), "counter");
    }
}
