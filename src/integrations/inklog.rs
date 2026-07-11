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
/// Creates a `LoggerManager` with default config, which installs a global
/// `tracing` subscriber and `log` crate bridge. The returned manager must
/// be kept alive for the duration of the application — dropping it signals
/// worker shutdown.
///
/// # Errors
///
/// Returns `Err(InklogError)` if the `LoggerManager` fails to construct.
pub async fn init_inklog_logger() -> Result<LoggerManager, InklogError> {
    LoggerManager::with_config(InklogConfig::default()).await
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
}
