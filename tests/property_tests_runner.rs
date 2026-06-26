//! 属性测试入口
//!
//! 使用 proptest 对限流算法进行属性测试

mod property_tests;

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::property_tests::*;
}
