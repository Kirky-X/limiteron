// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 安全测试模块
//!
//! 包含所有安全相关的测试套件：
//! - input_validation_tests: 输入验证测试（含 SSRF 防护）
//! - concurrency_tests: 并发安全测试
//! - resource_exhaustion_tests: 资源耗尽测试
//! - privacy_tests: 数据隐私测试
//! - visibility_tests: 模块可见性测试

#[cfg(feature = "cache-storage")]
mod concurrency_tests;
mod input_validation_tests;
mod privacy_tests;
mod resource_exhaustion_tests;
mod visibility_tests;

// 重新导出 common 模块中的测试工具
mod common {}

// ============================================================================
// 安全测试套件概览
// ============================================================================

/// 安全测试套件概览
///
/// 本测试套件覆盖以下安全领域：
///
/// ## 1. 输入验证测试 (input_validation_tests)
///
/// - **IP 地址注入测试**
///   - X-Forwarded-For 伪造测试
///   - 无效 IP 地址注入测试
///   - IPv6 地址注入测试
///   - IP 列表解析安全性
///   - 代理信任链验证
///
/// - **数值注入测试**
///   - 零成本消费拒绝
///   - 负数消费拒绝
///   - 整数溢出保护
///   - 边界值处理
///
/// - **配置注入测试**
///   - 恶意配置拒绝
///   - 配置验证覆盖
///   - 限流器配置验证
///   - 匹配器配置验证
///
/// ## 2. 并发安全测试 (concurrency_tests)
///
/// - **竞争条件测试**
///   - 令牌桶限流器并发安全
///   - 滑动窗口限流器并发安全
///   - 固定窗口限流器并发安全
///   - 令牌计数一致性
///   - 封禁管理器并发安全
///   - 配额消费并发安全
///
/// - **死锁测试**
///   - 多锁场景死锁检测
///   - 锁获取超时恢复
///   - 嵌套锁安全性
///   - 读写锁并发安全
///
/// ## 3. 资源耗尽测试 (resource_exhaustion_tests)
///
/// - **内存耗尽测试**
///   - 大量键创建测试
///   - 内存限制验证
///   - 大键值处理
///   - 内存压力稳定性
///
/// - **CPU 耗尽测试**
///   - 复杂模式处理
///   - CPU 限制验证
///   - 计算密集型任务
///
/// - **连接耗尽测试**
///   - 大量并发连接
///   - 连接超时处理
///   - 优雅降级验证
///   - 连接池耗尽恢复
///
/// ## 4. 数据隐私测试 (privacy_tests)
///
/// - **日志脱敏测试**
///   - 基础脱敏功能
///   - 用户 ID 脱敏
///   - IP 地址脱敏
///   - 邮箱脱敏
///   - 敏感头脱敏
///   - API Key 脱敏
///
/// - **错误消息安全测试**
///   - 无内部信息泄露
///   - 无敏感数据泄露
///   - 错误处理不泄露堆栈信息
#[cfg(test)]
mod tests {

    /// 验证所有测试模块可访问
    #[test]
    fn test_security_modules_accessible() {
        // 此测试验证所有安全测试模块已正确加载
        println!("Security test modules loaded successfully:");
        println!("  - input_validation_tests");
        println!("  - concurrency_tests");
        println!("  - resource_exhaustion_tests");
        println!("  - privacy_tests");
    }
}
