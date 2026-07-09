//! Authorization 示例
//!
//! 演示授权提供者 trait 的实现与使用，包括内置的 SimpleAuthorizationProvider。
//!
//! # 涵盖 API
//!
//! - `AuthorizationProvider` trait（`check_authorization(operation, operator, target).await`）
//! - `SimpleAuthorizationProvider`（`new`、`from_roles`、`is_authorized`、`add_role`、`remove_role`）
//! - 自定义 `AuthorizationProvider` 实现
//! - 通过 `Arc<dyn AuthorizationProvider>` 实现依赖注入
//!
//! # 运行方式
//!
//! ```bash
//! cargo run --bin authorization_demo
//! ```

use async_trait::async_trait;
use limiteron::authorization::{AuthorizationProvider, SimpleAuthorizationProvider};
use limiteron::error::FlowGuardError;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Authorization Demo ===\n");

    demo_simple_provider().await?;
    demo_role_management()?;
    demo_custom_provider().await?;
    demo_dependency_injection().await?;

    println!("\n=== All demos completed ===");
    Ok(())
}

/// 演示 SimpleAuthorizationProvider 基本用法
async fn demo_simple_provider() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 1. SimpleAuthorizationProvider ---\n");

    let provider =
        SimpleAuthorizationProvider::new(vec!["admin".to_string(), "moderator".to_string()]);

    println!("  Authorized roles: {:?}", provider.authorized_roles());

    // admin 可以执行任何操作
    let r1 = provider
        .check_authorization("create_ban", "admin", "192.168.1.1")
        .await;
    println!("\n  admin create_ban: {}", format_result(&r1));

    // moderator 也可以
    let r2 = provider
        .check_authorization("create_ban", "moderator", "user-123")
        .await;
    println!("  moderator create_ban: {}", format_result(&r2));

    // 普通用户被拒绝
    let r3 = provider
        .check_authorization("create_ban", "guest", "user-456")
        .await;
    println!("  guest create_ban: {}", format_result(&r3));
    println!();
    Ok(())
}

/// 演示角色管理：动态添加和移除角色
fn demo_role_management() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 2. Role Management ---\n");

    let mut provider = SimpleAuthorizationProvider::from_roles(["admin"]);

    println!("  Initial roles: {:?}", provider.authorized_roles());

    // 添加新角色
    provider.add_role("superuser".to_string());
    provider.add_role("auditor".to_string());
    // 重复添加不会生效
    provider.add_role("superuser".to_string());
    println!(
        "\n  After add_role(superuser, auditor): {:?}",
        provider.authorized_roles()
    );

    println!(
        "  is_authorized('admin')={}",
        provider.is_authorized("admin")
    );
    println!(
        "  is_authorized('superuser')={}",
        provider.is_authorized("superuser")
    );
    println!(
        "  is_authorized('guest')={}",
        provider.is_authorized("guest")
    );

    // 移除角色
    let removed = provider.remove_role("auditor");
    println!("\n  remove_role('auditor')={}", removed);
    println!(
        "  remove_role('nonexistent')={}",
        provider.remove_role("nonexistent")
    );
    println!("  Final roles: {:?}", provider.authorized_roles());
    println!();
    Ok(())
}

/// 演示自定义 AuthorizationProvider 实现
async fn demo_custom_provider() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 3. Custom AuthorizationProvider ---\n");

    /// 自定义授权提供者：基于操作-角色映射
    struct OperationRoleProvider {
        rules: ahash::AHashMap<String, Vec<String>>,
    }

    impl OperationRoleProvider {
        fn new() -> Self {
            let mut rules = ahash::AHashMap::new();
            rules.insert("create_ban".to_string(), vec!["admin".to_string()]);
            rules.insert("remove_ban".to_string(), vec!["admin".to_string()]);
            rules.insert(
                "view_ban".to_string(),
                vec!["admin".to_string(), "auditor".to_string()],
            );
            Self { rules }
        }
    }

    #[async_trait]
    impl AuthorizationProvider for OperationRoleProvider {
        async fn check_authorization(
            &self,
            operation: &str,
            operator: &str,
            _target: &str,
        ) -> Result<(), FlowGuardError> {
            match self.rules.get(operation) {
                Some(roles) if roles.iter().any(|r| r == operator) => Ok(()),
                Some(_) => Err(FlowGuardError::AuthorizationError(format!(
                    "操作者 '{}' 无权执行 '{}'",
                    operator, operation
                ))),
                None => Err(FlowGuardError::AuthorizationError(format!(
                    "未知操作: '{}'",
                    operation
                ))),
            }
        }
    }

    let provider = OperationRoleProvider::new();

    let r1 = provider
        .check_authorization("create_ban", "admin", "ip-1")
        .await;
    let r2 = provider
        .check_authorization("view_ban", "auditor", "ip-2")
        .await;
    let r3 = provider
        .check_authorization("create_ban", "auditor", "ip-3")
        .await;
    let r4 = provider
        .check_authorization("delete_all", "admin", "ip-4")
        .await;

    println!("  admin create_ban:  {}", format_result(&r1));
    println!("  auditor view_ban:  {}", format_result(&r2));
    println!("  auditor create_ban: {}", format_result(&r3));
    println!("  admin delete_all:  {}", format_result(&r4));
    println!();
    Ok(())
}

/// 演示通过 Arc<dyn AuthorizationProvider> 实现依赖注入
async fn demo_dependency_injection() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 4. Dependency Injection ---\n");

    // 使用 trait 对象实现多态
    let provider: Arc<dyn AuthorizationProvider> =
        Arc::new(SimpleAuthorizationProvider::from_roles([
            "admin".to_string()
        ]));

    // 模拟一个服务，注入授权提供者
    let ban_service = BanService { auth: provider };

    let r1 = ban_service.create_ban("admin", "192.168.1.1").await;
    let r2 = ban_service.create_ban("user", "192.168.1.2").await;

    println!("  BanService.create_ban('admin'): {}", format_result(&r1));
    println!("  BanService.create_ban('user'):  {}", format_result(&r2));
    println!();
    Ok(())
}

/// 模拟一个封禁服务，演示授权提供者的注入使用
struct BanService {
    auth: Arc<dyn AuthorizationProvider>,
}

impl BanService {
    async fn create_ban(&self, operator: &str, target: &str) -> Result<(), FlowGuardError> {
        self.auth
            .check_authorization("create_ban", operator, target)
            .await?;
        // 授权通过后执行封禁逻辑（此处仅作演示）
        Ok(())
    }
}

/// 格式化授权检查结果
fn format_result(result: &Result<(), FlowGuardError>) -> String {
    match result {
        Ok(()) => "✅ Authorized".to_string(),
        Err(e) => format!("❌ Denied ({})", e),
    }
}
