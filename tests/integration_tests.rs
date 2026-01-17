//! 集成测试入口
//!
//! 使用新的模块化测试结构

mod integration;
mod modules;

#[cfg(test)]
mod tests {
    // 集成测试在integration模块中定义
    // 使用 cargo test --test integration_tests 运行
}
