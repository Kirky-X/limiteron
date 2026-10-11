// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! DB 存储后端共享契约测试套件
//!
//! 以「同一组操作断言」跑在不同 DSN 后端上，保证 postgres / mysql /
//! sqlite 适配器行为一致（契约共享，新增后端只需以 DSN 接入）。

use limiteron::error::StorageError;
use limiteron::{
    BanRecord, BanStorage, BanTarget, ConsumeResult, QuotaInfo, QuotaStorage, Storage,
};
use std::sync::Arc;
use std::time::Duration;

/// Storage 契约：set → get → delete → get(miss) + TTL 过期语义
//
// 契约断言按后端逐个接入（当前仅 mysql_storage 引用），未接入方组合下暂无调用者。
#[cfg_attr(
    not(feature = "mysql"),
    expect(
        dead_code,
        reason = "contract suite is wired per backend; combinations other than mysql have no callers yet"
    )
)]
pub async fn storage_contract(storage: &Arc<dyn Storage>) -> Result<(), StorageError> {
    storage.set("contract:key", "v1", None).await?;
    assert_eq!(
        storage.get("contract:key").await?,
        Some("v1".to_string()),
        "get should return the written value after set"
    );

    storage.set("contract:ttl", "gone", Some(1)).await?;
    storage.delete("contract:key").await?;
    assert_eq!(
        storage.get("contract:key").await?,
        None,
        "should miss after delete"
    );

    assert_eq!(storage.get("contract:ttl").await?, Some("gone".to_string()));

    // TTL 过期语义：到期前已确认存在，1 秒 TTL 过后必须 miss
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(
        storage.get("contract:ttl").await?,
        None,
        "TTL=1s key should expire and miss"
    );

    Ok(())
}

/// BanStorage 契约：save → is_banned → list → remove
#[cfg_attr(
    not(feature = "mysql"),
    expect(
        dead_code,
        reason = "contract suite is wired per backend; combinations other than mysql have no callers yet"
    )
)]
pub async fn ban_contract(bans: &Arc<dyn BanStorage>) -> Result<(), StorageError> {
    let record = BanRecord {
        target: BanTarget::Ip("203.0.113.77".to_string()),
        ban_times: 1,
        duration: Duration::from_secs(600),
        banned_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(600),
        is_manual: true,
        reason: "contract".to_string(),
    };
    bans.save(&record).await?;

    let hit = bans
        .is_banned(&BanTarget::Ip("203.0.113.77".to_string()))
        .await?;
    assert!(hit.is_some(), "banned target should be queryable");

    let listed = bans.list_bans(true, 0, 100).await?;
    assert!(
        listed
            .iter()
            .any(|r| r.target == BanTarget::Ip("203.0.113.77".to_string())),
        "list_bans should contain the contract ban"
    );

    bans.remove_ban(&BanTarget::Ip("203.0.113.77".to_string()))
        .await?;
    let miss = bans
        .is_banned(&BanTarget::Ip("203.0.113.77".to_string()))
        .await?;
    assert!(miss.is_none(), "should miss after unban");
    Ok(())
}

/// QuotaStorage 契约：consume → get_quota → reset
#[cfg_attr(
    not(feature = "mysql"),
    expect(
        dead_code,
        reason = "contract suite is wired per backend; combinations other than mysql have no callers yet"
    )
)]
pub async fn quota_contract(quota: &Arc<dyn QuotaStorage>) -> Result<(), StorageError> {
    let ConsumeResult { allowed, .. } = quota
        .consume("contract_user", "api", 2, 10, Duration::from_secs(60))
        .await?;
    assert!(allowed, "consumption within quota should be allowed");

    let info: Option<QuotaInfo> = quota.get_quota("contract_user", "api").await?;
    let info = info.expect("there should be a quota record after consumption");
    assert_eq!(info.consumed, 2, "consumed should be recorded as 2");

    quota
        .reset("contract_user", "api", 10, Duration::from_secs(60))
        .await?;
    let after = quota.get_quota("contract_user", "api").await?;
    assert!(
        after.map(|i| i.consumed).unwrap_or(0) == 0,
        "consumed should reset to zero after reset"
    );
    Ok(())
}
