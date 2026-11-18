// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! doctor 与 config/hash 端点共存冒烟（config-audit-drift T009 软依赖）
#![cfg(all(feature = "cli", feature = "admin-api"))]

#[test]
fn doctor_remains_static_when_config_hash_endpoint_is_available() {
    let inv = limiteron::cli::run(["doctor", "--security"]);
    // 无 --config 时 doctor 仅做编译期 feature 面检查，不连接运行实例，
    // 因此 config/hash 端点存在与否不影响其检查项数量与退出码契约。
    assert_eq!(inv.output["command"], serde_json::json!("doctor"));
    assert_eq!(inv.output["checks"].as_array().unwrap().len(), 10);
    assert!([0, 1, 2].contains(&inv.exit_code));
}
