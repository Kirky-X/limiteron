//! 真实存储集成测试
//!
//! 这些测试需要真实的 PostgreSQL 数据库连接。
//! 运行前请启动 Docker Compose: `docker-compose up -d`
//!
//! 运行命令: `cargo test --test integration_tests -- --ignored`

pub mod postgres_ban;
pub mod postgres_quota;
pub mod postgres_storage;
