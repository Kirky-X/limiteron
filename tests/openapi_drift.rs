// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! OpenAPI 产物防漂移守卫
//!
//! `docs/openapi.json` 是入库产物；本文档断言内存构建（单一事实来源
//! `admin::openapi::openapi_document`）与产物逐字节一致。契约变更时测试
//! 红灯，执行 `UPDATE_OPENAPI=1 cargo test --features openapi --test
//! openapi_drift` 重写产物后再提交（重写 diff 必须人工复核——它是 API 契约
//! 的变更记录）。

use std::path::PathBuf;

const PRODUCED_AT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/openapi.json");

#[test]
fn openapi_artifact_matches_document() {
    let expected = limiteron::admin::openapi::openapi_document_pretty();
    let path = PathBuf::from(PRODUCED_AT);

    if std::env::var("UPDATE_OPENAPI").is_ok() {
        std::fs::write(&path, &expected).unwrap_or_else(|e| panic!("重写产物失败 {path:?}: {e}"));
        println!("openapi.json 已重写（{} bytes）", expected.len());
        return;
    }

    let actual =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取产物失败 {path:?}: {e}"));
    assert_eq!(
        actual, expected,
        "docs/openapi.json 与内存文档漂移；契约变更后以 UPDATE_OPENAPI=1 重写产物并人工复核 diff"
    );
}
