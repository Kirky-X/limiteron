//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 事件发射器
//!
//! 使用 tokio broadcast channel 实现事件的发布/订阅模式。

use crate::events::types::{Event, EventConfig};
use log::debug;
use std::sync::Arc;
use tokio::sync::broadcast;

/// 事件发射器
///
/// 使用 tokio broadcast channel 实现事件的发布/订阅模式。
/// 支持多个订阅者同时接收事件。
///
/// # 示例
///
/// ```rust
/// use limiteron::events::{EventEmitter, Event, EventType, EventConfig};
///
/// #[tokio::main]
/// async fn main() {
///     let config = EventConfig::enabled().with_channel_capacity(512);
///     let emitter = EventEmitter::new(config);
///
///     // 创建订阅者
///     let mut receiver = emitter.subscribe();
///
///     // 发射事件
///     let event = Event::new(EventType::RateLimitTriggered {
///         key: "192.168.1.1".to_string(),
///         rule_id: "rule_1".to_string(),
///         decision: "Deny".to_string(),
///     });
///     emitter.emit(event).await;
///
///     // 接收事件
///     if let Ok(received) = receiver.recv().await {
///         println!("Received event: {}", received.name());
///     }
/// }
/// ```
#[derive(Clone)]
pub struct EventEmitter {
    /// 广播发送器
    sender: broadcast::Sender<Event>,

    /// 配置
    config: Arc<EventConfig>,
}

impl EventEmitter {
    /// 创建新的事件发射器
    ///
    /// # 参数
    /// - `config`: 事件系统配置
    ///
    /// # 返回
    /// - 事件发射器实例
    pub fn new(config: EventConfig) -> Self {
        let config = Arc::new(config);
        let sender = broadcast::Sender::new(config.channel_capacity);

        debug!(
            "EventEmitter created with channel_capacity={}",
            config.channel_capacity
        );

        Self { sender, config }
    }

    /// 使用默认配置创建事件发射器
    pub fn with_default_config() -> Self {
        Self::new(EventConfig::default())
    }

    /// 创建事件发射器构建器
    ///
    /// 返回 [`EventEmitterBuilder`]，用于链式配置事件发射器。
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::events::EventEmitter;
    ///
    /// let emitter = EventEmitter::builder()
    ///     .with_channel_capacity(2048)
    ///     .with_webhook_urls(vec!["http://example.com/hook".to_string()])
    ///     .build();
    /// ```
    pub fn builder() -> EventEmitterBuilder {
        EventEmitterBuilder::new()
    }

    /// 发射事件
    ///
    /// # 参数
    /// - `event`: 要发射的事件
    ///
    /// # 返回
    /// - `Ok(usize)`: 成功接收事件的订阅者数量
    /// - `Err(broadcast::error::SendError<Event>)`: 发送失败
    pub async fn emit(&self, event: Event) -> Result<usize, broadcast::error::SendError<Event>> {
        if !self.config.enabled {
            debug!("Event system is disabled, dropping event: {}", event.name());
            return Ok(0);
        }

        if self.config.event_logging_enabled {
            debug!(
                "Emitting event: name={}, severity={}",
                event.name(),
                event.severity()
            );
        }

        let receiver_count = self.sender.send(event)?;

        if self.config.event_logging_enabled {
            debug!("Event emitted to {} receivers", receiver_count);
        }

        Ok(receiver_count)
    }

    /// 创建事件订阅者
    ///
    /// # 返回
    /// - `broadcast::Receiver<Event>`: 事件接收器
    ///
    /// # 注意
    ///
    /// 订阅者只会接收创建订阅后发射的事件。
    /// 如果通道已满，最旧的事件将被丢弃。
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        let receiver = self.sender.subscribe();
        debug!("New event subscriber created");
        receiver
    }

    /// 获取当前订阅者数量
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// 获取配置
    pub fn config(&self) -> &EventConfig {
        &self.config
    }

    /// 检查事件系统是否启用
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

/// 事件发射器构建器
///
/// 用于链式配置事件发射器。
///
/// # 示例
///
/// ```rust
/// use limiteron::events::EventEmitter;
///
/// let emitter = EventEmitter::builder()
///     .with_channel_capacity(2048)
///     .with_webhook_urls(vec!["http://example.com/hook".to_string()])
///     .build();
/// ```
pub struct EventEmitterBuilder {
    config: EventConfig,
}

impl EventEmitterBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self {
            config: EventConfig::default(),
        }
    }

    /// 设置通道容量
    pub fn with_channel_capacity(mut self, capacity: usize) -> Self {
        self.config.channel_capacity = capacity;
        self
    }

    /// 启用/禁用事件系统
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.config.enabled = enabled;
        self
    }

    /// 设置 Webhook URL 列表
    pub fn with_webhook_urls(mut self, urls: Vec<String>) -> Self {
        self.config.webhook_enabled = !urls.is_empty();
        self.config.webhook_urls = urls;
        self
    }

    /// 启用/禁用事件日志
    pub fn with_logging(mut self, enabled: bool) -> Self {
        self.config.event_logging_enabled = enabled;
        self
    }

    /// 构建事件发射器
    pub fn build(self) -> EventEmitter {
        EventEmitter::new(self.config)
    }
}

impl Default for EventEmitterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::types::EventType;

    #[tokio::test]
    async fn test_event_emitter_emit_and_receive() {
        let emitter = EventEmitter::with_default_config();
        let mut receiver = emitter.subscribe();

        let event = Event::new(EventType::RateLimitTriggered {
            key: "test_key".to_string(),
            rule_id: "test_rule".to_string(),
            decision: "Deny".to_string(),
        });

        let result = emitter.emit(event.clone()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);

        let received = receiver.recv().await.unwrap();
        assert_eq!(received.name(), event.name());
    }

    #[tokio::test]
    async fn test_event_emitter_multiple_subscribers() {
        let emitter = EventEmitter::with_default_config();
        let mut receiver1 = emitter.subscribe();
        let mut receiver2 = emitter.subscribe();

        let event = Event::new(EventType::BanApplied {
            target: "user123".to_string(),
            reason: "Test".to_string(),
            duration: 60,
        });

        let result = emitter.emit(event).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2);

        let received1 = receiver1.recv().await.unwrap();
        let received2 = receiver2.recv().await.unwrap();

        assert_eq!(received1.name(), received2.name());
    }

    #[tokio::test]
    async fn test_event_emitter_disabled() {
        let config = EventConfig {
            enabled: false,
            ..EventConfig::default()
        };
        let emitter = EventEmitter::new(config);

        let event = Event::new(EventType::CircuitStateChanged {
            from: "Closed".to_string(),
            to: "Open".to_string(),
        });

        let result = emitter.emit(event).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_event_emitter_receiver_count() {
        let emitter = EventEmitter::with_default_config();
        assert_eq!(emitter.receiver_count(), 0);

        let _receiver1 = emitter.subscribe();
        assert_eq!(emitter.receiver_count(), 1);

        let _receiver2 = emitter.subscribe();
        assert_eq!(emitter.receiver_count(), 2);
    }

    #[tokio::test]
    async fn test_event_emitter_clone() {
        let emitter1 = EventEmitter::with_default_config();
        let mut receiver = emitter1.subscribe();

        let emitter2 = emitter1.clone();

        let event = Event::new(EventType::QuotaAlert {
            user_id: "user1".to_string(),
            resource: "api_calls".to_string(),
            usage_percent: 85.0,
        });

        let result = emitter2.emit(event).await;
        assert!(result.is_ok());

        let received = receiver.recv().await.unwrap();
        assert_eq!(received.name(), "quota_alert");
    }

    #[test]
    fn test_event_emitter_builder() {
        let emitter = EventEmitterBuilder::new()
            .with_channel_capacity(2048)
            .enabled(true)
            .with_logging(false)
            .build();

        assert!(emitter.is_enabled());
        assert_eq!(emitter.config().channel_capacity, 2048);
        assert!(!emitter.config().event_logging_enabled);
    }

    #[tokio::test]
    async fn test_event_emitter_with_metadata() {
        use ahash::AHashMap as StdHashMap;

        let emitter = EventEmitter::with_default_config();
        let mut receiver = emitter.subscribe();

        let mut metadata = StdHashMap::new();
        metadata.insert("request_id".to_string(), serde_json::json!("req_123"));

        let event = Event::with_metadata(
            EventType::RateLimitTriggered {
                key: "key".to_string(),
                rule_id: "rule".to_string(),
                decision: "Deny".to_string(),
            },
            metadata,
        );

        emitter.emit(event).await.unwrap();
        let received = receiver.recv().await.unwrap();

        assert_eq!(
            received.metadata.get("request_id"),
            Some(&serde_json::json!("req_123"))
        );
    }
}
