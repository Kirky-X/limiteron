//! E2E测试入口
//!
//! 使用新的模块化测试结构

mod e2e;
mod modules;

#[cfg(test)]
mod tests {
    // E2E测试在e2e模块中定义
    // 使用 cargo test --test e2e_tests 运行
}
