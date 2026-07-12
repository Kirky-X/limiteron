// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 事件分发器
//!
//! 监听事件通道并将事件分发给注册的处理器和 Webhook。

use crate::events::EventEmitter;
use crate::events::{Event, EventHandler};
#[cfg(feature = "webhook")]
use crate::webhook_validator::validate_webhook_url;
use log::{debug, error, info, warn};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

/// 事件分发器
///
/// 监听事件通道并将事件分发给注册的处理器和 Webhook。
/// 支持动态注册和注销事件处理器。
///
/// # 示例
///
/// ```rust
/// use limiteron::{EventEmitter, EventDispatcher, EventConfig};
///
/// #[tokio::main]
/// async fn main() {
///     let emitter = EventEmitter::with_default_config();
///     let dispatcher = EventDispatcher::new(emitter.clone());
///
///     // 启动分发器
///     dispatcher.start().await;
///
///     // 发射事件会自动被分发
///     // ...
///
///     // 停止分发器
///     dispatcher.stop().await;
/// }
/// ```
pub struct EventDispatcher {
    /// 事件发射器
    emitter: EventEmitter,

    /// 注册的事件处理器
    handlers: Arc<RwLock<Vec<Arc<dyn EventHandler>>>>,

    /// 分发任务句柄
    dispatch_handle: Arc<RwLock<Option<JoinHandle<()>>>>,

    /// Webhook URL 列表
    webhook_urls: Arc<RwLock<Vec<String>>>,
}

impl EventDispatcher {
    /// 创建新的事件分发器
    ///
    /// # 参数
    /// - `emitter`: 事件发射器实例
    pub fn new(emitter: EventEmitter) -> Self {
        let webhook_urls = emitter.config().webhook_urls.clone();

        Self {
            emitter,
            handlers: Arc::new(RwLock::new(Vec::new())),
            dispatch_handle: Arc::new(RwLock::new(None)),
            webhook_urls: Arc::new(RwLock::new(webhook_urls)),
        }
    }

    /// 注册事件处理器
    ///
    /// # 参数
    /// - `handler`: 事件处理器实例
    pub async fn register_handler(&self, handler: Arc<dyn EventHandler>) {
        let mut handlers = self.handlers.write().await;
        info!("Registering event handler: {}", handler.name());
        handlers.push(handler);
    }

    /// 注销所有事件处理器
    pub async fn clear_handlers(&self) {
        let mut handlers = self.handlers.write().await;
        handlers.clear();
        info!("All event handlers cleared");
    }

    /// 添加 Webhook URL
    ///
    /// # 参数
    /// - `url`: Webhook URL
    pub async fn add_webhook_url(&self, url: String) {
        let mut urls = self.webhook_urls.write().await;
        if !urls.contains(&url) {
            info!("Adding webhook URL: {}", url);
            urls.push(url);
        }
    }

    /// 移除 Webhook URL
    ///
    /// # 参数
    /// - `url`: 要移除的 Webhook URL
    pub async fn remove_webhook_url(&self, url: &str) {
        let mut urls = self.webhook_urls.write().await;
        urls.retain(|u| u != url);
        info!("Removed webhook URL: {}", url);
    }

    /// 启动事件分发任务
    ///
    /// 此方法会启动一个后台任务，持续监听事件通道并分发事件。
    pub async fn start(&self) {
        let mut handle_guard = self.dispatch_handle.write().await;
        if handle_guard.is_some() {
            warn!("EventDispatcher is already running");
            return;
        }

        let handlers = self.handlers.clone();
        let webhook_urls = self.webhook_urls.clone();

        // 在 spawn 之前订阅，避免「start() 返回后立即 emit()」的竞态：
        // 若在 spawn 的任务内才 subscribe，emit 可能在 subscribe 之前执行，
        // 导致广播通道无接收者而丢失事件（SendError）。
        let mut receiver = self.emitter.subscribe();

        let handle = tokio::spawn(async move {
            info!("EventDispatcher started");

            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        debug!(
                            "Received event: name={}, severity={}",
                            event.name(),
                            event.severity()
                        );

                        // 分发给处理器
                        let handlers_snapshot = handlers.read().await.clone();
                        for handler in &handlers_snapshot {
                            if handler.accepts(&event.event_type) {
                                if let Err(e) = handler.handle(&event).await {
                                    error!(
                                        "Handler {} failed to process event {}: {}",
                                        handler.name(),
                                        event.name(),
                                        e
                                    );
                                }
                            }
                        }

                        // 发送到 Webhook
                        let urls = webhook_urls.read().await.clone();
                        for url in &urls {
                            if let Err(e) = send_webhook(url, &event).await {
                                error!("Failed to send webhook to {}: {}", url, e);
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("EventDispatcher lagged behind, dropped {} events", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("EventDispatcher channel closed, stopping");
                        break;
                    }
                }
            }
        });

        *handle_guard = Some(handle);
    }

    /// 停止事件分发任务
    pub async fn stop(&self) {
        let mut handle_guard = self.dispatch_handle.write().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
            info!("EventDispatcher stopped");
        }
    }

    /// 获取事件发射器
    pub fn emitter(&self) -> &EventEmitter {
        &self.emitter
    }

    /// 获取注册的处理器数量
    pub async fn handler_count(&self) -> usize {
        self.handlers.read().await.len()
    }
}

/// EventDispatcher 的 Drop 实现
///
/// 当 EventDispatcher 被丢弃时，停止分发后台任务，防止任务泄漏。
/// 使用 `try_write()` 而非 `write().await`，因为 `Drop::drop` 中不能使用 `.await`。
impl Drop for EventDispatcher {
    fn drop(&mut self) {
        if let Ok(mut handle_guard) = self.dispatch_handle.try_write() {
            if let Some(handle) = handle_guard.take() {
                handle.abort();
                debug!("EventDispatcher dispatch task aborted on drop");
            }
        }
    }
}

/// 发送事件到 Webhook
///
/// 当 webhook feature 启用时，使用 reqwest 发送 POST 请求。
/// 否则返回错误。
#[cfg(feature = "webhook")]
async fn send_webhook(
    url: &str,
    event: &Event,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 安全验证：检查 URL 是否安全（生产环境要求 HTTPS）
    validate_webhook_url(url, !cfg!(debug_assertions))
        .map_err(Box::<dyn std::error::Error + Send + Sync>::from)?;

    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .json(event)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    if response.status().is_success() {
        debug!("Webhook sent successfully to {}", url);
        Ok(())
    } else {
        Err(format!("Webhook returned error status: {}", response.status()).into())
    }
}

/// Webhook 未启用时的存根实现
#[cfg(not(feature = "webhook"))]
async fn send_webhook(
    _url: &str,
    _event: &Event,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    Err("Webhook feature is not enabled. Enable 'webhook' feature to use webhooks.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventType;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestHandler {
        call_count: Arc<AtomicU64>,
    }

    #[async_trait::async_trait]
    impl EventHandler for TestHandler {
        async fn handle(
            &self,
            _event: &Event,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn name(&self) -> &str {
            "TestHandler"
        }
    }

    #[tokio::test]
    async fn test_dispatcher_register_handler() {
        let emitter = EventEmitter::with_default_config();
        let dispatcher = EventDispatcher::new(emitter);

        assert_eq!(dispatcher.handler_count().await, 0);

        let handler = Arc::new(TestHandler {
            call_count: Arc::new(AtomicU64::new(0)),
        });
        dispatcher.register_handler(handler).await;

        assert_eq!(dispatcher.handler_count().await, 1);
    }

    #[tokio::test]
    async fn test_dispatcher_clear_handlers() {
        let emitter = EventEmitter::with_default_config();
        let dispatcher = EventDispatcher::new(emitter);

        let handler1 = Arc::new(TestHandler {
            call_count: Arc::new(AtomicU64::new(0)),
        });
        let handler2 = Arc::new(TestHandler {
            call_count: Arc::new(AtomicU64::new(0)),
        });

        dispatcher.register_handler(handler1).await;
        dispatcher.register_handler(handler2).await;
        assert_eq!(dispatcher.handler_count().await, 2);

        dispatcher.clear_handlers().await;
        assert_eq!(dispatcher.handler_count().await, 0);
    }

    #[tokio::test]
    async fn test_dispatcher_webhook_urls() {
        let emitter = EventEmitter::with_default_config();
        let dispatcher = EventDispatcher::new(emitter);

        dispatcher
            .add_webhook_url("https://example.com/hook1".to_string())
            .await;
        dispatcher
            .add_webhook_url("https://example.com/hook2".to_string())
            .await;
        // 重复添加不应产生重复项
        dispatcher
            .add_webhook_url("https://example.com/hook1".to_string())
            .await;

        let urls = dispatcher.webhook_urls.read().await;
        assert_eq!(urls.len(), 2);
        drop(urls); // 释放读锁，否则下方 remove_webhook_url 获取写锁会自死锁

        dispatcher
            .remove_webhook_url("https://example.com/hook1")
            .await;
        let urls = dispatcher.webhook_urls.read().await;
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "https://example.com/hook2");
    }

    #[tokio::test]
    async fn test_dispatcher_event_distribution() {
        let emitter = EventEmitter::with_default_config();
        let dispatcher = EventDispatcher::new(emitter.clone());

        let call_count = Arc::new(AtomicU64::new(0));
        let handler = Arc::new(TestHandler {
            call_count: call_count.clone(),
        });
        dispatcher.register_handler(handler).await;

        // 启动分发器
        dispatcher.start().await;

        // 发射一个事件
        let event = Event::new(EventType::RateLimitTriggered {
            key: "test".to_string(),
            rule_id: "rule".to_string(),
            decision: "Deny".to_string(),
        });
        emitter.emit(event).await.unwrap();

        // 轮询等待处理器被调用（tarpaulin 插桩会显著降低执行速度，固定 sleep 不可靠）
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(2);
        let mut observed = 0u64;
        while tokio::time::Instant::now() < deadline {
            observed = call_count.load(Ordering::SeqCst);
            if observed >= 1 {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        }

        // 始终在断言前停止分发器，避免断言失败时后台任务泄漏导致后续测试挂起
        dispatcher.stop().await;

        // 验证处理器被调用恰好一次
        assert_eq!(observed, 1, "handler should be called exactly once");
    }

    #[tokio::test]
    async fn test_dispatcher_stop_and_restart() {
        let emitter = EventEmitter::with_default_config();
        let dispatcher = EventDispatcher::new(emitter);

        // 启动
        dispatcher.start().await;
        assert!(dispatcher.dispatch_handle.read().await.is_some());

        // 再次启动不应创建新任务
        dispatcher.start().await;
        assert!(dispatcher.dispatch_handle.read().await.is_some());

        // 停止
        dispatcher.stop().await;
        assert!(dispatcher.dispatch_handle.read().await.is_none());

        // 重新启动
        dispatcher.start().await;
        assert!(dispatcher.dispatch_handle.read().await.is_some());

        // 清理
        dispatcher.stop().await;
    }
}
