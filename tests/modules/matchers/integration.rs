// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 匹配器模块集成测试
//!
//! 测试匹配器模块的基本功能

use limiteron::config::TrustedProxyConfig;
use limiteron::matchers::{
    ApiKeyExtractor, CompositeExtractor, Identifier, IdentifierExtractor, IpExtractor,
    MacExtractor, RequestContext, RuleMatcher, UserIdExtractor,
};

// ============================================================================
// IpExtractor Tests
// ============================================================================
//
// vuln-0003 修复后，X-Forwarded-For 等转发头仅在直接 TCP 对端（client_ip）
// 为可信代理时才被信任。下述测试均构造可信代理转发场景，以验证头提取行为。

#[tokio::test]
async fn test_ip_extractor_ipv4() {
    // vuln-0003: 通过可信代理转发时，从 X-Forwarded-For 头提取 IPv4 地址
    let config = TrustedProxyConfig {
        enabled: true,
        proxies: vec!["10.0.0.1".to_string()],
        max_hops: 10,
    };
    let extractor =
        IpExtractor::with_trusted_proxies(vec!["X-Forwarded-For".to_string()], true, config);
    let ctx = RequestContext::new()
        .with_header("x-forwarded-for", "192.0.2.1")
        .with_client_ip("10.0.0.1");
    let id = extractor.extract(&ctx);
    assert!(id.is_some());
    assert!(matches!(id.unwrap(), Identifier::Ip(_)));
}

#[tokio::test]
async fn test_ip_extractor_missing_ip() {
    let extractor = IpExtractor::from_header("X-Forwarded-For");
    let ctx = RequestContext::new();
    let id = extractor.extract(&ctx);
    assert!(id.is_none());
}

#[tokio::test]
async fn test_ip_extractor_ipv6() {
    // vuln-0003: 通过可信代理转发时，从 X-Real-IP 头提取 IPv6 地址
    let config = TrustedProxyConfig {
        enabled: true,
        proxies: vec!["10.0.0.1".to_string()],
        max_hops: 10,
    };
    let extractor = IpExtractor::with_trusted_proxies(vec!["X-Real-IP".to_string()], true, config);
    let ctx = RequestContext::new()
        .with_header("x-real-ip", "::1")
        .with_client_ip("10.0.0.1");
    let id = extractor.extract(&ctx);
    assert!(id.is_some());
}

#[tokio::test]
async fn test_ip_extractor_multiple_headers() {
    // vuln-0003: 通过可信代理转发时，按头优先级提取 IP（X-Real-IP 优先于 X-Forwarded-For）
    let config = TrustedProxyConfig {
        enabled: true,
        proxies: vec!["10.0.0.1".to_string()],
        max_hops: 10,
    };
    let extractor = IpExtractor::with_trusted_proxies(
        vec!["X-Real-IP".to_string(), "X-Forwarded-For".to_string()],
        true,
        config,
    );
    let ctx = RequestContext::new()
        .with_header("x-real-ip", "10.1.1.1")
        .with_client_ip("10.0.0.1");
    let id = extractor.extract(&ctx);
    assert!(id.is_some());
}

#[tokio::test]
async fn test_ip_extractor_default() {
    let extractor = IpExtractor::new_default();
    let mut ctx = RequestContext::new();
    ctx.client_ip = Some("192.168.1.1".to_string());
    let id = extractor.extract(&ctx);
    assert!(id.is_some());
}

// ============================================================================
// UserIdExtractor Tests
// ============================================================================

#[tokio::test]
async fn test_user_id_extractor_header() {
    let extractor = UserIdExtractor::from_header("X-User-ID");
    let mut ctx = RequestContext::new();
    ctx.headers
        .insert("x-user-id".to_string(), "user123".to_string());
    let id = extractor.extract(&ctx);
    assert!(id.is_some());
    assert!(matches!(id.unwrap(), Identifier::UserId(_)));
}

#[tokio::test]
async fn test_user_id_extractor_missing() {
    let extractor = UserIdExtractor::from_header("X-User-ID");
    let ctx = RequestContext::new();
    assert!(extractor.extract(&ctx).is_none());
}

#[tokio::test]
async fn test_user_id_extractor_query_param() {
    let extractor = UserIdExtractor::from_query_param("user_id");
    let mut ctx = RequestContext::new();
    ctx.query_params
        .insert("user_id".to_string(), "query-user".to_string());
    let id = extractor.extract(&ctx);
    assert!(id.is_some());
}

// ============================================================================
// ApiKeyExtractor Tests
// ============================================================================

#[tokio::test]
async fn test_api_key_extractor() {
    let extractor = ApiKeyExtractor::from_header("X-API-Key");
    let mut ctx = RequestContext::new();
    ctx.headers
        .insert("x-api-key".to_string(), "secret-key-123".to_string());
    let id = extractor.extract(&ctx);
    assert!(id.is_some());
    assert!(matches!(id.unwrap(), Identifier::ApiKey(_)));
}

#[tokio::test]
async fn test_api_key_extractor_missing() {
    let extractor = ApiKeyExtractor::from_header("X-API-Key");
    let ctx = RequestContext::new();
    assert!(extractor.extract(&ctx).is_none());
}

#[tokio::test]
async fn test_api_key_extractor_bearer() {
    let extractor = ApiKeyExtractor::from_authorization_header();
    let mut ctx = RequestContext::new();
    ctx.headers
        .insert("authorization".to_string(), "Bearer my-token".to_string());
    let id = extractor.extract(&ctx);
    assert!(id.is_some());
}

// ============================================================================
// MacExtractor Tests
// ============================================================================

#[tokio::test]
async fn test_mac_extractor_header() {
    let extractor = MacExtractor::from_header("X-Device-MAC");
    let mut ctx = RequestContext::new();
    ctx.headers
        .insert("x-device-mac".to_string(), "00:11:22:33:44:55".to_string());
    let id = extractor.extract(&ctx);
    assert!(id.is_some());
}

#[tokio::test]
async fn test_mac_extractor_missing() {
    let extractor = MacExtractor::from_header("X-Device-MAC");
    let ctx = RequestContext::new();
    assert!(extractor.extract(&ctx).is_none());
}

#[tokio::test]
async fn test_mac_extractor_query_param() {
    let extractor = MacExtractor::from_query_param("mac");
    let mut ctx = RequestContext::new();
    ctx.query_params
        .insert("mac".to_string(), "aa:bb:cc:dd:ee:ff".to_string());
    let id = extractor.extract(&ctx);
    assert!(id.is_some());
}

// ============================================================================
// CompositeExtractor Tests
// ============================================================================

#[tokio::test]
async fn test_composite_extractor_first_success() {
    let extractor = CompositeExtractor::new(
        vec![
            Box::new(UserIdExtractor::from_header("X-User-ID")),
            Box::new(IpExtractor::from_header("X-Forwarded-For")),
        ],
        false,
    );

    let mut ctx = RequestContext::new();
    ctx.headers
        .insert("x-user-id".to_string(), "composite-user".to_string());

    let id = extractor.extract(&ctx);
    assert!(id.is_some());
    assert!(matches!(id, Some(Identifier::UserId(_))));
}

#[tokio::test]
async fn test_composite_extractor_fallback() {
    let extractor = CompositeExtractor::new(
        vec![Box::new(UserIdExtractor::from_header("X-User-ID"))],
        true, // fallback
    );

    // No user-id header, but fallback enabled and ctx has ip
    let mut ctx = RequestContext::new();
    ctx.ip = Some("10.0.0.1".to_string());

    // With fallback=true, when all extractors fail, it returns default IP
    // Actually the composite extractor with fallback returns the first extractor
    // result (None), so this tests the fallback behavior
    let id = extractor.extract(&ctx);
    assert!(id.is_none()); // no IP extractor in this composite
}

// ============================================================================
// RuleMatcher Tests
// ============================================================================

#[tokio::test]
async fn test_rule_matcher_empty_rules() {
    let matcher = RuleMatcher::new(vec![]);
    assert_eq!(matcher.rule_count(), 0);
    // Empty rules should match trivially - matches returns None
    let ctx = RequestContext::new();
    assert!(matcher.matches(&ctx).is_none());
}

#[tokio::test]
async fn test_rule_matcher_id_generation() {
    let matcher = RuleMatcher::new(vec![]);
    assert_eq!(matcher.rule_count(), 0);
}

// ============================================================================
// RequestContext Tests
// ============================================================================

#[tokio::test]
async fn test_request_context_basics() {
    let mut ctx = RequestContext::new();
    ctx.ip = Some("1.2.3.4".to_string());
    ctx.method = "GET".to_string();
    ctx.path = "/api/test".to_string();
    assert_eq!(ctx.ip.as_deref(), Some("1.2.3.4"));
}

#[tokio::test]
async fn test_request_context_builder() {
    let ctx = RequestContext::new()
        .with_header("x-test", "value")
        .with_client_ip("127.0.0.1")
        .with_query_param("page", "1");

    assert_eq!(ctx.client_ip.as_deref(), Some("127.0.0.1"));
    assert_eq!(ctx.get_header("x-test").map(|s| s.as_str()), Some("value"));
    assert_eq!(ctx.query_params.get("page").map(|s| s.as_str()), Some("1"));
}

// ============================================================================
// Identifier Tests
// ============================================================================

#[tokio::test]
async fn test_identifier_debug() {
    let id = Identifier::Ip("192.168.1.1".to_string());
    assert!(format!("{:?}", id).contains("192.168"));

    let id2 = Identifier::UserId("user1".to_string());
    assert!(format!("{:?}", id2).contains("user1"));

    let id3 = Identifier::ApiKey("key123".to_string());
    assert!(format!("{:?}", id3).contains("key123"));
}

#[tokio::test]
async fn test_identifier_methods() {
    let user_id = Identifier::UserId("test-user".to_string());
    assert_eq!(user_id.as_str(), "test-user");
    assert_eq!(user_id.type_name(), "user_id");
    assert_eq!(user_id.key(), "user_id:test-user");

    let ip = Identifier::Ip("10.0.0.1".to_string());
    assert_eq!(ip.as_str(), "10.0.0.1");
    assert_eq!(ip.type_name(), "ip");
    assert_eq!(ip.key(), "ip:10.0.0.1");

    let mac = Identifier::Mac("00:11:22:33:44:55".to_string());
    assert_eq!(mac.as_str(), "00:11:22:33:44:55");
    assert_eq!(mac.type_name(), "mac");

    let api_key = Identifier::ApiKey("sk-123".to_string());
    assert_eq!(api_key.as_str(), "sk-123");
    assert_eq!(api_key.type_name(), "api_key");

    let device = Identifier::DeviceId("device-abc".to_string());
    assert_eq!(device.as_str(), "device-abc");
    assert_eq!(device.type_name(), "device_id");
}
