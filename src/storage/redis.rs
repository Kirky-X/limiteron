//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! Redis storage backend implementation
//!
//! This module provides [`RedisStorage`], a Redis-backed implementation of the
//! [`Storage`], [`QuotaStorage`], and [`BanStorage`] traits.
//!
//! # Design
//!
//! - **Storage**: plain Redis string keys (`GET`/`SET`/`SET EX`/`DEL`).
//! - **QuotaStorage**: each quota is a Redis Hash at `quota:{user_id}:{resource}`
//!   with fields `consumed`/`limit`/`window_start`/`window_end`. Consumption is
//!   performed atomically via a Lua script (`EVAL`).
//! - **BanStorage**: each ban is a Redis Hash at
//!   `ban:{target_type}:{target_value}` with individual fields. A Hash (rather
//!   than a single JSON string) is required so that `HINCRBY`/`HGET` can operate
//!   on `ban_times` atomically, as specified by the trait contract.
//!
//! Requires the `redis-storage` feature.

use crate::error::{ConsumeResult, StorageError};
use crate::storage::{
    BanHistory, BanRecord, BanStorage, BanTarget, QuotaInfo, QuotaStorage, Storage,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::time::Duration;

/// Lua script implementing atomic check-and-consume for a quota hash key.
///
/// Returns `{1, new_consumed}` when the consume is allowed, `{0, current}`
/// when it would exceed the limit. Resets the window if it has expired.
const CHECK_AND_CONSUME_SCRIPT: &str = r#"
local key = KEYS[1]
local cost = tonumber(ARGV[1])
local limit = tonumber(ARGV[2])
local window = tonumber(ARGV[3])
local now = tonumber(ARGV[4])
local current = tonumber(redis.call('HGET', key, 'consumed') or '0')
local wend = tonumber(redis.call('HGET', key, 'window_end') or '0')
local expired = (wend == 0) or (now > wend)
if expired then
    current = 0
end
if current + cost > limit then
    return {0, current}
end
local new_consumed = current + cost
if expired then
    redis.call('HSET', key,
        'consumed', tostring(new_consumed),
        'limit', tostring(limit),
        'window_start', tostring(now),
        'window_end', tostring(now + window))
else
    redis.call('HSET', key,
        'consumed', tostring(new_consumed),
        'limit', tostring(limit))
end
redis.call('EXPIRE', key, window)
return {1, new_consumed}
"#;

/// Lua script atomically resetting a quota hash key.
///
/// Performs `DEL` + `HSET` + `EXPIRE` in a single atomic step so that a
/// concurrent `consume` cannot observe a half-deleted or TTL-less quota.
const RESET_QUOTA_SCRIPT: &str = r#"
local key = KEYS[1]
local consumed = ARGV[1]
local limit = ARGV[2]
local window_start = ARGV[3]
local window_end = ARGV[4]
local window = tonumber(ARGV[5])

redis.call('DEL', key)
redis.call('HSET', key, 'consumed', consumed, 'limit', limit, 'window_start', window_start, 'window_end', window_end)
if window > 0 then
    redis.call('EXPIRE', key, window)
end
return 1
"#;

/// Lua script atomically incrementing `ban_times` only if the ban key exists.
///
/// Returns the new value (>= 1) when the key exists, or `0` when it does not.
const INCREMENT_BAN_SCRIPT: &str = r#"
if redis.call('EXISTS', KEYS[1]) == 0 then
    return 0
end
return redis.call('HINCRBY', KEYS[1], 'ban_times', 1)
"#;

/// Lua script atomically saving a ban record.
///
/// Performs `HSET` + `EXPIRE` in a single atomic step so that a crash between
/// the two cannot leave a ban key without a TTL (which would make it permanent).
const SAVE_BAN_SCRIPT: &str = r#"
local key = KEYS[1]
local target = ARGV[1]
local ban_times = ARGV[2]
local duration_secs = ARGV[3]
local banned_at = ARGV[4]
local expires_at = ARGV[5]
local is_manual = ARGV[6]
local reason = ARGV[7]

redis.call('HSET', key,
    'target', target,
    'ban_times', ban_times,
    'duration_secs', duration_secs,
    'banned_at', banned_at,
    'expires_at', expires_at,
    'is_manual', is_manual,
    'reason', reason
)
local ttl = tonumber(duration_secs)
if ttl > 0 then
    redis.call('EXPIRE', key, ttl)
end
return 1
"#;

/// Convert a [`redis::RedisError`] into a [`StorageError`].
///
/// Connection/auth failures map to transient/permanent variants; everything
/// else is treated as a query failure.
impl From<redis::RedisError> for StorageError {
    fn from(e: redis::RedisError) -> Self {
        match e.kind() {
            redis::ErrorKind::AuthenticationFailed => {
                StorageError::AuthenticationError(e.to_string())
            }
            redis::ErrorKind::IoError | redis::ErrorKind::ClientError => {
                StorageError::ConnectionError(e.to_string())
            }
            _ => StorageError::QueryError(e.to_string()),
        }
    }
}

/// Redis-backed storage implementing [`Storage`], [`QuotaStorage`], and
/// [`BanStorage`].
///
/// Uses a [`redis::aio::ConnectionManager`] (auto-reconnecting, cheaply
/// cloneable) for connection management.
pub struct RedisStorage {
    /// Auto-reconnecting async connection manager.
    conn: redis::aio::ConnectionManager,
}

impl RedisStorage {
    /// Creates a new `RedisStorage` from a Redis URL (e.g.
    /// `redis://127.0.0.1:6379/`).
    ///
    /// This is async because establishing the [`ConnectionManager`] is async and
    /// may fail.
    pub async fn new(url: &str) -> Result<Self, StorageError> {
        let client = redis::Client::open(url).map_err(StorageError::from)?;
        let conn = redis::aio::ConnectionManager::new(client)
            .await
            .map_err(StorageError::from)?;
        Ok(Self { conn })
    }

    /// Creates a new `RedisStorage` from an existing [`redis::Client`].
    ///
    /// This is async (and returns `Result`) because establishing the
    /// [`ConnectionManager`] is inherently async and may fail.
    pub async fn from_client(client: redis::Client) -> Result<Self, StorageError> {
        let conn = redis::aio::ConnectionManager::new(client)
            .await
            .map_err(StorageError::from)?;
        Ok(Self { conn })
    }

    /// Returns a cheaply-cloned async connection handle for issuing commands.
    fn conn(&self) -> redis::aio::ConnectionManager {
        self.conn.clone()
    }

    /// Checks whether a key exists in Redis.
    ///
    /// Uses the `EXISTS` command. Returns `true` if the key exists, `false`
    /// otherwise.
    pub async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let mut conn = self.conn();
        let count: i64 = redis::cmd("EXISTS").arg(key).query_async(&mut conn).await?;
        Ok(count > 0)
    }

    /// Scans all keys matching `pattern` using `SCAN` (non-blocking).
    async fn scan_keys(&self, pattern: &str) -> Result<Vec<String>, StorageError> {
        let mut conn = self.conn();
        let mut cursor: String = "0".to_string();
        let mut keys: Vec<String> = Vec::new();
        loop {
            let (next_cursor, batch): (String, Vec<String>) = redis::cmd("SCAN")
                .arg(&cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(100i64)
                .query_async(&mut conn)
                .await?;
            keys.extend(batch);
            cursor = next_cursor;
            if cursor == "0" || cursor.parse::<u64>().unwrap_or(0) == 0 {
                break;
            }
        }
        Ok(keys)
    }
}

#[async_trait]
impl Storage for RedisStorage {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        let mut conn = self.conn();
        let val: Option<String> = redis::cmd("GET").arg(key).query_async(&mut conn).await?;
        Ok(val)
    }

    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError> {
        let mut conn = self.conn();
        match ttl {
            Some(secs) => {
                let _: () = redis::cmd("SETEX")
                    .arg(key)
                    .arg(secs)
                    .arg(value)
                    .query_async(&mut conn)
                    .await?;
            }
            None => {
                let _: () = redis::cmd("SET")
                    .arg(key)
                    .arg(value)
                    .query_async(&mut conn)
                    .await?;
            }
        }
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let mut conn = self.conn();
        let _: () = redis::cmd("DEL").arg(key).query_async(&mut conn).await?;
        Ok(())
    }
}

#[async_trait]
impl QuotaStorage for RedisStorage {
    async fn get_quota(
        &self,
        user_id: &str,
        resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError> {
        let key = quota_key(user_id, resource);
        let mut conn = self.conn();
        let fields: Vec<String> = redis::cmd("HGETALL")
            .arg(&key)
            .query_async(&mut conn)
            .await?;
        if fields.is_empty() {
            return Ok(None);
        }
        let consumed = find_field(&fields, "consumed")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let limit = find_field(&fields, "limit")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let window_start = find_field(&fields, "window_start")
            .and_then(parse_ts)
            .unwrap_or_else(Utc::now);
        let window_end = find_field(&fields, "window_end")
            .and_then(parse_ts)
            .unwrap_or_else(Utc::now);
        Ok(Some(QuotaInfo {
            consumed,
            limit,
            window_start,
            window_end,
        }))
    }

    async fn consume(
        &self,
        user_id: &str,
        resource: &str,
        cost: u64,
        limit: u64,
        window: Duration,
    ) -> Result<ConsumeResult, StorageError> {
        let key = quota_key(user_id, resource);
        let mut conn = self.conn();
        let now_ts = Utc::now().timestamp();
        let window_secs = window.as_secs() as i64;
        let res: Vec<i64> = redis::cmd("EVAL")
            .arg(CHECK_AND_CONSUME_SCRIPT)
            .arg(1i64)
            .arg(&key)
            .arg(cost)
            .arg(limit)
            .arg(window_secs)
            .arg(now_ts)
            .query_async(&mut conn)
            .await?;
        let allowed = res.first().map(|v| *v == 1).unwrap_or(false);
        let consumed = res.get(1).copied().unwrap_or(0) as u64;
        let remaining = limit.saturating_sub(consumed);
        let usage_percent = if limit > 0 {
            (consumed as f64 / limit as f64) * 100.0
        } else {
            0.0
        };
        Ok(ConsumeResult {
            allowed,
            remaining,
            alert_triggered: false,
            usage_percent,
        })
    }

    async fn reset(
        &self,
        user_id: &str,
        resource: &str,
        limit: u64,
        window: Duration,
    ) -> Result<(), StorageError> {
        let key = quota_key(user_id, resource);
        let mut conn = self.conn();
        let now_ts = Utc::now().timestamp();
        let window_secs = window.as_secs() as i64;
        let window_end = now_ts + window_secs;
        let _: i64 = redis::cmd("EVAL")
            .arg(RESET_QUOTA_SCRIPT)
            .arg(1i64)
            .arg(&key)
            .arg(0u64)
            .arg(limit)
            .arg(now_ts)
            .arg(window_end)
            .arg(window_secs)
            .query_async(&mut conn)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl BanStorage for RedisStorage {
    async fn is_banned(&self, target: &BanTarget) -> Result<Option<BanRecord>, StorageError> {
        let key = ban_key(target);
        let mut conn = self.conn();
        let fields: Vec<String> = redis::cmd("HGETALL")
            .arg(&key)
            .query_async(&mut conn)
            .await?;
        if fields.is_empty() {
            return Ok(None);
        }
        let record = match parse_ban_record(&key, &fields) {
            Some(r) => r,
            None => return Ok(None),
        };
        if record.expires_at <= Utc::now() {
            // Expired: clean up lazily (mirrors MemoryBanStorage semantics).
            let _: () = redis::cmd("DEL").arg(&key).query_async(&mut conn).await?;
            return Ok(None);
        }
        Ok(Some(record))
    }

    async fn save(&self, record: &BanRecord) -> Result<(), StorageError> {
        let key = ban_key(&record.target);
        let mut conn = self.conn();
        let target_json = serde_json::to_string(&record.target)
            .map_err(|e| StorageError::QueryError(e.to_string()))?;
        let duration_secs = record.duration.as_secs();
        let _: i64 = redis::cmd("EVAL")
            .arg(SAVE_BAN_SCRIPT)
            .arg(1i64)
            .arg(&key)
            .arg(&target_json)
            .arg(record.ban_times)
            .arg(duration_secs)
            .arg(record.banned_at.to_rfc3339())
            .arg(record.expires_at.to_rfc3339())
            .arg(if record.is_manual { "true" } else { "false" })
            .arg(&record.reason)
            .query_async(&mut conn)
            .await?;
        Ok(())
    }

    async fn get_history(&self, target: &BanTarget) -> Result<Option<BanHistory>, StorageError> {
        let key = ban_key(target);
        let mut conn = self.conn();
        let fields: Vec<String> = redis::cmd("HGETALL")
            .arg(&key)
            .query_async(&mut conn)
            .await?;
        if fields.is_empty() {
            return Ok(None);
        }
        let ban_times = find_field(&fields, "ban_times")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        let last_banned_at = find_field(&fields, "banned_at")
            .and_then(parse_ts)
            .unwrap_or_else(Utc::now);
        Ok(Some(BanHistory {
            ban_times,
            last_banned_at,
        }))
    }

    async fn increment_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let key = ban_key(target);
        let mut conn = self.conn();
        let result: i64 = redis::cmd("EVAL")
            .arg(INCREMENT_BAN_SCRIPT)
            .arg(1i64)
            .arg(&key)
            .query_async(&mut conn)
            .await?;
        if result <= 0 {
            Ok(0)
        } else {
            Ok(result as u64)
        }
    }

    async fn get_ban_times(&self, target: &BanTarget) -> Result<u64, StorageError> {
        let key = ban_key(target);
        let mut conn = self.conn();
        let val: Option<String> = redis::cmd("HGET")
            .arg(&key)
            .arg("ban_times")
            .query_async(&mut conn)
            .await?;
        Ok(val.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0))
    }

    async fn remove_ban(&self, target: &BanTarget) -> Result<(), StorageError> {
        let key = ban_key(target);
        let mut conn = self.conn();
        let _: () = redis::cmd("DEL").arg(&key).query_async(&mut conn).await?;
        Ok(())
    }

    async fn cleanup_expired_bans(&self) -> Result<u64, StorageError> {
        let keys = self.scan_keys("ban:*").await?;
        let mut removed: u64 = 0;
        let now = Utc::now();
        for key in keys {
            let mut conn = self.conn();
            let fields: Vec<String> = redis::cmd("HGETALL")
                .arg(&key)
                .query_async(&mut conn)
                .await?;
            let expired = find_field(&fields, "expires_at")
                .and_then(parse_ts)
                .map(|t| t <= now)
                .unwrap_or(false);
            if expired {
                let deleted: i64 = redis::cmd("DEL").arg(&key).query_async(&mut conn).await?;
                if deleted > 0 {
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }

    async fn list_bans(
        &self,
        active_only: bool,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BanRecord>, StorageError> {
        let keys = self.scan_keys("ban:*").await?;
        let now = Utc::now();
        let mut records: Vec<BanRecord> = Vec::new();
        for key in keys {
            let mut conn = self.conn();
            let fields: Vec<String> = redis::cmd("HGETALL")
                .arg(&key)
                .query_async(&mut conn)
                .await?;
            if let Some(record) = parse_ban_record(&key, &fields) {
                let is_active = record.expires_at > now;
                if !active_only || is_active {
                    records.push(record);
                }
            }
        }
        let start = offset as usize;
        let take = limit as usize;
        Ok(records.into_iter().skip(start).take(take).collect())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Builds the Redis key for a ban record: `ban:{type}:{value}`.
fn ban_key(target: &BanTarget) -> String {
    let (kind, value) = ban_target_parts(target);
    format!("ban:{}:{}", kind, value)
}

/// Returns the (`type`, `value`) pair used in ban keys and matching the serde
/// tag of [`BanTarget`].
fn ban_target_parts(target: &BanTarget) -> (&'static str, String) {
    match target {
        BanTarget::Ip(v) => ("ip", v.clone()),
        BanTarget::UserId(v) => ("user", v.clone()),
        BanTarget::Mac(v) => ("mac", v.clone()),
    }
}

/// Builds the Redis key for a quota record: `quota:{user_id}:{resource}`.
fn quota_key(user_id: &str, resource: &str) -> String {
    format!("quota:{}:{}", user_id, resource)
}

/// Looks up a value by name in a flat alternating `[k, v, k, v, ...]` hash
/// reply (as returned by `HGETALL` when decoded into `Vec<String>`).
fn find_field<'a>(fields: &'a [String], name: &str) -> Option<&'a str> {
    let mut iter = fields.iter();
    while let Some(k) = iter.next() {
        if let Some(v) = iter.next() {
            if k == name {
                return Some(v.as_str());
            }
        }
    }
    None
}

/// Parses an RFC3339 timestamp field into a UTC `DateTime`.
fn parse_ts(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Reconstructs a [`BanRecord`] from a flat `HGETALL` reply.
///
/// `expires_at` is a safety-critical field: when it is missing or unparseable
/// the record is treated as corrupt and `None` is returned (with a warning log
/// identifying `key`) so callers cannot mistake a stale/expired ban for a
/// freshly-issued one. `banned_at` falls back to `Utc::now()` since its absence
/// has no security impact.
fn parse_ban_record(key: &str, fields: &[String]) -> Option<BanRecord> {
    let target_str = find_field(fields, "target")?;
    let target: BanTarget = serde_json::from_str(target_str).ok()?;
    let ban_times = find_field(fields, "ban_times")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let duration_secs = find_field(fields, "duration_secs")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let banned_at = find_field(fields, "banned_at")
        .and_then(parse_ts)
        .unwrap_or_else(Utc::now);
    let expires_at = match find_field(fields, "expires_at").and_then(parse_ts) {
        Some(ts) => ts,
        None => {
            log::warn!(
                "corrupt ban record at key {}: missing or invalid 'expires_at' field, ignoring record",
                key
            );
            return None;
        }
    };
    let is_manual = find_field(fields, "is_manual")
        .map(|v| v == "true")
        .unwrap_or(false);
    let reason = find_field(fields, "reason").unwrap_or("").to_string();
    Some(BanRecord {
        target,
        ban_times,
        duration: Duration::from_secs(duration_secs),
        banned_at,
        expires_at,
        is_manual,
        reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{BanStorage, QuotaStorage, Storage};

    const REDIS_URL: &str = "redis://127.0.0.1:6379/";

    // ========================================================================
    // 纯函数单元测试（不依赖 Redis 连接）
    // ========================================================================

    #[test]
    fn test_ban_key_ip() {
        let target = BanTarget::Ip("192.168.1.1".to_string());
        assert_eq!(ban_key(&target), "ban:ip:192.168.1.1");
    }

    #[test]
    fn test_ban_key_user_id() {
        let target = BanTarget::UserId("user123".to_string());
        assert_eq!(ban_key(&target), "ban:user:user123");
    }

    #[test]
    fn test_ban_key_mac() {
        let target = BanTarget::Mac("00:11:22:33:44:55".to_string());
        assert_eq!(ban_key(&target), "ban:mac:00:11:22:33:44:55");
    }

    #[test]
    fn test_ban_target_parts_all_variants() {
        let (kind, value) = ban_target_parts(&BanTarget::Ip("1.2.3.4".to_string()));
        assert_eq!(kind, "ip");
        assert_eq!(value, "1.2.3.4");

        let (kind, value) = ban_target_parts(&BanTarget::UserId("u1".to_string()));
        assert_eq!(kind, "user");
        assert_eq!(value, "u1");

        let (kind, value) = ban_target_parts(&BanTarget::Mac("aa:bb".to_string()));
        assert_eq!(kind, "mac");
        assert_eq!(value, "aa:bb");
    }

    #[test]
    fn test_quota_key_basic() {
        assert_eq!(quota_key("user1", "resource1"), "quota:user1:resource1");
    }

    #[test]
    fn test_quota_key_empty_parts() {
        assert_eq!(quota_key("", ""), "quota::");
    }

    #[test]
    fn test_find_field_found() {
        let fields = vec![
            "key1".to_string(),
            "value1".to_string(),
            "key2".to_string(),
            "value2".to_string(),
        ];
        assert_eq!(find_field(&fields, "key1"), Some("value1"));
        assert_eq!(find_field(&fields, "key2"), Some("value2"));
    }

    #[test]
    fn test_find_field_missing() {
        let fields = vec!["key1".to_string(), "value1".to_string()];
        assert_eq!(find_field(&fields, "nonexistent"), None);
    }

    #[test]
    fn test_find_field_empty() {
        let fields: Vec<String> = vec![];
        assert_eq!(find_field(&fields, "any"), None);
    }

    #[test]
    fn test_find_field_odd_length() {
        // 奇数长度（最后一个 key 没有 value）
        let fields = vec!["key1".to_string(), "value1".to_string(), "key2".to_string()];
        assert_eq!(find_field(&fields, "key2"), None);
    }

    #[test]
    fn test_parse_ts_valid() {
        let ts = "2026-01-01T00:00:00Z";
        let parsed = parse_ts(ts);
        assert!(parsed.is_some());
    }

    #[test]
    fn test_parse_ts_invalid() {
        assert!(parse_ts("not-a-timestamp").is_none());
        assert!(parse_ts("").is_none());
    }

    #[test]
    fn test_parse_ban_record_full() {
        let fields = vec![
            "target".to_string(),
            r#"{"type":"ip","value":"1.2.3.4"}"#.to_string(),
            "ban_times".to_string(),
            "3".to_string(),
            "duration_secs".to_string(),
            "3600".to_string(),
            "banned_at".to_string(),
            "2026-01-01T00:00:00Z".to_string(),
            "expires_at".to_string(),
            "2026-01-01T01:00:00Z".to_string(),
            "is_manual".to_string(),
            "true".to_string(),
            "reason".to_string(),
            "abuse".to_string(),
        ];
        let record = parse_ban_record("ban:ip:1.2.3.4", &fields);
        assert!(record.is_some());
        let record = record.unwrap();
        assert_eq!(record.ban_times, 3);
        assert_eq!(record.duration, Duration::from_secs(3600));
        assert!(record.is_manual);
        assert_eq!(record.reason, "abuse");
    }

    #[test]
    fn test_parse_ban_record_missing_expires_at_returns_none() {
        let fields = vec![
            "target".to_string(),
            r#"{"type":"ip","value":"1.2.3.4"}"#.to_string(),
            "ban_times".to_string(),
            "1".to_string(),
            "duration_secs".to_string(),
            "60".to_string(),
        ];
        let record = parse_ban_record("ban:ip:1.2.3.4", &fields);
        assert!(
            record.is_none(),
            "missing expires_at should return None (safety-critical)"
        );
    }

    #[test]
    fn test_parse_ban_record_invalid_expires_at_returns_none() {
        let fields = vec![
            "target".to_string(),
            r#"{"type":"ip","value":"1.2.3.4"}"#.to_string(),
            "expires_at".to_string(),
            "not-a-timestamp".to_string(),
        ];
        let record = parse_ban_record("ban:ip:1.2.3.4", &fields);
        assert!(record.is_none(), "invalid expires_at should return None");
    }

    #[test]
    fn test_parse_ban_record_missing_target_returns_none() {
        let fields = vec!["ban_times".to_string(), "1".to_string()];
        let record = parse_ban_record("ban:ip:1.2.3.4", &fields);
        assert!(record.is_none(), "missing target should return None");
    }

    #[test]
    fn test_parse_ban_record_invalid_target_json_returns_none() {
        let fields = vec![
            "target".to_string(),
            "not-json".to_string(),
            "expires_at".to_string(),
            "2026-01-01T01:00:00Z".to_string(),
        ];
        let record = parse_ban_record("ban:ip:1.2.3.4", &fields);
        assert!(record.is_none(), "invalid target JSON should return None");
    }

    #[test]
    fn test_parse_ban_record_default_values() {
        // 缺少 ban_times/duration_secs/is_manual/reason 时应有默认值
        let fields = vec![
            "target".to_string(),
            r#"{"type":"user","value":"u1"}"#.to_string(),
            "expires_at".to_string(),
            "2026-01-01T01:00:00Z".to_string(),
        ];
        let record = parse_ban_record("ban:user:u1", &fields);
        assert!(record.is_some());
        let record = record.unwrap();
        assert_eq!(record.ban_times, 0);
        assert_eq!(record.duration, Duration::from_secs(0));
        assert!(!record.is_manual);
        assert_eq!(record.reason, "");
    }

    #[test]
    fn test_parse_ban_record_is_manual_false_explicit() {
        let fields = vec![
            "target".to_string(),
            r#"{"type":"ip","value":"1.2.3.4"}"#.to_string(),
            "expires_at".to_string(),
            "2026-01-01T01:00:00Z".to_string(),
            "is_manual".to_string(),
            "false".to_string(),
        ];
        let record = parse_ban_record("ban:ip:1.2.3.4", &fields);
        assert!(record.is_some());
        assert!(
            !record.unwrap().is_manual,
            "explicit 'false' must map to false"
        );
    }

    #[test]
    fn test_parse_ban_record_is_manual_unknown_string_treated_as_false() {
        let fields = vec![
            "target".to_string(),
            r#"{"type":"ip","value":"1.2.3.4"}"#.to_string(),
            "expires_at".to_string(),
            "2026-01-01T01:00:00Z".to_string(),
            "is_manual".to_string(),
            "yes".to_string(),
        ];
        let record = parse_ban_record("ban:ip:1.2.3.4", &fields);
        assert!(record.is_some());
        assert!(
            !record.unwrap().is_manual,
            "any string other than 'true' must be false (safety: only explicit 'true' is manual)"
        );
    }

    // ========================================================================
    // From<redis::RedisError> for StorageError 转换测试
    // ========================================================================

    #[test]
    fn test_from_redis_error_authentication_failed_maps_to_authentication_error() {
        let err: redis::RedisError = (redis::ErrorKind::AuthenticationFailed, "auth failed").into();
        let storage_err: StorageError = err.into();
        match storage_err {
            StorageError::AuthenticationError(msg) => {
                assert!(
                    msg.contains("auth failed"),
                    "error message should contain original"
                );
            }
            other => panic!("expected AuthenticationError, got {other:?}"),
        }
    }

    #[test]
    fn test_from_redis_error_io_error_maps_to_connection_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let redis_err: redis::RedisError = io_err.into();
        let storage_err: StorageError = redis_err.into();
        match storage_err {
            StorageError::ConnectionError(msg) => {
                assert!(
                    msg.contains("refused"),
                    "error message should contain original"
                );
            }
            other => panic!("expected ConnectionError, got {other:?}"),
        }
    }

    #[test]
    fn test_from_redis_error_client_error_maps_to_connection_error() {
        let err: redis::RedisError = (redis::ErrorKind::ClientError, "client bug").into();
        let storage_err: StorageError = err.into();
        match storage_err {
            StorageError::ConnectionError(msg) => {
                assert!(
                    msg.contains("client bug"),
                    "error message should contain original"
                );
            }
            other => panic!("expected ConnectionError, got {other:?}"),
        }
    }

    #[test]
    fn test_from_redis_error_other_kind_maps_to_query_error() {
        // 用 ResponseError 测试 `_` 分支
        let err: redis::RedisError = (redis::ErrorKind::ResponseError, "bad response").into();
        let storage_err: StorageError = err.into();
        match storage_err {
            StorageError::QueryError(msg) => {
                assert!(
                    msg.contains("bad response"),
                    "error message should contain original"
                );
            }
            other => panic!("expected QueryError, got {other:?}"),
        }
    }

    #[test]
    fn test_from_redis_error_type_error_maps_to_query_error() {
        // 验证非 AuthenticationFailed/IoError/ClientError 的另一种 ErrorKind 也走 QueryError
        let err: redis::RedisError = (redis::ErrorKind::TypeError, "type mismatch").into();
        let storage_err: StorageError = err.into();
        match storage_err {
            StorageError::QueryError(msg) => {
                assert!(msg.contains("type mismatch"));
            }
            other => panic!("expected QueryError, got {other:?}"),
        }
    }

    // ========================================================================
    // Lua 脚本常量内容验证（不变量测试，防止脚本被意外修改）
    // ========================================================================

    #[test]
    fn test_check_and_consume_script_contains_expected_redis_commands() {
        assert!(!CHECK_AND_CONSUME_SCRIPT.is_empty());
        assert!(
            CHECK_AND_CONSUME_SCRIPT.contains("HGET"),
            "must read consumed value"
        );
        assert!(
            CHECK_AND_CONSUME_SCRIPT.contains("HSET"),
            "must write new consumed value"
        );
        assert!(
            CHECK_AND_CONSUME_SCRIPT.contains("EXPIRE"),
            "must set TTL on quota key"
        );
        assert!(
            CHECK_AND_CONSUME_SCRIPT.contains("tonumber"),
            "must parse numeric ARGV"
        );
        // 关键不变量：脚本必须返回 {1, ...} 表示允许，{0, ...} 表示拒绝
        assert!(
            CHECK_AND_CONSUME_SCRIPT.contains("return {1,"),
            "must return 1 on allow"
        );
        assert!(
            CHECK_AND_CONSUME_SCRIPT.contains("return {0,"),
            "must return 0 on reject"
        );
    }

    #[test]
    fn test_reset_quota_script_contains_expected_redis_commands() {
        assert!(!RESET_QUOTA_SCRIPT.is_empty());
        assert!(
            RESET_QUOTA_SCRIPT.contains("DEL"),
            "must delete existing key first"
        );
        assert!(
            RESET_QUOTA_SCRIPT.contains("HSET"),
            "must write fresh hash fields"
        );
        assert!(RESET_QUOTA_SCRIPT.contains("EXPIRE"), "must set TTL");
        assert!(
            RESET_QUOTA_SCRIPT.contains("window"),
            "must accept window parameter"
        );
    }

    #[test]
    fn test_increment_ban_script_contains_expected_redis_commands() {
        assert!(!INCREMENT_BAN_SCRIPT.is_empty());
        assert!(
            INCREMENT_BAN_SCRIPT.contains("EXISTS"),
            "must check key existence first"
        );
        assert!(
            INCREMENT_BAN_SCRIPT.contains("HINCRBY"),
            "must atomically increment ban_times"
        );
        // 关键不变量：key 不存在时返回 0
        assert!(
            INCREMENT_BAN_SCRIPT.contains("return 0"),
            "must return 0 when key missing"
        );
    }

    #[test]
    fn test_save_ban_script_contains_expected_redis_commands() {
        assert!(!SAVE_BAN_SCRIPT.is_empty());
        assert!(
            SAVE_BAN_SCRIPT.contains("HSET"),
            "must write all ban fields"
        );
        assert!(
            SAVE_BAN_SCRIPT.contains("EXPIRE"),
            "must set TTL based on duration_secs"
        );
        // 关键不变量：TTL > 0 时才设置 EXPIRE（永久 ban 不应设置 TTL）
        assert!(
            SAVE_BAN_SCRIPT.contains("if ttl > 0"),
            "must guard EXPIRE with ttl check"
        );
    }

    // ========================================================================
    // 构造函数错误路径测试（不依赖 Redis 连接）
    // ========================================================================

    #[tokio::test]
    async fn test_redis_storage_new_rejects_empty_url() {
        // Client::open("") 失败于 URL 解析阶段，不需要 Redis 服务器
        let result = RedisStorage::new("").await;
        let err = match result {
            Ok(_) => panic!("empty URL must return error at parse stage"),
            Err(e) => e,
        };
        // 验证错误类型：URL 解析错误通常映射为 QueryError（因为不在 AuthenticationFailed/IoError/ClientError 列表）
        assert!(
            !matches!(err, StorageError::AuthenticationError(_)),
            "URL parse error should not be AuthenticationError"
        );
    }

    #[tokio::test]
    async fn test_redis_storage_new_rejects_malformed_url() {
        // 无效 URL scheme 应在 Client::open 阶段失败
        let result = RedisStorage::new("not-a-redis-url-scheme://invalid").await;
        assert!(
            result.is_err(),
            "malformed URL must return error at parse stage"
        );
    }

    async fn create_storage() -> RedisStorage {
        RedisStorage::new(REDIS_URL)
            .await
            .expect("failed to connect to Redis at 127.0.0.1:6379")
    }

    /// Deletes a raw Redis key (test cleanup helper).
    async fn del_key(storage: &RedisStorage, key: &str) {
        let mut conn = storage.conn();
        let _: () = redis::cmd("DEL")
            .arg(key)
            .query_async(&mut conn)
            .await
            .expect("DEL should succeed");
    }

    #[tokio::test]
    #[ignore = "requires a running Redis server at 127.0.0.1:6379"]
    async fn test_redis_storage_set_get() {
        let storage = create_storage().await;
        let key = "test:redis:set_get";
        del_key(&storage, key).await;
        storage.set(key, "hello", None).await.unwrap();
        let v = storage.get(key).await.unwrap();
        assert_eq!(v, Some("hello".to_string()));
        del_key(&storage, key).await;
    }

    #[tokio::test]
    #[ignore = "requires a running Redis server at 127.0.0.1:6379"]
    async fn test_redis_storage_delete() {
        let storage = create_storage().await;
        let key = "test:redis:delete";
        storage.set(key, "v", None).await.unwrap();
        assert!(storage.get(key).await.unwrap().is_some());
        del_key(&storage, key).await;
        let v = storage.get(key).await.unwrap();
        assert!(v.is_none(), "key should be gone after delete");
    }

    #[tokio::test]
    #[ignore = "requires a running Redis server at 127.0.0.1:6379"]
    async fn test_redis_storage_exists() {
        let storage = create_storage().await;
        let key = "test:redis:exists";
        del_key(&storage, key).await;
        assert!(!storage.exists(key).await.unwrap(), "key should not exist");
        storage.set(key, "v", None).await.unwrap();
        assert!(
            storage.exists(key).await.unwrap(),
            "key should exist after set"
        );
        del_key(&storage, key).await;
        assert!(
            !storage.exists(key).await.unwrap(),
            "key should not exist after delete"
        );
    }

    #[tokio::test]
    #[ignore = "requires a running Redis server at 127.0.0.1:6379"]
    async fn test_redis_storage_ttl() {
        let storage = create_storage().await;
        let key = "test:redis:ttl";
        storage.set(key, "v", Some(1)).await.unwrap();
        let v = storage.get(key).await.unwrap();
        assert_eq!(v, Some("v".to_string()));
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let v = storage.get(key).await.unwrap();
        assert!(v.is_none(), "key should expire after TTL");
    }

    #[tokio::test]
    #[ignore = "requires a running Redis server at 127.0.0.1:6379"]
    async fn test_redis_ban_storage_add_check() {
        let storage = create_storage().await;
        let target = BanTarget::Ip("203.0.113.7".to_string());
        let _: () = storage.remove_ban(&target).await.unwrap();
        let now = Utc::now();
        let rec = BanRecord {
            target: target.clone(),
            ban_times: 2,
            duration: Duration::from_secs(300),
            banned_at: now,
            expires_at: now + chrono::Duration::seconds(300),
            is_manual: true,
            reason: "abuse".to_string(),
        };
        storage.save(&rec).await.unwrap();
        let found = storage
            .is_banned(&target)
            .await
            .unwrap()
            .expect("ban should exist");
        assert_eq!(found.ban_times, 2);
        assert!(found.is_manual);
        assert_eq!(found.reason, "abuse");
        assert_eq!(found.target, target);
        // increment should bump to 3
        let n = storage.increment_ban_times(&target).await.unwrap();
        assert_eq!(n, 3);
        let n = storage.get_ban_times(&target).await.unwrap();
        assert_eq!(n, 3);
        let _: () = storage.remove_ban(&target).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires a running Redis server at 127.0.0.1:6379"]
    async fn test_redis_ban_storage_remove() {
        let storage = create_storage().await;
        let target = BanTarget::UserId("test_redis_user_remove".to_string());
        let now = Utc::now();
        let rec = BanRecord {
            target: target.clone(),
            ban_times: 1,
            duration: Duration::from_secs(60),
            banned_at: now,
            expires_at: now + chrono::Duration::seconds(60),
            is_manual: false,
            reason: "r".to_string(),
        };
        storage.save(&rec).await.unwrap();
        assert!(storage.is_banned(&target).await.unwrap().is_some());
        let _: () = storage.remove_ban(&target).await.unwrap();
        assert!(storage.is_banned(&target).await.unwrap().is_none());
        // get_ban_times on removed target -> 0
        assert_eq!(storage.get_ban_times(&target).await.unwrap(), 0);
    }

    #[tokio::test]
    #[ignore = "requires a running Redis server at 127.0.0.1:6379"]
    async fn test_redis_quota_storage_consume() {
        let storage = create_storage().await;
        let user = "test_redis_quota_user";
        let res = "consume";
        // reset to a clean state with limit 100
        storage
            .reset(user, res, 100, Duration::from_secs(60))
            .await
            .unwrap();
        let r = storage
            .consume(user, res, 30, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(r.allowed, "first consume within limit should be allowed");
        assert_eq!(r.remaining, 70);
        let r2 = storage
            .consume(user, res, 80, 100, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!r2.allowed, "30+80=110 > 100 should be rejected");
        assert_eq!(r2.remaining, 70, "remaining unchanged after rejection");
        del_key(&storage, &format!("quota:{}:{}", user, res)).await;
    }

    #[tokio::test]
    #[ignore = "requires a running Redis server at 127.0.0.1:6379"]
    async fn test_redis_quota_storage_reset() {
        let storage = create_storage().await;
        let user = "test_redis_quota_user2";
        let res = "reset";
        storage
            .consume(user, res, 50, 1000, Duration::from_secs(60))
            .await
            .unwrap();
        storage
            .reset(user, res, 500, Duration::from_secs(120))
            .await
            .unwrap();
        let q = storage
            .get_quota(user, res)
            .await
            .unwrap()
            .expect("quota should exist after reset");
        assert_eq!(q.consumed, 0, "reset should zero consumed");
        assert_eq!(q.limit, 500);
        del_key(&storage, &format!("quota:{}:{}", user, res)).await;
    }

    #[tokio::test]
    #[ignore = "requires a running Redis server at 127.0.0.1:6379"]
    async fn test_redis_lua_check_and_consume() {
        let storage = create_storage().await;
        let user = "test_redis_lua_user";
        let res = "check";
        storage
            .reset(user, res, 3, Duration::from_secs(60))
            .await
            .unwrap();
        // consume 1 three times -> all allowed, consumed climbs to 3
        for i in 1..=3u64 {
            let r = storage
                .consume(user, res, 1, 3, Duration::from_secs(60))
                .await
                .unwrap();
            assert!(r.allowed, "consume #{} should be allowed", i);
            assert_eq!(r.remaining, 3 - i);
        }
        // 4th consume -> rejected (3 + 1 = 4 > 3)
        let r = storage
            .consume(user, res, 1, 3, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!r.allowed, "consume over limit should be rejected");
        assert_eq!(r.remaining, 0);
        let q = storage
            .get_quota(user, res)
            .await
            .unwrap()
            .expect("quota should exist");
        assert_eq!(q.consumed, 3, "rejected consume must not increment");
        del_key(&storage, &format!("quota:{}:{}", user, res)).await;
    }
}
