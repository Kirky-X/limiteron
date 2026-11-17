# Unreleased Changes

This document records limiteron's breaking changes and migration guidance. For the full version history see [docs/CHANGELOG.md](../CHANGELOG.md).

## Unreleased

## 0.3.0-rc.9

### Breaking Changes

- **Removal of the `inklog` feature and the `limiteron::integrations::inklog` module**: limiteron no longer forwards
  inklog structured-logging initialization entry points to the application layer. The module is removed entirely, and the public APIs
  `init_inklog_logger` / `init_inklog_logger_with_config` / `LimiteronSinkRateLimit`
  disappear along with it. The `inklog` entry is removed from the `full` preset accordingly.

  The canonical API for structured logging has always lived on the inklog side (`inklog::init_inklog_logger`,
  inklog/src/lib.rs:335); what limiteron kept was only a thin wrapper around inklog's capabilities,
  and removing the forwarding does not change how limiteron's own `log::`/`tracing::` calls are routed.

- **The `inklog` feature name no longer exists**: `features = ["inklog"]` in a downstream
  `Cargo.toml` fails with an unknown-feature error; remove that entry. Applications that need structured
  logging should depend on inklog directly and call its canonical API.

- **The `log-redaction` feature is retained**: log redaction capability is unchanged; the masking engine is still carried by trait-kit's
  `DataMasker` / `MaskRuleRegistry` (the single source of truth for rules and priorities
  remains on the trait-kit side; limiteron does not build a second implementation). This dependency is an implementation
  detail of `log-redaction` and is not exposed as a standalone feature.

### Changed

- The `full` preset no longer includes `log-redaction`: masking is an optional borrow of the trait-kit masking engine, and `full` must not transitively pull in trait-kit. Enable `log-redaction` explicitly when needed.
- Self-developed cross-repo dependencies (oxcache / dbnexus / trait-kit / inklog / confers) are always introduced via the crates.io registry: CI and release flows check out only this repo, and `path` dependencies pointing at sibling directories outside the repo make manifest resolution fail outright (`failed to load source for dependency`). For local integration testing of unreleased versions, use a path override outside the workspace (`paths` in `<workspace root>/.cargo/config.toml`) pointing at the sibling repo's working tree; that override file is not part of this repo.
- `POST /api/v1/check/batch` per-item `error` field changed from raw error text to stable categories (`validation` / `storage` / `config` / `authorization` / `throttled` / `ban` / `circuit_breaker` / `internal`): raw error text lands only in server-side logs, never in the response body. **Breaking change** — downstreams that parse error text must switch to category-based determination.

### Migration Guidance

- Per-item error handling for the batch check endpoint: switch to stable categories; stop matching error text.

  ```json
  // Before (per-item error is raw error text)
  { "index": 0, "allowed": false, "decision": "Error", "error": "Storage error: ..." }

  // After (error is a stable category; raw text lands only in server-side logs)
  { "index": 0, "allowed": false, "decision": "Error", "error": "storage" }
  ```

- Structured logging initialization: use the inklog canonical API.

  ```rust
  // Before (limiteron forwarding entry, removed)
  let _manager = limiteron::integrations::inklog::init_inklog_logger().await?;

  // After (inklog canonical API)
  inklog::init_inklog_logger().await?;
  ```

  For custom configuration use `inklog::init_inklog_logger_with_config(config)`
  (inklog/src/lib.rs:367). Note that the inklog canonical entry has process-level singleton semantics: repeated calls
  return `Err(InklogError::ConfigError)`.

- Dependency declaration: declare the inklog dependency directly at the application layer (current stable version `0.3.0`,
  `default-features = false`), and remove limiteron's `features = ["inklog"]`.

- Sink write rate limiting: `LimiteronSinkRateLimit` (per-target bucketing + ERROR passthrough + budget returned on write failure)
  was removed along with the module. Implementors who need that rate-limit port should implement inklog's
  `SinkRateLimit` port directly (inklog/src/support/io/sink/rate_limit.rs:47); for a plain global
  token bucket, inklog's built-in `TokenBucketRateLimit` is available
  (inklog/src/support/io/sink/rate_limit.rs:84), and when no rate limiting is needed use
  `NoOpRateLimit` (inklog/src/support/io/sink/rate_limit.rs:65).
