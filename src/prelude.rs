//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! Prelude module - Commonly used types for quick imports
//!
//! This module re-exports the most commonly used types from Limiteron,
//! allowing users to import them with a single `use limiteron::prelude::*;`
//! statement instead of importing each type individually.

// Core types - always available
pub use crate::config::types::FlowControlConfig;
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
pub use crate::ban::BanManager;

#[cfg(feature = "circuit-breaker")]
pub use crate::circuit::CircuitBreaker;

#[cfg(feature = "quota-control")]
pub use crate::quota_controller::QuotaController;

#[cfg(feature = "macros")]
pub use crate::macros::flow_control;

// DbStorage removed as part of direct-inheritance refactoring
// Use dbnexus::DbStorage directly instead
