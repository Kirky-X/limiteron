//! 集成测试模块
//!
//! 测试各组件之间的集成和交互

#[cfg(feature = "postgres")]
#[allow(unused_imports)]
mod postgres_test;
#[cfg(feature = "redis")]
#[allow(unused_imports)]
mod redis_test;

#[cfg(feature = "postgres")]
#[allow(unused_imports)]
pub use postgres_test::*;
#[cfg(feature = "redis")]
#[allow(unused_imports)]
pub use redis_test::*;
