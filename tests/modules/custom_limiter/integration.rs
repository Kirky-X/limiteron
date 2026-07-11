// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 自定义限流器模块集成测试

use limiteron::custom_limiter::{CustomLimiterRegistry, LimiterStats};

#[tokio::test]
async fn test_custom_limiter_registry() {
    let registry = CustomLimiterRegistry::new();
    assert_eq!(registry.len(), 0);
}

#[tokio::test]
async fn test_limiter_stats() {
    let stats = LimiterStats::default();
    assert_eq!(stats.total_requests, 0);
    assert_eq!(stats.allowed_requests, 0);
    assert_eq!(stats.rejected_requests, 0);
}

#[tokio::test]
async fn test_limiter_stats_new() {
    let stats = LimiterStats::new(100, 80, 20);
    assert_eq!(stats.total_requests, 100);
    assert_eq!(stats.allowed_requests, 80);
    assert_eq!(stats.rejected_requests, 20);
}

#[tokio::test]
async fn test_limiter_stats_rates() {
    let stats = LimiterStats::new(100, 80, 20);
    assert!((stats.allow_rate() - 0.8).abs() < f64::EPSILON);
    assert!((stats.rejection_rate() - 0.2).abs() < f64::EPSILON);
}

#[tokio::test]
async fn test_registry_contains() {
    let registry = CustomLimiterRegistry::new();
    assert!(!registry.contains("nonexistent").await);
}

#[tokio::test]
async fn test_registry_list() {
    let registry = CustomLimiterRegistry::new();
    let list = registry.list().await;
    assert!(list.is_empty());
}

#[tokio::test]
async fn test_registry_count() {
    let registry = CustomLimiterRegistry::new();
    assert_eq!(registry.count().await, 0);
}
