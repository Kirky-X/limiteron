//! 地理位置和设备类型匹配示例
//!
//! 演示如何使用GeoMatcher和DeviceMatcher进行高级匹配。

use limiteron::{
    device_matcher::{DeviceCondition, DeviceMatcher, DeviceType},
    geo_matcher::{GeoCondition, GeoMatcher},
};
use std::net::IpAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 地理位置和设备类型匹配示例 ===\n");

    // 1. 创建设备匹配器
    println!("1. 创建DeviceMatcher...");
    let device_matcher = DeviceMatcher::new().await?;

    // 2. 解析不同设备的User-Agent
    println!("\n2. 解析设备信息:");

    let test_user_agents = vec![
        "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X) AppleWebKit/605.1.15",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124",
        "Mozilla/5.0 (iPad; CPU OS 14_0 like Mac OS X) AppleWebKit/605.1.15",
        "curl/7.68.0",
        "Mozilla/5.0 (Linux; Android 10; SM-G960F) AppleWebKit/537.36",
    ];

    for ua in test_user_agents {
        let info = device_matcher.parse(ua)?;
        println!("  - {}", info.description());
        println!("    设备类型: {:?}", info.device_type);
        println!("    浏览器: {:?}", info.browser);
        println!("    操作系统: {:?}", info.os);
    }

    // 3. 设备匹配
    println!("\n3. 设备匹配测试:");

    let mobile_condition = DeviceCondition::device_types(vec![DeviceType::Mobile]);
    let desktop_condition = DeviceCondition::device_types(vec![DeviceType::Desktop]);
    let api_condition = DeviceCondition::device_types(vec![DeviceType::API]);

    let test_ua = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)";
    println!("  User-Agent: {}", test_ua);
    println!(
        "  匹配移动设备? {}",
        device_matcher.matches_user_agent(test_ua, &mobile_condition)?
    );
    println!(
        "  匹配桌面设备? {}",
        device_matcher.matches_user_agent(test_ua, &desktop_condition)?
    );
    println!(
        "  匹配API客户端? {}",
        device_matcher.matches_user_agent(test_ua, &api_condition)?
    );

    // 4. 创建地理匹配器（需要GeoLite2数据库文件）
    println!("\n4. 创建GeoMatcher...");
    println!("  注意：需要GeoLite2-City.mmdb数据库文件");

    // 尝试创建GeoMatcher
    let geo_matcher = match GeoMatcher::new("GeoLite2-City.mmdb").await {
        Ok(matcher) => {
            println!("  GeoMatcher创建成功！");

            // 5. IP地理位置查询
            println!("\n5. IP地理位置查询:");
            let test_ips = vec![
                "114.114.114.114", // 中国
                "8.8.8.8",         // 美国
                "1.1.1.1",         // 美国
            ];

            for ip_str in test_ips {
                if let Ok(ip) = ip_str.parse::<IpAddr>() {
                    match matcher.lookup(ip) {
                        Ok(info) => {
                            println!("  - {}: {}", ip, info.description());
                            println!("    国家: {:?}", info.country_code);
                            println!("    城市: {:?}", info.city);
                            println!("    大洲: {:?}", info.continent);
                        }
                        Err(e) => {
                            println!("  - {}: 查询失败 - {}", ip, e);
                        }
                    }
                }
            }

            Some(matcher)
        }
        Err(e) => {
            println!("  GeoMatcher创建失败: {}", e);
            println!("  请从MaxMind官网下载GeoLite2-City.mmdb文件");
            println!("  跳过地理位置查询演示。");
            None
        }
    };

    // 6. 地理匹配
    if let Some(ref matcher) = geo_matcher {
        println!("\n6. 地理匹配测试:");

        let china_condition = GeoCondition::countries(vec!["CN".to_string()]);
        let us_condition = GeoCondition::countries(vec!["US".to_string()]);

        let test_ip: IpAddr = "114.114.114.114".parse()?;
        println!("  IP: {}", test_ip);
        println!(
            "  匹配中国? {}",
            matcher.matches_ip(test_ip, &china_condition)?
        );
        println!(
            "  匹配美国? {}",
            matcher.matches_ip(test_ip, &us_condition)?
        );
    }

    // 7. 自定义规则
    println!("\n7. 自定义设备规则:");
    let mut custom_matcher = DeviceMatcher::new().await?;

    custom_matcher.add_custom_rule(
        "MyApp",
        r"MyApp/\d+\.\d+",
        DeviceType::Mobile,
        Some("MyApp".to_string()),
        Some("Android".to_string()),
    );

    let custom_ua = "MyApp/1.0.0 (Android 10)";
    let info = custom_matcher.parse(custom_ua)?;
    println!("  User-Agent: {}", custom_ua);
    println!("  设备类型: {:?}", info.device_type);
    println!("  浏览器: {:?}", info.browser);
    println!("  操作系统: {:?}", info.os);

    // 8. 批量处理
    println!("\n8. 批量处理:");
    let user_agents = vec![
        "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)".to_string(),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64)".to_string(),
        "curl/7.68.0".to_string(),
    ];

    let results = device_matcher.batch_parse(&user_agents);
    println!("  批量解析 {} 个User-Agent，结果:", user_agents.len());
    for (i, result) in results.iter().enumerate() {
        match result {
            Ok(info) => println!("    [{}] {}", i, info.description()),
            Err(e) => println!("    [{}] 错误: {}", i, e),
        }
    }

    // 9. 缓存统计
    println!("\n9. 缓存统计:");
    let device_stats = device_matcher.cache_stats();
    println!("  DeviceMatcher缓存大小: {}", device_stats.size);
    println!("  DeviceMatcher缓存限制: {}", device_stats.limit);

    if let Some(ref matcher) = geo_matcher {
        let geo_stats = matcher.cache_stats();
        println!("  GeoMatcher缓存大小: {}", geo_stats.size);
        println!("  GeoMatcher缓存限制: {}", geo_stats.limit);
    }

    println!("\n=== 示例完成 ===");
    Ok(())
}
