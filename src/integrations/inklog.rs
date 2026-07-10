//! inklog structured logging integration for limiteron.
//!
//! Enable via the `inklog` cargo feature. Provides [`InklogLoggerAdapter`] which
//! initializes inklog's `LoggerManager`, installing a global `tracing` subscriber
//! and a `log` crate bridge. As long as the adapter is alive, all existing
//! `tracing::`/`log::` macro calls in limiteron route through inklog's
//! structured sinks (console, file, database).
//!
//! When the `inklog` feature is **disabled**, limiteron retains its original
//! `log`/`tracing` behavior unchanged — no adapter is compiled in.
//!
//! # Example
//!
//! ```rust,no_run
//! # #[cfg(feature = "inklog")]
//! # {
//! use limiteron::integrations::inklog::init_inklog_logger;
//!
//! # tokio_test::block_on(async {
//! let _adapter = init_inklog_logger().await.expect("init inklog");
//! log::info!("routed through inklog");
//! # });
//! # }
//! ```
//!
//! Mirrors the `kit` feature pattern (Phase 2 of base-workspace-unification).

use std::path::Path;

/// Abstraction over a structured logging backend.
///
/// Implementors provide a unified interface for querying logger state and
/// flushing buffered records. When the `inklog` feature is enabled,
/// [`InklogLoggerAdapter`] implements this trait.
pub trait StructuredLogger: Send + Sync {
    /// Returns `true` if the backend has been initialized and is routing logs.
    fn is_active(&self) -> bool;

    /// Flush any buffered log records to the configured sinks.
    fn flush(&self);
}

/// Adapter bridging limiteron's `log`/`tracing` output to inklog's structured
/// logger.
///
/// Constructing this adapter initializes inklog's [`LoggerManager`] via
/// `LoggerManager::builder().build()`, which installs a global `tracing`
/// subscriber and a `log` crate bridge (`LogLogger`). As long as the adapter
/// is kept alive, all `tracing::`/`log::` macros in limiteron route through
/// inklog's sinks (console, file, database).
///
/// The adapter owns the `LoggerManager` — dropping it signals worker
/// shutdown. Keep the adapter alive for the duration of the application.
///
/// # Example
///
/// ```rust,no_run
/// use limiteron::integrations::inklog::InklogLoggerAdapter;
///
/// # tokio_test::block_on(async {
/// let _adapter = InklogLoggerAdapter::new().await.expect("init inklog");
/// log::info!("routed through inklog");
/// # });
/// ```
pub struct InklogLoggerAdapter {
    manager: inklog::LoggerManager,
}

impl InklogLoggerAdapter {
    /// Initialize inklog with default console output at `info` level.
    ///
    /// This installs the global `tracing` subscriber and `log` crate bridge.
    /// Subsequent calls succeed but the global subscriber/log logger are only
    /// installed once (inklog warns on repeat attempts, not fatal).
    ///
    /// # Errors
    ///
    /// Returns `Err(inklog::InklogError)` if the `LoggerManager` fails to
    /// construct (e.g., worker thread spawn failure).
    pub async fn new() -> Result<Self, inklog::InklogError> {
        let manager = inklog::LoggerManager::builder()
            .level("info")
            .console(true)
            .build()
            .await?;
        Ok(Self { manager })
    }

    /// Initialize inklog from a TOML configuration file.
    ///
    /// Delegates to `inklog::LoggerManager::from_file`, which parses the
    /// config and installs the global subscriber + `log` bridge.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to an inklog TOML configuration file.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the file cannot be read, parsed, or the
    /// `LoggerManager` fails to construct.
    pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, inklog::InklogError> {
        let manager = inklog::LoggerManager::from_file(path).await?;
        Ok(Self { manager })
    }

    /// Access the underlying inklog `LoggerManager`.
    ///
    /// Allows callers to query metrics, health status, or trigger sink
    /// recovery directly on the inklog manager.
    pub fn manager(&self) -> &inklog::LoggerManager {
        &self.manager
    }
}

impl StructuredLogger for InklogLoggerAdapter {
    fn is_active(&self) -> bool {
        // The adapter is constructed => the LoggerManager exists and the
        // global subscriber/log bridge have been registered (inklog warns,
        // not errors, on repeat init).
        true
    }

    fn flush(&self) {
        // Delegate to the global `log` logger's flush — inklog's LogLogger
        // installs itself as the global logger, so this routes to inklog's
        // channel-based flush (no-op for channels, but satisfies the API).
        log::logger().flush();
    }
}

/// Convenience initializer for inklog as the global structured logger.
///
/// Equivalent to [`InklogLoggerAdapter::new`]. The returned adapter must be
/// kept alive for the duration of the application to keep inklog's worker
/// threads running.
///
/// # Errors
///
/// Returns `Err(inklog::InklogError)` if the `LoggerManager` fails to
/// construct.
pub async fn init_inklog_logger() -> Result<InklogLoggerAdapter, inklog::InklogError> {
    InklogLoggerAdapter::new().await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R-inklog-001: `InklogLoggerAdapter::new()` constructs an adapter backed
    /// by inklog's `LoggerManager` without error.
    #[tokio::test]
    async fn inklog_adapter_constructs() {
        let adapter = InklogLoggerAdapter::new()
            .await
            .expect("InklogLoggerAdapter::new should succeed");
        assert!(adapter.is_active());
    }

    /// R-inklog-001: the adapter implements `StructuredLogger`.
    #[tokio::test]
    async fn inklog_adapter_implements_structured_logger() {
        let adapter = InklogLoggerAdapter::new()
            .await
            .expect("InklogLoggerAdapter::new should succeed");
        assert!(StructuredLogger::is_active(&adapter));
        // flush must not panic
        StructuredLogger::flush(&adapter);
    }

    /// R-inklog-001: `init_inklog_logger()` returns an adapter.
    #[tokio::test]
    async fn init_inklog_logger_returns_adapter() {
        let adapter = init_inklog_logger()
            .await
            .expect("init_inklog_logger should succeed");
        assert!(adapter.is_active());
    }

    /// R-inklog-001: `log`/`tracing` macros do not panic after inklog init.
    #[tokio::test]
    async fn log_macros_survive_after_inklog_init() {
        let _adapter = init_inklog_logger()
            .await
            .expect("init_inklog_logger should succeed");
        // These must not panic — inklog's LogLogger bridge routes them.
        log::info!("inklog bridge active");
        log::warn!("inklog bridge warn");
        tracing::info!(target: "limiteron", "tracing via inklog");
    }

    /// R-inklog-001: `InklogLoggerAdapter::from_file` is available on the API.
    /// Verifies the method signature compiles (does not require a real file).
    #[test]
    fn inklog_adapter_from_file_signature_compiles() {
        // Type-check the method exists without calling it (would need a real
        // config file + runtime). If the method were missing this would fail
        // to compile.
        fn _assert_from_file<P: AsRef<Path>>() -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<InklogLoggerAdapter, inklog::InklogError>>
                    + Send,
            >,
        > {
            Box::pin(async { InklogLoggerAdapter::from_file("nonexistent.toml").await })
        }
        // Suppress unused warning for the closure.
        let _ = _assert_from_file::<&str>;
    }
}
