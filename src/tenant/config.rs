// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 租户命名空间配置
//!
//! 定义租户命名空间结构，用于限流键的租户隔离。

use serde::{Deserialize, Serialize};
use std::fmt;

/// 转义 tenant_id 中的 ":" 为 "::" 防止命名空间前缀注入。
///
/// `Namespace::qualify_key` 将键格式化为 `tenant:{tenant_id}:env:{environment}:{key}`。
/// 若 tenant_id 包含 ":"，可能与其它租户的前缀冲突（如 tenant_id="a:b" + env="c"
/// 与 tenant_id="a" + env="b:c" 产生相同前缀）。转义后消除歧义。
pub fn sanitize_tenant_id(tenant_id: &str) -> String {
    tenant_id.replace(':', "::")
}

/// 转义 environment 中的 ":" 为 "::" 防止命名空间前缀注入。
///
/// 与 `sanitize_tenant_id` 同理：environment 字段也被插值到命名空间前缀中，
/// 未转义的 ":" 会允许跨环境的前缀冲突。
pub fn sanitize_environment(environment: &str) -> String {
    environment.replace(':', "::")
}

/// 租户命名空间
///
/// 用于标识请求所属的租户和环境，确保限流键在不同租户之间隔离。
///
/// # 示例
///
/// ```rust
/// use limiteron::Namespace;
///
/// let namespace = Namespace::new("tenant-123", "production");
/// assert_eq!(namespace.tenant_id(), "tenant-123");
/// assert_eq!(namespace.environment(), "production");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Namespace {
    /// 租户唯一标识
    tenant_id: String,
    /// 环境标识（如: production, staging, development）
    environment: String,
}

impl Namespace {
    /// 创建新的命名空间
    ///
    /// # 参数
    ///
    /// - `tenant_id`: 租户唯一标识
    /// - `environment`: 环境标识
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::Namespace;
    ///
    /// let ns = Namespace::new("acme-corp", "production");
    /// ```
    pub fn new(tenant_id: impl Into<String>, environment: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            environment: environment.into(),
        }
    }

    /// 获取租户 ID
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    /// 获取环境标识
    pub fn environment(&self) -> &str {
        &self.environment
    }

    /// 生成命名空间的唯一前缀
    ///
    /// 格式: `tenant:{tenant_id}:env:{environment}`，其中 tenant_id 和 environment
    /// 中的 ":" 被转义为 "::" 防止前缀注入（不同租户/环境的键冲突）。
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::Namespace;
    ///
    /// let ns = Namespace::new("acme", "prod");
    /// assert_eq!(ns.prefix(), "tenant:acme:env:prod");
    /// ```
    pub fn prefix(&self) -> String {
        format!(
            "tenant:{}:env:{}",
            sanitize_tenant_id(&self.tenant_id),
            sanitize_environment(&self.environment)
        )
    }

    /// 为给定的限流键添加命名空间前缀
    ///
    /// # 参数
    ///
    /// - `key`: 原始限流键
    ///
    /// # 返回
    ///
    /// 带命名空间前缀的限流键
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::Namespace;
    ///
    /// let ns = Namespace::new("acme", "prod");
    /// let namespaced_key = ns.qualify_key("rl:user:123:rule1");
    /// assert_eq!(namespaced_key, "tenant:acme:env:prod:rl:user:123:rule1");
    /// ```
    pub fn qualify_key(&self, key: &str) -> String {
        format!("{}:{}", self.prefix(), key)
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.prefix())
    }
}

impl Default for Namespace {
    /// 创建默认的命名空间（全局租户，开发环境）
    fn default() -> Self {
        Self {
            tenant_id: "global".to_string(),
            environment: "development".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_creation() {
        let ns = Namespace::new("tenant-123", "production");
        assert_eq!(ns.tenant_id(), "tenant-123");
        assert_eq!(ns.environment(), "production");
    }

    #[test]
    fn test_namespace_prefix() {
        let ns = Namespace::new("acme", "prod");
        assert_eq!(ns.prefix(), "tenant:acme:env:prod");
    }

    #[test]
    fn test_namespace_qualify_key() {
        let ns = Namespace::new("acme", "prod");
        let qualified = ns.qualify_key("rl:user:123:rule1");
        assert_eq!(qualified, "tenant:acme:env:prod:rl:user:123:rule1");
    }

    #[test]
    fn test_namespace_display() {
        let ns = Namespace::new("tenant-1", "staging");
        assert_eq!(format!("{}", ns), "tenant:tenant-1:env:staging");
    }

    #[test]
    fn test_namespace_default() {
        let ns = Namespace::default();
        assert_eq!(ns.tenant_id(), "global");
        assert_eq!(ns.environment(), "development");
    }

    #[test]
    fn test_namespace_equality() {
        let ns1 = Namespace::new("tenant-1", "prod");
        let ns2 = Namespace::new("tenant-1", "prod");
        let ns3 = Namespace::new("tenant-2", "prod");

        assert_eq!(ns1, ns2);
        assert_ne!(ns1, ns3);
    }

    #[test]
    fn test_namespace_hash() {
        use ahash::AHashSet;

        let ns1 = Namespace::new("tenant-1", "prod");
        let ns2 = Namespace::new("tenant-1", "prod");
        let ns3 = Namespace::new("tenant-2", "prod");

        let mut set = AHashSet::new();
        set.insert(ns1.clone());
        set.insert(ns2.clone());
        set.insert(ns3.clone());

        assert_eq!(set.len(), 2);
        assert!(set.contains(&ns1));
        assert!(set.contains(&ns3));
    }

    #[test]
    fn test_namespace_serialization() {
        let ns = Namespace::new("tenant-123", "production");
        let json = serde_json::to_string(&ns).unwrap();

        let deserialized: Namespace = serde_json::from_str(&json).unwrap();
        assert_eq!(ns, deserialized);
    }

    #[test]
    fn test_namespace_with_empty_strings() {
        let ns = Namespace::new("", "");
        assert_eq!(ns.tenant_id(), "");
        assert_eq!(ns.environment(), "");
        assert_eq!(ns.prefix(), "tenant::env:");
    }

    #[test]
    fn test_namespace_with_special_characters() {
        // MEDIUM-fix: tenant_id/environment 中的 ":" 被转义为 "::" 防止前缀注入
        let ns = Namespace::new("tenant_a/b:c@d!", "prod-v1.2.3");
        assert_eq!(ns.tenant_id(), "tenant_a/b:c@d!");
        assert_eq!(ns.environment(), "prod-v1.2.3");
        // ":" → "::" 转义；其他特殊字符（/ @ !）不变
        assert_eq!(ns.prefix(), "tenant:tenant_a/b::c@d!:env:prod-v1.2.3");
    }

    // ========================================================================
    // namespace key prefix injection 修复测试
    //
    // 验证策略：
    // 1. sanitize_tenant_id / sanitize_environment 将 ":" 转义为 "::"
    // 2. 无 ":" 的输入保持不变（向后兼容）
    // 3. 不同 (tenant_id, environment) 对不再产生冲突的 qualified key
    // ========================================================================

    #[test]
    fn test_sanitize_tenant_id_escapes_colon() {
        assert_eq!(sanitize_tenant_id("a:b"), "a::b");
        assert_eq!(sanitize_tenant_id("a:b:c"), "a::b::c");
        assert_eq!(sanitize_tenant_id("::"), "::::");
    }

    #[test]
    fn test_sanitize_tenant_id_no_colon_unchanged() {
        assert_eq!(sanitize_tenant_id("acme"), "acme");
        assert_eq!(sanitize_tenant_id(""), "");
        assert_eq!(sanitize_tenant_id("tenant-123"), "tenant-123");
    }

    #[test]
    fn test_sanitize_environment_escapes_colon() {
        assert_eq!(sanitize_environment("prod:v2"), "prod::v2");
        assert_eq!(sanitize_environment("a:b:c"), "a::b::c");
    }

    #[test]
    fn test_sanitize_environment_no_colon_unchanged() {
        assert_eq!(sanitize_environment("production"), "production");
        assert_eq!(sanitize_environment(""), "");
        assert_eq!(sanitize_environment("prod-v1.2.3"), "prod-v1.2.3");
    }

    #[test]
    fn test_namespace_prefix_injection_environment_no_collision() {
        // 无转义时，(env="prod", key="rl:user") 与 (env="prod:rl", key="user")
        // 会产生相同的 qualified key —— 前缀注入漏洞。
        // 转义后 environment 中的 ":" 变为 "::"，两个 key 必须不同。
        let ns_normal = Namespace::new("acme", "prod");
        let ns_injected = Namespace::new("acme", "prod:rl");
        let key_normal = ns_normal.qualify_key("rl:user");
        let key_injected = ns_injected.qualify_key("user");
        assert_ne!(
            key_normal, key_injected,
            "environment 中的 ':' 必须被转义，防止 qualified key 冲突"
        );
    }

    #[test]
    fn test_namespace_prefix_injection_tenant_id_no_collision() {
        // 无转义时，(tenant="a:env:b", env="c") 与 (tenant="a", env="b:env:c")
        // 会产生相同的 prefix —— 前缀注入漏洞。
        // 转义后 tenant_id 中的 ":" 变为 "::"，两个 prefix 必须不同。
        let ns_injected_tenant = Namespace::new("a:env:b", "c");
        let ns_injected_env = Namespace::new("a", "b:env:c");
        assert_ne!(
            ns_injected_tenant.prefix(),
            ns_injected_env.prefix(),
            "tenant_id 中的 ':' 必须被转义，防止 prefix 冲突"
        );
    }

    #[test]
    fn test_namespace_qualify_key_stable_for_safe_ids() {
        // 无 ":" 的 tenant_id/environment 不受转义影响（向后兼容）
        let ns = Namespace::new("acme-corp", "production");
        assert_eq!(
            ns.qualify_key("rl:user:123"),
            "tenant:acme-corp:env:production:rl:user:123"
        );
    }

    #[test]
    fn test_namespace_qualify_key_edge_cases() {
        let ns = Namespace::new("acme", "prod");
        assert_eq!(ns.qualify_key(""), "tenant:acme:env:prod:");
        assert_eq!(ns.qualify_key("::"), "tenant:acme:env:prod:::");
    }

    #[test]
    fn test_namespace_display_matches_prefix() {
        let ns = Namespace::new("tenant-1", "staging");
        assert_eq!(format!("{}", ns), ns.prefix());
    }

    #[test]
    fn test_namespace_clone_independent() {
        let ns1 = Namespace::new("original", "prod");
        let ns2 = ns1.clone();
        // modify ns2 via qualify_key (it returns a new String, doesn't mutate)
        // ns2 should still equal ns1 since we didn't mutate
        assert_eq!(ns1, ns2);
        // verify they really are different objects via serialization roundtrip
        let json2 = serde_json::to_string(&ns2).unwrap();
        let ns2_roundtrip: Namespace = serde_json::from_str(&json2).unwrap();
        assert_eq!(ns1, ns2_roundtrip);
    }
}
