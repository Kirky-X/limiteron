//! 端到端测试模块
//!
//! 测试完整的业务流程和场景

mod scenarios;

#[cfg(test)]
mod tests {
    // E2E 场景测试在 scenarios 模块中定义
    // 使用 cargo test --test e2e_tests 运行
}
