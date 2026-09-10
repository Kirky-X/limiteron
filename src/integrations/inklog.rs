// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! inklog structured logging integration for limiteron.
//!
//! Enable via the `inklog` cargo feature. Provides [`init_inklog_logger`]
//! which initializes inklog's `LoggerManager`, installing a global `tracing`
//! subscriber and a `log` crate bridge. As long as the manager is alive, all
//! existing `tracing::`/`log::` macro calls in limiteron route through
//! inklog's structured sinks (console, file, database).
//!
//! When the `inklog` feature is **disabled**, limiteron retains its original
//! `log`/`tracing` behavior unchanged.
//!
//! # Example
//!
//! ```rust,no_run
//! # #[cfg(feature = "inklog")]
//! # {
//! use limiteron::integrations::inklog::init_inklog_logger;
//!
//! # tokio_test::block_on(async {
//! let _manager = init_inklog_logger().await.expect("init inklog");
//! log::info!("routed through inklog");
//! # });
//! # }
//! ```

/// Re-export inklog core types for direct access.
pub use ::inklog::{InklogConfig, InklogError, LoggerManager};

/// Initialize inklog as the global structured logging backend.
///
/// Creates a `LoggerManager` with default config. Equivalent to
/// `init_inklog_logger_with_config(InklogConfig::default())`.
///
/// # Errors
///
/// Returns `Err(InklogError)` if the `LoggerManager` fails to construct.
pub async fn init_inklog_logger() -> Result<LoggerManager, InklogError> {
    init_inklog_logger_with_config(InklogConfig::default()).await
}

/// Initialize inklog with a custom configuration.
///
/// Accepts an [`InklogConfig`] so callers can control log level, output sinks
/// (console / file / database), and per-crate target levels. The returned
/// manager must be kept alive for the duration of the application.
///
/// # Audit event bridge
///
/// When the `inklog` feature is enabled, all `log::info!` / `tracing::info!`
/// calls inside limiteron — including the audit logger’s `write_batch` — are
/// automatically routed through inklog’s structured sinks. HMAC signing stays
/// in the limiteron application layer (audit-log feature) and is unaffected.
///
/// # Errors
///
/// Returns `Err(InklogError)` if the `LoggerManager` fails to construct.
///
/// # Example
///
/// ```rust,no_run
/// # #[cfg(feature = "inklog")]
/// # {
/// use limiteron::integrations::inklog::{init_inklog_logger_with_config, InklogConfig};
///
/// # tokio_test::block_on(async {
/// let mut config = InklogConfig::default();
/// config.global_level = "debug".to_string();
/// let _manager = init_inklog_logger_with_config(config).await.expect("init");
/// # });
/// # }
/// ```
pub async fn init_inklog_logger_with_config(
    config: InklogConfig,
) -> Result<LoggerManager, InklogError> {
    LoggerManager::with_config(config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R-inklog-001: `init_inklog_logger()` returns a `LoggerManager`.
    #[tokio::test]
    #[serial_test::serial]
    async fn init_inklog_logger_returns_manager() {
        let result = init_inklog_logger().await;
        assert!(result.is_ok(), "init_inklog_logger should return Ok");
    }

    /// R-inklog-001: `log`/`tracing` macros do not panic after inklog init.
    #[tokio::test]
    #[serial_test::serial]
    async fn log_macros_survive_after_inklog_init() {
        let _manager = init_inklog_logger()
            .await
            .expect("init_inklog_logger should succeed");
        log::info!("inklog bridge active");
        log::warn!("inklog bridge warn");
        tracing::info!(target: "limiteron", "tracing via inklog");
    }

    /// R-inklog-001: `init_inklog_logger()` is idempotent (repeat calls don't panic).
    #[tokio::test]
    #[serial_test::serial]
    async fn init_inklog_logger_is_idempotent() {
        let _first = init_inklog_logger()
            .await
            .expect("first init should succeed");
        let second = init_inklog_logger().await;
        assert!(
            second.is_ok(),
            "second init should still return Ok (install failure downgraded to warn)"
        );
    }

    /// T053: `init_inklog_logger_with_config` accepts custom InklogConfig.
    #[tokio::test]
    #[serial_test::serial]
    async fn init_inklog_logger_with_config_works() {
        let config = InklogConfig::default();
        let result = init_inklog_logger_with_config(config).await;
        assert!(
            result.is_ok(),
            "init_inklog_logger_with_config should return Ok"
        );
    }
}
