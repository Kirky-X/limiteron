// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 事件分发器
//!
//! 监听事件通道并将事件分发给注册的处理器和 Webhook。

use crate::error::WebhookUrlError;
use crate::events::EventEmitter;
// `Event` 仅被 webhook 发送路径（send_webhook 及其调用点）消费，
// 与 webhook 特性同门控，避免 webhook-off 构建下 unused import 告警。
#[cfg(not(feature = "webhook"))]
use crate::events::EventHandler;
#[cfg(feature = "webhook")]
use crate::events::{Event, EventHandler};
#[cfg(feature = "webhook")]
use crate::webhook_validator::{
    WebhookTransportPolicy, validate_webhook_url_async, validate_webhook_url_for_send,
    webhook_http_client,
};
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

    /// 共享 HTTP 客户端（webhook 外发复用连接池；
    /// Client 内部为 Arc，克隆廉价。每次发送新建 Client 会丢失
    /// TCP/TLS 复用，高事件量下造成套接字抖动）
    ///
    /// 构建失败（TLS 后端初始化失败等）时为 `None`：分发器照常工作，
    /// 仅 webhook 外发被跳过。
    #[cfg(feature = "webhook")]
    http_client: Option<reqwest::Client>,
}

impl EventDispatcher {
    /// 创建新的事件分发器
    ///
    /// # 参数
    /// - `emitter`: 事件发射器实例
    pub fn new(emitter: EventEmitter) -> Self {
        let webhook_urls = emitter.config().webhook_urls.clone();

        // 事件分发的传输策略与本模块的 URL 校验口径一致（`!cfg!(debug_assertions)`：
        // release 要求 HTTPS，debug 放行明文便于本地联调）。
        #[cfg(feature = "webhook")]
        let http_client = {
            let policy = if !cfg!(debug_assertions) {
                WebhookTransportPolicy::HttpsRequired
            } else {
                WebhookTransportPolicy::Lax
            };
            match webhook_http_client(policy) {
                Ok(client) => Some(client),
                Err(e) => {
                    error!("Failed to build webhook HTTP client, webhook delivery disabled: {e}");
                    None
                }
            }
        };

        Self {
            emitter,
            handlers: Arc::new(RwLock::new(Vec::new())),
            dispatch_handle: Arc::new(RwLock::new(None)),
            webhook_urls: Arc::new(RwLock::new(webhook_urls)),
            #[cfg(feature = "webhook")]
            http_client,
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
    ///
    /// webhook feature 启用时在存储前预校验 URL：非法/内网 URL
    /// 立即拒绝并告警，而非作为无效条目累积到发送阶段才暴露。预校验含
    /// DNS 解析结果判定（域名解析到内网同样拒绝）。
    ///
    /// 校验失败的原因只记入告警、不返回给调用方；需要据此重试的调用方
    /// 请用 [`EventDispatcher::try_add_webhook_url`]。
    pub async fn add_webhook_url(&self, url: String) {
        if let Err(e) = self.try_add_webhook_url(url.clone()).await {
            let summary = crate::logging::redaction::redact_url(Some(&url));
            match e {
                WebhookUrlError::Rejected(msg) => {
                    warn!("Rejected invalid webhook URL ({}): {}", summary, msg);
                }
                WebhookUrlError::DnsTransient(msg) => {
                    warn!(
                        "Webhook URL pre-check failed, retryable ({}): {}",
                        summary, msg
                    );
                }
            }
        }
    }

    /// 添加 Webhook URL 并返回校验结果
    ///
    /// 与 [`EventDispatcher::add_webhook_url`] 的区别：校验失败时**不写入**
    /// URL 列表，并把失败原因返回给调用方——[`WebhookUrlError::DnsTransient`]
    /// 表示 DNS 解析失败/预检超时等瞬时故障（可重试），
    /// [`WebhookUrlError::Rejected`] 表示永久性拒绝（重试无意义）。
    ///
    /// webhook feature 未启用时不做校验，恒返回 `Ok(())`。
    pub async fn try_add_webhook_url(&self, url: String) -> Result<(), WebhookUrlError> {
        #[cfg(feature = "webhook")]
        validate_webhook_url_async(&url, !cfg!(debug_assertions)).await?;

        let mut urls = self.webhook_urls.write().await;
        if !urls.contains(&url) {
            info!(
                "Adding webhook URL: {}",
                crate::logging::redaction::redact_url(Some(&url))
            );
            urls.push(url);
        }
        Ok(())
    }

    /// 移除 Webhook URL
    ///
    /// # 参数
    /// - `url`: 要移除的 Webhook URL
    pub async fn remove_webhook_url(&self, url: &str) {
        let mut urls = self.webhook_urls.write().await;
        urls.retain(|u| u != url);
        info!(
            "Removed webhook URL: {}",
            crate::logging::redaction::redact_url(Some(url))
        );
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
        #[cfg(feature = "webhook")]
        let webhook_urls = self.webhook_urls.clone();
        #[cfg(feature = "webhook")]
        let http_client = self.http_client.clone();

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
                            if handler.accepts(&event.event_type)
                                && let Err(e) = handler.handle(&event).await
                            {
                                error!(
                                    "Handler {} failed to process event {}: {}",
                                    handler.name(),
                                    event.name(),
                                    e
                                );
                            }
                        }

                        // 发送到 Webhook（webhook 特性关闭时无 http_client 且
                        // send_webhook 为 2 参存根，整个分派块必须同门控）
                        #[cfg(feature = "webhook")]
                        if let Some(client) = http_client.as_ref() {
                            let urls = webhook_urls.read().await.clone();
                            for url in &urls {
                                if let Err(e) = send_webhook(client, url, &event).await {
                                    error!(
                                        "Failed to send webhook to {}: {}",
                                        crate::logging::redaction::redact_url(Some(url)),
                                        e
                                    );
                                }
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
        if let Ok(mut handle_guard) = self.dispatch_handle.try_write()
            && let Some(handle) = handle_guard.take()
        {
            handle.abort();
            debug!("EventDispatcher dispatch task aborted on drop");
        }
    }
}

/// 签名头对（header 名, header 值）×2：时间戳 + 签名
#[cfg(feature = "webhook")]
type SigningHeaders = Option<[(reqwest::header::HeaderName, reqwest::header::HeaderValue); 2]>;

/// 构造 webhook 请求负载与签名头（纯函数便于单测）。
///
/// 返回 `(body, signing_headers)`：
/// - 未设置进程级签名器 → `signing_headers = None`，body 为 `serde_json` 序列化
///   （向后兼容，无签名头）；
/// - 已设置 → body 为**同一份序列化字节**（签名覆盖线上精确字节），
///   `signing_headers = Some([(X-Limiteron-Timestamp, v), (X-Limiteron-Signature, v)])`。
#[cfg(feature = "webhook")]
pub(crate) fn signed_webhook_payload(
    event: &Event,
) -> Result<(String, SigningHeaders), Box<dyn std::error::Error + Send + Sync>> {
    use crate::events::webhook_signature::{
        SIGNATURE_HEADER, TIMESTAMP_HEADER, global_webhook_signer,
    };
    use reqwest::header::HeaderValue;

    // 头名是编译期常量：const 构造避免每次外发都做 HeaderName 解析
    const TS_NAME: reqwest::header::HeaderName =
        reqwest::header::HeaderName::from_static("x-limiteron-timestamp");
    const SIG_NAME: reqwest::header::HeaderName =
        reqwest::header::HeaderName::from_static("x-limiteron-signature");
    debug_assert_eq!(TS_NAME.as_str(), TIMESTAMP_HEADER.to_ascii_lowercase());
    debug_assert_eq!(SIG_NAME.as_str(), SIGNATURE_HEADER.to_ascii_lowercase());

    let payload = serde_json::to_string(event)?;
    Ok(match global_webhook_signer() {
        Some(signer) => {
            let signature = signer.sign(&payload);
            let to_header_value = |v: String| {
                HeaderValue::from_str(&v)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
            };
            let ts_value = to_header_value(signature.timestamp_header_value())?;
            let sig_value = to_header_value(signature.header_value().to_string())?;
            (payload, Some([(TS_NAME, ts_value), (SIG_NAME, sig_value)]))
        }
        None => (payload, None),
    })
}

/// 发送事件到 Webhook
///
/// 当 webhook feature 启用时，使用 reqwest 发送 POST 请求。
/// 否则返回错误。
///
/// 若已设置进程级签名器（`webhook_signature::global_webhook_signer`），
/// 请求体以序列化后的精确字节发送，并附带
/// `X-Limiteron-Timestamp` + `X-Limiteron-Signature: sha256=<hex>` 签名头
/// （HMAC-SHA256(secret, "{timestamp}.{payload}")，时间戳防重放窗口默认 300s）。
#[cfg(feature = "webhook")]
pub(crate) async fn send_webhook(
    client: &reqwest::Client,
    url: &str,
    event: &Event,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 安全验证：字面量判定（协议要求 / 字面量地址段）；域名面由连接期的
    // SSRF 解析器权威判定——发送路径不重复 DNS 预检（预检的瞬时故障会把
    // 本可成功的投递判死，且同一投递解析两次）。
    validate_webhook_url_for_send(url, !cfg!(debug_assertions))?;

    let (payload, signing_headers) = signed_webhook_payload(event)?;

    let mut request = client
        .post(url)
        .timeout(std::time::Duration::from_secs(5))
        .header(reqwest::header::CONTENT_TYPE, "application/json");

    if let Some([(ts_name, ts_value), (sig_name, sig_value)]) = signing_headers {
        request = request
            .header(ts_name, ts_value)
            .header(sig_name, sig_value);
    }

    let response = request.body(payload).send().await?;

    if response.status().is_success() {
        debug!(
            "Webhook sent successfully to {}",
            crate::logging::redaction::redact_url(Some(url))
        );
        Ok(())
    } else {
        Err(format!("Webhook returned error status: {}", response.status()).into())
    }
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

    /// 注册路径的校验失败必须返回给调用方（可区分永久拒绝与瞬时故障），
    /// 且被拒绝的 URL 不得写入列表
    #[cfg(feature = "webhook")]
    #[tokio::test]
    async fn test_try_add_webhook_url_surfaces_rejection() {
        let emitter = EventEmitter::with_default_config();
        let dispatcher = EventDispatcher::new(emitter);

        let err = dispatcher
            .try_add_webhook_url("https://127.0.0.1/hook".to_string())
            .await
            .expect_err("a loopback webhook URL must be rejected");
        assert!(
            matches!(err, WebhookUrlError::Rejected(_)),
            "a literal rejection must be permanent, got: {err:?}"
        );

        let urls = dispatcher.webhook_urls.read().await;
        assert!(
            urls.is_empty(),
            "a rejected URL must not be stored, got: {urls:?}"
        );
    }

    /// DNS 解析失败属瞬时故障：按可重试的瞬时错误上报，且不写入列表
    #[cfg(feature = "webhook")]
    #[tokio::test]
    async fn test_try_add_webhook_url_surfaces_transient_failure() {
        let emitter = EventEmitter::with_default_config();
        let dispatcher = EventDispatcher::new(emitter);

        // .invalid 是 RFC 2606 保留域名，恒不解析
        let err = dispatcher
            .try_add_webhook_url("https://nonexistent-host-xyzzy.invalid/hook".to_string())
            .await
            .expect_err("a non-resolvable host must fail the pre-check");
        assert!(
            matches!(err, WebhookUrlError::DnsTransient(_)),
            "DNS resolution failure must be transient, got: {err:?}"
        );

        let urls = dispatcher.webhook_urls.read().await;
        assert!(
            urls.is_empty(),
            "a URL that failed the pre-check must not be stored, got: {urls:?}"
        );
    }

    /// 薄封装只记告警：校验失败不写入列表，也不向调用方抛错
    #[cfg(feature = "webhook")]
    #[tokio::test]
    async fn test_add_webhook_url_ignores_validation_failure() {
        let emitter = EventEmitter::with_default_config();
        let dispatcher = EventDispatcher::new(emitter);

        dispatcher
            .add_webhook_url("https://127.0.0.1/hook".to_string())
            .await;

        let urls = dispatcher.webhook_urls.read().await;
        assert!(
            urls.is_empty(),
            "a rejected URL must not be stored, got: {urls:?}"
        );
    }

    /// 发送路径的预检必须有界：解析器不可用时返回错误，而不是挂起分派任务
    #[cfg(feature = "webhook")]
    #[tokio::test]
    async fn test_send_webhook_precheck_failure_does_not_hang() {
        let client = webhook_http_client(WebhookTransportPolicy::Lax)
            .expect("the webhook client must build");
        let event = Event::new(EventType::RateLimitTriggered {
            key: "test".to_string(),
            rule_id: "rule".to_string(),
            decision: "Deny".to_string(),
        });

        // .invalid 是 RFC 2606 保留域名，恒不解析
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            send_webhook(
                &client,
                "https://nonexistent-host-xyzzy.invalid/hook",
                &event,
            ),
        )
        .await
        .expect("a stalled resolver must not hang the send path");
        assert!(
            result.is_err(),
            "a host that fails the pre-check must not be sent to"
        );
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

        assert_eq!(observed, 1, "handler should be called exactly once");
    }

    #[tokio::test]
    async fn test_dispatcher_stop_and_restart() {
        let emitter = EventEmitter::with_default_config();
        let dispatcher = EventDispatcher::new(emitter);

        dispatcher.start().await;
        assert!(dispatcher.dispatch_handle.read().await.is_some());

        // 再次启动不应创建新任务
        dispatcher.start().await;
        assert!(dispatcher.dispatch_handle.read().await.is_some());

        dispatcher.stop().await;
        assert!(dispatcher.dispatch_handle.read().await.is_none());

        // 重新启动
        dispatcher.start().await;
        assert!(dispatcher.dispatch_handle.read().await.is_some());

        dispatcher.stop().await;
    }
    // webhook 签名头注入（signed_webhook_payload 纯函数）

    /// 已设置进程级签名器时：body 与签名头自洽（签名覆盖线上精确字节）
    #[cfg(feature = "webhook")]
    #[tokio::test]
    async fn test_t614_signed_webhook_payload_self_consistent() {
        use crate::events::webhook_signature::{
            WebhookSigner, global_webhook_signer, set_global_webhook_signer,
        };

        // 进程级 signer 为 set-once；同测试二进制内可能已被其他用例设置。
        // 未设置则此处安装，保证后续断言有 signer 可用。
        if global_webhook_signer().is_none() {
            assert!(set_global_webhook_signer(WebhookSigner::new(
                "dispatcher-t614-secret"
            )));
        }
        let signer = global_webhook_signer().expect("signer installed");

        let event = Event::new(EventType::RateLimitTriggered {
            key: "192.0.2.7".to_string(),
            rule_id: "t614_rule".to_string(),
            decision: "Deny".to_string(),
        });

        let (body, headers) = signed_webhook_payload(&event).unwrap();
        let [(ts_name, ts_value), (sig_name, sig_value)] =
            headers.expect("signer set → headers present");
        assert_eq!(ts_name.as_str(), "x-limiteron-timestamp");
        assert_eq!(sig_name.as_str(), "x-limiteron-signature");

        // 头值格式：时间戳为数字；签名为 sha256=<hex>
        let ts: i64 = ts_value
            .to_str()
            .unwrap()
            .parse()
            .expect("timestamp header numeric");
        let sig = sig_value.to_str().unwrap();
        assert!(sig.starts_with("sha256="));

        // 用全局 signer 校验：签名必须与传输 body 匹配（防篡改核心链路）
        assert!(
            signer.verify(&body, ts, sig).is_ok(),
            "signature must verify against the transmitted body"
        );
        // body 是合法 JSON 且与事件同源
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            parsed["event_type"]["data"]["key"],
            serde_json::json!("192.0.2.7")
        );
    }
}
