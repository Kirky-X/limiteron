//! 匹配器模块集成测试
//!
//! 测试匹配器与其他组件的集成

use limiteron::matchers::{Matcher, RequestContext};

/// 测试IP匹配器与存储的集成
#[tokio::test]
async fn test_ip_matcher_with_storage() {
    use limiteron::matchers::IpMatcher;
    use limiteron::storage::MemoryStorage;

    let storage = MemoryStorage::new();
    let matcher = IpMatcher::new(vec!["192.168.1.0/24".to_string()]);

    let mut ctx = RequestContext::new();
    ctx.ip = Some("192.168.1.100".to_string());

    // 匹配成功
    assert!(matcher.matches(&ctx));

    // 消费配额
    let result = storage
        .consume(
            "user1",
            "resource1",
            10,
            1000,
            std::time::Duration::from_secs(60),
        )
        .await
        .unwrap();
    assert!(result.allowed);
}

/// 测试用户ID匹配器与存储的集成
#[tokio::test]
async fn test_user_matcher_with_storage() {
    use limiteron::matchers::UserMatcher;
    use limiteron::storage::MemoryStorage;

    let storage = MemoryStorage::new();
    let matcher = UserMatcher::new(vec!["user1".to_string(), "user2".to_string()]);

    let mut ctx = RequestContext::new();
    ctx.user_id = Some("user1".to_string());

    // 匹配成功
    assert!(matcher.matches(&ctx));

    // 消费配额
    let result = storage
        .consume(
            "user1",
            "resource1",
            10,
            1000,
            std::time::Duration::from_secs(60),
        )
        .await
        .unwrap();
    assert!(result.allowed);
}

/// 测试设备ID匹配器与存储的集成
#[tokio::test]
async fn test_device_matcher_with_storage() {
    use limiteron::matchers::DeviceMatcher;
    use limiteron::storage::MemoryStorage;

    let storage = MemoryStorage::new();
    let matcher = DeviceMatcher::new(vec!["device1".to_string(), "device2".to_string()]);

    let mut ctx = RequestContext::new();
    ctx.device_id = Some("device1".to_string());

    // 匹配成功
    assert!(matcher.matches(&ctx));

    // 消费配额
    let result = storage
        .consume(
            "user1",
            "resource1",
            10,
            1000,
            std::time::Duration::from_secs(60),
        )
        .await
        .unwrap();
    assert!(result.allowed);
}

/// 测试匹配器的复合条件
#[tokio::test]
async fn test_matcher_composite_conditions() {
    use limiteron::matchers::{IpMatcher, UserMatcher};

    let ip_matcher = IpMatcher::new(vec!["192.168.1.0/24".to_string()]);
    let user_matcher = UserMatcher::new(vec!["user1".to_string()]);

    let mut ctx = RequestContext::new();
    ctx.ip = Some("192.168.1.100".to_string());
    ctx.user_id = Some("user1".to_string());

    // 两个匹配器都应该匹配
    assert!(ip_matcher.matches(&ctx));
    assert!(user_matcher.matches(&ctx));
}
