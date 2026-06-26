//! 属性测试模块
//!
//! 使用 proptest 对限流算法进行属性测试，验证在各种输入条件下的不变性。
//!
//! ## 测试覆盖
//! - Token Bucket: 令牌补充、容量限制、并发安全
//! - Fixed Window: 窗口重置、计数限制、边界条件
//! - Sliding Window: 时间窗口滑动、计数准确性
//! - 并发场景: 多线程竞争下的限制保证

pub mod concurrency;
pub mod fixed_window;
pub mod sliding_window;
pub mod token_bucket;

/// 属性测试配置
///
/// 使用固定seed确保测试可重现
pub const PROPTEST_SEED: u64 = 42;
