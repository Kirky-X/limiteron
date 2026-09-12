// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! DB 存储后端共享契约测试套件
//!
//! 以「同一组操作断言」跑在不同 DSN 后端上，保证 postgres / mysql /
//! sqlite 适配器行为一致（契约共享，新增后端只需以 DSN 接入）。
#![allow(dead_code)]

use limiteron::error::StorageError;
use limiteron::{
    BanRecord, BanStorage, BanTarget, ConsumeResult, QuotaInfo, QuotaStorage, Storage,
};
use std::sync::Arc;
use std::time::Duration;

/// Storage 契约：set → get → delete → get(miss) + TTL 过期语义
pub async fn storage_contract(storage: &Arc<dyn Storage>) -> Result<(), StorageError> {
    storage.set("contract:key", "v1", None).await?;
    assert_eq!(
        storage.get("contract:key").await?,
        Some("v1".to_string()),
        "set 后 get 应返回写入值"
    );

    storage.set("contract:ttl", "gone", Some(1)).await?;
    storage.delete("contract:key").await?;
    assert_eq!(storage.get("contract:key").await?, None, "删除后应 miss");

    assert_eq!(storage.get("contract:ttl").await?, Some("gone".to_string()));

    // TTL 过期语义：到期前已确认存在，1 秒 TTL 过后必须 miss
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(
        storage.get("contract:ttl").await?,
        None,
        "TTL=1s 的 key 到期后应过期 miss"
    );

    Ok(())
}

/// BanStorage 契约：save → is_banned → list → remove
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
    assert!(hit.is_some(), "已封禁目标应可查询");

    let listed = bans.list_bans(true, 0, 100).await?;
    assert!(
        listed
            .iter()
            .any(|r| r.target == BanTarget::Ip("203.0.113.77".to_string())),
        "list_bans 应包含契约封禁"
    );

    bans.remove_ban(&BanTarget::Ip("203.0.113.77".to_string()))
        .await?;
    let miss = bans
        .is_banned(&BanTarget::Ip("203.0.113.77".to_string()))
        .await?;
    assert!(miss.is_none(), "解封后应 miss");
    Ok(())
}

/// QuotaStorage 契约：consume → get_quota → reset
pub async fn quota_contract(quota: &Arc<dyn QuotaStorage>) -> Result<(), StorageError> {
    let ConsumeResult { allowed, .. } = quota
        .consume("contract_user", "api", 2, 10, Duration::from_secs(60))
        .await?;
    assert!(allowed, "额度内消费应放行");

    let info: Option<QuotaInfo> = quota.get_quota("contract_user", "api").await?;
    let info = info.expect("消费后应有配额记录");
    assert_eq!(info.consumed, 2, "消费量应记录 2");

    quota
        .reset("contract_user", "api", 10, Duration::from_secs(60))
        .await?;
    let after = quota.get_quota("contract_user", "api").await?;
    assert!(
        after.map(|i| i.consumed).unwrap_or(0) == 0,
        "reset 后消费量应归零"
    );
    Ok(())
}
