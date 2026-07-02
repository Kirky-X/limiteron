//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 事件类型定义
//!
//! 提供事件系统使用的类型、枚举和元数据结构。

use ahash::AHashMap as StdHashMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 事件类型枚举
///
/// 定义了系统中所有可能触发的事件类型。
/// 每种事件类型携带不同的上下文数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum EventType {
    /// 限流触发事件
    ///
    /// 当请求被限流时触发此事件。
    RateLimitTriggered {
        /// 限流键（如 IP 地址、用户 ID 等）
        key: String,
        /// 触发限流的规则 ID
        rule_id: String,
        /// 限流决策（Allow、Deny、Fallback 等）
        decision: String,
    },

    /// 封禁应用事件
    ///
    /// 当目标被封禁时触发此事件。
    BanApplied {
        /// 封禁目标（如 IP、用户 ID 等）
        target: String,
        /// 封禁原因
        reason: String,
        /// 封禁时长（秒）
        duration: u64,
    },

    /// 封禁过期事件
    ///
    /// 当封禁自动过期时触发此事件。
    BanExpired {
        /// 封禁目标
        target: String,
    },

    /// 熔断器状态变更事件
    ///
    /// 当熔断器状态发生变化时触发此事件。
    CircuitStateChanged {
        /// 变更前状态
        from: String,
        /// 变更后状态
        to: String,
    },

    /// 配额告警事件
    ///
    /// 当配额使用率达到告警阈值时触发此事件。
    QuotaAlert {
        /// 用户 ID
        user_id: String,
        /// 资源类型
        resource: String,
        /// 使用率百分比（0-100）
        usage_percent: f64,
    },
}

impl EventType {
    /// 获取事件类型的名称
    pub fn name(&self) -> &'static str {
        match self {
            EventType::RateLimitTriggered { .. } => "rate_limit_triggered",
            EventType::BanApplied { .. } => "ban_applied",
            EventType::BanExpired { .. } => "ban_expired",
            EventType::CircuitStateChanged { .. } => "circuit_state_changed",
            EventType::QuotaAlert { .. } => "quota_alert",
        }
    }

    /// 获取事件的严重级别
    ///
    /// 返回 0-10 的数值，数值越高表示越严重。
    pub fn severity(&self) -> u8 {
        match self {
            EventType::RateLimitTriggered { decision, .. } => match decision.as_str() {
                "Deny" => 6,
                "Fallback" => 4,
                _ => 2,
            },
            EventType::BanApplied { .. } => 8,
            EventType::BanExpired { .. } => 1,
            EventType::CircuitStateChanged { to, .. } => match to.as_str() {
                "Open" => 9,
                "HalfOpen" => 5,
                "Closed" => 1,
                _ => 3,
            },
            EventType::QuotaAlert { usage_percent, .. } => {
                if *usage_percent >= 90.0 {
                    7
                } else if *usage_percent >= 75.0 {
                    5
                } else {
                    3
                }
            }
        }
    }
}

/// 事件结构体
///
/// 系统中所有事件的统一包装结构，包含事件类型、时间戳和扩展元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// 事件类型
    pub event_type: EventType,

    /// 事件发生时间
    pub timestamp: DateTime<Utc>,

    /// 扩展元数据
    ///
    /// 允许携带任意额外的上下文信息。
    #[serde(default)]
    pub metadata: StdHashMap<String, serde_json::Value>,
}

impl Event {
    /// 创建新事件
    pub fn new(event_type: EventType) -> Self {
        Self {
            event_type,
            timestamp: Utc::now(),
            metadata: StdHashMap::new(),
        }
    }

    /// 创建新事件并附带元数据
    pub fn with_metadata(
        event_type: EventType,
        metadata: StdHashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            event_type,
            timestamp: Utc::now(),
            metadata,
        }
    }

    /// 添加元数据
    pub fn add_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }

    /// 获取事件名称
    pub fn name(&self) -> &'static str {
        self.event_type.name()
    }

    /// 获取事件严重级别
    pub fn severity(&self) -> u8 {
        self.event_type.severity()
    }
}

/// 事件处理器 trait
///
/// 实现此 trait 可以处理特定类型的事件。
#[async_trait::async_trait]
pub trait EventHandler: Send + Sync {
    /// 处理事件
    ///
    /// # 参数
    /// - `event`: 要处理的事件
    ///
    /// # 返回
    /// - `Ok(())`: 处理成功
    /// - `Err(Box<dyn std::error::Error>)`: 处理失败
    async fn handle(&self, event: &Event) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// 获取处理器名称（用于日志和调试）
    fn name(&self) -> &str;

    /// 是否处理指定类型的事件
    ///
    /// 默认实现：处理所有事件类型。
    fn accepts(&self, _event_type: &EventType) -> bool {
        true
    }
}

/// 事件系统配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventConfig {
    /// 是否启用事件系统
    pub enabled: bool,

    /// 广播通道容量（最大缓冲事件数）
    pub channel_capacity: usize,

    /// 是否启用 Webhook 推送
    pub webhook_enabled: bool,

    /// Webhook URL 列表
    pub webhook_urls: Vec<String>,

    /// 是否启用事件日志
    pub event_logging_enabled: bool,
}

impl Default for EventConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            channel_capacity: 1024,
            webhook_enabled: false,
            webhook_urls: Vec::new(),
            event_logging_enabled: true,
        }
    }
}

impl EventConfig {
    /// 创建启用的配置
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            ..Default::default()
        }
    }

    /// 设置通道容量
    pub fn with_channel_capacity(mut self, capacity: usize) -> Self {
        self.channel_capacity = capacity;
        self
    }

    /// 启用 Webhook 推送
    pub fn with_webhook_urls(mut self, urls: Vec<String>) -> Self {
        self.webhook_enabled = !urls.is_empty();
        self.webhook_urls = urls;
        self
    }

    /// 禁用事件日志
    pub fn without_logging(mut self) -> Self {
        self.event_logging_enabled = false;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_name() {
        let rate_limit = EventType::RateLimitTriggered {
            key: "192.168.1.1".to_string(),
            rule_id: "rule_1".to_string(),
            decision: "Deny".to_string(),
        };
        assert_eq!(rate_limit.name(), "rate_limit_triggered");

        let ban_applied = EventType::BanApplied {
            target: "192.168.1.1".to_string(),
            reason: "Excessive requests".to_string(),
            duration: 3600,
        };
        assert_eq!(ban_applied.name(), "ban_applied");
    }

    #[test]
    fn test_event_type_severity() {
        let deny = EventType::RateLimitTriggered {
            key: "key".to_string(),
            rule_id: "rule".to_string(),
            decision: "Deny".to_string(),
        };
        assert_eq!(deny.severity(), 6);

        let circuit_open = EventType::CircuitStateChanged {
            from: "Closed".to_string(),
            to: "Open".to_string(),
        };
        assert_eq!(circuit_open.severity(), 9);

        let quota_high = EventType::QuotaAlert {
            user_id: "user1".to_string(),
            resource: "api_calls".to_string(),
            usage_percent: 95.0,
        };
        assert_eq!(quota_high.severity(), 7);
    }

    #[test]
    fn test_event_creation() {
        let event_type = EventType::BanApplied {
            target: "user123".to_string(),
            reason: "Test".to_string(),
            duration: 60,
        };

        let event = Event::new(event_type.clone());
        assert!(event.timestamp <= Utc::now());
        assert!(event.metadata.is_empty());
        assert_eq!(event.name(), "ban_applied");
    }

    #[test]
    fn test_event_with_metadata() {
        let event_type = EventType::RateLimitTriggered {
            key: "key".to_string(),
            rule_id: "rule".to_string(),
            decision: "Deny".to_string(),
        };

        let mut metadata = StdHashMap::new();
        metadata.insert("request_id".to_string(), serde_json::json!("req_123"));

        let event = Event::with_metadata(event_type, metadata.clone());
        assert_eq!(event.metadata.len(), 1);
        assert_eq!(
            event.metadata.get("request_id"),
            Some(&serde_json::json!("req_123"))
        );
    }

    #[test]
    fn test_event_add_metadata() {
        let event_type = EventType::BanExpired {
            target: "user123".to_string(),
        };

        let event = Event::new(event_type).add_metadata(
            "cleanup_reason".to_string(),
            serde_json::json!("auto_cleanup"),
        );

        assert_eq!(event.metadata.len(), 1);
    }

    #[test]
    fn test_event_config_default() {
        let config = EventConfig::default();
        assert!(config.enabled);
        assert_eq!(config.channel_capacity, 1024);
        assert!(!config.webhook_enabled);
        assert!(config.webhook_urls.is_empty());
        assert!(config.event_logging_enabled);
    }

    #[test]
    fn test_event_config_builder() {
        let config = EventConfig::enabled()
            .with_channel_capacity(2048)
            .with_webhook_urls(vec!["http://example.com/hook".to_string()])
            .without_logging();

        assert!(config.enabled);
        assert_eq!(config.channel_capacity, 2048);
        assert!(config.webhook_enabled);
        assert_eq!(config.webhook_urls.len(), 1);
        assert!(!config.event_logging_enabled);
    }

    #[test]
    fn test_event_serialization() {
        let event_type = EventType::CircuitStateChanged {
            from: "Closed".to_string(),
            to: "Open".to_string(),
        };

        let event = Event::new(event_type);
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name(), event.name());
    }

    /// 测试 EventHandler trait 的 accepts 默认实现
    /// 默认实现应返回 true（处理所有事件类型）
    #[test]
    fn test_event_handler_accepts_default() {
        struct DummyHandler;

        #[async_trait::async_trait]
        impl EventHandler for DummyHandler {
            async fn handle(
                &self,
                _event: &Event,
            ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                Ok(())
            }

            fn name(&self) -> &str {
                "dummy"
            }
        }

        let handler = DummyHandler;
        let event_type = EventType::RateLimitTriggered {
            key: "k".to_string(),
            rule_id: "r".to_string(),
            decision: "Deny".to_string(),
        };
        // 默认 accepts 应返回 true
        assert!(handler.accepts(&event_type));

        let ban_event = EventType::BanApplied {
            target: "t".to_string(),
            reason: "r".to_string(),
            duration: 60,
        };
        assert!(handler.accepts(&ban_event));
    }
}
