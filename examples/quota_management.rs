//! 配额管理示例

use limiteron::config::FlowControlConfig;
use limiteron::Governor;

#[tokio::main]
async fn main() {
    #[cfg(feature = "telemetry")]
    tracing_subscriber::fmt::init();

    #[cfg(not(feature = "telemetry"))]
    println!("启用telemetry feature以查看日志");

    let config = FlowControlConfig {
        version: "1.0".to_string(),
        global: limiteron::config::GlobalConfig {
            storage: "memory".to_string(),
            cache: "memory".to_string(),
            metrics: "prometheus".to_string(),
        },
        rules: vec![],
    };

    let storage = std::sync::Arc::new(limiteron::storage::MemoryStorage::new());
    let ban_storage = std::sync::Arc::new(limiteron::storage::MemoryStorage::new());
    let governor = Governor::new(config, storage, ban_storage, None, None)
        .await
        .unwrap();

    println!("配额管理示例");
    let ctx = limiteron::RequestContext {
        user_id: None,
        ip: None,
        mac: None,
        device_id: None,
        api_key: None,
        headers: std::collections::HashMap::new(),
        path: "/api/quota".to_string(),
        method: "GET".to_string(),
        client_ip: None,
        query_params: std::collections::HashMap::new(),
    };
    match governor.check(&ctx).await {
        Ok(limiteron::Decision::Allowed(_)) => {
            println!("配额检查通过");
        }
        _ => {
            println!("配额不足");
        }
    }
}
