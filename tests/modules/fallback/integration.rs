//! 降级模块集成测试

use limiteron::fallback::{ComponentType, FallbackConfig, FallbackManager, FallbackStrategy};

#[tokio::test]
async fn test_fallback_strategy_variants() {
    // Verify all strategy variants exist
    let _ = FallbackStrategy::FailOpen;
    let _ = FallbackStrategy::FailClosed;
}

#[tokio::test]
async fn test_component_type_variants() {
    let _ = ComponentType::Postgres;
    let _ = ComponentType::Redis;
    let _ = ComponentType::Ban;
    let _ = ComponentType::Quota;
}

#[tokio::test]
async fn test_fallback_config() {
    let config = FallbackConfig::new(ComponentType::Storage, FallbackStrategy::FailClosed);
    assert_eq!(config.component, ComponentType::Storage);
    assert_eq!(config.strategy, FallbackStrategy::FailClosed);
    assert!(config.enabled);
}

#[tokio::test]
async fn test_fallback_config_builder() {
    let config = FallbackConfig::new(ComponentType::Config, FallbackStrategy::FailOpen)
        .timeout(std::time::Duration::from_secs(5))
        .max_retries(3);
    assert!(!config.enabled); // still uses default
}
