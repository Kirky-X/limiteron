//! Copyright (c) 2026, Kirky.X
//!
//! MIT License
//!
//! 租户命名空间配置
//!
//! 定义租户命名空间结构，用于限流键的租户隔离。

use serde::{Deserialize, Serialize};
use std::fmt;

/// 租户命名空间
///
/// 用于标识请求所属的租户和环境，确保限流键在不同租户之间隔离。
///
/// # 示例
///
/// ```rust
/// use limiteron::tenant::Namespace;
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
    /// use limiteron::tenant::Namespace;
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
    /// 格式: `tenant:{tenant_id}:env:{environment}`
    ///
    /// # 示例
    ///
    /// ```rust
    /// use limiteron::tenant::Namespace;
    ///
    /// let ns = Namespace::new("acme", "prod");
    /// assert_eq!(ns.prefix(), "tenant:acme:env:prod");
    /// ```
    pub fn prefix(&self) -> String {
        format!("tenant:{}:env:{}", self.tenant_id, self.environment)
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
    /// use limiteron::tenant::Namespace;
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
        use std::collections::HashSet;

        let ns1 = Namespace::new("tenant-1", "prod");
        let ns2 = Namespace::new("tenant-1", "prod");
        let ns3 = Namespace::new("tenant-2", "prod");

        let mut set = HashSet::new();
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
}
