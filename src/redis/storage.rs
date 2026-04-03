//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! Redis storage implementation for the Storage trait.
//!
//! Provides distributed key-value storage with TTL support
//! using Redis as the backend.

use crate::error::StorageError;
use crate::storage::Storage;
use async_trait::async_trait;
use redis::Client;
use std::sync::Arc;

/// Redis storage implementation
///
/// Implements the `Storage` trait using Redis as the backend.
/// Supports distributed key-value storage with TTL (time-to-live).
///
/// # Thread Safety
///
/// This struct implements `Send + Sync` and can be safely shared
/// across threads using `Arc`.
///
/// # Connection Management
///
/// Uses `redis::Client` which maintains a connection pool internally.
/// Each operation gets a connection from the pool.
///
/// # Example
///
/// ```rust,ignore
/// use limiteron::redis::RedisStorage;
/// use limiteron::storage::Storage;
///
/// let storage = RedisStorage::new("redis://127.0.0.1:6379/")?;
/// storage.set("key", "value", Some(60)).await?;
/// let value = storage.get("key").await?;
/// ```
pub struct RedisStorage {
    /// Redis client connection
    client: Arc<Client>,
}

impl RedisStorage {
    /// Create a new Redis storage instance
    ///
    /// # Arguments
    /// * `client` - Redis client instance
    ///
    /// # Returns
    /// * `RedisStorage` - Storage instance
    pub fn new(client: Client) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    /// Create a new Redis storage instance from connection string
    ///
    /// # Arguments
    /// * `connection_string` - Redis connection string (e.g., "redis://127.0.0.1:6379/")
    ///
    /// # Returns
    /// * `Ok(RedisStorage)` - Success
    /// * `Err(StorageError)` - Connection error
    pub fn from_connection_string(connection_string: &str) -> Result<Self, StorageError> {
        let client = Client::open(connection_string).map_err(|e| {
            StorageError::ConnectionError(format!("Failed to create Redis client: {}", e))
        })?;

        Ok(Self::new(client))
    }

    /// Get the Redis client reference
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Get an async connection to Redis
    async fn get_connection(&self) -> Result<redis::aio::MultiplexedConnection, StorageError> {
        self.client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| StorageError::ConnectionError(format!("Failed to get connection: {}", e)))
    }
}

#[async_trait]
#[allow(clippy::let_unit_value)]
impl Storage for RedisStorage {
    /// Get value by key
    ///
    /// # Arguments
    /// * `key` - Storage key
    ///
    /// # Returns
    /// * `Ok(Some(String))` - Value found
    /// * `Ok(None)` - Key not found or expired
    /// * `Err(StorageError)` - Query error
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        let mut conn = self.get_connection().await?;

        let result: Option<String> = redis::cmd("GET")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| StorageError::QueryError(format!("Failed to get key '{}': {}", key, e)))?;

        Ok(result)
    }

    /// Set value with optional TTL
    ///
    /// # Arguments
    /// * `key` - Storage key
    /// * `value` - Value to store
    /// * `ttl` - Time-to-live in seconds (None = no expiration)
    ///
    /// # Returns
    /// * `Ok(())` - Success
    /// * `Err(StorageError)` - Query error
    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError> {
        let mut conn = self.get_connection().await?;

        match ttl {
            Some(ttl_seconds) => {
                // Use SET with EX parameter for atomic set with expiration
                let _: () = redis::cmd("SET")
                    .arg(key)
                    .arg(value)
                    .arg("EX")
                    .arg(ttl_seconds)
                    .query_async(&mut conn)
                    .await
                    .map_err(|e| {
                        StorageError::QueryError(format!(
                            "Failed to set key '{}' with TTL: {}",
                            key, e
                        ))
                    })?;
            }
            None => {
                let _: () = redis::cmd("SET")
                    .arg(key)
                    .arg(value)
                    .query_async(&mut conn)
                    .await
                    .map_err(|e| {
                        StorageError::QueryError(format!("Failed to set key '{}': {}", key, e))
                    })?;
            }
        }

        Ok(())
    }

    /// Delete value by key
    ///
    /// # Arguments
    /// * `key` - Storage key
    ///
    /// # Returns
    /// * `Ok(())` - Success
    /// * `Err(StorageError)` - Query error
    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let mut conn = self.get_connection().await?;

        let _: () = redis::cmd("DEL")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| {
                StorageError::QueryError(format!("Failed to delete key '{}': {}", key, e))
            })?;

        Ok(())
    }
}

impl std::fmt::Debug for RedisStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisStorage")
            .field("client", &"<Redis Client>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redis_storage_debug() {
        // Test debug output without actual connection
        let client = Client::open("redis://127.0.0.1:6379/").unwrap();
        let storage = RedisStorage::new(client);
        let debug_str = format!("{:?}", storage);
        assert!(debug_str.contains("RedisStorage"));
    }

    #[test]
    fn test_redis_storage_from_connection_string() {
        let result = RedisStorage::from_connection_string("redis://127.0.0.1:6379/");
        assert!(result.is_ok());
    }

    #[test]
    fn test_redis_storage_from_invalid_connection_string() {
        let result = RedisStorage::from_connection_string("invalid://url");
        assert!(result.is_err());
        match result {
            Err(StorageError::ConnectionError(_)) => {}
            _ => panic!("Expected ConnectionError"),
        }
    }
}
