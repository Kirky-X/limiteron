//! 错误处理测试
//!
//! 验证并行执行中的错误处理机制

use limiteron::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 错误处理测试 ===");

    // 创建测试配置
    let config = create_test_config();
    let storage = Arc::new(limiteron::storage::MemoryStorage::new());
    let ban_storage = Arc::new(limiteron::storage::MemoryStorage::new());

    #[cfg(feature = "monitoring")]
    let metrics = Arc::new(limiteron::telemetry::Metrics::new());
    #[cfg(feature = "telemetry")]
    let tracer = Arc::new(limiteron::telemetry::Tracer::new(false));

    let governor = Arc::new(
        Governor::new(
            config,
            storage,
            ban_storage,
            #[cfg(feature = "monitoring")]
            Some(metrics),
            #[cfg(feature = "telemetry")]
            Some(tracer),
        )
        .await?,
    );

    test_error_handling(&governor).await?;
    test_parallel_error_isolation(&governor).await?;

    Ok(())
}

fn create_test_config() -> FlowControlConfig {
    use limiteron::config::{ActionConfig, GlobalConfig, LimiterConfig, Matcher, Rule};

    FlowControlConfig {
        version: "1.0".to_string(),
        global: GlobalConfig {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
        },
        rules: vec![Rule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            priority: 100,
            matchers: vec![Matcher::User {
                user_ids: vec!["*".to_string()],
            }],
            limiters: vec![LimiterConfig::TokenBucket {
                capacity: 1000,
                refill_rate: 100,
            }],
            action: ActionConfig {
                on_exceed: "allow".to_string(),
                ban: None,
            },
        }],
    }
}

async fn test_error_handling(governor: &Arc<Governor>) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- 错误处理测试 ---");

    let normal_result = governor.check_resource_parallel("test_user").await;
    assert!(normal_result.is_ok());
    println!("✓ 正常请求处理正确");

    let empty_result = governor.check_resource_parallel("").await;
    println!("空用户ID处理: {:?}", empty_result);

    let long_user_id = "a".repeat(10000);
    let long_result = governor.check_resource_parallel(&long_user_id).await;
    println!("超长用户ID处理: {:?}", long_result);

    let special_chars = "test\x00user\x01\x02";
    let special_result = governor.check_resource_parallel(special_chars).await;
    println!("特殊字符用户ID处理: {:?}", special_result);

    Ok(())
}

async fn test_parallel_error_isolation(
    governor: &Arc<Governor>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- 并行错误隔离测试 ---");

    let long_user_id = "a".repeat(1000);
    let test_cases = vec![
        ("normal_user", "正常用户"),
        ("", "空用户ID"),
        (&*long_user_id, "超长用户ID"),
        ("test\x00user", "特殊字符用户"),
    ];

    let mut handles = Vec::new();

    for (user_id, description) in test_cases {
        let governor = governor.clone();
        let user_id = user_id.to_string();
        let description = description.to_string();
        let handle = tokio::spawn(async move {
            let result = governor.check_resource_parallel(&user_id).await;
            (description, result)
        });
        handles.push(handle);
    }

    for handle in handles {
        let (description, result) = handle.await?;
        match &result {
            Ok(_) => println!("✓ {}: 成功", description),
            Err(e) => println!("✗ {}: 错误 - {:?}", description, e),
        }
    }

    Ok(())
}
