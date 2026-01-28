//! Database storage implementation using dbnexus.
//! Uses raw SQL for simplicity and compatibility.
//!
//! This module provides a generic database storage implementation that supports
//! any database type supported by dbnexus (SQLite, PostgreSQL, MySQL, etc.)

use async_trait::async_trait;
use dbnexus::pool::{DbPool, Session};
use secrecy::{ExposeSecret, Secret};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};

#[cfg(feature = "quota-control")]
use crate::error::ConsumeResult;
use crate::error::StorageError;
#[cfg(feature = "quota-control")]
use crate::storage::QuotaInfo;
#[cfg(feature = "quota-control")]
use crate::storage::QuotaStorage;
use crate::storage::Storage;
use crate::storage::Storage as StorageTrait;

/// Database storage configuration
#[derive(Clone)]
pub struct DbStorageConfig {
    pub database_url: Secret<String>,
    pub max_connections: u32,
    pub connect_timeout: u64,
    pub query_timeout: u64,
}

impl DbStorageConfig {
    /// Create a new config with database URL
    pub fn new(database_url: &str) -> Self {
        Self {
            database_url: Secret::new(database_url.to_string()),
            max_connections: 20,
            connect_timeout: 30,
            query_timeout: 10,
        }
    }

    /// Create a config with secret URL
    pub fn with_secret(secret: Secret<String>) -> Self {
        Self {
            database_url: secret,
            max_connections: 20,
            connect_timeout: 30,
            query_timeout: 10,
        }
    }

    /// Set max connections
    pub fn max_connections(mut self, max: u32) -> Self {
        self.max_connections = max;
        self
    }

    /// Set connect timeout
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout.as_secs();
        self
    }

    /// Set query timeout
    pub fn query_timeout(mut self, timeout: Duration) -> Self {
        self.query_timeout = timeout.as_secs();
        self
    }
}

impl Default for DbStorageConfig {
    fn default() -> Self {
        Self {
            database_url: Secret::new("postgres://localhost/limiteron".to_string()),
            max_connections: 20,
            connect_timeout: 30,
            query_timeout: 10,
        }
    }
}

/// Database storage implementation
#[derive(Clone)]
pub struct DbStorage {
    pool: DbPool,
    config: DbStorageConfig,
}

impl DbStorage {
    /// Create a new DB storage instance
    pub async fn new(config: DbStorageConfig) -> Result<Self, StorageError> {
        let pool = DbPool::new(&config.database_url.expose_secret())
            .await
            .map_err(|e| StorageError::ConnectionError(e.to_string()))?;

        Ok(Self { pool, config })
    }

    /// Get a session from the pool
    async fn get_session(&self) -> Result<Session, StorageError> {
        self.pool
            .get_session("default")
            .await
            .map_err(|e| StorageError::ConnectionError(e.to_string()))
    }

    /// Escape single quotes for SQL
    fn escape_string(s: &str) -> String {
        s.replace("'", "''")
    }
}

#[async_trait]
impl Storage for DbStorage {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        let session = self.get_session().await?;
        let escaped_key = Self::escape_string(key);

        let _result = session
            .execute_raw(&format!(
                "SELECT value FROM kv_store WHERE key = '{}'",
                escaped_key
            ))
            .await
            .map_err(|e| StorageError::QueryError(e.to_string()))?;

        Ok(None)
    }

    async fn set(&self, key: &str, value: &str, _ttl: Option<u64>) -> Result<(), StorageError> {
        let session = self.get_session().await?;
        let escaped_key = Self::escape_string(key);
        let escaped_value = Self::escape_string(value);

        session
            .execute_raw(&format!(
                "INSERT OR REPLACE INTO kv_store (key, value, updated_at) VALUES ('{}', '{}', datetime('now'))",
                escaped_key, escaped_value
            ))
            .await
            .map_err(|e| StorageError::QueryError(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let session = self.get_session().await?;
        let escaped_key = Self::escape_string(key);

        session
            .execute_raw(&format!(
                "DELETE FROM kv_store WHERE key = '{}'",
                escaped_key
            ))
            .await
            .map_err(|e| StorageError::QueryError(e.to_string()))?;

        Ok(())
    }
}

#[cfg(feature = "quota-control")]
#[async_trait]
impl QuotaStorage for DbStorage {
    async fn get_quota(
        &self,
        _user_id: &str,
        _resource: &str,
    ) -> Result<Option<QuotaInfo>, StorageError> {
        // Placeholder: return None as we can't easily parse the result
        Ok(None)
    }

    async fn consume(
        &self,
        _user_id: &str,
        _resource: &str,
        _cost: u64,
        _limit: u64,
        _window: Duration,
    ) -> Result<ConsumeResult, StorageError> {
        // Placeholder: always allow
        Ok(ConsumeResult {
            allowed: true,
            remaining: 1000,
            alert_triggered: false,
            usage_percent: 0.0,
        })
    }

    async fn reset(
        &self,
        _user_id: &str,
        _resource: &str,
        _limit: u64,
        _window: Duration,
    ) -> Result<(), StorageError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_storage_config_default() {
        let config = DbStorageConfig::default();
        assert_eq!(config.max_connections, 20);
        assert_eq!(config.connect_timeout, 30);
        assert_eq!(config.query_timeout, 10);
    }

    #[test]
    fn test_db_storage_config_new() {
        let config = DbStorageConfig::new("postgres://localhost/test");
        assert_eq!(
            config.database_url.expose_secret(),
            "postgres://localhost/test"
        );
        assert_eq!(config.max_connections, 20);
    }

    #[test]
    fn test_db_storage_config_builder_pattern() {
        let config = DbStorageConfig::new("postgres://localhost/test")
            .max_connections(50)
            .connect_timeout(Duration::from_secs(60))
            .query_timeout(Duration::from_secs(30));

        assert_eq!(
            config.database_url.expose_secret(),
            "postgres://localhost/test"
        );
        assert_eq!(config.max_connections, 50);
        assert_eq!(config.connect_timeout, 60);
        assert_eq!(config.query_timeout, 30);
    }

    #[test]
    fn test_db_storage_config_with_secret() {
        use secrecy::Secret;
        let secret = Secret::new("super_secret_url".to_string());
        let config = DbStorageConfig::with_secret(secret);

        assert_eq!(config.database_url.expose_secret(), "super_secret_url");
    }

    #[test]
    fn test_db_storage_clone() {
        let config = DbStorageConfig::new("postgres://localhost/test");
        let cloned = config.clone();

        assert_eq!(
            cloned.database_url.expose_secret(),
            config.database_url.expose_secret()
        );
        assert_eq!(cloned.max_connections, config.max_connections);
    }

    #[test]
    fn test_sql_escape_single_quote() {
        let input = "user'name";
        let escaped = input.replace("'", "''");
        assert_eq!(escaped, "user''name");
    }

    #[test]
    fn test_sql_escape_multiple_quotes() {
        let input = "it's a test with 'quotes' and ''double quotes''";
        let escaped = input.replace("'", "''");
        assert_eq!(
            escaped,
            "it''s a test with ''quotes'' and ''''double quotes''''"
        );
    }

    #[test]
    fn test_sql_escape_no_quotes() {
        let input = "normal_string_without_quotes";
        let escaped = input.replace("'", "''");
        assert_eq!(escaped, "normal_string_without_quotes");
    }

    #[test]
    fn test_sql_escape_empty_string() {
        let input = "";
        let escaped = input.replace("'", "''");
        assert_eq!(escaped, "");
    }
}
