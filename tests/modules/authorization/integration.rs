//! 授权模块集成测试
//!
//! 测试授权模块的完整功能

use limiteron::authorization::{
    AllowAllAuthorizationProvider, AuthorizationProvider, DenyAllAuthorizationProvider,
    SimpleAuthorizationProvider,
};
use limiteron::error::FlowGuardError;
use std::collections::HashMap;

// ============================================================================
// SimpleAuthorizationProvider Tests
// ============================================================================

#[tokio::test]
async fn test_simple_provider_authorized() {
    let provider = SimpleAuthorizationProvider::new(vec!["admin".to_string(), "mod".to_string()]);
    assert!(provider
        .check_authorization("create_ban", "admin", "192.168.1.1")
        .await
        .is_ok());
    assert!(provider
        .check_authorization("create_ban", "mod", "10.0.0.1")
        .await
        .is_ok());
}

#[tokio::test]
async fn test_simple_provider_unauthorized() {
    let provider = SimpleAuthorizationProvider::new(vec!["admin".to_string()]);
    let result = provider
        .check_authorization("create_ban", "guest", "192.168.1.1")
        .await;
    assert!(result.is_err());
    match result {
        Err(FlowGuardError::AuthorizationError(msg)) => {
            assert!(msg.contains("guest"));
        }
        _ => panic!("expected AuthorizationError"),
    }
}

#[tokio::test]
async fn test_simple_provider_empty_roles() {
    let provider = SimpleAuthorizationProvider::new(vec![]);
    let result = provider
        .check_authorization("create_ban", "anyone", "target")
        .await;
    assert!(result.is_err());
}

#[test]
fn test_simple_provider_is_authorized() {
    let provider = SimpleAuthorizationProvider::new(vec!["admin".to_string(), "mod".to_string()]);
    assert!(provider.is_authorized("admin"));
    assert!(provider.is_authorized("mod"));
    assert!(!provider.is_authorized("guest"));
    assert!(!provider.is_authorized(""));
}

#[test]
fn test_simple_provider_from_iter() {
    let provider = SimpleAuthorizationProvider::from_iter(["admin", "superuser"]);
    assert_eq!(provider.authorized_roles().len(), 2);
    assert!(provider.is_authorized("admin"));
    assert!(provider.is_authorized("superuser"));
}

#[test]
fn test_simple_provider_add_role() {
    let mut provider = SimpleAuthorizationProvider::new(vec!["admin".to_string()]);
    provider.add_role("moderator".to_string());
    assert!(provider.is_authorized("moderator"));
    // duplicate add
    provider.add_role("moderator".to_string());
    assert_eq!(provider.authorized_roles().len(), 2);
}

#[test]
fn test_simple_provider_remove_role() {
    let mut provider = SimpleAuthorizationProvider::new(vec![
        "admin".to_string(),
        "mod".to_string(),
    ]);
    assert!(provider.remove_role("mod"));
    assert!(!provider.is_authorized("mod"));
    assert!(!provider.remove_role("nonexistent"));
}

#[tokio::test]
async fn test_simple_provider_ignores_operation_and_target() {
    let provider = SimpleAuthorizationProvider::new(vec!["admin".to_string()]);
    // operation and target don't affect authorization decision
    assert!(provider
        .check_authorization("remove_ban", "admin", "any_target")
        .await
        .is_ok());
    assert!(provider
        .check_authorization("unknown_op", "admin", "any_target")
        .await
        .is_ok());
}

// ============================================================================
// AllowAllAuthorizationProvider Tests
// ============================================================================

#[tokio::test]
async fn test_allow_all_always_passes() {
    let provider = AllowAllAuthorizationProvider;
    assert!(provider
        .check_authorization("any_op", "anyone", "any_target")
        .await
        .is_ok());
    assert!(provider
        .check_authorization("create_ban", "guest", "192.168.1.1")
        .await
        .is_ok());
}

// ============================================================================
// DenyAllAuthorizationProvider Tests
// ============================================================================

#[tokio::test]
async fn test_deny_all_always_denies() {
    let provider = DenyAllAuthorizationProvider;
    let result = provider
        .check_authorization("any_op", "admin", "any_target")
        .await;
    assert!(result.is_err());
    match result {
        Err(FlowGuardError::AuthorizationError(msg)) => {
            assert!(msg.contains("拒绝"));
        }
        _ => panic!("expected AuthorizationError"),
    }
}

// ============================================================================
// OperationAuthorizationProvider Tests (requires ban-manager feature)
// ============================================================================

#[cfg(feature = "ban-manager")]
#[tokio::test]
async fn test_operation_provider_roles() {
    let mut roles = HashMap::new();
    roles.insert("create_ban".to_string(), vec!["admin".to_string(), "mod".to_string()]);
    roles.insert("remove_ban".to_string(), vec!["admin".to_string()]);

    let provider = OperationAuthorizationProvider::new(roles);

    // mod can create ban
    assert!(provider
        .check_authorization("create_ban", "mod", "192.168.1.1")
        .await
        .is_ok());
    // mod cannot remove ban
    assert!(provider
        .check_authorization("remove_ban", "mod", "192.168.1.1")
        .await
        .is_err());
    // admin can do both
    assert!(provider
        .check_authorization("create_ban", "admin", "10.0.0.1")
        .await
        .is_ok());
    assert!(provider
        .check_authorization("remove_ban", "admin", "10.0.0.1")
        .await
        .is_ok());
    // unknown operation
    assert!(provider
        .check_authorization("unknown_op", "admin", "target")
        .await
        .is_err());
}

#[cfg(feature = "ban-manager")]
#[test]
fn test_operation_provider_has_operation() {
    let mut roles = HashMap::new();
    roles.insert("create_ban".to_string(), vec!["admin".to_string()]);
    let provider = OperationAuthorizationProvider::new(roles);
    assert!(provider.has_operation("create_ban"));
    assert!(!provider.has_operation("remove_ban"));
}

#[cfg(feature = "ban-manager")]
#[test]
fn test_operation_provider_get_roles() {
    let mut roles = HashMap::new();
    roles.insert("create_ban".to_string(), vec!["admin".to_string(), "mod".to_string()]);
    let provider = OperationAuthorizationProvider::new(roles);
    assert_eq!(provider.get_roles("create_ban").unwrap().len(), 2);
    assert!(provider.get_roles("remove_ban").is_none());
}

#[cfg(feature = "ban-manager")]
#[test]
fn test_operation_provider_builder() {
    let provider = OperationAuthorizationProvider::builder()
        .with_operation("create_ban", vec!["admin".to_string(), "mod".to_string()])
        .with_operation("remove_ban", vec!["admin".to_string()])
        .build();
    assert!(provider.has_operation("create_ban"));
    assert!(provider.has_operation("remove_ban"));
    assert_eq!(provider.get_roles("create_ban").unwrap().len(), 2);
}

// ============================================================================
// Trait object tests
// ============================================================================

#[tokio::test]
async fn test_trait_object_dynamic_dispatch() {
    let providers: Vec<Box<dyn AuthorizationProvider>> = vec![
        Box::new(SimpleAuthorizationProvider::new(vec!["admin".to_string()])),
        Box::new(AllowAllAuthorizationProvider),
    ];

    assert!(providers[0]
        .check_authorization("op", "admin", "target")
        .await
        .is_ok());
    assert!(providers[1]
        .check_authorization("op", "anyone", "target")
        .await
        .is_ok());
}
