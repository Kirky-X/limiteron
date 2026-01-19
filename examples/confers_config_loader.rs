//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 使用 ConfigLoader 从文件加载配置的示例
//!
//! 运行方式：
//! ```bash
//! cargo run --example confers_config_loader --features confers
//! ```

use ahash::AHashMap as HashMap;
use ahash::HashMapExt;
use limiteron::config_loader::ConfigLoader;
use limiteron::storage::MemoryStorage;
use std::io::Write;
use tempfile::NamedTempFile;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建临时配置文件
    let mut temp_file = NamedTempFile::with_suffix(".yaml")?;
    writeln!(
        temp_file,
        r#"
version: "1.0"
global:
  storage: "memory"
  cache: "memory"
  metrics: "prometheus"
rules:
  - id: "api_rate_limit"
    name: "API Rate Limit"
    priority: 100
    matchers:
      - type: User
        user_ids: ["*"]
    limiters:
      - type: TokenBucket
        capacity: 1000
        refill_rate: 100
    action:
      on_exceed: "reject"
"#
    )?;

    println!("从配置文件加载配置: {}", temp_file.path().display());

    // 使用 ConfigLoader 加载配置
    let config = ConfigLoader::load_from_file(temp_file.path())?;

    println!("✅ 配置加载成功！");
    println!("版本: {}", config.version);
    println!("存储类型: {}", config.global.storage);
    println!("规则数量: {}", config.rules.len());

    for rule in &config.rules {
        println!("  规则: {} (优先级: {})", rule.name, rule.priority);
        println!("    匹配器数量: {}", rule.matchers.len());
        println!("    限流器数量: {}", rule.limiters.len());
    }

    // 创建存储后端
    let storage = std::sync::Arc::new(MemoryStorage::new());
    let ban_storage = std::sync::Arc::new(MemoryStorage::new());

    // 使用配置创建 Governor
    let governor =
        limiteron::Governor::from_config_file(temp_file.path(), storage, ban_storage).await?;

    println!("✅ Governor 创建成功！");

    // 测试请求检查
    use limiteron::matchers::RequestContext;

    let context = RequestContext {
        user_id: Some("user123".to_string()),
        ip: Some("192.168.1.1".to_string()),
        mac: None,
        device_id: None,
        api_key: None,
        headers: HashMap::new(),
        path: "/api/test".to_string(),
        method: "GET".to_string(),
        client_ip: Some("192.168.1.1".to_string()),
        query_params: HashMap::new(),
    };

    let decision = governor.check(&context).await?;
    println!("请求决策: {:?}", decision);

    Ok(())
}
