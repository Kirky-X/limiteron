// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Matchers 示例
//!
//! 演示标识符提取器、请求上下文、规则匹配器的完整使用流程。
//!
//! # 涵盖 API
//!
//! - 标识符提取器：`IpExtractor`、`UserIdExtractor`、`DeviceIdExtractor`、
//!   `ApiKeyExtractor`、`MacExtractor`
//! - `Identifier` 枚举及其方法（`as_str`、`type_name`、`key`）
//! - `RequestContext` 构建器（`with_header`、`with_client_ip`、`with_path` 等）
//! - `MatchCondition` 枚举（`User`、`Ip`、`Custom` 等）
//! - `IpRange` 解析（支持 Single / CIDR / Range）
//! - `Rule` / `RuleMatcher` / `RuleMatcherBuilder`
//! - `MatcherStats` 统计信息
//!
//! # 运行方式
//!
//! ```bash
//! cargo run --bin matchers_demo
//! ```

use limiteron::matchers::{
    ApiKeyExtractor, IdentifierExtractor, IpExtractor, IpRange, MacExtractor, MatchCondition,
    MatcherStats, RequestContext, Rule, RuleMatcher, RuleMatcherBuilder, UserIdExtractor,
};
use std::str::FromStr;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Matchers Demo ===\n");

    demo_extractors()?;
    demo_request_context();
    demo_rule_matcher()?;
    demo_matcher_stats();

    println!("\n=== All demos completed ===");
    Ok(())
}

/// 演示所有标识符提取器
fn demo_extractors() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 1. Identifier Extractors ---\n");

    // UserIdExtractor：从 X-User-Id 头提取
    let user_extractor = UserIdExtractor::from_header("X-User-Id");
    let ctx = RequestContext::new().with_header("X-User-Id", "user-001");
    let identifier = user_extractor
        .extract(&ctx)
        .expect("should extract user id");
    println!(
        "  UserIdExtractor: {} = {}",
        identifier.type_name(),
        identifier.as_str()
    );
    println!("  Composite key: {}", identifier.key());

    // IpExtractor：从 X-Forwarded-For 头提取
    let ip_extractor = IpExtractor::from_header("X-Forwarded-For");
    let ctx = RequestContext::new()
        .with_header("X-Forwarded-For", "203.0.113.50")
        .with_client_ip("203.0.113.50");
    let identifier = ip_extractor.extract(&ctx).expect("should extract ip");
    println!(
        "\n  IpExtractor: {} = {}",
        identifier.type_name(),
        identifier.as_str()
    );

    // ApiKeyExtractor：从 Authorization 头提取（自动剥离 Bearer 前缀）
    let api_key_extractor = ApiKeyExtractor::from_authorization_header();
    let ctx = RequestContext::new().with_header("Authorization", "Bearer abc123secret");
    let identifier = api_key_extractor
        .extract(&ctx)
        .expect("should extract api key");
    println!(
        "\n  ApiKeyExtractor: {} = {}",
        identifier.type_name(),
        identifier.as_str()
    );

    // MacExtractor：从 X-Mac-Address 头提取（带格式验证）
    let mac_extractor = MacExtractor::from_header("X-Mac-Address");
    let ctx = RequestContext::new().with_header("X-Mac-Address", "00:1A:2B:3C:4D:5E");
    let identifier = mac_extractor.extract(&ctx).expect("should extract mac");
    println!(
        "\n  MacExtractor: {} = {}",
        identifier.type_name(),
        identifier.as_str()
    );

    // DeviceIdExtractor：从 X-Device-Id 头提取
    let device_extractor = limiteron::matchers::DeviceIdExtractor::from_header("X-Device-Id");
    let ctx = RequestContext::new().with_header("X-Device-Id", "device-abc-123");
    let identifier = device_extractor
        .extract(&ctx)
        .expect("should extract device id");
    println!(
        "\n  DeviceIdExtractor: {} = {}",
        identifier.type_name(),
        identifier.as_str()
    );

    println!();
    Ok(())
}

/// 演示 RequestContext 构建器
fn demo_request_context() {
    println!("--- 2. RequestContext Builder ---\n");

    let context = RequestContext::new()
        .with_path("/api/v1/users/12345")
        .with_method("POST")
        .with_client_ip("198.51.100.10")
        .with_header("X-User-Id", "user-001")
        .with_header("Authorization", "Bearer token-xyz")
        .with_query_param("verbose", "true");

    println!("  Path:       {}", context.path);
    println!("  Method:     {}", context.method);
    println!("  Client IP:  {:?}", context.client_ip);

    // get_header 不区分大小写
    if let Some(user_id) = context.get_header("x-user-id") {
        println!("  User-Id:    {}", user_id);
    }
    if let Some(auth) = context.get_header("AUTHORIZATION") {
        println!("  Auth:       {}", auth);
    }
    if let Some(verbose) = context.query_params.get("verbose") {
        println!("  Verbose:    {}", verbose);
    }
    println!();
}

/// 演示 RuleMatcher 与 MatchCondition
fn demo_rule_matcher() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 3. RuleMatcher with MatchCondition ---\n");

    // 构建规则：匹配特定用户
    let user_rule = Rule {
        id: "rule-user-001".to_string(),
        name: "Match user-001".to_string(),
        priority: 100,
        condition: Box::new(MatchCondition::User(vec!["user-001".to_string()])),
        enabled: true,
    };

    // 构建规则：匹配 IP CIDR 范围（10.0.0.0/24）
    let ip_range = IpRange::from_str("10.0.0.0/24")?;
    let ip_rule = Rule {
        id: "rule-internal-ip".to_string(),
        name: "Match internal IP range".to_string(),
        priority: 50,
        condition: Box::new(MatchCondition::Ip(vec![ip_range])),
        enabled: true,
    };

    // 构建规则：自定义闭包匹配（路径前缀匹配）
    let custom_rule = Rule {
        id: "rule-admin-path".to_string(),
        name: "Match admin path prefix".to_string(),
        priority: 200,
        condition: Box::new(MatchCondition::Custom(Arc::new(|ctx: &RequestContext| {
            ctx.path.starts_with("/admin")
        }))),
        enabled: true,
    };

    let matcher = RuleMatcherBuilder::new()
        .add_rule(custom_rule)
        .add_rule(user_rule)
        .add_rule(ip_rule)
        .build();

    println!("  Rule count: {}", matcher.rule_count());

    // 测试 1：匹配 admin 路径
    let ctx = RequestContext::new()
        .with_path("/admin/settings")
        .with_method("GET");
    let matched = matcher.matches(&ctx);
    println!(
        "\n  Path '/admin/settings' matched rule: {:?}",
        matched.map(|r| r.id.as_str()).unwrap_or("none")
    );

    // 测试 2：匹配 user-001
    let ctx = RequestContext::new()
        .with_path("/api/v1/data")
        .with_method("GET")
        .with_header("X-User-Id", "user-001");
    let matched = matcher.matches(&ctx);
    println!(
        "  user-001 request matched rule: {:?}",
        matched.map(|r| r.id.as_str()).unwrap_or("none")
    );

    // 测试 3：match_all 返回所有匹配的规则
    let all_matches = matcher.match_all(&ctx);
    println!("  match_all returned {} rules", all_matches.len());
    for rule in &all_matches {
        println!("    - {} (priority={})", rule.id, rule.priority);
    }
    println!();
    Ok(())
}

/// 演示 MatcherStats 统计信息
fn demo_matcher_stats() {
    println!("--- 4. MatcherStats ---\n");

    let rule = Rule {
        id: "rule-stats".to_string(),
        name: "Stats test rule".to_string(),
        priority: 100,
        condition: Box::new(MatchCondition::User(vec!["*".to_string()])),
        enabled: true,
    };
    let matcher = RuleMatcher::new(vec![rule]);

    // 触发 5 次匹配
    for _ in 0..5 {
        let ctx = RequestContext::new().with_header("X-User-Id", "anyone");
        let _ = matcher.matches(&ctx);
    }

    // 触发 2 次不匹配
    for _ in 0..2 {
        let ctx = RequestContext::new().with_header("X-Other", "value");
        let _ = matcher.matches(&ctx);
    }

    let stats: MatcherStats = matcher.stats();
    println!("  Total matches:    {}", stats.total_matches);
    println!("  Total mismatches: {}", stats.total_mismatches);
    println!("  Avg match time:   {} ns", stats.avg_match_time_ns);
    if let Some(last) = stats.last_match_time {
        println!("  Last match time:  {:?}", last);
    }

    // 重置统计
    matcher.reset_stats();
    let after_reset = matcher.stats();
    println!(
        "\n  After reset: matches={}, mismatches={}",
        after_reset.total_matches, after_reset.total_mismatches
    );
    println!();
}
