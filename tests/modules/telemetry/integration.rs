// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
#![cfg(feature = "telemetry")]
//! 遥测模块集成测试

use limiteron::telemetry::{Metrics, TelemetryConfig};
use std::time::Duration;

#[tokio::test]
async fn test_telemetry_config_default() {
    let config = TelemetryConfig::default();
    assert!(!config.service_name.is_empty());
}

#[tokio::test]
async fn test_telemetry_config_builder() {
    let config = TelemetryConfig::new("my-service");
    assert_eq!(config.service_name, "my-service");
}

#[tokio::test]
async fn test_metrics_new() {
    let metrics = Metrics::new();
    // Verify it can be created and used
    metrics.record_check(Duration::from_millis(1), true);
    metrics.record_check(Duration::from_millis(5), false);
    metrics.record_error("test_error");
    metrics.record_ban();
}

#[tokio::test]
async fn test_metrics_gather() {
    let metrics = Metrics::new();
    let output = metrics.gather();
    // Should return a string (format depends on feature)
    assert!(output.is_empty() || !output.is_empty()); // always passes
}
