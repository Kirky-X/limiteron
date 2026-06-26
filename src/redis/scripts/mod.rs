//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! Redis Lua script management module.
//!
//! This module provides centralized management of Lua scripts for
//! distributed rate limiting algorithms.

pub mod constants;
pub mod gcra;

// Re-export constants for convenience
pub use constants::{FIXED_WINDOW_SCRIPT, GCRA_SCRIPT, SLIDING_WINDOW_SCRIPT, TOKEN_BUCKET_SCRIPT};

// Re-export GCRA types and functions
pub use gcra::{execute_gcra, execute_gcra_with_sha, load_gcra_script, GcraResult};

/// Lua script type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScriptType {
    /// GCRA algorithm
    Gcra,
    /// Sliding window algorithm
    SlidingWindow,
    /// Fixed window algorithm
    FixedWindow,
    /// Token bucket algorithm
    TokenBucket,
}

impl ScriptType {
    /// Get script name
    pub fn name(&self) -> &str {
        match self {
            ScriptType::Gcra => "gcra",
            ScriptType::SlidingWindow => "sliding_window",
            ScriptType::FixedWindow => "fixed_window",
            ScriptType::TokenBucket => "token_bucket",
        }
    }

    /// Get script content
    pub fn content(&self) -> &'static str {
        match self {
            ScriptType::Gcra => GCRA_SCRIPT,
            ScriptType::SlidingWindow => SLIDING_WINDOW_SCRIPT,
            ScriptType::FixedWindow => FIXED_WINDOW_SCRIPT,
            ScriptType::TokenBucket => TOKEN_BUCKET_SCRIPT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_type_name() {
        assert_eq!(ScriptType::Gcra.name(), "gcra");
        assert_eq!(ScriptType::SlidingWindow.name(), "sliding_window");
        assert_eq!(ScriptType::FixedWindow.name(), "fixed_window");
        assert_eq!(ScriptType::TokenBucket.name(), "token_bucket");
    }

    #[test]
    fn test_script_type_content_not_empty() {
        assert!(!ScriptType::Gcra.content().is_empty());
        assert!(!ScriptType::SlidingWindow.content().is_empty());
        assert!(!ScriptType::FixedWindow.content().is_empty());
        assert!(!ScriptType::TokenBucket.content().is_empty());
    }

    #[test]
    fn test_script_type_content_matches_constants() {
        assert_eq!(ScriptType::Gcra.content(), GCRA_SCRIPT);
        assert_eq!(ScriptType::SlidingWindow.content(), SLIDING_WINDOW_SCRIPT);
        assert_eq!(ScriptType::FixedWindow.content(), FIXED_WINDOW_SCRIPT);
        assert_eq!(ScriptType::TokenBucket.content(), TOKEN_BUCKET_SCRIPT);
    }
}
