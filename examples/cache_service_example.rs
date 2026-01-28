//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! Cache Service Usage Examples
//!
//! This example demonstrates how to use the unified cache service with
//! dependency injection support.
//!
//! Run with:
//! ```bash
//! cargo run --example cache_service_example --features cache-service
//! ```

use limiteron::cache::{CacheService, CacheServiceConfig, MockCacheService, OxCacheService};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Example 1: Basic Memory Cache Usage
///
/// Creates an in-memory cache service and demonstrates basic operations.
#[tokio::main]
async fn main() {
    println!("\n🗄️  Example 1: Basic Memory Cache Usage\n");

    if let Err(e) = run_example_1().await {
        eprintln!("❌ Example 1 failed: {}", e);
        return;
    }

    println!("\n🎉 All examples completed successfully!\n");
    println!("📚 Learn more: docs/USER_GUIDE.md");
}

async fn run_example_1() -> Result<(), Box<dyn std::error::Error>> {
    // Create cache service with default configuration
    let config = CacheServiceConfig::default();
    let cache: Arc<dyn CacheService> = Arc::new(OxCacheService::new(config).await?);

    // Set a value
    cache.set("user:1", "Alice", None).await?;
    println!("✅ Set 'user:1' = 'Alice'");

    // Get the value
    let value = cache.get("user:1").await?;
    println!("✅ Got 'user:1' = {:?}", value);
    assert_eq!(value, Some("Alice".to_string()));

    // Test cache miss
    let miss = cache.get("nonexistent").await?;
    println!("✅ Cache miss for 'nonexistent': {:?}", miss);
    assert_eq!(miss, None);

    // Delete the value
    cache.delete("user:1").await?;
    println!("✅ Deleted 'user:1'");

    // Verify deletion
    let deleted = cache.get("user:1").await?;
    assert_eq!(deleted, None);

    println!("\n✅ Example 1 passed!\n");
    Ok(())
}
