// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! SQLite 嵌入式后端测试（feature `sqlite`）
//!
//! 全部用例运行于 `sqlite::memory:`，无需 Docker / 外部服务（沙箱可跑）。
//! 覆盖三条主线：
//! 1. sqlite 方言建表 DDL + StorageFactory 全链路（KV / 封禁 / 配额适配器）；
//! 2. 原生 SQL 路径（封禁原子计数、配额原子消费）在 sqlite 上的行为等价性；
//! 3. 与 Redis 形后端（`cache-redis` 的 Cache* 适配器）的关键行为对拍。

use super::{DBNexusBanStorageAdapter, DBNexusQuotaStorageAdapter, DBNexusStorageAdapter};
use super::{StorageFactory, StorageFactoryConfig};
use crate::storage::{BanRecord, BanStorage, BanTarget, QuotaStorage, Storage};
use chrono::Utc;
use dbnexus::DbPool;
use std::sync::Arc;
use std::time::Duration;

/// 建立带完整 schema 的内存 sqlite 连接池
async fn init_pool() -> Arc<DbPool> {
    let pool = DbPool::new("sqlite::memory:")
        .await
        .expect("sqlite memory pool");
    let pool = Arc::new(pool);
    create_schema(&pool).await;
    pool
}

/// 在池上执行 sqlite 方言建表 DDL
async fn create_schema(pool: &DbPool) {
    let session = pool.get_session("admin").await.expect("session");
    let conn = session.connection().expect("connection");
    for stmt in crate::create_all_tables_ddl_sqlite().split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        sea_orm::ConnectionTrait::execute_unprepared(conn, stmt)
            .await
            .unwrap_or_else(|e| panic!("schema statement failed: {e}\n{stmt}"));
    }
}

fn make_ban(value: &str, expires_in_secs: i64) -> BanRecord {
    let now = Utc::now();
    BanRecord {
        target: BanTarget::Ip(value.to_string()),
        ban_times: 1,
        duration: Duration::from_secs(expires_in_secs.unsigned_abs()),
        banned_at: now,
        expires_at: now + chrono::Duration::seconds(expires_in_secs),
        is_manual: true,
        reason: "sqlite-test".to_string(),
    }
}

fn sqlite_triple(
    pool: Arc<DbPool>,
) -> (Arc<dyn Storage>, Arc<dyn BanStorage>, Arc<dyn QuotaStorage>) {
    (
        Arc::new(DBNexusStorageAdapter::new(pool.clone())),
        Arc::new(DBNexusBanStorageAdapter::new(pool.clone())),
        Arc::new(DBNexusQuotaStorageAdapter::new(pool)),
    )
}

// ============================================================================
// Storage（KV）适配器
// ============================================================================

#[tokio::test]
async fn kv_set_get_delete_roundtrip() {
    let (storage, _, _) = sqlite_triple(init_pool().await);
    assert_eq!(storage.get("missing").await.unwrap(), None);

    storage.set("k", "v1", None).await.unwrap();
    assert_eq!(storage.get("k").await.unwrap().as_deref(), Some("v1"));

    storage.set("k", "v2", None).await.unwrap();
    assert_eq!(storage.get("k").await.unwrap().as_deref(), Some("v2"));

    storage.delete("k").await.unwrap();
    assert_eq!(storage.get("k").await.unwrap(), None);
}

#[tokio::test]
async fn kv_ttl_expiry() {
    let (storage, _, _) = sqlite_triple(init_pool().await);
    storage.set("ttl-key", "v", Some(1)).await.unwrap();
    assert_eq!(storage.get("ttl-key").await.unwrap().as_deref(), Some("v"));
    tokio::time::sleep(Duration::from_millis(1150)).await;
    assert_eq!(storage.get("ttl-key").await.unwrap(), None);
}

// ============================================================================
// BanStorage 适配器（含原生 SQL 原子路径）
// ============================================================================

#[tokio::test]
async fn ban_save_is_banned_and_fields() {
    let (_, ban, _) = sqlite_triple(init_pool().await);
    let record = make_ban("203.0.113.1", 3600);
    ban.save(&record).await.unwrap();

    let loaded = ban
        .is_banned(&record.target)
        .await
        .unwrap()
        .expect("banned");
    assert_eq!(loaded.target, BanTarget::Ip("203.0.113.1".to_string()));
    assert_eq!(loaded.ban_times, 1);
    assert!(loaded.is_manual);
    assert_eq!(loaded.reason, "sqlite-test");
    assert_eq!(ban.get_ban_times(&record.target).await.unwrap(), 1);
}

#[tokio::test]
async fn ban_upsert_increments_atomically() {
    let (_, ban, _) = sqlite_triple(init_pool().await);
    let record = make_ban("203.0.113.2", 3600);

    assert_eq!(ban.upsert_ban_record(&record).await.unwrap(), 1);
    assert_eq!(ban.upsert_ban_record(&record).await.unwrap(), 2);
    assert_eq!(ban.upsert_ban_record(&record).await.unwrap(), 3);
    assert_eq!(ban.increment_ban_times(&record.target).await.unwrap(), 4);
    assert_eq!(ban.get_ban_times(&record.target).await.unwrap(), 4);
}

#[tokio::test]
async fn ban_increment_missing_record_is_error() {
    let (_, ban, _) = sqlite_triple(init_pool().await);
    let target = BanTarget::Ip("203.0.113.3".to_string());
    assert!(ban.increment_ban_times(&target).await.is_err());
}

#[tokio::test]
async fn ban_expired_invisible_and_cleanup_removes() {
    let (_, ban, _) = sqlite_triple(init_pool().await);
    ban.save(&make_ban("203.0.113.4", 3600)).await.unwrap();
    ban.save(&make_ban("203.0.113.5", -1)).await.unwrap();

    assert!(
        ban.is_banned(&BanTarget::Ip("203.0.113.5".to_string()))
            .await
            .unwrap()
            .is_none(),
        "过期封禁不应可见"
    );
    assert_eq!(ban.cleanup_expired_bans().await.unwrap(), 1);
    assert!(
        ban.is_banned(&BanTarget::Ip("203.0.113.4".to_string()))
            .await
            .unwrap()
            .is_some(),
        "活跃封禁不应被清理"
    );
}

#[tokio::test]
async fn ban_remove_clears_record() {
    let (_, ban, _) = sqlite_triple(init_pool().await);
    let record = make_ban("203.0.113.6", 3600);
    ban.save(&record).await.unwrap();
    ban.remove_ban(&record.target).await.unwrap();
    assert!(ban.is_banned(&record.target).await.unwrap().is_none());
    assert_eq!(ban.get_ban_times(&record.target).await.unwrap(), 0);
}

#[tokio::test]
async fn ban_list_bans_active_only_and_pagination() {
    let (_, ban, _) = sqlite_triple(init_pool().await);
    ban.save(&make_ban("203.0.113.10", 3600)).await.unwrap();
    ban.save(&make_ban("203.0.113.11", 3600)).await.unwrap();
    ban.save(&make_ban("203.0.113.12", -1)).await.unwrap();

    let all = ban.list_bans(false, 0, 10).await.unwrap();
    assert_eq!(all.len(), 3);
    let active = ban.list_bans(true, 0, 10).await.unwrap();
    assert_eq!(active.len(), 2);
    let paged = ban.list_bans(false, 1, 1).await.unwrap();
    assert_eq!(paged.len(), 1);
}

// ============================================================================
// QuotaStorage 适配器（原子消费 / 退款 / 重置 / 窗口翻滚）
// ============================================================================

#[tokio::test]
async fn quota_consume_exhaustion_refund_reset() {
    let (_, _, quota) = sqlite_triple(init_pool().await);
    let window = Duration::from_secs(3600);

    let r1 = quota.consume("u1", "api", 5, 10, window).await.unwrap();
    assert!(r1.allowed);
    assert_eq!(r1.remaining, 5);

    let r2 = quota.consume("u1", "api", 5, 10, window).await.unwrap();
    assert!(r2.allowed);
    assert_eq!(r2.remaining, 0);

    let r3 = quota.consume("u1", "api", 1, 10, window).await.unwrap();
    assert!(!r3.allowed, "额度耗尽后应拒绝");
    assert_eq!(r3.remaining, 0);

    let returned = quota.refund("u1", "api", 2, 10, window).await.unwrap();
    assert_eq!(returned, 8, "退款返回归还后的账本值（10 - 2）");

    let r4 = quota.consume("u1", "api", 2, 10, window).await.unwrap();
    assert!(r4.allowed);
    assert_eq!(r4.remaining, 0);

    quota.reset("u1", "api", 10, window).await.unwrap();
    let r5 = quota.consume("u1", "api", 10, 10, window).await.unwrap();
    assert!(r5.allowed, "重置后额度应满");
}

#[tokio::test]
async fn quota_refund_clamps_and_noop() {
    let (_, _, quota) = sqlite_triple(init_pool().await);
    let window = Duration::from_secs(3600);

    assert_eq!(
        quota.refund("ghost", "api", 5, 10, window).await.unwrap(),
        0,
        "无记录退款应为 no-op 返回 0"
    );

    quota.consume("u2", "api", 5, 10, window).await.unwrap();
    assert_eq!(
        quota.refund("u2", "api", 50, 10, window).await.unwrap(),
        0,
        "超额退款应钳制账本不为负"
    );
}

#[tokio::test]
async fn quota_window_rollover_starts_fresh() {
    let (_, _, quota) = sqlite_triple(init_pool().await);
    let window = Duration::from_secs(1);

    assert!(
        quota
            .consume("u3", "api", 1, 1, window)
            .await
            .unwrap()
            .allowed
    );
    assert!(
        !quota
            .consume("u3", "api", 1, 1, window)
            .await
            .unwrap()
            .allowed
    );

    tokio::time::sleep(Duration::from_millis(1150)).await;
    assert!(
        quota.get_quota("u3", "api").await.unwrap().is_none(),
        "过期窗口记录不应作为活跃配额可见"
    );
    assert!(
        quota
            .consume("u3", "api", 1, 1, window)
            .await
            .unwrap()
            .allowed,
        "新窗口应从零开始记账"
    );
}

#[tokio::test]
async fn quota_cost_over_i64_is_rejected() {
    let (_, _, quota) = sqlite_triple(init_pool().await);
    let window = Duration::from_secs(3600);
    let r = quota
        .consume("u4", "api", u64::MAX, 10, window)
        .await
        .unwrap();
    assert!(!r.allowed, "超出 i64 的 cost 必须显式拒绝而非账本回绕");
}

// ============================================================================
// StorageFactory 全链路 + BanManager 适配
// ============================================================================

#[tokio::test]
async fn factory_creates_usable_sqlite_backends() {
    let mut factory = StorageFactory::new(StorageFactoryConfig::sqlite(":memory:"));
    factory.initialize(None).await.expect("initialize");
    factory.create_schema().await.expect("create_schema");
    let (storage, ban, quota) = factory.create_all().await.expect("create_all");

    storage.set("fk", "fv", None).await.unwrap();
    assert_eq!(storage.get("fk").await.unwrap().as_deref(), Some("fv"));

    let record = make_ban("203.0.113.20", 3600);
    ban.save(&record).await.unwrap();
    assert!(ban.is_banned(&record.target).await.unwrap().is_some());

    quota
        .consume("fu", "api", 1, 10, Duration::from_secs(60))
        .await
        .unwrap();
}

#[cfg(feature = "ban-manager")]
#[tokio::test]
async fn ban_manager_runs_on_sqlite_storage() {
    use crate::ban::BanSource;

    let pool = init_pool().await;
    let storage: Arc<dyn BanStorage> = Arc::new(DBNexusBanStorageAdapter::new(pool));
    let manager = crate::BanManager::builder()
        .with_storage(storage)
        .build()
        .await
        .expect("ban manager");

    let target = BanTarget::Ip("198.51.100.7".to_string());
    manager
        .create_ban(
            target.clone(),
            "sqlite-backed manager".to_string(),
            BanSource::Manual {
                operator: "test".to_string(),
            },
            serde_json::json!({}),
            Some(Duration::from_secs(3600)),
        )
        .await
        .expect("create ban");
    assert!(
        manager.is_banned(&target).await.unwrap().is_some(),
        "BanManager 经 sqlite 存储的封禁应可见"
    );
}

// ============================================================================
// 与 Redis 形后端（cache-redis）行为对拍（关键用例）
// ============================================================================
// Redis 后端在本仓即 oxcache 支撑的 Cache* 适配器；沙箱无 Redis 时以
// DashMapMemoryBackend 作为同一代码路径的传输层（生产仅替换传输层实现）。

#[cfg(feature = "cache-redis")]
mod redis_parity {
    use super::*;
    use crate::cache::{CacheBanStorage, CacheQuotaStorage, CacheStorage};
    use crate::error::ConsumeResult;
    use oxcache::backend::memory::DashMapMemoryBackend;

    async fn cache_triple() -> (Arc<dyn Storage>, Arc<dyn BanStorage>, Arc<dyn QuotaStorage>) {
        let backend: Arc<dyn oxcache::backend::CacheBackend> =
            Arc::new(DashMapMemoryBackend::new());
        (
            Arc::new(CacheStorage::new(backend.clone())),
            Arc::new(CacheBanStorage::new(backend.clone())),
            Arc::new(CacheQuotaStorage::new(backend)),
        )
    }

    /// KV 场景：写入 → 覆盖 → 删除的可观测序列
    async fn kv_scenario(s: &dyn Storage) -> Vec<Option<String>> {
        let mut out = Vec::new();
        s.set("p-k", "v1", None).await.unwrap();
        out.push(s.get("p-k").await.unwrap());
        s.set("p-k", "v2", None).await.unwrap();
        out.push(s.get("p-k").await.unwrap());
        s.delete("p-k").await.unwrap();
        out.push(s.get("p-k").await.unwrap());
        out
    }

    /// 封禁场景：upsert 计数、可见性、过期不可见、清理、解封
    async fn ban_scenario(b: &dyn BanStorage) -> (u64, u64, bool, bool, u64, usize, usize, u64) {
        let target = BanTarget::Ip("203.0.113.30".to_string());
        let expired_target = BanTarget::Ip("203.0.113.31".to_string());

        let first = b
            .upsert_ban_record(&make_ban("203.0.113.30", 3600))
            .await
            .unwrap();
        let second = b
            .upsert_ban_record(&make_ban("203.0.113.30", 3600))
            .await
            .unwrap();
        let visible = b.is_banned(&target).await.unwrap().is_some();

        b.save(&make_ban("203.0.113.31", -1)).await.unwrap();
        // 过期不可见语义：Redis 形后端由 TTL 驱逐保证（内存传输层同样按
        // TTL 过期，save 对已过期记录给的存活下限为 1s），等待驱逐窗口后
        // 两侧再对拍；sqlite 侧由 expires_at 时间戳过滤即时保证
        tokio::time::sleep(Duration::from_millis(1150)).await;
        let expired_visible = b.is_banned(&expired_target).await.unwrap().is_some();

        let cleaned = b.cleanup_expired_bans().await.unwrap();
        b.remove_ban(&target).await.unwrap();
        let times_after_remove = b.get_ban_times(&target).await.unwrap();
        let active = b.list_bans(true, 0, 100).await.unwrap().len();
        let all = b.list_bans(false, 0, 100).await.unwrap().len();
        (
            first,
            second,
            visible,
            expired_visible,
            cleaned,
            active,
            all,
            times_after_remove,
        )
    }

    /// 配额场景：消费 → 耗尽 → 退款 → 重置的裁决序列
    async fn quota_scenario(q: &dyn QuotaStorage) -> Vec<ConsumeResult> {
        let window = Duration::from_secs(3600);
        let mut out = Vec::new();
        out.push(q.consume("p-u", "api", 5, 10, window).await.unwrap());
        out.push(q.consume("p-u", "api", 5, 10, window).await.unwrap());
        out.push(q.consume("p-u", "api", 1, 10, window).await.unwrap());
        let returned = q.refund("p-u", "api", 4, 10, window).await.unwrap();
        assert_eq!(returned, 6, "退款返回归还后的账本值（10 - 4）");
        out.push(q.consume("p-u", "api", 4, 10, window).await.unwrap());
        q.reset("p-u", "api", 10, window).await.unwrap();
        out.push(q.consume("p-u", "api", 10, 10, window).await.unwrap());
        out
    }

    fn assert_ban_parity(
        a: &(u64, u64, bool, bool, u64, usize, usize, u64),
        b: &(u64, u64, bool, bool, u64, usize, usize, u64),
    ) {
        assert_eq!(a.0, b.0, "首次 upsert 计数");
        assert_eq!(a.1, b.1, "二次 upsert 计数");
        assert_eq!(a.2, b.2, "活跃封禁可见性");
        assert_eq!(a.3, b.3, "过期封禁不可见");
        assert_eq!(a.4, b.4, "过期清理计数");
        assert_eq!(a.5, b.5, "解封后活跃清单");
        assert_eq!(a.6, b.6, "清单总数");
        assert_eq!(a.7, b.7, "解封后 get_ban_times 语义");
    }

    fn assert_quota_parity(a: &[ConsumeResult], b: &[ConsumeResult]) {
        assert_eq!(a.len(), b.len());
        for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            assert_eq!(x.allowed, y.allowed, "step {i} allowed");
            assert_eq!(x.remaining, y.remaining, "step {i} remaining");
            assert_eq!(x.usage_percent, y.usage_percent, "step {i} usage_percent");
        }
    }

    #[tokio::test]
    async fn kv_behavior_matches_redis_backend() {
        let (sqlite, _, _) = sqlite_triple(init_pool().await);
        let (cache, _, _) = cache_triple().await;
        assert_eq!(
            kv_scenario(&*sqlite).await,
            kv_scenario(&*cache).await,
            "KV 行为必须与 Redis 形后端一致"
        );
    }

    #[tokio::test]
    async fn ban_behavior_matches_redis_backend() {
        let (_, sqlite, _) = sqlite_triple(init_pool().await);
        let (_, cache, _) = cache_triple().await;
        let a = ban_scenario(&*sqlite).await;
        let b = ban_scenario(&*cache).await;
        assert_ban_parity(&a, &b);
    }

    #[tokio::test]
    async fn quota_behavior_matches_redis_backend() {
        let (_, _, sqlite) = sqlite_triple(init_pool().await);
        let (_, _, cache) = cache_triple().await;
        let a = quota_scenario(&*sqlite).await;
        let b = quota_scenario(&*cache).await;
        assert_quota_parity(&a, &b);
    }
}
