//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 封禁管理器模块
//!
//! 提供封禁记录的CRUD操作、指数退避算法和封禁优先级管理。
//!
//! # 功能
//!
//! - 封禁记录CRUD操作
//! - 指数退避算法（自动计算封禁时长）
//! - 封禁优先级管理（IP > User > MAC > Device > APIKey）
//! - 自动解封定时任务
//! - 完整的审计日志
//! - 并行封禁检查（性能提升 50-70%）
//!
//! # 示例
//!
//! ```rust
//! use limiteron::ban::{BanManager, BanManagerConfig, BanSource, BanTarget};
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     // 使用默认配置创建 BanManager
//!     let ban_manager = BanManager::new().await.unwrap();
//!
//!     // 手动封禁一个用户
//!     let target = BanTarget::UserId("user123".to_string());
//!     let reason = "违反服务条款".to_string();
//!     let source = BanSource::Manual { operator: "admin".to_string() };
//!
//!     let result = ban_manager.create_ban(target, reason, source, serde_json::json!({}), None).await;
//!     match result {
//!         Ok(ban_detail) => println!("用户已被封禁: {}", ban_detail.id),
//!         Err(e) => eprintln!("封禁失败: {}", e),
//!     }
//! }
//! ```

pub mod types;

pub use types::{
    BackoffConfig, BanDetail, BanFilter, BanManager, BanManagerBuilder, BanManagerConfig,
    BanPriority, BanSource, BanTarget, AUTO_UNBAN_INTERVAL_SECS, DEFAULT_PAGINATION_LIMIT,
    FIRST_BAN_DURATION_SECS, FOURTH_BAN_DURATION_SECS, MAX_BAN_DURATION_SECS,
    MAX_PAGINATION_LIMIT, SECOND_BAN_DURATION_SECS, THIRD_BAN_DURATION_SECS,
};
