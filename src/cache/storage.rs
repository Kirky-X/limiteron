// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
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

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use oxcache::backend::interface::{BackendKind, CacheConnector, CacheReader, CacheWriter};
    use oxcache::error::CacheError;
    use std::sync::Mutex;

    /// Mock CacheBackend for testing CacheStorage.
    ///
    /// `get_factory` produces the `get` result on each call. `write_factory`
    /// produces errors for failing write (set/delete) calls.
    #[allow(clippy::type_complexity)]
    struct MockBackend {
        get_factory: Mutex<Box<dyn Fn() -> Result<Option<Vec<u8>>, CacheError> + Send + Sync>>,
        write_factory: Mutex<Option<Box<dyn Fn() -> CacheError + Send + Sync>>>,
    }

    impl MockBackend {
        fn new_with_value(value: Option<Vec<u8>>) -> Self {
            let v = value.clone();
            Self {
                get_factory: Mutex::new(Box::new(move || Ok(v.clone()))),
                write_factory: Mutex::new(None),
            }
        }

        fn new_failing_get<F>(err_factory: F) -> Self
        where
            F: Fn() -> CacheError + Send + Sync + 'static,
        {
            Self {
                get_factory: Mutex::new(Box::new(move || Err(err_factory()))),
                write_factory: Mutex::new(None),
            }
        }

        fn new_failing_writes<F>(err_factory: F) -> Self
        where
            F: Fn() -> CacheError + Send + Sync + 'static,
        {
            Self {
                get_factory: Mutex::new(Box::new(|| Ok(None))),
                write_factory: Mutex::new(Some(Box::new(err_factory))),
            }
        }
    }

    #[async_trait]
    impl CacheReader for MockBackend {
        async fn get(&self, _key: &str) -> Result<Option<Vec<u8>>, CacheError> {
            let factory = self.get_factory.lock().unwrap();
            factory()
        }
        async fn exists(&self, _key: &str) -> Result<bool, CacheError> {
            Ok(true)
        }
        async fn ttl(&self, _key: &str) -> Result<Option<Duration>, CacheError> {
            Ok(None)
        }
        async fn len(&self) -> Result<u64, CacheError> {
            Ok(0)
        }
        async fn capacity(&self) -> Result<u64, CacheError> {
            Ok(100)
        }
        #[allow(clippy::disallowed_types)]
        async fn stats(&self) -> Result<std::collections::HashMap<String, String>, CacheError> {
            Ok(std::collections::HashMap::new())
        }
    }

    #[async_trait]
    impl CacheWriter for MockBackend {
        async fn set(
            &self,
            _key: &str,
            _value: Vec<u8>,
            _ttl: Option<Duration>,
        ) -> Result<(), CacheError> {
            let factory = self.write_factory.lock().unwrap();
            match factory.as_ref() {
                Some(f) => Err(f()),
                None => Ok(()),
            }
        }
        async fn delete(&self, _key: &str) -> Result<(), CacheError> {
            let factory = self.write_factory.lock().unwrap();
            match factory.as_ref() {
                Some(f) => Err(f()),
                None => Ok(()),
            }
        }
        async fn clear(&self) -> Result<(), CacheError> {
            Ok(())
        }
        async fn expire(&self, _key: &str, _ttl: Duration) -> Result<bool, CacheError> {
            Ok(true)
        }
    }

    #[async_trait]
    impl CacheConnector for MockBackend {
        async fn health_check(&self) -> Result<(), CacheError> {
            Ok(())
        }
        async fn shutdown(&self) {}
        fn backend_kind(&self) -> BackendKind {
            BackendKind::Mock
        }
    }

    #[tokio::test]
    async fn test_cache_storage_get_returns_value() {
        let backend = Arc::new(MockBackend::new_with_value(Some(b"hello".to_vec())));
        let storage = CacheStorage::new(backend);
        let val = storage.get("k").await.unwrap();
        assert_eq!(val, Some("hello".to_string()));
    }

    #[tokio::test]
    async fn test_cache_storage_get_returns_none() {
        let backend = Arc::new(MockBackend::new_with_value(None));
        let storage = CacheStorage::new(backend);
        let val = storage.get("missing").await.unwrap();
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn test_cache_storage_get_maps_connection_error() {
        let backend = Arc::new(MockBackend::new_failing_get(|| {
            CacheError::Connection("net down".to_string())
        }));
        let storage = CacheStorage::new(backend);
        let err = storage.get("k").await.unwrap_err();
        match err {
            StorageError::ConnectionError(msg) => assert!(msg.contains("net down")),
            other => panic!("expected ConnectionError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_cache_storage_get_maps_timeout_error() {
        let backend = Arc::new(MockBackend::new_failing_get(|| {
            CacheError::Timeout("5s".to_string())
        }));
        let storage = CacheStorage::new(backend);
        let err = storage.get("k").await.unwrap_err();
        match err {
            StorageError::ConnectionError(msg) => assert!(msg.contains("5s")),
            other => panic!("expected ConnectionError for timeout, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_cache_storage_get_maps_other_error_to_query_error() {
        let backend = Arc::new(MockBackend::new_failing_get(|| {
            CacheError::NotFound("absent".to_string())
        }));
        let storage = CacheStorage::new(backend);
        let err = storage.get("k").await.unwrap_err();
        match err {
            StorageError::QueryError(msg) => assert!(msg.contains("absent")),
            other => panic!("expected QueryError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_cache_storage_set_success() {
        let backend = Arc::new(MockBackend::new_with_value(None));
        let storage = CacheStorage::new(backend);
        storage.set("k", "v", None).await.unwrap();
    }

    #[tokio::test]
    async fn test_cache_storage_set_with_ttl() {
        let backend = Arc::new(MockBackend::new_with_value(None));
        let storage = CacheStorage::new(backend);
        storage.set("k", "v", Some(60)).await.unwrap();
    }

    #[tokio::test]
    async fn test_cache_storage_set_maps_error() {
        let backend = Arc::new(MockBackend::new_failing_writes(|| {
            CacheError::Operation("boom".to_string())
        }));
        let storage = CacheStorage::new(backend);
        let err = storage.set("k", "v", None).await.unwrap_err();
        match err {
            StorageError::QueryError(msg) => assert!(msg.contains("boom")),
            other => panic!("expected QueryError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_cache_storage_delete_success() {
        let backend = Arc::new(MockBackend::new_with_value(None));
        let storage = CacheStorage::new(backend);
        storage.delete("k").await.unwrap();
    }

    #[tokio::test]
    async fn test_cache_storage_delete_maps_connection_error() {
        let backend = Arc::new(MockBackend::new_failing_writes(|| {
            CacheError::Connection("lost".to_string())
        }));
        let storage = CacheStorage::new(backend);
        let err = storage.delete("k").await.unwrap_err();
        match err {
            StorageError::ConnectionError(msg) => assert!(msg.contains("lost")),
            other => panic!("expected ConnectionError, got {other:?}"),
        }
    }

    #[test]
    fn test_map_error_connection_variant() {
        let err = map_error(CacheError::Connection("c".to_string()));
        assert!(matches!(err, StorageError::ConnectionError(_)));
    }

    #[test]
    fn test_map_error_timeout_variant() {
        let err = map_error(CacheError::Timeout("t".to_string()));
        assert!(matches!(err, StorageError::ConnectionError(_)));
    }

    #[test]
    fn test_map_error_other_variants_become_query_error() {
        // 抽样验证非 Connection/Timeout 的变体都映射为 QueryError
        let cases: Vec<CacheError> = vec![
            CacheError::Serialization("s".to_string()),
            CacheError::Operation("o".to_string()),
            CacheError::NotFound("n".to_string()),
            CacheError::NotSupported("ns".to_string()),
        ];
        for e in cases {
            let mapped = map_error(e);
            assert!(
                matches!(mapped, StorageError::QueryError(_)),
                "expected QueryError"
            );
        }
    }

    #[test]
    fn test_cache_storage_new_preserves_backend() {
        let backend = Arc::new(MockBackend::new_with_value(None));
        let _storage = CacheStorage::new(backend);
        // 构造成功即可（无 panic）
    }
}
