//! 集成测试模块
//!
//! 测试各组件之间的集成和交互
//!
//! 注意：集成测试严禁使用Mock对象，必须测试真实的功能交互。
//! Mock存储的测试已移至 tests/common/mock_tests.rs

pub mod ban_manager_storage;
pub mod cache_storage;
pub mod circuit_breaker_fallback;
pub mod config;
pub mod governor_limiters;
pub mod matcher_limiter;
pub mod quota_alert;
pub mod real_storage;
