// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 租户解析器 trait
//!
//! 定义租户解析器接口，用于从请求上下文中提取租户命名空间。

use crate::matchers::RequestContext;
use crate::tenant::Namespace;
use ahash::AHashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// 租户解析器 trait
///
/// 用于从请求上下文中解析租户命名空间，实现租户隔离的限流。
///
/// # 示例
///
/// ```rust
/// use limiteron::{TenantResolver, Namespace};
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
/// ```rust,ignore
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
/// # 安全边界（必读）
///
/// **Header 值完全由请求方控制**：无白名单时，调用方可逐请求轮换该头，
/// 使请求落入自选的租户命名空间——命名空间参与封禁键改写，全局封禁对
/// 携带该头的请求即失效，且可借此探测他人租户的封禁状态。因此：
///
/// - 仅在**可信网关剥离/覆写**该头（客户端无法直达）时使用本解析器；
/// - 否则用 [`HeaderTenantResolver::with_allowed_tenants`] 限定可接受的
///   租户 ID：未命中的头值不解析租户（回退默认命名空间），不给调用方
///   自选命名空间的机会；
/// - 租户身份应由已认证主体（JWT claim / mTLS 证书 / 会话）派生——本
///   crate 提供 [`TenantResolver`] 扩展点与 [`DefaultTenantResolver`]。
///
/// **白名单未命中的副作用**：`resolve` 返回 `None` 时决策键回退到默认
/// 命名空间——伪造头值的流量会并入全局桶（可消耗全局额度），但不会
/// 获得自选命名空间。两者只能二选一：把伪造流量隔离到独立命名空间会
/// 使全局封禁不再覆盖该流量（可绕封禁）。库侧不替调用方做此取舍。
///
/// 同 crate 的 X-Forwarded-For 采信有显式信任模型（仅当对端 ∈ 可信代理
/// 才采信，见 `middleware::tower_middleware`），租户头不应弱于该口径。
///
/// # 示例
///
/// ```rust,ignore
/// use limiteron::tenant::{HeaderTenantResolver, TenantResolver};
/// use limiteron::RequestContext;
///
/// let resolver = HeaderTenantResolver::new("X-Tenant-ID", "production")
///     .with_allowed_tenants(["acme-corp"]);
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
    /// 可信租户白名单（`None` = 不限制，信任网关已覆写该头）
    allowed_tenants: Option<AHashSet<String>>,
    /// 未设白名单时的告警去抖（每次解析仅告警一次）
    unrestricted_warned: Arc<AtomicBool>,
}

impl HeaderTenantResolver {
    /// 创建新的 Header 租户解析器
    ///
    /// # 参数
    ///
    /// - `header_name`: 包含租户 ID 的 Header 名称
    /// - `environment`: 环境标识
    ///
    /// 不设白名单：调用方须自行保证该头由可信网关覆写（见类型文档的
    /// 「安全边界」）。未设白名单时，首次解析到该头即告警一次——该头的
    /// 值完全由请求方控制，库侧无法替调用方建立身份绑定，只能显性留痕。
    pub fn new(header_name: impl Into<String>, environment: impl Into<String>) -> Self {
        Self {
            header_name: header_name.into(),
            environment: environment.into(),
            allowed_tenants: None,
            unrestricted_warned: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 限定可接受的租户 ID 白名单
    ///
    /// 未在白名单内的头值不解析为租户（`resolve` 返回 `None`，请求回退
    /// 默认命名空间），避免请求方自选租户命名空间。
    ///
    /// 空白名单 = 拒绝所有租户头：携带该头的流量全部并入默认命名空间，
    /// 且配置时告警一次（空白名单通常是误配，不应静默生效）。
    #[must_use]
    pub fn with_allowed_tenants<I, S>(mut self, tenants: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let allowed: AHashSet<String> = tenants.into_iter().map(Into::into).collect();
        if allowed.is_empty() {
            // 空白名单会把所有租户头的流量并入默认命名空间（租户维度隔离失效），
            // 且未命中白名单的告警路径不适用——必须显性留痕
            log::warn!(
                target: "limiteron",
                "tenant allowlist for header '{}' is empty; every tenant header will fall back to the default namespace",
                self.header_name
            );
        }
        self.allowed_tenants = Some(allowed);
        self
    }
}

impl TenantResolver for HeaderTenantResolver {
    fn resolve(&self, ctx: &RequestContext) -> Option<Namespace> {
        let tenant_id = ctx.get_header(&self.header_name)?;
        if self.allowed_tenants.is_none() && !self.unrestricted_warned.swap(true, Ordering::Relaxed)
        {
            // 未设白名单：租户 ID 取自客户端可控的头，库侧无认证身份可绑定，
            // 只能显性留痕（每次解析仅告警一次，避免日志洪泛）
            log::warn!(
                target: "limiteron",
                "tenant header '{}' is client-controlled and no tenant allowlist is configured; \
                 ensure a trusted gateway overwrites it, or restrict the accepted tenant IDs \
                 with with_allowed_tenants",
                self.header_name
            );
        }
        if let Some(allowed) = &self.allowed_tenants
            && !allowed.contains(tenant_id.as_str())
        {
            return None;
        }
        Some(Namespace::new(tenant_id, &self.environment))
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
        let resolver = DefaultTenantResolver;
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

    #[test]
    fn test_header_tenant_resolver_allowlist_rejects_unknown_tenant() {
        // 白名单外的租户 ID 不解析（回退默认命名空间），不给请求方
        // 自选租户命名空间的机会
        let resolver = HeaderTenantResolver::new("X-Tenant-ID", "production")
            .with_allowed_tenants(["acme-corp"]);
        let ctx = RequestContext::new().with_header("X-Tenant-ID", "other-corp");
        assert!(resolver.resolve(&ctx).is_none());

        let ctx = RequestContext::new().with_header("X-Tenant-ID", "acme-corp");
        let ns = resolver.resolve(&ctx).unwrap();
        assert_eq!(ns.tenant_id(), "acme-corp");
    }

    #[test]
    fn test_header_tenant_resolver_allowlist_multi_entry() {
        // 白名单含多项时逐项命中判定不受集合内部布局影响：命中项解析为租户，
        // 未命中项回退默认命名空间
        let resolver = HeaderTenantResolver::new("X-Tenant-ID", "production")
            .with_allowed_tenants(["acme-corp", "globex", "initech"]);
        for tenant in ["acme-corp", "globex", "initech"] {
            let ctx = RequestContext::new().with_header("X-Tenant-ID", tenant);
            let ns = resolver.resolve(&ctx).unwrap();
            assert_eq!(ns.tenant_id(), tenant);
            assert_eq!(ns.environment(), "production");
        }

        let ctx = RequestContext::new().with_header("X-Tenant-ID", "not-in-list");
        assert!(resolver.resolve(&ctx).is_none());
    }

    #[test]
    fn test_header_tenant_resolver_empty_allowlist_rejects_all_tenants() {
        // 空白名单 = 拒绝所有租户头：不得被当作"不限制"而放行租户命名空间
        let resolver = HeaderTenantResolver::new("X-Tenant-ID", "production")
            .with_allowed_tenants(Vec::<String>::new());
        let ctx = RequestContext::new().with_header("X-Tenant-ID", "acme-corp");
        assert!(resolver.resolve(&ctx).is_none());

        // 未设白名单的解析器仍按原语义解析租户（空白名单不放行）
        let unrestricted = HeaderTenantResolver::new("X-Tenant-ID", "production");
        let ns = unrestricted.resolve(&ctx).unwrap();
        assert_eq!(ns.tenant_id(), "acme-corp");
    }
}
