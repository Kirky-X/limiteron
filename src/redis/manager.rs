//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! Script manager for Redis Lua scripts.
//!
//! Manages loading and caching of Lua scripts using EVALSHA for
//! better performance in distributed rate limiting scenarios.

use crate::error::StorageError;
use crate::redis::scripts::ScriptType;
use ahash::AHashMap as HashMap;
use parking_lot::RwLock;

/// Cached script information
#[derive(Debug, Clone)]
struct ScriptCache {
    /// Script SHA for EVALSHA
    sha: String,
    /// Script type
    script_type: ScriptType,
}

/// Script manager for Redis Lua scripts
///
/// Manages the loading and caching of Lua scripts in Redis.
/// Uses EVALSHA for better performance after initial load.
///
/// # Thread Safety
///
/// This struct implements `Send + Sync` and can be safely shared
/// across threads using `Arc`.
///
/// # Example
///
/// ```rust,ignore
/// use limiteron::redis::ScriptManager;
///
/// let manager = ScriptManager::new();
/// manager.load_all_scripts(&redis_client).await?;
/// ```
pub struct ScriptManager {
    /// Cached script SHAs
    scripts: RwLock<HashMap<ScriptType, ScriptCache>>,
}

impl ScriptManager {
    /// Create a new script manager
    pub fn new() -> Self {
        Self {
            scripts: RwLock::new(HashMap::default()),
        }
    }

    /// Load a single script into Redis
    ///
    /// # Arguments
    /// * `conn` - Redis connection
    /// * `script_type` - Type of script to load
    ///
    /// # Returns
    /// * `Ok(())` - Success
    /// * `Err(StorageError)` - Load error
    pub async fn load_script(
        &self,
        conn: &mut redis::aio::Connection,
        script_type: ScriptType,
    ) -> Result<(), StorageError> {
        let sha: String = redis::cmd("SCRIPT")
            .arg("LOAD")
            .arg(script_type.content())
            .query_async(conn)
            .await
            .map_err(|e| {
                StorageError::QueryError(format!(
                    "Failed to load script {}: {}",
                    script_type.name(),
                    e
                ))
            })?;

        let cache = ScriptCache { sha, script_type };

        self.scripts.write().insert(script_type, cache);
        Ok(())
    }

    /// Load all supported scripts into Redis
    ///
    /// # Arguments
    /// * `conn` - Redis connection
    ///
    /// # Returns
    /// * `Ok(())` - Success
    /// * `Err(StorageError)` - Load error
    pub async fn load_all_scripts(
        &self,
        conn: &mut redis::aio::Connection,
    ) -> Result<(), StorageError> {
        let script_types = [
            ScriptType::Gcra,
            ScriptType::SlidingWindow,
            ScriptType::FixedWindow,
            ScriptType::TokenBucket,
        ];

        for script_type in script_types {
            self.load_script(conn, script_type).await?;
        }

        Ok(())
    }

    /// Get the SHA for a script type
    ///
    /// # Arguments
    /// * `script_type` - Type of script
    ///
    /// # Returns
    /// * `Some(String)` - Script SHA if loaded
    /// * `None` - Script not loaded
    pub fn get_script_sha(&self, script_type: ScriptType) -> Option<String> {
        self.scripts
            .read()
            .get(&script_type)
            .map(|cache| cache.sha.clone())
    }

    /// Check if a script is loaded
    ///
    /// # Arguments
    /// * `script_type` - Type of script
    ///
    /// # Returns
    /// * `true` - Script is loaded
    /// * `false` - Script is not loaded
    pub fn is_script_loaded(&self, script_type: ScriptType) -> bool {
        self.scripts.read().contains_key(&script_type)
    }

    /// Get all loaded script types
    ///
    /// # Returns
    /// Vector of loaded script types
    pub fn loaded_scripts(&self) -> Vec<ScriptType> {
        self.scripts.read().keys().cloned().collect()
    }

    /// Execute a script by type using EVALSHA
    ///
    /// Falls back to EVAL if script is not loaded.
    ///
    /// # Arguments
    /// * `conn` - Redis connection
    /// * `script_type` - Type of script to execute
    /// * `keys` - Redis keys
    /// * `args` - Script arguments
    ///
    /// # Returns
    /// * `Ok(Vec<i64>)` - Script result
    /// * `Err(StorageError)` - Execution error
    pub async fn execute_script(
        &self,
        conn: &mut redis::aio::Connection,
        script_type: ScriptType,
        keys: &[&str],
        args: &[&str],
    ) -> Result<Vec<i64>, StorageError> {
        // Try EVALSHA first
        if let Some(sha) = self.get_script_sha(script_type) {
            match self.execute_evalsha(conn, &sha, keys, args).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    // If NOSCRIPT, fall through to EVAL
                    if !e.to_string().contains("NOSCRIPT") {
                        return Err(e);
                    }
                }
            }
        }

        // Fall back to EVAL
        self.execute_eval(conn, script_type, keys, args).await
    }

    /// Execute script using EVALSHA
    async fn execute_evalsha(
        &self,
        conn: &mut redis::aio::Connection,
        sha: &str,
        keys: &[&str],
        args: &[&str],
    ) -> Result<Vec<i64>, StorageError> {
        let mut cmd = redis::cmd("EVALSHA");
        cmd.arg(sha).arg(keys.len() as i64);

        for key in keys {
            cmd.arg(key);
        }

        for arg in args {
            cmd.arg(arg);
        }

        cmd.query_async(conn)
            .await
            .map_err(|e| StorageError::QueryError(format!("EVALSHA execution failed: {}", e)))
    }

    /// Execute script using EVAL
    async fn execute_eval(
        &self,
        conn: &mut redis::aio::Connection,
        script_type: ScriptType,
        keys: &[&str],
        args: &[&str],
    ) -> Result<Vec<i64>, StorageError> {
        let mut cmd = redis::cmd("EVAL");
        cmd.arg(script_type.content()).arg(keys.len() as i64);

        for key in keys {
            cmd.arg(key);
        }

        for arg in args {
            cmd.arg(arg);
        }

        let result: Vec<i64> = cmd
            .query_async(conn)
            .await
            .map_err(|e| StorageError::QueryError(format!("EVAL execution failed: {}", e)))?;

        // If EVAL succeeded and we didn't have the SHA cached, cache it now
        // Note: We can't get the SHA from EVAL directly, but next load_script call will cache it
        Ok(result)
    }
}

impl Default for ScriptManager {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ScriptManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let scripts = self.scripts.read();
        f.debug_struct("ScriptManager")
            .field("loaded_scripts", &scripts.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_manager_new() {
        let manager = ScriptManager::new();
        assert_eq!(manager.loaded_scripts().len(), 0);
    }

    #[test]
    fn test_script_manager_default() {
        let manager = ScriptManager::default();
        assert_eq!(manager.loaded_scripts().len(), 0);
    }

    #[test]
    fn test_script_manager_debug() {
        let manager = ScriptManager::new();
        let debug_str = format!("{:?}", manager);
        assert!(debug_str.contains("ScriptManager"));
    }

    #[test]
    fn test_script_manager_not_loaded() {
        let manager = ScriptManager::new();
        assert!(!manager.is_script_loaded(ScriptType::Gcra));
        assert!(manager.get_script_sha(ScriptType::Gcra).is_none());
    }
}
