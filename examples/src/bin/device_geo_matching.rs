//! Device & Geo Matching 示例
//!
//! 演示 limiteron 的设备匹配和地理匹配功能：User-Agent 解析、设备类型识别、
//! IP 地理位置查询、地理条件匹配。
//!
//! # 涵盖 API
//!
//! ## Device Matching
//! - `DeviceType` 枚举（`parse`、`as_str`、`is_mobile`、`is_desktop`、`is_api`）
//! - `DeviceInfo`（`empty`、`is_empty`、`description`）
//! - `DeviceCondition`（`empty`、`device_types`、`browsers`、`os`、`is_empty`、`matches`）
//! - `DeviceMatcher`（`new`、`builder`、`parse`、`batch_parse`、`matches_user_agent`、
//!   `matches`、`add_custom_rule`、`remove_custom_rule`、`cache_stats`、`clear_cache`）
//! - `DeviceMatcherBuilder`（`new`、`cache_size_limit`、`add_custom_rule`、`build`）
//!
//! ## Geo Matching
//! - `GeoInfo`（`empty`、`is_empty`、`description`）
//! - `GeoCondition`（`empty`、`countries`、`cities`、`continents`、`is_empty`、`matches`）
//! - `GeoMatcher`（`new`、`lookup`、`batch_lookup`、`matches_ip`、`matches`、
//!   `clear_cache`、`cache_stats`）
//!
//! # 运行方式
//!
//! ```bash
//! cargo run --bin device_geo_matching --features "device-matching,geo-matching"
//! ```
//!
//! # 注意
//!
//! - DeviceMatcher 无外部依赖，woothee 解析器内置规则
//! - GeoMatcher 需要 MaxMind GeoLite2-City.mmdb 数据库文件（通常 >50MB）
//! - 若 GeoLite2 数据库不存在，GeoMatcher 部分将跳过实际 IP 查询演示

use limiteron::matchers::device::{
    DeviceCacheStats, DeviceCondition, DeviceInfo, DeviceMatcher, DeviceMatcherBuilder,
    DeviceType,
};
use limiteron::matchers::geo::{GeoCacheStats, GeoCondition, GeoInfo, GeoMatcher};
use std::net::IpAddr;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Device & Geo Matching Demo ===\n");

    demo_device_type_enum();
    demo_device_info();
    demo_device_condition();
    demo_device_matcher().await?;
    demo_device_matcher_builder().await?;
    demo_device_custom_rules().await?;
    demo_device_cache().await?;

    demo_geo_info();
    demo_geo_condition();
    demo_geo_matcher().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

/// 演示 DeviceType 枚举
fn demo_device_type_enum() {
    println!("--- DeviceType Enum ---");

    // parse 从字符串解析
    let mobile = DeviceType::parse("mobile");
    let desktop = DeviceType::parse("desktop");
    let tablet = DeviceType::parse("tablet");
    let api = DeviceType::parse("api");
    let unknown = DeviceType::parse("invalid");

    println!(
        "parse('mobile')={:?}, parse('desktop')={:?}, parse('tablet')={:?}",
        mobile, desktop, tablet
    );
    println!(
        "parse('api')={:?}, parse('invalid')={:?}",
        api, unknown
    );

    // as_str 转换为字符串
    println!(
        "as_str: mobile='{}', desktop='{}', tablet='{}', api='{}', unknown='{}'",
        DeviceType::Mobile.as_str(),
        DeviceType::Desktop.as_str(),
        DeviceType::Tablet.as_str(),
        DeviceType::API.as_str(),
        DeviceType::Unknown.as_str()
    );

    // 类型判断方法
    assert!(DeviceType::Mobile.is_mobile());
    assert!(DeviceType::Tablet.is_mobile());
    assert!(!DeviceType::Desktop.is_mobile());
    assert!(DeviceType::Desktop.is_desktop());
    assert!(DeviceType::API.is_api());
    assert!(!DeviceType::Mobile.is_api());
    println!("is_mobile/is_desktop/is_api 验证通过");

    // Display trait
    println!("Display: {} | {} | {}", mobile, desktop, api);

    println!();
}

/// 演示 DeviceInfo 结构
fn demo_device_info() {
    println!("--- DeviceInfo ---");

    // empty 创建空设备信息
    let empty_info = DeviceInfo::empty();
    assert!(empty_info.is_empty());
    println!(
        "empty: is_empty={}, description='{}'",
        empty_info.is_empty(),
        empty_info.description()
    );

    // 完整设备信息
    let mobile_info = DeviceInfo {
        device_type: DeviceType::Mobile,
        browser: Some("Safari".to_string()),
        browser_version: Some("14.0".to_string()),
        os: Some("iOS".to_string()),
        os_version: Some("14.0".to_string()),
        user_agent: None,
    };
    println!("mobile_info: description='{}'", mobile_info.description());

    let desktop_info = DeviceInfo {
        device_type: DeviceType::Desktop,
        browser: Some("Chrome".to_string()),
        browser_version: Some("91.0".to_string()),
        os: Some("Windows".to_string()),
        os_version: Some("10".to_string()),
        user_agent: None,
    };
    println!("desktop_info: description='{}'", desktop_info.description());

    let api_info = DeviceInfo {
        device_type: DeviceType::API,
        browser: None,
        browser_version: None,
        os: None,
        os_version: None,
        user_agent: None,
    };
    println!("api_info: description='{}'", api_info.description());

    // Default trait
    let default_info = DeviceInfo::default();
    assert!(default_info.is_empty());
    println!("DeviceInfo::default() is_empty={}", default_info.is_empty());

    // PartialEq
    assert_eq!(empty_info, DeviceInfo::empty());
    println!("DeviceInfo PartialEq 验证通过");

    println!();
}

/// 演示 DeviceCondition 匹配条件
fn demo_device_condition() {
    println!("--- DeviceCondition ---");

    // empty 创建空条件
    let empty_cond = DeviceCondition::empty();
    assert!(empty_cond.is_empty());
    println!("empty condition: is_empty={}", empty_cond.is_empty());

    // 空条件匹配任何设备
    let info = DeviceInfo::empty();
    assert!(empty_cond.matches(&info));
    println!("空条件匹配空设备信息: true");

    // device_types 条件
    let mobile_cond = DeviceCondition::device_types(vec![DeviceType::Mobile, DeviceType::Tablet]);
    let mobile_info = DeviceInfo {
        device_type: DeviceType::Mobile,
        browser: None,
        browser_version: None,
        os: None,
        os_version: None,
        user_agent: None,
    };
    let desktop_info = DeviceInfo {
        device_type: DeviceType::Desktop,
        browser: None,
        browser_version: None,
        os: None,
        os_version: None,
        user_agent: None,
    };
    assert!(mobile_cond.matches(&mobile_info));
    assert!(!mobile_cond.matches(&desktop_info));
    println!("device_types([Mobile, Tablet]) 匹配 Mobile: true, Desktop: false");

    // browsers 条件
    let browser_cond =
        DeviceCondition::browsers(vec!["Safari".to_string(), "Chrome".to_string()]);
    let safari_info = DeviceInfo {
        device_type: DeviceType::Mobile,
        browser: Some("Safari".to_string()),
        browser_version: None,
        os: None,
        os_version: None,
        user_agent: None,
    };
    let firefox_info = DeviceInfo {
        device_type: DeviceType::Desktop,
        browser: Some("Firefox".to_string()),
        browser_version: None,
        os: None,
        os_version: None,
        user_agent: None,
    };
    assert!(browser_cond.matches(&safari_info));
    assert!(!browser_cond.matches(&firefox_info));
    println!("browsers([Safari, Chrome]) 匹配 Safari: true, Firefox: false");

    // os 条件
    let os_cond = DeviceCondition::os(vec!["iOS".to_string(), "Android".to_string()]);
    let ios_info = DeviceInfo {
        device_type: DeviceType::Mobile,
        browser: None,
        browser_version: None,
        os: Some("iOS".to_string()),
        os_version: None,
        user_agent: None,
    };
    let windows_info = DeviceInfo {
        device_type: DeviceType::Desktop,
        browser: None,
        browser_version: None,
        os: Some("Windows".to_string()),
        os_version: None,
        user_agent: None,
    };
    assert!(os_cond.matches(&ios_info));
    assert!(!os_cond.matches(&windows_info));
    println!("os([iOS, Android]) 匹配 iOS: true, Windows: false");

    // Default trait
    let default_cond = DeviceCondition::default();
    assert!(default_cond.is_empty());
    println!("DeviceCondition::default() is_empty={}", default_cond.is_empty());

    println!();
}

/// 演示 DeviceMatcher 基本使用
async fn demo_device_matcher() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- DeviceMatcher ---");

    // new 创建匹配器
    let matcher = DeviceMatcher::new().await?;
    println!("DeviceMatcher::new() 创建成功");

    // parse 解析 User-Agent
    let user_agents = vec![
        "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X) AppleWebKit/605.1.15",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        "Mozilla/5.0 (Linux; Android 10; SM-G973F) AppleWebKit/537.36",
        "curl/7.68.0",
        "GoogleBot/2.1",
    ];

    println!("\n单条解析:");
    for ua in &user_agents {
        let info = matcher.parse(ua).await?;
        println!(
            "  UA: {}",
            if ua.len() > 60 { &ua[..60] } else { ua }
        );
        println!(
            "    -> type={:?}, browser={:?}, os={:?}, desc='{}'",
            info.device_type,
            info.browser,
            info.os,
            info.description()
        );
    }

    // batch_parse 批量解析
    println!("\n批量解析:");
    let ua_strings: Vec<String> = user_agents.iter().map(|s| s.to_string()).collect();
    let results = matcher.batch_parse(&ua_strings).await;
    println!("批量解析 {} 条 User-Agent", results.len());
    for (i, result) in results.iter().enumerate() {
        match result {
            Ok(info) => println!(
                "  [{}] type={:?}, browser={:?}",
                i, info.device_type, info.browser
            ),
            Err(e) => println!("  [{}] 解析失败: {}", i, e),
        }
    }

    println!();
    Ok(())
}

/// 演示 DeviceMatcherBuilder 构建器模式
async fn demo_device_matcher_builder() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- DeviceMatcherBuilder ---");

    // 基本构建器
    let matcher = DeviceMatcher::builder()
        .cache_size_limit(5000)
        .build()
        .await?;
    println!("DeviceMatcherBuilder 构建成功, cache_size_limit=5000");

    // 解析验证
    let info = matcher
        .parse("Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)")
        .await?;
    println!("iPhone UA 解析: type={:?}", info.device_type);

    // Default builder
    let default_builder = DeviceMatcherBuilder::default();
    let _matcher2 = default_builder.build().await?;
    println!("DeviceMatcherBuilder::default() 构建成功");

    // new() 等同于 default()
    let _builder = DeviceMatcherBuilder::new();
    println!("DeviceMatcherBuilder::new() 创建成功");

    println!();
    Ok(())
}

/// 演示自定义规则和条件匹配
async fn demo_device_custom_rules() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Device Custom Rules & Matching ---");

    let mut matcher = DeviceMatcher::new().await?;

    // add_custom_rule 添加自定义规则
    matcher.add_custom_rule(
        "MyCustomApp",
        r"MyCustomApp/\d+\.\d+",
        DeviceType::Mobile,
        Some("MyCustomApp".to_string()),
        Some("Android".to_string()),
    );
    println!("添加自定义规则: MyCustomApp");

    // 解析自定义 App UA
    let custom_ua = "MyCustomApp/1.0 (Android 10)";
    let info = matcher.parse(custom_ua).await?;
    println!(
        "自定义 UA 解析: type={:?}, browser={:?}, os={:?}",
        info.device_type, info.browser, info.os
    );

    // remove_custom_rule 移除规则
    let removed = matcher.remove_custom_rule("MyCustomApp");
    assert!(removed, "移除规则应成功");
    println!("移除自定义规则: MyCustomApp (成功)");

    let not_removed = matcher.remove_custom_rule("NonExistent");
    assert!(!not_removed, "移除不存在的规则应失败");
    println!("移除不存在的规则: NonExistent (失败，符合预期)");

    // matches_user_agent 检查 UA 是否匹配条件
    let condition = DeviceCondition::device_types(vec![DeviceType::Mobile]);
    let iphone_ua = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)";
    let matched = matcher.matches_user_agent(iphone_ua, &condition).await?;
    println!("iPhone UA 匹配 Mobile 条件: {}", matched);

    let windows_ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)";
    let matched2 = matcher.matches_user_agent(windows_ua, &condition).await?;
    println!("Windows UA 匹配 Mobile 条件: {}", matched2);

    // matches 检查 DeviceInfo 是否匹配条件
    let mobile_info = DeviceInfo {
        device_type: DeviceType::Mobile,
        browser: Some("Safari".to_string()),
        browser_version: None,
        os: Some("iOS".to_string()),
        os_version: None,
        user_agent: None,
    };
    let browser_cond = DeviceCondition::browsers(vec!["Safari".to_string()]);
    let matched3 = matcher.matches(&mobile_info, &browser_cond);
    println!("Mobile Safari 匹配 Safari 浏览器条件: {}", matched3);

    println!();
    Ok(())
}

/// 演示设备缓存统计
async fn demo_device_cache() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Device Cache ---");

    let matcher = DeviceMatcher::new().await?;

    // 解析多个 UA 触发缓存
    let ua = "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)";
    matcher.parse(ua).await?;
    matcher.parse(ua).await?; // 第二次应命中缓存
    matcher.parse(ua).await?; // 第三次应命中缓存

    // cache_stats 获取缓存统计
    let stats: DeviceCacheStats = matcher.cache_stats().await;
    println!(
        "缓存统计: size={}, limit={}, hits={}, misses={}, hit_rate={:.2}%",
        stats.size, stats.limit, stats.hits, stats.misses, stats.hit_rate
    );

    // clear_cache 清空缓存
    matcher.clear_cache().await;
    let stats_after: DeviceCacheStats = matcher.cache_stats().await;
    println!(
        "清空后: size={}, hits={}, misses={}",
        stats_after.size, stats_after.hits, stats_after.misses
    );

    println!();
    Ok(())
}

/// 演示 GeoInfo 结构
fn demo_geo_info() {
    println!("--- GeoInfo ---");

    // empty 创建空地理信息
    let empty_info = GeoInfo::empty();
    assert!(empty_info.is_empty());
    println!(
        "empty: is_empty={}, description='{}'",
        empty_info.is_empty(),
        empty_info.description()
    );

    // 完整地理信息
    let beijing_info = GeoInfo {
        country_code: Some("CN".to_string()),
        country_name: Some("China".to_string()),
        city: Some("Beijing".to_string()),
        continent: Some("Asia".to_string()),
        longitude: Some(116.4),
        latitude: Some(39.9),
        timezone: Some("Asia/Shanghai".to_string()),
    };
    println!("beijing_info: description='{}'", beijing_info.description());
    assert!(!beijing_info.is_empty());

    let tokyo_info = GeoInfo {
        country_code: Some("JP".to_string()),
        country_name: Some("Japan".to_string()),
        city: Some("Tokyo".to_string()),
        continent: Some("Asia".to_string()),
        longitude: Some(139.69),
        latitude: Some(35.69),
        timezone: Some("Asia/Tokyo".to_string()),
    };
    println!("tokyo_info: description='{}'", tokyo_info.description());

    // 仅有国家信息
    let country_only = GeoInfo {
        country_code: None,
        country_name: Some("United States".to_string()),
        city: None,
        continent: None,
        longitude: None,
        latitude: None,
        timezone: None,
    };
    println!("country_only: description='{}'", country_only.description());

    // 空信息描述
    println!("empty description='{}'", GeoInfo::empty().description());

    // Default trait
    let default_info = GeoInfo::default();
    assert!(default_info.is_empty());
    println!("GeoInfo::default() is_empty={}", default_info.is_empty());

    // PartialEq
    assert_eq!(empty_info, GeoInfo::empty());
    println!("GeoInfo PartialEq 验证通过");

    println!();
}

/// 演示 GeoCondition 匹配条件
fn demo_geo_condition() {
    println!("--- GeoCondition ---");

    // empty 创建空条件
    let empty_cond = GeoCondition::empty();
    assert!(empty_cond.is_empty());
    println!("empty condition: is_empty={}", empty_cond.is_empty());

    // 空条件匹配任何地理信息
    let info = GeoInfo::empty();
    assert!(empty_cond.matches(&info));
    println!("空条件匹配空地理信息: true");

    // countries 条件
    let asia_cond = GeoCondition::countries(vec!["CN".to_string(), "JP".to_string()]);
    let beijing_info = GeoInfo {
        country_code: Some("CN".to_string()),
        country_name: Some("China".to_string()),
        city: Some("Beijing".to_string()),
        continent: Some("Asia".to_string()),
        longitude: Some(116.4),
        latitude: Some(39.9),
        timezone: Some("Asia/Shanghai".to_string()),
    };
    let us_info = GeoInfo {
        country_code: Some("US".to_string()),
        country_name: Some("United States".to_string()),
        city: None,
        continent: None,
        longitude: None,
        latitude: None,
        timezone: None,
    };
    assert!(asia_cond.matches(&beijing_info));
    assert!(!asia_cond.matches(&us_info));
    println!("countries([CN, JP]) 匹配 CN: true, US: false");

    // cities 条件
    let city_cond = GeoCondition::cities(vec!["Beijing".to_string(), "Shanghai".to_string()]);
    assert!(city_cond.matches(&beijing_info));
    let empty_city_info = GeoInfo::empty();
    assert!(!city_cond.matches(&empty_city_info));
    println!("cities([Beijing, Shanghai]) 匹配 Beijing: true, empty: false");

    // continents 条件
    let continent_cond = GeoCondition::continents(vec!["Asia".to_string()]);
    assert!(continent_cond.matches(&beijing_info));
    assert!(!continent_cond.matches(&us_info));
    println!("continents([Asia]) 匹配 Asia: true, non-Asia: false");

    // Default trait
    let default_cond = GeoCondition::default();
    assert!(default_cond.is_empty());
    println!(
        "GeoCondition::default() is_empty={}",
        default_cond.is_empty()
    );

    println!();
}

/// 演示 GeoMatcher 地理匹配器
async fn demo_geo_matcher() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- GeoMatcher ---");

    // GeoMatcher 需要GeoLite2数据库文件
    // 尝试常见路径，若不存在则跳过实际查询演示
    let possible_paths = [
        "GeoLite2-City.mmdb",
        "./data/GeoLite2-City.mmdb",
        "/usr/share/GeoIP/GeoLite2-City.mmdb",
        "/var/lib/GeoIP/GeoLite2-City.mmdb",
    ];

    let db_path = possible_paths
        .iter()
        .find(|p| Path::new(p).exists())
        .copied();

    if db_path.is_none() {
        println!("未找到 GeoLite2 数据库文件，跳过实际 IP 查询演示");
        println!("提示: 从 https://dev.maxmind.com/geoip/geolite2-free-geolocation-data");
        println!("      下载 GeoLite2-City.mmdb 并放置在当前目录");

        // 演示 GeoMatcher::new 的错误处理
        let result = GeoMatcher::new("nonexistent.mmdb").await;
        match result {
            Ok(_) => println!("意外: 不存在的文件路径创建成功"),
            Err(e) => println!("GeoMatcher::new('nonexistent.mmdb') 返回错误（符合预期）: {}", e),
        }

        // 仍然演示静态方法 matches
        let info = GeoInfo {
            country_code: Some("CN".to_string()),
            country_name: Some("China".to_string()),
            city: Some("Beijing".to_string()),
            continent: Some("Asia".to_string()),
            longitude: Some(116.4),
            latitude: Some(39.9),
            timezone: Some("Asia/Shanghai".to_string()),
        };
        let condition = GeoCondition::countries(vec!["CN".to_string()]);

        // GeoMatcher::matches 是同步方法，需要 matcher 实例
        // 由于无法创建 matcher，使用 GeoCondition::matches 代替演示
        let matched = condition.matches(&info);
        println!(
            "GeoCondition::matches (静态): CN info 匹配 CN 条件 = {}",
            matched
        );

        println!();
        return Ok(());
    }

    let path = db_path.unwrap();
    println!("找到 GeoLite2 数据库: {}", path);

    // new 创建匹配器
    let matcher = GeoMatcher::new(path).await?;
    println!("GeoMatcher 创建成功");

    // lookup 查询单个 IP
    let ip: IpAddr = "114.114.114.114".parse()?;
    let info = matcher.lookup(ip).await?;
    println!(
        "lookup(114.114.114.114): country={:?}, city={:?}, desc='{}'",
        info.country_code, info.city, info.description()
    );

    // batch_lookup 批量查询
    let ips: Vec<IpAddr> = vec![
        "114.114.114.114".parse()?,
        "8.8.8.8".parse()?,
        "1.1.1.1".parse()?,
    ];
    let results = matcher.batch_lookup(&ips).await;
    println!("批量查询 {} 个 IP:", results.len());
    for (i, result) in results.iter().enumerate() {
        match result {
            Ok(info) => println!(
                "  [{}] {} -> {}",
                i, ips[i], info.description()
            ),
            Err(e) => println!("  [{}] {} -> 错误: {}", i, ips[i], e),
        }
    }

    // matches_ip 检查 IP 是否匹配条件
    let cn_cond = GeoCondition::countries(vec!["CN".to_string()]);
    let matched = matcher.matches_ip(ip, &cn_cond).await?;
    println!("114.114.114.114 匹配 CN 条件: {}", matched);

    // matches 检查 GeoInfo 是否匹配条件
    let test_info = GeoInfo {
        country_code: Some("US".to_string()),
        country_name: Some("United States".to_string()),
        city: None,
        continent: None,
        longitude: None,
        latitude: None,
        timezone: None,
    };
    let us_cond = GeoCondition::countries(vec!["US".to_string()]);
    let matched2 = matcher.matches(&test_info, &us_cond);
    println!("US info 匹配 US 条件: {}", matched2);

    // cache_stats 缓存统计
    let stats: GeoCacheStats = matcher.cache_stats().await;
    println!(
        "缓存统计: size={}, limit={}, hits={}, misses={}, hit_rate={:.2}%",
        stats.size, stats.limit, stats.hits, stats.misses, stats.hit_rate
    );

    // clear_cache 清空缓存
    matcher.clear_cache().await;
    let stats_after: GeoCacheStats = matcher.cache_stats().await;
    println!(
        "清空后: size={}, hits={}, misses={}",
        stats_after.size, stats_after.hits, stats_after.misses
    );

    println!();
    Ok(())
}
