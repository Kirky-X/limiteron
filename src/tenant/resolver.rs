//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 租户解析器 trait
//!
//! 定义租户解析器接口，用于从请求上下文中提取租户命名空间。

use crate::matchers::RequestContext;
use crate::tenant::Namespace;

/// 租户解析器 trait
///
/// 用于从请求上下文中解析租户命名空间，实现租户隔离的限流。
///
/// # 示例
///
/// ```rust
/// use limiteron::tenant::{TenantResolver, Namespace};
/// use limiteron::RequestContext;
///
/// struct HeaderTenantResolver;
///
/// impl TenantResolver for HeaderTenantResolver {
///     fn resolve(&self, ctx: &RequestContext) -> Option<Namespace> {
///         ctx.get_header("X-Tenant-ID")
///             .map(|tenant_id| Namespace::new(tenant_id, "production"))
///     }
/// }
/// ```
pub trait TenantResolver: Send + Sync {
    /// 从请求上下文中解析租户命名空间
    ///
    /// # 参数
    ///
    /// - `ctx`: 请求上下文
    ///
    /// # 返回
    ///
    /// - `Some(Namespace)`: 成功解析到租户命名空间
    /// - `None`: 无法解析租户（将使用默认命名空间）
    fn resolve(&self, ctx: &RequestContext) -> Option<Namespace>;
}

/// 默认租户解析器
///
/// 始终返回默认命名空间（global/development）。
///
/// # 示例
///
/// ```rust
/// use limiteron::tenant::{DefaultTenantResolver, TenantResolver};
/// use limiteron::RequestContext;
///
/// let resolver = DefaultTenantResolver;
/// let ctx = RequestContext::new();
/// let namespace = resolver.resolve(&ctx).unwrap();
/// assert_eq!(namespace.tenant_id(), "global");
/// ```
#[derive(Debug, Clone, Default)]
pub struct DefaultTenantResolver;

impl TenantResolver for DefaultTenantResolver {
    fn resolve(&self, _ctx: &RequestContext) -> Option<Namespace> {
        Some(Namespace::default())
    }
}

/// 基于 Header 的租户解析器
///
/// 从指定的 HTTP Header 中提取租户 ID。
///
/// # 示例
///
/// ```rust
/// use limiteron::tenant::{HeaderTenantResolver, TenantResolver};
/// use limiteron::RequestContext;
///
/// let resolver = HeaderTenantResolver::new("X-Tenant-ID", "production");
/// let ctx = RequestContext::new().with_header("X-Tenant-ID", "acme-corp");
/// let namespace = resolver.resolve(&ctx).unwrap();
/// assert_eq!(namespace.tenant_id(), "acme-corp");
/// assert_eq!(namespace.environment(), "production");
/// ```
#[derive(Debug, Clone)]
pub struct HeaderTenantResolver {
    /// Header 名称
    header_name: String,
    /// 环境标识
    environment: String,
}

impl HeaderTenantResolver {
    /// 创建新的 Header 租户解析器
    ///
    /// # 参数
    ///
    /// - `header_name`: 包含租户 ID 的 Header 名称
    /// - `environment`: 环境标识
    pub fn new(header_name: impl Into<String>, environment: impl Into<String>) -> Self {
        Self {
            header_name: header_name.into(),
            environment: environment.into(),
        }
    }
}

impl TenantResolver for HeaderTenantResolver {
    fn resolve(&self, ctx: &RequestContext) -> Option<Namespace> {
        ctx.get_header(&self.header_name)
            .map(|tenant_id| Namespace::new(tenant_id, &self.environment))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_tenant_resolver() {
        let resolver = DefaultTenantResolver;
        let ctx = RequestContext::new();
        let namespace = resolver.resolve(&ctx).unwrap();

        assert_eq!(namespace.tenant_id(), "global");
        assert_eq!(namespace.environment(), "development");
    }

    #[test]
    fn test_header_tenant_resolver_success() {
        let resolver = HeaderTenantResolver::new("X-Tenant-ID", "production");
        let ctx = RequestContext::new().with_header("X-Tenant-ID", "acme-corp");
        let namespace = resolver.resolve(&ctx).unwrap();

        assert_eq!(namespace.tenant_id(), "acme-corp");
        assert_eq!(namespace.environment(), "production");
    }

    #[test]
    fn test_header_tenant_resolver_missing_header() {
        let resolver = HeaderTenantResolver::new("X-Tenant-ID", "production");
        let ctx = RequestContext::new();
        let result = resolver.resolve(&ctx);

        assert!(result.is_none());
    }

    #[test]
    fn test_header_tenant_resolver_case_insensitive() {
        let resolver = HeaderTenantResolver::new("x-tenant-id", "production");
        let ctx = RequestContext::new().with_header("X-Tenant-ID", "acme-corp");
        let namespace = resolver.resolve(&ctx).unwrap();

        assert_eq!(namespace.tenant_id(), "acme-corp");
    }

    #[test]
    fn test_default_produces_default_namespace() {
        let resolver = DefaultTenantResolver::default();
        let ctx = RequestContext::new();
        let namespace = resolver.resolve(&ctx).unwrap();

        assert_eq!(namespace, Namespace::default());
    }

    #[test]
    fn test_header_resolver_with_different_environments() {
        let resolver = HeaderTenantResolver::new("X-Env", "staging");
        let ctx = RequestContext::new().with_header("X-Env", "customer-42");
        let ns = resolver.resolve(&ctx).unwrap();

        assert_eq!(ns.tenant_id(), "customer-42");
        assert_eq!(ns.environment(), "staging");
    }

    #[test]
    fn test_header_resolver_non_matching_header_name() {
        let resolver = HeaderTenantResolver::new("Authorization", "production");
        let ctx = RequestContext::new().with_header("X-Tenant-ID", "acme");
        let result = resolver.resolve(&ctx);

        assert!(
            result.is_none(),
            "Authorization header should not match X-Tenant-ID"
        );
    }

    #[test]
    fn test_header_resolver_empty_header_value() {
        let resolver = HeaderTenantResolver::new("X-Tenant-ID", "production");
        let ctx = RequestContext::new().with_header("X-Tenant-ID", "");
        let ns = resolver.resolve(&ctx).unwrap();

        assert_eq!(
            ns.tenant_id(),
            "",
            "tenant_id should be empty string from empty header value"
        );
    }

    #[test]
    fn test_header_resolver_debug_output() {
        let resolver = HeaderTenantResolver::new("X-Tenant-ID", "production");
        let debug = format!("{:?}", resolver);
        assert!(
            debug.contains("HeaderTenantResolver"),
            "Debug output should contain type name"
        );
        assert!(
            debug.contains("X-Tenant-ID") || debug.contains("x-tenant-id"),
            "Debug output should contain header name"
        );
    }
}
