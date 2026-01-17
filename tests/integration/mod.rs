//! 集成测试模块
//!
//! 测试各组件之间的集成和交互

#[cfg(feature = "postgres")]
mod postgres_test;
#[cfg(feature = "redis")]
mod redis_test;

#[cfg(feature = "postgres")]
pub use postgres_test::*;
#[cfg(feature = "redis")]
pub use redis_test::*;
