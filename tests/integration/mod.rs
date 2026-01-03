//! 集成测试模块
//!
//! 测试各组件之间的集成和交互

mod postgres_test;
mod redis_test;

pub use postgres_test::*;
pub use redis_test::*;
