use crate::error::StorageError;
use crate::storage::Storage;
use async_trait::async_trait;
use oxcache::backend::CacheBackend;
use oxcache::error::CacheError;
use std::sync::Arc;
use std::time::Duration;

pub struct CacheStorage {
    backend: Arc<dyn CacheBackend>,
}

impl CacheStorage {
    pub fn new(backend: Arc<dyn CacheBackend>) -> Self {
        Self { backend }
    }
}

fn map_error(e: CacheError) -> StorageError {
    match e {
        CacheError::Connection(_) => StorageError::ConnectionError(e.to_string()),
        CacheError::Timeout(_) => StorageError::ConnectionError(e.to_string()),
        _ => StorageError::QueryError(e.to_string()),
    }
}

#[async_trait]
impl Storage for CacheStorage {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        self.backend
            .get(key)
            .await
            .map(|opt| opt.map(|b| String::from_utf8_lossy(&b).to_string()))
            .map_err(map_error)
    }

    async fn set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StorageError> {
        let ttl = ttl.map(Duration::from_secs);
        self.backend
            .set(key, value.as_bytes().to_vec(), ttl)
            .await
            .map_err(map_error)
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.backend.delete(key).await.map_err(map_error)
    }
}
