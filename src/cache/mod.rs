// Copyright (c) 2026, Kirky.X
//
// MIT License
//
// Unified Cache Service with Dependency Injection
//
// Provides a consistent interface for cache operations across the limiteron
// framework, with support for multiple backends and per-entry TTL.
//
// # Features
//
// - **Dependency Injection**: Inject `Arc<dyn CacheService>` into components
// - **Multiple Backends**: Support for memory and Redis backends
// - **Per-Entry TTL**: Set TTL per cache entry using oxcache's `set_with_ttl`
// - **Unified Configuration**: Centralized configuration for all cache settings
//
// # Feature Flag
//
// This module is only available when the `cache-service` feature is enabled.

#[cfg(feature = "cache-service")]
pub mod cache_trait;

#[cfg(feature = "cache-service")]
pub mod service;

#[cfg(feature = "cache-service")]
pub mod config;

// Re-exports for public API
#[cfg(feature = "cache-service")]
pub use config::{CacheServiceConfig, MemoryCacheConfig, RedisCacheConfig};

#[cfg(feature = "cache-service")]
pub use cache_trait::{CacheBackend, CacheService};

#[cfg(feature = "cache-service")]
pub use cache_trait::MockCacheService;

#[cfg(feature = "cache-service")]
pub use service::OxCacheService;
