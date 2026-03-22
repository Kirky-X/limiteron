//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 封禁操作授权模块
//!
//! 提供封禁操作的授权检查机制，确保只有授权用户才能执行封禁操作。
//!
//! # 功能
//!
//! - 授权检查 trait 定义
//! - 简单授权提供者（基于角色列表）
//! - 测试用授权提供者（允许/拒绝所有操作）
//!
//! # 示例
//!
//! ```rust
//! use limiteron::authorization::{AuthorizationProvider, SimpleAuthorizationProvider};
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     // 创建简单授权提供者
//!     let provider = SimpleAuthorizationProvider::new(vec![
//!         "admin".to_string(),
//!         "moderator".to_string(),
//!     ]);
//!
//!     // 检查授权
//!     let result = provider.check_authorization("create_ban", "admin", "user123").await;
//!     assert!(result.is_ok());
//! }
//! ```

use async_trait::async_trait;

use crate::error::FlowGuardError;

/// 授权提供者 trait
///
/// 实现此 trait 以提供自定义的授权检查逻辑。
///
/// # 实现要求
///
/// - 必须实现 `Send + Sync` 以支持多线程环境
/// - 所有方法必须是异步的
/// - 返回 `Ok(())` 表示授权通过
/// - 返回 `Err(FlowGuardError::AuthorizationError)` 表示授权失败
///
/// # 示例
///
/// ```rust
/// use limiteron::authorization::AuthorizationProvider;
/// use limiteron::error::FlowGuardError;
/// use async_trait::async_trait;
///
/// pub struct MyAuthorizationProvider;
///
/// #[async_trait]
/// impl AuthorizationProvider for MyAuthorizationProvider {
///     async fn check_authorization(
///         &self,
///         operation: &str,
///         operator: &str,
///         target: &str,
///     ) -> Result<(), FlowGuardError> {
///         // 实现自定义授权逻辑
///         if operator == "admin" {
///             Ok(())
///         } else {
///             Err(FlowGuardError::AuthorizationError(
///                 "Only admin can perform this operation".to_string(),
///             ))
///         }
///     }
/// }
/// ```
#[async_trait]
pub trait AuthorizationProvider: Send + Sync {
    /// 检查封禁操作授权
    ///
    /// # 参数
    ///
    /// * `operation` - 操作类型（如 "create_ban", "remove_ban", "update_ban"）
    /// * `operator` - 操作者标识（如用户名、角色、用户ID等）
    /// * `target` - 目标标识（被封禁的目标，如 IP 地址、用户ID等）
    ///
    /// # 返回
    ///
    /// * `Ok(())` - 授权通过
    /// * `Err(FlowGuardError::AuthorizationError)` - 授权失败
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::authorization::AuthorizationProvider;
    ///
    /// async fn check_permission(provider: &dyn AuthorizationProvider) {
    ///     // 检查是否允许创建封禁
    ///     let result = provider
    ///         .check_authorization("create_ban", "admin", "192.168.1.1")
    ///         .await;
    ///
    ///     match result {
    ///         Ok(()) => println!("授权通过"),
    ///         Err(e) => println!("授权失败: {}", e),
    ///     }
    /// }
    /// ```
    async fn check_authorization(
        &self,
        operation: &str,
        operator: &str,
        target: &str,
    ) -> Result<(), FlowGuardError>;
}

/// 简单授权提供者
///
/// 基于角色列表的简单授权实现。只有列表中的角色才能执行操作。
///
/// # 特点
///
/// - 简单的角色列表检查
/// - 不区分操作类型
/// - 不检查目标
/// - 适合简单的授权场景
///
/// # 示例
///
/// ```rust
/// use limiteron::authorization::SimpleAuthorizationProvider;
/// use limiteron::authorization::AuthorizationProvider;
///
/// #[tokio::main]
/// async fn main() {
///     // 创建授权提供者，只允许 admin 和 moderator 角色
///     let provider = SimpleAuthorizationProvider::new(vec![
///         "admin".to_string(),
///         "moderator".to_string(),
///     ]);
///
///     // admin 角色授权通过
///     assert!(provider.check_authorization("create_ban", "admin", "user123").await.is_ok());
///
///     // moderator 角色授权通过
///     assert!(provider.check_authorization("create_ban", "moderator", "user456").await.is_ok());
///
///     // 其他角色授权失败
///     assert!(provider.check_authorization("create_ban", "user", "user789").await.is_err());
/// }
/// ```
pub struct SimpleAuthorizationProvider {
    /// 授权角色列表
    authorized_roles: Vec<String>,
}

impl SimpleAuthorizationProvider {
    /// 创建新的简单授权提供者
    ///
    /// # 参数
    ///
    /// * `authorized_roles` - 授权角色列表，只有列表中的角色才能执行操作
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::authorization::SimpleAuthorizationProvider;
    ///
    /// let provider = SimpleAuthorizationProvider::new(vec![
    ///     "admin".to_string(),
    ///     "super_admin".to_string(),
    /// ]);
    /// ```
    pub fn new(authorized_roles: Vec<String>) -> Self {
        Self { authorized_roles }
    }

    /// 从迭代器创建授权提供者
    ///
    /// # 参数
    ///
    /// * `roles` - 角色迭代器
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::authorization::SimpleAuthorizationProvider;
    ///
    /// let provider = SimpleAuthorizationProvider::from_roles(["admin", "moderator"]);
    /// ```
    pub fn from_roles<I, S>(roles: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            authorized_roles: roles.into_iter().map(|s| s.into()).collect(),
        }
    }

    /// 检查角色是否在授权列表中
    ///
    /// # 参数
    ///
    /// * `role` - 要检查的角色
    ///
    /// # 返回
    ///
    /// 如果角色在授权列表中返回 `true`，否则返回 `false`
    pub fn is_authorized(&self, role: &str) -> bool {
        self.authorized_roles.iter().any(|r| r == role)
    }

    /// 获取授权角色列表
    ///
    /// # 返回
    ///
    /// 授权角色列表的引用
    pub fn authorized_roles(&self) -> &[String] {
        &self.authorized_roles
    }

    /// 添加授权角色
    ///
    /// # 参数
    ///
    /// * `role` - 要添加的角色
    pub fn add_role(&mut self, role: String) {
        if !self.authorized_roles.contains(&role) {
            self.authorized_roles.push(role);
        }
    }

    /// 移除授权角色
    ///
    /// # 参数
    ///
    /// * `role` - 要移除的角色
    ///
    /// # 返回
    ///
    /// 如果角色存在并被移除返回 `true`，否则返回 `false`
    pub fn remove_role(&mut self, role: &str) -> bool {
        if let Some(pos) = self.authorized_roles.iter().position(|r| r == role) {
            self.authorized_roles.remove(pos);
            true
        } else {
            false
        }
    }
}

#[async_trait]
impl AuthorizationProvider for SimpleAuthorizationProvider {
    async fn check_authorization(
        &self,
        _operation: &str,
        operator: &str,
        _target: &str,
    ) -> Result<(), FlowGuardError> {
        if self.is_authorized(operator) {
            Ok(())
        } else {
            Err(FlowGuardError::AuthorizationError(format!(
                "操作者 '{}' 未被授权执行此操作",
                operator
            )))
        }
    }
}

/// 允许所有操作的授权提供者（用于测试）
///
/// 此提供者允许所有操作，不进行任何授权检查。
/// 主要用于测试环境或开发环境。
///
/// # 安全警告
///
/// **不要在生产环境中使用此提供者！**
///
/// # 示例
///
/// ```rust
/// use limiteron::authorization::{AllowAllAuthorizationProvider, AuthorizationProvider};
///
/// #[tokio::main]
/// async fn main() {
///     let provider = AllowAllAuthorizationProvider;
///
///     // 所有操作都会被允许
///     assert!(provider.check_authorization("create_ban", "anyone", "anything").await.is_ok());
/// }
/// ```
pub struct AllowAllAuthorizationProvider;

#[async_trait]
impl AuthorizationProvider for AllowAllAuthorizationProvider {
    async fn check_authorization(
        &self,
        _operation: &str,
        _operator: &str,
        _target: &str,
    ) -> Result<(), FlowGuardError> {
        Ok(())
    }
}

/// 拒绝所有操作的授权提供者（用于测试）
///
/// 此提供者拒绝所有操作，用于测试授权失败的场景。
///
/// # 示例
///
/// ```rust
/// use limiteron::authorization::{DenyAllAuthorizationProvider, AuthorizationProvider};
///
/// #[tokio::main]
/// async fn main() {
///     let provider = DenyAllAuthorizationProvider;
///
///     // 所有操作都会被拒绝
///     assert!(provider.check_authorization("create_ban", "admin", "anything").await.is_err());
/// }
/// ```
pub struct DenyAllAuthorizationProvider;

#[async_trait]
impl AuthorizationProvider for DenyAllAuthorizationProvider {
    async fn check_authorization(
        &self,
        _operation: &str,
        _operator: &str,
        _target: &str,
    ) -> Result<(), FlowGuardError> {
        Err(FlowGuardError::AuthorizationError(
            "所有操作都被拒绝".to_string(),
        ))
    }
}

/// 基于操作的授权提供者
///
/// 为不同操作配置不同的授权角色。
///
/// # 示例
///
/// ```rust
/// use limiteron::authorization::{OperationAuthorizationProvider, AuthorizationProvider};
/// use std::collections::HashMap;
///
/// #[tokio::main]
/// async fn main() {
///     let mut operation_roles = HashMap::new();
///     operation_roles.insert("create_ban".to_string(), vec!["admin".to_string(), "moderator".to_string()]);
///     operation_roles.insert("remove_ban".to_string(), vec!["admin".to_string()]);
///
///     let provider = OperationAuthorizationProvider::new(operation_roles);
///
///     // moderator 可以创建封禁
///     assert!(provider.check_authorization("create_ban", "moderator", "user123").await.is_ok());
///
///     // moderator 不能删除封禁
///     assert!(provider.check_authorization("remove_ban", "moderator", "user123").await.is_err());
///
///     // admin 可以删除封禁
///     assert!(provider.check_authorization("remove_ban", "admin", "user123").await.is_ok());
/// }
/// ```
#[cfg(feature = "ban-manager")]
pub struct OperationAuthorizationProvider {
    /// 操作到授权角色的映射
    operation_roles: ahash::AHashMap<String, Vec<String>>,
}

#[cfg(feature = "ban-manager")]
impl OperationAuthorizationProvider {
    /// 创建新的基于操作的授权提供者
    ///
    /// # 参数
    ///
    /// * `operation_roles` - 操作到授权角色列表的映射
    pub fn new(operation_roles: ahash::AHashMap<String, Vec<String>>) -> Self {
        Self { operation_roles }
    }

    /// 创建构建器
    pub fn builder() -> OperationAuthorizationProviderBuilder {
        OperationAuthorizationProviderBuilder::default()
    }

    /// 检查操作是否有配置
    ///
    /// # 参数
    ///
    /// * `operation` - 操作类型
    ///
    /// # 返回
    ///
    /// 如果操作有配置返回 `true`，否则返回 `false`
    pub fn has_operation(&self, operation: &str) -> bool {
        self.operation_roles.contains_key(operation)
    }

    /// 获取操作的授权角色列表
    ///
    /// # 参数
    ///
    /// * `operation` - 操作类型
    ///
    /// # 返回
    ///
    /// 如果操作存在，返回角色列表的引用；否则返回 `None`
    pub fn get_roles(&self, operation: &str) -> Option<&[String]> {
        self.operation_roles.get(operation).map(|v| v.as_slice())
    }
}

#[cfg(feature = "ban-manager")]
#[async_trait]
impl AuthorizationProvider for OperationAuthorizationProvider {
    async fn check_authorization(
        &self,
        operation: &str,
        operator: &str,
        _target: &str,
    ) -> Result<(), FlowGuardError> {
        match self.operation_roles.get(operation) {
            Some(roles) => {
                if roles.iter().any(|r| r == operator) {
                    Ok(())
                } else {
                    Err(FlowGuardError::AuthorizationError(format!(
                        "操作者 '{}' 未被授权执行操作 '{}'",
                        operator, operation
                    )))
                }
            }
            None => Err(FlowGuardError::AuthorizationError(format!(
                "未知的操作类型: '{}'",
                operation
            ))),
        }
    }
}

/// 基于操作的授权提供者构建器
#[cfg(feature = "ban-manager")]
#[derive(Default)]
pub struct OperationAuthorizationProviderBuilder {
    operation_roles: ahash::AHashMap<String, Vec<String>>,
}

#[cfg(feature = "ban-manager")]
impl OperationAuthorizationProviderBuilder {
    /// 添加操作的授权角色
    ///
    /// # 参数
    ///
    /// * `operation` - 操作类型
    /// * `roles` - 授权角色列表
    pub fn with_operation(mut self, operation: impl Into<String>, roles: Vec<String>) -> Self {
        self.operation_roles.insert(operation.into(), roles);
        self
    }

    /// 构建 OperationAuthorizationProvider
    pub fn build(self) -> OperationAuthorizationProvider {
        OperationAuthorizationProvider::new(self.operation_roles)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_authorization_provider_authorized() {
        let provider =
            SimpleAuthorizationProvider::new(vec!["admin".to_string(), "moderator".to_string()]);

        // admin 角色应该通过
        let result = provider
            .check_authorization("create_ban", "admin", "user123")
            .await;
        assert!(result.is_ok());

        // moderator 角色应该通过
        let result = provider
            .check_authorization("create_ban", "moderator", "user456")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_simple_authorization_provider_unauthorized() {
        let provider = SimpleAuthorizationProvider::new(vec!["admin".to_string()]);

        // 普通用户应该被拒绝
        let result = provider
            .check_authorization("create_ban", "user", "target123")
            .await;
        assert!(result.is_err());

        match result {
            Err(FlowGuardError::AuthorizationError(msg)) => {
                assert!(msg.contains("user"));
            }
            _ => panic!("期望 AuthorizationError"),
        }
    }

    #[tokio::test]
    async fn test_simple_authorization_provider_empty_roles() {
        let provider = SimpleAuthorizationProvider::new(vec![]);

        // 空角色列表，所有人都应该被拒绝
        let result = provider
            .check_authorization("create_ban", "admin", "target123")
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_simple_authorization_provider_from_roles() {
        let provider = SimpleAuthorizationProvider::from_roles(["admin", "moderator"]);
        assert_eq!(provider.authorized_roles().len(), 2);
        assert!(provider.is_authorized("admin"));
        assert!(provider.is_authorized("moderator"));
    }

    #[test]
    fn test_simple_authorization_provider_add_remove_role() {
        let mut provider = SimpleAuthorizationProvider::new(vec!["admin".to_string()]);

        // 添加角色
        provider.add_role("moderator".to_string());
        assert!(provider.is_authorized("moderator"));

        // 重复添加不会增加
        provider.add_role("moderator".to_string());
        assert_eq!(provider.authorized_roles().len(), 2);

        // 移除角色
        assert!(provider.remove_role("moderator"));
        assert!(!provider.is_authorized("moderator"));

        // 移除不存在的角色
        assert!(!provider.remove_role("nonexistent"));
    }

    #[tokio::test]
    async fn test_allow_all_authorization_provider() {
        let provider = AllowAllAuthorizationProvider;

        // 所有操作都应该通过
        assert!(provider
            .check_authorization("create_ban", "anyone", "anything")
            .await
            .is_ok());
        assert!(provider
            .check_authorization("remove_ban", "anyone", "anything")
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_deny_all_authorization_provider() {
        let provider = DenyAllAuthorizationProvider;

        // 所有操作都应该被拒绝
        assert!(provider
            .check_authorization("create_ban", "admin", "anything")
            .await
            .is_err());
        assert!(provider
            .check_authorization("remove_ban", "admin", "anything")
            .await
            .is_err());
    }

    #[cfg(feature = "ban-manager")]
    #[tokio::test]
    async fn test_operation_authorization_provider() {
        let mut operation_roles = ahash::AHashMap::new();
        operation_roles.insert(
            "create_ban".to_string(),
            vec!["admin".to_string(), "moderator".to_string()],
        );
        operation_roles.insert("remove_ban".to_string(), vec!["admin".to_string()]);

        let provider = OperationAuthorizationProvider::new(operation_roles);

        // moderator 可以创建封禁
        assert!(provider
            .check_authorization("create_ban", "moderator", "user123")
            .await
            .is_ok());

        // moderator 不能删除封禁
        assert!(provider
            .check_authorization("remove_ban", "moderator", "user123")
            .await
            .is_err());

        // admin 可以删除封禁
        assert!(provider
            .check_authorization("remove_ban", "admin", "user123")
            .await
            .is_ok());

        // 未知操作应该被拒绝
        assert!(provider
            .check_authorization("unknown_op", "admin", "user123")
            .await
            .is_err());
    }

    #[cfg(feature = "ban-manager")]
    #[test]
    fn test_operation_authorization_provider_builder() {
        let provider = OperationAuthorizationProvider::builder()
            .with_operation(
                "create_ban",
                vec!["admin".to_string(), "moderator".to_string()],
            )
            .with_operation("remove_ban", vec!["admin".to_string()])
            .build();

        assert!(provider.has_operation("create_ban"));
        assert!(provider.has_operation("remove_ban"));
        assert!(!provider.has_operation("unknown"));

        assert_eq!(provider.get_roles("create_ban").unwrap().len(), 2);
        assert_eq!(provider.get_roles("remove_ban").unwrap().len(), 1);
    }

    #[test]
    fn test_authorization_error_message() {
        let error = FlowGuardError::AuthorizationError("测试授权错误".to_string());
        assert_eq!(error.to_string(), "授权错误: 测试授权错误");
    }
}
