//! Prelude module - Commonly used types for quick imports
//!
//! This module re-exports the most commonly used types from Limiteron,
//! allowing users to import them with a single `use limiteron::prelude::*;`
//! statement instead of importing each type individually.

// Core types - always available
pub use crate::config::FlowControlConfig;
pub use crate::error::{Decision, FlowGuardError};
pub use crate::governor::Governor;

// Common matchers
pub use crate::matchers::{
    Identifier, IdentifierExtractor, IpExtractor, RequestContext, UserIdExtractor,
};

// Common limiters
pub use crate::limiters::{FixedWindowLimiter, SlidingWindowLimiter, TokenBucketLimiter};

// Feature-gated exports
#[cfg(feature = "ban-manager")]
pub use crate::ban_manager::BanManager;

#[cfg(feature = "circuit-breaker")]
pub use crate::circuit_breaker::CircuitBreaker;

#[cfg(feature = "quota-control")]
pub use crate::quota_controller::QuotaController;

#[cfg(feature = "macros")]
pub use crate::macros::flow_control;

#[cfg(feature = "postgres")]
pub use crate::postgres_storage::PostgresStorage;

#[cfg(feature = "redis")]
pub use crate::redis_storage::RedisStorage;
