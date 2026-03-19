//! Limiteron Examples
//!
//! This crate demonstrates how to use the limiteron library for:
//! - Rate limiting (Token Bucket, Sliding Window, Fixed Window, Concurrency)
//! - Circuit breaking (fault tolerance pattern)
//! - Quota control (usage tracking and limits)
//! - Ban management (IP/User/MAC bans)
//!
//! # Running Examples
//!
//! ```bash
//! # Rate limiters (core functionality)
//! cargo run --bin rate_limiters
//!
//! # Circuit breaker (requires circuit-breaker feature)
//! cargo run --bin circuit_breaker --features circuit-breaker
//!
//! # Quota control (requires quota-control feature)
//! cargo run --bin quota_control --features quota-control
//!
//! # Ban manager (requires ban-manager feature)
//! cargo run --bin ban_manager --features ban-manager
//! ```

pub mod storage;

pub use storage::MemoryBanStorage;
pub use storage::MemoryQuotaStorage;
