// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! confers configuration integration for limiteron.
//!
//! Enable via the `config-confers` cargo feature for file/env config loading,
//! or `config-confers-reload` for hot-reload with file watching.
//!
//! # Architecture
//!
//! - [`load_config_from_file`] — Load [`FlowControlConfig`] from a TOML/JSON/YAML
//!   file using confers' `load_file` utility.
//! - [`watch_and_reload`] — Watch config file for changes, validate the new
//!   config, and atomically swap it into the Governor's `Arc<RwLock<FlowControlConfig>>`.
//!   On validation failure the old config is retained (rollback).
//!
//! # Example
//!
//! ```rust,ignore
//! use limiteron::integrations::confers::{load_config_from_file, watch_and_reload};
//! use limiteron::Governor;
//! use std::sync::Arc;
//!
//! let config = load_config_from_file("limiteron.toml").await?;
//! let governor = Governor::builder()
//!     .with_config(config.clone())
//!     // ... other builder calls
//!     .build()
//!     .await?;
//!
//! // Hot-reload: watch file and atomically swap governor config on change
//! let governor_config_handle = governor.config_handle();
//! let _guard = watch_and_reload("limiteron.toml", governor_config_handle).await?;
//! ```

use crate::config::FlowControlConfig;
use crate::error::LimiteronError;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Load [`FlowControlConfig`] from a configuration file.
///
/// Supports TOML, JSON, and YAML formats (auto-detected by file extension).
/// Uses confers' `load_file` utility for parsing.
///
/// # Errors
///
/// Returns `Err(LimiteronError)` if the file cannot be read or parsed.
pub async fn load_config_from_file(path: &str) -> Result<FlowControlConfig, LimiteronError> {
    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| LimiteronError::ConfigError(format!("failed to read config file: {}", e)))?;

    let format = detect_format(path);
    parse_config(&content, format)
}

/// Handle for atomically swapping the Governor's live config.
///
/// Obtained from [`Governor::config_handle`] (or constructed manually).
/// The `watch_and_reload` function uses this to push validated config updates.
pub type GovernorConfigHandle = Arc<RwLock<FlowControlConfig>>;

/// Watch a config file for changes and atomically reload the Governor config.
///
/// Uses confers' `FsWatcher` to monitor the file. When a change is detected:
/// 1. Re-read and parse the file into a new [`FlowControlConfig`].
/// 2. Validate the new config (basic structural checks).
/// 3. If valid, atomically swap it into the `GovernorConfigHandle`.
/// 4. If invalid, log the error and **retain the old config** (rollback).
///
/// Spawns a background tokio task for the watch loop. Returns a
/// `tokio_util::sync::CancellationToken` that can be used to stop the watcher.
///
/// # Errors
///
/// Returns `Err(LimiteronError)` if the watcher cannot be started.
#[cfg(feature = "config-confers-reload")]
pub async fn watch_and_reload(
    path: &str,
    config_handle: GovernorConfigHandle,
) -> Result<tokio_util::sync::CancellationToken, LimiteronError> {
    let path_buf = std::path::PathBuf::from(path);

    let mut watcher = confers::FsWatcher::new(&path_buf, 500)
        .await
        .map_err(|e| LimiteronError::ConfigError(format!("failed to create watcher: {}", e)))?;

    let cancel_token = tokio_util::sync::CancellationToken::new();
    let cancel_clone = cancel_token.clone();
    let file_path = path.to_string();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_clone.cancelled() => {
                    log::info!("confers watcher: cancelled, stopping");
                    break;
                }
                event = watcher.recv() => {
                    match event {
                        Some(_changed_path) => {
                            log::info!("confers watcher: config file changed, reloading");
                            match load_config_from_file(&file_path).await {
                                Ok(new_config) => {
                                    // Validate: rules must not be empty
                                    if new_config.rules.is_empty() {
                                        log::warn!(
                                            "confers reload: new config has empty rules, rolling back"
                                        );
                                        continue;
                                    }
                                    // Atomic swap
                                    let mut guard = config_handle.write().await;
                                    *guard = new_config;
                                    log::info!("confers reload: config updated successfully");
                                }
                                Err(e) => {
                                    log::error!(
                                        "confers reload: failed to parse config, rolling back: {}",
                                        e
                                    );
                                    // Rollback: keep old config (do nothing)
                                }
                            }
                        }
                        None => {
                            log::warn!("confers watcher: watcher stopped unexpectedly");
                            break;
                        }
                    }
                }
            }
        }
    });

    Ok(cancel_token)
}

// ============================================================================
// Internal helpers
// ============================================================================

#[derive(Debug, Clone, Copy)]
enum ConfigFormat {
    Toml,
    Json,
    Yaml,
}

fn detect_format(path: &str) -> ConfigFormat {
    match Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
    {
        "json" => ConfigFormat::Json,
        "yaml" | "yml" => ConfigFormat::Yaml,
        _ => ConfigFormat::Toml, // default
    }
}

fn parse_config(content: &str, format: ConfigFormat) -> Result<FlowControlConfig, LimiteronError> {
    match format {
        ConfigFormat::Toml => toml::from_str(content)
            .map_err(|e| LimiteronError::ConfigError(format!("TOML parse error: {}", e))),
        ConfigFormat::Json => serde_json::from_str(content)
            .map_err(|e| LimiteronError::ConfigError(format!("JSON parse error: {}", e))),
        ConfigFormat::Yaml => serde_yaml_ng::from_str(content)
            .map_err(|e| LimiteronError::ConfigError(format!("YAML parse error: {}", e))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_format_toml() {
        assert!(matches!(detect_format("config.toml"), ConfigFormat::Toml));
    }

    #[test]
    fn test_detect_format_json() {
        assert!(matches!(detect_format("config.json"), ConfigFormat::Json));
    }

    #[test]
    fn test_detect_format_yaml() {
        assert!(matches!(detect_format("config.yaml"), ConfigFormat::Yaml));
        assert!(matches!(detect_format("config.yml"), ConfigFormat::Yaml));
    }

    #[test]
    fn test_detect_format_default() {
        assert!(matches!(detect_format("config"), ConfigFormat::Toml));
        assert!(matches!(detect_format("config.txt"), ConfigFormat::Toml));
    }

    #[test]
    fn test_parse_config_json_valid() {
        let json = r#"{
            "version": "0.1.0",
            "global": {
                "storage": "memory",
                "cache": "memory",
                "metrics": "prometheus"
            },
            "rules": []
        }"#;
        let result = parse_config(json, ConfigFormat::Json);
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.version, "0.1.0");
    }

    #[test]
    fn test_parse_config_json_invalid() {
        let result = parse_config("not json", ConfigFormat::Json);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_config_from_file_not_found() {
        let result = load_config_from_file("/nonexistent/path.toml").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_config_from_file_json_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_config.json");
        let config_json = r#"{
            "version": "1.0.0",
            "global": {
                "storage": "memory",
                "cache": "memory",
                "metrics": "prometheus"
            },
            "rules": []
        }"#;
        std::fs::write(&path, config_json).unwrap();

        let result = load_config_from_file(path.to_str().unwrap()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().version, "1.0.0");
    }
}
