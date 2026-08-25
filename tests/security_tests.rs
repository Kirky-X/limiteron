// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 安全测试套件入口
//!
//! 运行方式：cargo test --test security_tests
//!
//! 本测试套件包含以下测试模块：
//! - 输入验证测试
//! - 并发安全测试
//! - 资源耗尽测试
//! - 数据隐私测试

// 引入安全测试模块
mod security;

// 引入公共测试工具
mod common;

// ============================================================================
// 测试套件入口
// ============================================================================

/// 安全测试套件主入口
///
/// 运行所有安全测试：
/// ```bash
/// cargo test --test security_tests
/// ```
///
/// 运行特定测试模块：
/// ```bash
/// # 输入验证测试
/// cargo test --test security_tests input_validation
///
/// # 并发安全测试
/// cargo test --test security_tests concurrency
///
/// # 资源耗尽测试
/// cargo test --test security_tests resource
///
/// # 数据隐私测试
/// cargo test --test security_tests privacy
/// ```
///
/// 运行特定测试：
/// ```bash
/// cargo test --test security_tests test_x_forwarded_for_spoofing
/// ```
#[cfg(test)]
mod tests {
    use super::*;

    /// 测试套件健康检查
    #[test]
    fn test_security_suite_health() {
        println!("========================================");
        println!("Limiteron Security Test Suite");
        println!("========================================");
        println!();
        println!("Test Modules:");
        println!("  1. Input Validation Tests");
        println!("     - IP address injection");
        println!("     - Numeric injection");
        println!("     - Configuration injection");
        println!();
        println!("  2. Concurrency Safety Tests");
        println!("     - Race condition tests");
        println!("     - Deadlock tests");
        println!();
        println!("  3. Resource Exhaustion Tests");
        println!("     - Memory exhaustion");
        println!("     - CPU exhaustion");
        println!("     - Connection exhaustion");
        println!();
        println!("  4. Data Privacy Tests");
        println!("     - Log redaction");
        println!("     - Error message security");
        println!();
        println!("========================================");
        println!("All security test modules loaded successfully!");
        println!("========================================");
    }

    /// 验证测试工具可用
    #[test]
    fn test_common_tools_available() {
        // 验证 RequestContextBuilder 可用
        let _ctx = common::RequestContextBuilder::new()
            .user_id("test_user")
            .ip("192.168.1.1")
            .build();

        println!("Common test tools verified successfully");
    }
}
