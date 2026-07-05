//! 真实存储集成测试
//!
//! 这些测试需要真实的外部服务连接：
//! - PostgreSQL: 需要启动 Docker Compose: `docker-compose up -d`
//! - Redis: 需要启动 Redis 服务器或使用 Docker
//!
//! 运行命令: `cargo test --test integration_tests -- --ignored`

// PostgreSQL 存储测试（包含 Storage、BanStorage、QuotaStorage 测试）
#[cfg(feature = "postgres")]
pub mod postgres_storage;
