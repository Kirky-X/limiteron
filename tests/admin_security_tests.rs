// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
#![cfg(feature = "admin-api")]

use limiteron::admin::config::AdminApiConfig;

#[test]
fn test_config_validate_empty_api_key_when_enabled() {
    let config = AdminApiConfig {
        api_key: String::new(),
        enabled: true,
        ..Default::default()
    };
    let result = config.validate();
    assert!(
        result.is_err(),
        "Empty API key with enabled=true should error"
    );
}

#[test]
fn test_config_validate_short_api_key() {
    let config = AdminApiConfig {
        api_key: "shortkey12345".to_string(),
        enabled: true,
        ..Default::default()
    };
    let result = config.validate();
    assert!(
        result.is_err(),
        "API key shorter than 16 chars should error"
    );
}

#[test]
fn test_config_validate_disabled_no_api_key_required() {
    let config = AdminApiConfig {
        api_key: String::new(),
        enabled: false,
        ..Default::default()
    };
    let result = config.validate();
    assert!(
        result.is_ok(),
        "Disabled config with empty key should be valid"
    );
}

#[test]
fn test_config_validate_valid_config() {
    let config = AdminApiConfig {
        api_key: "this_is_a_valid_api_key_32ch".to_string(),
        enabled: true,
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_ok(), "Valid config should pass validation");
}

#[test]
fn test_config_new_requires_api_key() {
    let config = AdminApiConfig::new("my-secure-api-key-12345");
    assert_eq!(config.api_key, "my-secure-api-key-12345");
    assert!(config.enabled);
}

#[test]
fn test_config_default_disabled() {
    let config = AdminApiConfig::default();
    assert!(!config.enabled, "Default config should have enabled: false");
}
