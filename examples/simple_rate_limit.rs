//! 简单限流示例

use limiteron::config::FlowControlConfig;
use limiteron::Governor;

#[tokio::main]
async fn main() {
    #[cfg(feature = "telemetry")]
    tracing_subscriber::fmt::init();

    #[cfg(not(feature = "telemetry"))]
    println!("启用telemetry feature以查看日志");

    // 创建配置
    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
        },
        rules: vec![],
    };

    // 创建Governor
    let storage = std::sync::Arc::new(limiteron::storage::MemoryStorage::new());
    let ban_storage = std::sync::Arc::new(limiteron::storage::MemoryStorage::new());
    let governor = Governor::new(config, storage, ban_storage, None, None)
        .await
        .unwrap();

    // 检查请求
    let ctx = limiteron::RequestContext {
        user_id: None,
        ip: None,
        mac: None,
        device_id: None,
        api_key: None,
        headers: std::collections::HashMap::new(),
        path: "/api/test".to_string(),
        method: "GET".to_string(),
        client_ip: None,
        query_params: std::collections::HashMap::new(),
    };
    match governor.check(&ctx).await {
        Ok(limiteron::Decision::Allowed(_)) => {
            println!("请求被允许");
        }
        Ok(limiteron::Decision::Rejected(reason)) => {
            println!("请求被拒绝: {}", reason);
        }
        Ok(limiteron::Decision::Banned(info)) => {
            println!("请求被封禁: {}", info.reason);
        }
        Err(e) => {
            eprintln!("错误: {}", e);
        }
    }
}
