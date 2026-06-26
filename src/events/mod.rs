//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 事件系统模块
//!
//! 提供事件发布/订阅系统，支持事件发射、分发和 Webhook 推送。
//!
//! # 功能
//!
//! - 事件发射器 (EventEmitter): 使用 tokio broadcast channel 实现事件发布
//! - 事件分发器 (EventDispatcher): 监听事件并分发给处理器和 Webhook
//! - 事件类型 (EventType): 预定义的系统事件类型
//! - 事件处理器 (EventHandler): 可扩展的事件处理接口
//!
//! # 示例
//!
//! ```rust
//! use limiteron::events::{EventEmitter, EventDispatcher, Event, EventType};
//!
//! #[tokio::main]
//! async fn main() {
//!     // 创建事件发射器
//!     let emitter = EventEmitter::with_default_config();
//!
//!     // 创建分发器并启动
//!     let dispatcher = EventDispatcher::new(emitter.clone());
//!     dispatcher.start().await;
//!
//!     // 发射事件
//!     let event = Event::new(EventType::RateLimitTriggered {
//!         key: "192.168.1.1".to_string(),
//!         rule_id: "rule_1".to_string(),
//!         decision: "Deny".to_string(),
//!     });
//!     emitter.emit(event).await.unwrap();
//!
//!     // 停止分发器
//!     dispatcher.stop().await;
//! }
//! ```
//!
//! # 事件类型
//!
//! | 事件类型 | 描述 | 严重级别 |
//! |---------|------|---------|
//! | `RateLimitTriggered` | 限流触发 | 2-6 |
//! | `BanApplied` | 封禁应用 | 8 |
//! | `BanExpired` | 封禁过期 | 1 |
//! | `CircuitStateChanged` | 熔断器状态变更 | 1-9 |
//! | `QuotaAlert` | 配额告警 | 3-7 |

// 子模块
mod dispatcher;
mod emitter;
mod types;

// Re-export all public types
pub use dispatcher::EventDispatcher;
pub use emitter::{EventEmitter, EventEmitterBuilder};
pub use types::{Event, EventConfig, EventHandler, EventType};
