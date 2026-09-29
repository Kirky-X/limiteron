// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Custom Matchers 示例
//!
//! 演示自定义匹配器 trait 的实现、注册表使用、以及内置匹配器
//! （HeaderMatcher 前缀/相等模式、MethodMatcher、TimeWindowMatcher、
//! RegexPathMatcher——后者需 `regex-matching` 特性）。
//!
//! # 涵盖 API
//!
//! - `CustomMatcher` trait（`name`、`matches`、`load_config`）
//! - `CustomMatcherRegistry`（`new`、`register`、`unregister`、`match_with`、`contains`、`list`、`count`）
//! - `HeaderMatcher`（`new`、`with_match_mode` 前缀/相等、`with_case_sensitive`、`builder`）
//! - `MethodMatcher`（`new`、`builder`，方法名大小写规范化）
//! - `TimeWindowMatcher`（`new`、`builder`）
//! - `RegexPathMatcher`（feature `regex-matching`）
//!
//! # 运行方式
//!
//! ```bash
//! cargo run --bin custom_matchers
//! ```

use limiteron::async_trait::async_trait;
use limiteron::error::LimiteronError;
use limiteron::matchers::RequestContext;
#[cfg(feature = "regex-matching")]
use limiteron::matchers::custom::RegexPathMatcher;
use limiteron::matchers::custom::{
    CustomMatcher, CustomMatcherRegistry, HeaderMatchMode, HeaderMatcher, MethodMatcher,
    TimeWindowMatcher,
};
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Custom Matchers Demo ===\n");

    demo_builtin_matchers().await?;
    demo_method_matcher().await?;
    #[cfg(feature = "regex-matching")]
    demo_regex_path_matcher().await?;
    demo_custom_matcher_trait().await?;
    demo_registry_management().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

/// 演示内置的 HeaderMatcher 与 TimeWindowMatcher
async fn demo_builtin_matchers() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 1. Built-in HeaderMatcher & TimeWindowMatcher ---\n");

    // HeaderMatcher：匹配 X-Api-Version 头的值
    let header_matcher =
        HeaderMatcher::new("X-Api-Version", vec!["v1".to_string(), "v2".to_string()])?
            .with_case_sensitive(false);

    let ctx_v1 = RequestContext::new().with_header("X-Api-Version", "v1");
    let ctx_v3 = RequestContext::new().with_header("X-Api-Version", "v3");

    let m1 = header_matcher.matches(&ctx_v1).await?;
    let m2 = header_matcher.matches(&ctx_v3).await?;
    println!("  HeaderMatcher: 'v1' matches={}, 'v3' matches={}", m1, m2);
    println!(
        "    header_name={}, allowed={:?}",
        header_matcher.header_name(),
        header_matcher.allowed_values()
    );

    // HeaderMatcher Builder 模式
    let built = HeaderMatcher::builder()
        .header_name("X-Region")
        .allowed_values(vec!["us".to_string(), "eu".to_string()])
        .case_sensitive(true)
        .build()?;
    println!(
        "\n  Builder pattern: header={:?}, case_sensitive via new()={}",
        built.header_name(),
        built.header_name() == "x-region"
    );

    // HeaderMatcher 前缀模式：租户命名空间匹配（team-a 及其子命名空间）
    let prefix_matcher = HeaderMatcher::new("X-Tenant", vec!["team-a".to_string()])?
        .with_match_mode(HeaderMatchMode::Prefix);
    let m_prefix = prefix_matcher
        .matches(&RequestContext::new().with_header("X-Tenant", "team-a-prod"))
        .await?;
    println!(
        "\n  HeaderMatcher prefix mode: 'team-a-prod' starts-with 'team-a' matches={m_prefix}"
    );

    // TimeWindowMatcher：匹配工作时间（9-18 点）
    let time_matcher = TimeWindowMatcher::new(9, 18);
    let ctx = RequestContext::new();
    let matched = time_matcher.matches(&ctx).await?;
    println!(
        "\n  TimeWindowMatcher (9-18): current hour matches={}",
        matched
    );

    // TimeWindowMatcher Builder 模式
    let night = TimeWindowMatcher::builder()
        .start_hour(22)
        .end_hour(23)
        .build();
    let night_matched = night.matches(&ctx).await?;
    println!("  TimeWindowMatcher (22-23): matches={}", night_matched);
    println!();
    Ok(())
}

/// 演示 MethodMatcher（方法名大小写规范化）
async fn demo_method_matcher() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 1b. MethodMatcher ---\n");

    let matcher = MethodMatcher::new(vec!["GET".to_string(), "Post".to_string()])?;
    println!("  methods (normalized): {:?}", matcher.methods());

    let m1 = matcher
        .matches(&RequestContext::new().with_method("GET"))
        .await?;
    let m2 = matcher
        .matches(&RequestContext::new().with_method("POST"))
        .await?;
    let m3 = matcher
        .matches(&RequestContext::new().with_method("DELETE"))
        .await?;
    println!("  GET matches={m1}, post(→POST) matches={m2}, DELETE matches={m3}");
    println!();
    Ok(())
}

/// 演示 RegexPathMatcher（feature `regex-matching`）
#[cfg(feature = "regex-matching")]
async fn demo_regex_path_matcher() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 1c. RegexPathMatcher (feature: regex-matching) ---\n");

    let matcher = RegexPathMatcher::new("^/api/v[0-9]+/users", false)?;
    let m1 = matcher
        .matches(&RequestContext::new().with_path("/api/v2/users/42"))
        .await?;
    let m2 = matcher
        .matches(&RequestContext::new().with_path("/web/users"))
        .await?;
    println!("  pattern: {}", matcher.pattern());
    println!("  '/api/v2/users/42' matches={m1}, '/web/users' matches={m2}");

    // 大小写不敏感模式
    let insensitive = RegexPathMatcher::new("^/Admin", true)?;
    let m3 = insensitive
        .matches(&RequestContext::new().with_path("/admin/dashboard"))
        .await?;
    println!("  '(?i)^/Admin': '/admin/dashboard' matches={m3}");
    println!();
    Ok(())
}

/// 演示自定义 CustomMatcher trait 实现
async fn demo_custom_matcher_trait() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 2. Custom CustomMatcher Implementation ---\n");

    // 自定义匹配器：根据请求路径前缀匹配
    struct PathPrefixMatcher {
        prefix: String,
    }

    #[async_trait]
    impl CustomMatcher for PathPrefixMatcher {
        fn name(&self) -> &str {
            "path-prefix"
        }

        async fn matches(&self, context: &RequestContext) -> Result<bool, LimiteronError> {
            Ok(context.path.starts_with(&self.prefix))
        }

        fn load_config(&mut self, config: Value) -> Result<(), LimiteronError> {
            if let Some(prefix) = config.get("prefix").and_then(Value::as_str) {
                self.prefix = prefix.to_string();
                Ok(())
            } else {
                Err(LimiteronError::ConfigError(
                    "missing 'prefix' field".to_string(),
                ))
            }
        }
    }

    let matcher = PathPrefixMatcher {
        prefix: "/api/v1".to_string(),
    };

    let ctx_match = RequestContext::new().with_path("/api/v1/users");
    let ctx_no_match = RequestContext::new().with_path("/admin/settings");

    let m1 = matcher.matches(&ctx_match).await?;
    let m2 = matcher.matches(&ctx_no_match).await?;
    println!("  PathPrefixMatcher('/api/v1'):");
    println!("    '/api/v1/users' matches={}", m1);
    println!("    '/admin/settings' matches={}", m2);

    // 测试 load_config
    let mut configurable = PathPrefixMatcher {
        prefix: String::new(),
    };
    let config = serde_json::json!({"prefix": "/admin"});
    configurable.load_config(config)?;
    let ctx_admin = RequestContext::new().with_path("/admin/dashboard");
    let m3 = configurable.matches(&ctx_admin).await?;
    println!("\n  After load_config({{prefix:'/admin'}}):");
    println!("    '/admin/dashboard' matches={}", m3);
    println!();
    Ok(())
}

/// 演示 CustomMatcherRegistry 注册表管理
async fn demo_registry_management() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 3. CustomMatcherRegistry ---\n");

    let registry = CustomMatcherRegistry::new();

    // 注册 HeaderMatcher
    let header_matcher =
        HeaderMatcher::new("X-Role", vec!["admin".to_string(), "superuser".to_string()])?;
    registry
        .register("role-check".to_string(), Box::new(header_matcher))
        .await?;

    // 注册 TimeWindowMatcher
    let time_matcher = TimeWindowMatcher::new(9, 18);
    registry
        .register("business-hours".to_string(), Box::new(time_matcher))
        .await?;

    // 注册 MethodMatcher
    let method_matcher = MethodMatcher::new(vec!["GET".to_string(), "POST".to_string()])?;
    registry
        .register("http-methods".to_string(), Box::new(method_matcher))
        .await?;

    println!("  Registered matchers: {:?}", registry.list().await);
    println!("  Registry count: {}", registry.count().await);
    println!(
        "  Contains 'role-check': {}",
        registry.contains("role-check").await
    );
    println!(
        "  Contains 'unknown': {}",
        registry.contains("unknown").await
    );

    // 使用 match_with 执行匹配
    let ctx = RequestContext::new()
        .with_path("/api/v1/data")
        .with_header("X-Role", "admin");

    let role_match = registry.match_with("role-check", &ctx).await?;
    let hours_match = registry.match_with("business-hours", &ctx).await?;
    println!("\n  Request with X-Role=admin:");
    println!("    role-check matches={}", role_match);
    println!("    business-hours matches={}", hours_match);

    // 注销匹配器
    registry.unregister("role-check").await?;
    println!("\n  After unregister 'role-check':");
    println!("    count={}", registry.count().await);
    println!("    contains={}", registry.contains("role-check").await);
    let method_match = registry
        .match_with("http-methods", &RequestContext::new().with_method("GET"))
        .await?;
    println!("    http-methods (GET) matches={method_match}");

    // 清空注册表
    registry.clear().await;
    println!("\n  After clear: count={}", registry.count().await);
    println!();
    Ok(())
}
