//! limiteron integration modules with external frameworks.
//!
//! Integrations are feature-gated so the core limiteron library stays
//! dependency-free when integrations are not needed.

#[cfg(feature = "kit")]
pub mod kit;
