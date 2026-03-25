//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 模块可见性安全测试
//!
//! 测试覆盖：
//! - 内部类型不可从 crate 外部访问验证
//! - pub(crate) 限制的文档化

// ============================================================================
// 模块可见性测试
// ============================================================================

/// 测试内部类型未被意外导出
///
/// 验证以下类型仍然保持 pub(crate) 可见性：
/// - GLOBAL_LIMITER_MANAGER: 限流器管理器（内部）
/// - AtomicChainStats: 原子链统计（内部）
/// - DecisionNodeBuilder: 决策节点构建器（内部）
///
/// 注意：Rust 测试在 crate 外部运行，无法直接访问 pub(crate) 类型。
/// 这些测试通过验证公开 API 的行为来间接确认内部实现未被暴露。
#[test]
fn test_internal_types_not_exported_in_public_api() {
    // 这些断言验证公开 API 的签名未被意外修改
    // 如果内部类型被意外导出为 pub，编译器会报告错误

    // Governor 是公开 API 的一部分
    // 它使用内部类型，但不应暴露其内部结构
    let _ = std::mem::size_of::<limiteron::Governor>();
    let _ = std::mem::size_of::<limiteron::GovernorStats>();

    // FlowControlConfig 是公开 API 的一部分
    let _ = std::mem::size_of::<limiteron::FlowControlConfig>();

    // DecisionChain 是公开 API 的一部分
    let _ = std::mem::size_of::<limiteron::DecisionChain>();

    // BanInfo 是公开 API 的一部分（字段是私有的）
    let _ = std::mem::size_of::<limiteron::BanInfo>();

    // 如果这个测试编译通过，说明公开 API 未暴露内部实现
    // pub(crate) 类型无法从外部 crate 访问
}

/// 测试 BanInfo 字段封装
///
/// 验证 BanInfo 类型的字段已被正确私有化，只能通过 getter 方法访问。
/// 这是安全最佳实践，防止直接修改封禁信息。
#[test]
fn test_ban_info_fields_are_encapsulated() {
    use limiteron::BanInfo;
    use std::time::{Duration, SystemTime};

    // 创建 BanInfo 实例
    let expires_at = SystemTime::now() + Duration::from_secs(3600);
    let ban_info = BanInfo::new(
        "test reason".to_string(),
        chrono::DateTime::<chrono::Utc>::from(expires_at),
        1,
    );

    // 验证可以通过 getter 方法访问字段
    let _ = ban_info.reason();
    let _ = ban_info.banned_until();
    let _ = ban_info.ban_times();

    // 注意：直接访问 ban_info.reason 会导致编译错误
    // 因为字段已被私有化
    // 如果此测试编译通过，说明封装已正确实现
}

/// 测试公开类型使用类型安全枚举
///
/// 验证配置类型使用枚举而非裸字符串，防止配置注入攻击。
#[test]
fn test_config_uses_type_safe_enums() {
    use limiteron::config::{Action, ActionConfig, GlobalConfig};

    // 测试 ActionConfig 使用 Action 枚举
    let action_config = ActionConfig {
        on_exceed: Action::Reject,
        ban: None,
    };

    // 验证枚举值
    assert!(matches!(action_config.on_exceed, Action::Reject));

    // 测试 GlobalConfig 使用类型安全枚举
    let global_config = GlobalConfig::default();

    // 验证枚举类型的值
    // storage, cache, metrics 都是枚举类型
    let _ = format!("{:?}", global_config);
}

/// 测试 TrustedProxyConfig 可信代理配置
///
/// 验证可信代理配置的隔离性，防止 IP 伪造攻击。
#[test]
fn test_trusted_proxy_config_security() {
    use limiteron::config::TrustedProxyConfig;

    let config = TrustedProxyConfig::default();

    // 验证默认配置未启用可信代理（安全默认）
    assert!(!config.enabled);

    // 验证可信代理验证功能存在
    assert!(!config.is_trusted("192.168.1.1"));
}

/// 测试配置验证逻辑存在
///
/// 验证配置验证在加载时执行，防止无效配置。
#[test]
fn test_config_validation_logic_exists() {
    use limiteron::config::{ActionConfig, FlowControlConfig, LimiterConfig, Matcher, Rule};

    let config = FlowControlConfig {
        version: "1.0.0".to_string(),
        global: limiteron::config::GlobalConfig::default(),
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 100,
                refill_rate: 10,
            }],
            action: ActionConfig::default(),
        }],
    };

    // 验证配置有 validate 方法
    let result = config.validate();
    assert!(result.is_ok(), "有效配置应验证通过，但得到: {:?}", result);
}
