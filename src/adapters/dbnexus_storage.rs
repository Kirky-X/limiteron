// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! DBNexusStorageAdapter - DBNexus-based implementation of Storage trait
//!
//! This adapter provides a complete Storage trait implementation using DBNexus
//! for all database operations. It handles key-value storage with optional TTL.

use crate::dbnexus_entities::{KeyValueActiveModel, KeyValueEntity, KeyValueModel};
use crate::error::StorageError;
use crate::storage::Storage;
use async_trait::async_trait;
use chrono::Utc;
use dbnexus::{DbPool, Session};
use sea_orm::entity::prelude::*;
use std::sync::Arc;

/// DBNexus-based storage adapter
pub struct DBNexusStorageAdapter {
    pool: Arc<DbPool>,
}

impl DBNexusStorageAdapter {
    /// Create a new DBNexusStorageAdapter
    pub fn new(pool: Arc<DbPool>) -> Self {
        Self { pool }
    }

    /// Get a session from the pool
    async fn get_session(&self) -> Result<Session, StorageError> {
        self.pool
            .get_session("admin")
            .await
            .map_err(|e| StorageError::ConnectionError(e.to_string()))
    }

    /// Get a connection from the session (for direct sea-orm operations)
    fn get_conn(session: &Session) -> Result<&DatabaseConnection, StorageError> {
        session
            .connection()
            .map_err(|e| StorageError::ConnectionError(e.to_string()))
    }
}

#[async_trait]
impl Storage for DBNexusStorageAdapter {
    /// Get a value by key
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        let session = self.get_session().await?;
        let conn = Self::get_conn(&session)?;

        // Find the key-value record
        match KeyValueEntity::find_by_id(key.to_string()).one(conn).await {
            Ok(Some(model)) => {
                // Check if expired
                if let Some(expires_at) = model.expires_at {
                    if expires_at < Utc::now() {
                        // Expired - delete and return None
                        let _ = KeyValueEntity::delete_by_id(key.to_string())
                            .exec(conn)
                            .await;
                        return Ok(None);
                    }
                }
                Ok(Some(model.value))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::QueryError(e.to_string())),
        }
    }

    /// Set a value with optional TTL (in seconds)
    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError> {
        let session = self.get_session().await?;
        let conn = Self::get_conn(&session)?;
        let now = Utc::now();

        // Calculate expiration time
        let expires_at = ttl.map(|seconds| now + chrono::Duration::seconds(seconds as i64));

        // Check if record exists
        let exists = KeyValueEntity::find_by_id(key.to_string())
            .one(conn)
            .await
            .is_ok();

        let model = KeyValueModel {
            key: key.to_string(),
            value: value.to_string(),
            expires_at,
            created_at: now,
            updated_at: now,
        };

        if exists {
            // Update existing record using sea-orm ActiveModel
            let active_model: KeyValueActiveModel = model.into();
            active_model.save(conn).await.map_err(|e| {
                StorageError::QueryError(format!("Failed to update key-value: {}", e))
            })?;
        } else {
            // Insert new record
            let active_model: KeyValueActiveModel = model.into();
            active_model.insert(conn).await.map_err(|e| {
                StorageError::QueryError(format!("Failed to insert key-value: {}", e))
            })?;
        }

        Ok(())
    }

    /// Delete a value by key
    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let session = self.get_session().await?;
        let conn = Self::get_conn(&session)?;

        KeyValueEntity::delete_by_id(key.to_string())
            .exec(conn)
            .await
            .map_err(|e| StorageError::QueryError(format!("Failed to delete key: {}", e)))?;

        Ok(())
    }
}
