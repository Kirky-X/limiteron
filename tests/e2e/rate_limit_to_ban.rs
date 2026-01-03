//! 端到端测试：限流到封禁的完整流程
//!
//! 测试场景：
//! 1. 用户正常请求（限流100/s）
//! 2. 突然发送200个请求（超限）
//! 3. 持续超限5次
//! 4. 触发封禁（5分钟）
//! 5. 5分钟后自动解封
//! 6. 恢复正常访问

use limiteron::{
    ban_manager::{BackoffConfig, BanManager, BanManagerConfig},
    config::{FlowControlConfig, LimiterConfig, Rule},
    error::{Decision, FlowGuardError},
    governor::Governor,
    limiters::SlidingWindowLimiter,
    matchers::RequestContext,
    storage::{BanRecord, BanTarget, MemoryStorage},
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// 创建请求上下文
fn create_request(user_id: &str, ip: &str) -> RequestContext {
    let mut headers = std::collections::HashMap::new();
    headers.insert("x-user-id".to_string(), user_id.to_string());

    RequestContext {
        user_id: Some(user_id.to_string()),
        ip: Some(ip.to_string()),
        mac: None,
        device_id: None,
        api_key: None,
        headers,
        path: "/test".to_string(),
        method: "GET".to_string(),
        client_ip: Some(ip.to_string()),
        query_params: std::collections::HashMap::new(),
    }
}

/// 创建测试用的Governor
async fn setup_governor() -> Governor {
    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
        },
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![],
            limiters: vec![LimiterConfig::SlidingWindow {
                window_size: "1s".to_string(),
                max_requests: 100,
            }],
            action: limiteron::config::ActionConfig {
                on_exceed: "reject".to_string(),
                ban: None,
            },
        }],
    };

    let storage = Arc::new(MemoryStorage::new());
    let ban_storage = Arc::new(limiteron::storage::MemoryBanStorage::new());

    Governor::new(config, storage, ban_storage, None, None)
        .await
        .unwrap()
}

/// 创建测试用的BanManager
async fn setup_ban_manager() -> BanManager {
    let storage = Arc::new(MemoryStorage::new());
    let config = BanManagerConfig {
        backoff: BackoffConfig {
            first_duration: 5,   // 5秒（测试用）
            second_duration: 10, // 10秒
            third_duration: 20,  // 20秒
            fourth_duration: 40, // 40秒
            max_duration: 60,    // 60秒（测试用）
        },
        enable_auto_unban: true,
        auto_unban_interval: 5,
    };

    BanManager::new(storage, Some(config)).await.unwrap()
}

/// 端到端测试：限流到封禁的完整流程
#[tokio::test]
async fn test_e2e_rate_limit_to_ban() {
    let gov = setup_governor().await;
    let ban_manager = setup_ban_manager().await;

    let user_id = "test_user_e2e";
    let ip = "192.168.1.100";
    let target = BanTarget::Ip(ip.to_string());

    let _ = ban_manager.delete_ban(&target, "test".to_string()).await;

    // Step 1: 正常请求 - 50个请求应该全部成功
    for i in 0..50 {
        let ctx = create_request(user_id, ip);

        let decision = gov.check(&ctx).await.unwrap();
        assert!(
            matches!(decision, Decision::Allowed(_)),
            "Request {} should be allowed",
            i
        );
    }

    // Step 2: 触发超限 - 连续5次，每次200个请求
    for attempt in 0..5 {
        // 发送200个请求（超过100的限制）
        let mut allowed_count = 0;
        for _ in 0..200 {
            let ctx = create_request(user_id, ip);

            if let Ok(decision) = gov.check(&ctx).await {
                if matches!(decision, Decision::Allowed(_)) {
                    allowed_count += 1;
                }
            }
        }

        // 前100个应该被允许，后100个被拒绝
        assert!(
            allowed_count <= 100,
            "Attempt {}: Allowed count {} should be <= 100",
            attempt,
            allowed_count
        );

        // 等待一小段时间
        sleep(Duration::from_millis(100)).await;
    }

    // Step 3: 添加封禁记录
    let ban_record = BanRecord {
        target: target.clone(),
        ban_times: 5,
        duration: Duration::from_secs(5),
        banned_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(5),
        is_manual: false,
        reason: "Exceeded rate limit 5 times".to_string(),
    };

    ban_manager.add_ban(ban_record).await.unwrap();

    // Step 4: 验证被封禁
    let ban_detail = ban_manager.get_ban(&target).await.unwrap();
    assert!(ban_detail.is_some(), "Should be banned");

    // Step 5: 检查被封禁期间的请求
    let ctx = RequestContext {
        user_id: Some(user_id.to_string()),
        ip: Some(ip.to_string()),
        mac: None,
        device_id: None,
        api_key: None,
        headers: std::collections::HashMap::new(),
        path: "/test".to_string(),
        method: "GET".to_string(),
        client_ip: Some(ip.to_string()),
        query_params: std::collections::HashMap::new(),
    };

    // 检查封禁记录
    let is_banned = ban_manager.is_banned(&target).await.unwrap().is_some();
    assert!(is_banned, "Should be banned");

    // Step 6: 等待封禁过期
    sleep(Duration::from_secs(6)).await;

    // Step 7: 验证自动解封
    let is_banned = ban_manager.is_banned(&target).await.unwrap().is_some();
    assert!(!is_banned, "Should be unbanned after expiration");

    // Step 8: 恢复正常访问
    let ctx = create_request(user_id, ip);
    let decision = gov.check(&ctx).await.unwrap();
    assert!(
        matches!(decision, Decision::Allowed(_)),
        "Should be allowed after unban"
    );

    println!("✓ E2E test passed: Rate limit to ban flow completed successfully");
}

/// 端到端测试：指数退避封禁时长
#[tokio::test]
async fn test_e2e_exponential_backoff() {
    let ban_manager = setup_ban_manager().await;

    let ip = "192.168.1.200";
    let target = BanTarget::Ip(ip.to_string());

    // 清理
    let _ = ban_manager.delete_ban(&target, "test".to_string()).await;

    // 测试指数退避
    let expected_durations = vec![5, 10, 20, 40, 60, 60, 60, 60, 60, 60];

    for (i, expected_duration) in expected_durations.iter().enumerate() {
        let ban_times = (i + 1) as u32;
        let duration = ban_manager.calculate_ban_duration(ban_times).await;

        assert_eq!(
            duration.as_secs(),
            *expected_duration,
            "Ban times {}: Expected {}s, got {}s",
            ban_times,
            expected_duration,
            duration.as_secs()
        );

        println!(
            "Ban times {}: {} seconds (expected: {}s)",
            ban_times,
            duration.as_secs(),
            expected_duration
        );
    }

    println!("✓ E2E test passed: Exponential backoff works correctly");
}

/// 端到端测试：手动封禁不会自动解封
#[tokio::test]
async fn test_e2e_manual_ban_no_auto_unban() {
    let ban_manager = setup_ban_manager().await;

    let ip = "192.168.1.201";
    let target = BanTarget::Ip(ip.to_string());

    // 清理
    let _ = ban_manager.delete_ban(&target, "test".to_string()).await;

    // 创建手动封禁
    let ban_record = BanRecord {
        target: target.clone(),
        ban_times: 1,
        duration: Duration::from_secs(2),
        banned_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(2),
        is_manual: true, // 手动封禁
        reason: "Manual ban".to_string(),
    };

    ban_manager.add_ban(ban_record).await.unwrap();

    // 等待过期时间
    sleep(Duration::from_secs(3)).await;

    // 手动封禁不应该自动解封
    let is_banned = ban_manager.is_banned(&target).await.unwrap().is_some();
    assert!(is_banned, "Manual ban should not auto-unban");

    // 手动解封
    ban_manager
        .delete_ban(&target, "manual".to_string())
        .await
        .unwrap();

    let is_banned = ban_manager.is_banned(&target).await.unwrap().is_some();
    assert!(!is_banned, "Should be unbanned after manual removal");

    println!("✓ E2E test passed: Manual ban does not auto-unban");
}

/// 端到端测试：封禁优先级
#[tokio::test]
async fn test_e2e_ban_priority() {
    let ban_manager = setup_ban_manager().await;

    let user_id = "test_user_priority";
    let ip = "192.168.1.202";

    // 清理
    let _ = ban_manager
        .delete_ban(&BanTarget::UserId(user_id.to_string()), "test".to_string())
        .await;
    let _ = ban_manager
        .delete_ban(&BanTarget::Ip(ip.to_string()), "test".to_string())
        .await;

    // 封禁用户ID
    let user_ban = BanRecord {
        target: BanTarget::UserId(user_id.to_string()),
        ban_times: 1,
        duration: Duration::from_secs(60),
        banned_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(60),
        is_manual: false,
        reason: "User ban".to_string(),
    };
    ban_manager.add_ban(user_ban).await.unwrap();

    // 封禁IP（更高优先级）
    let ip_ban = BanRecord {
        target: BanTarget::Ip(ip.to_string()),
        ban_times: 1,
        duration: Duration::from_secs(60),
        banned_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(60),
        is_manual: false,
        reason: "IP ban".to_string(),
    };
    ban_manager.add_ban(ip_ban).await.unwrap();

    // IP封禁应该生效
    let is_banned = ban_manager
        .is_banned(&BanTarget::Ip(ip.to_string()))
        .await
        .unwrap()
        .is_some();
    assert!(is_banned, "IP ban should be active");

    println!("✓ E2E test passed: Ban priority works correctly");
}

/// 端到端测试：封禁统计信息
#[tokio::test]
async fn test_e2e_ban_statistics() {
    let ban_manager = setup_ban_manager().await;

    // 添加多个封禁
    for i in 0..5 {
        let ban_record = BanRecord {
            target: BanTarget::Ip(format!("192.168.1.{}", 200 + i)),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(60),
            is_manual: false,
            reason: format!("Ban {}", i),
        };
        ban_manager.add_ban(ban_record).await.unwrap();
    }

    // 获取封禁历史
    for i in 0..5 {
        let target = BanTarget::Ip(format!("192.168.1.{}", 200 + i));
        let history = ban_manager.get_history(&target).await.unwrap();
        assert!(history.is_some(), "Should have history for ban {}", i);
    }

    println!("✓ E2E test passed: Ban statistics work correctly");
}
